//! Hash equivalence data in hashserv caches.

use calf_granary_server_db::queries::hashserv::{
    FindOuthash, FindTask, delete_orphan_outhashes, existing_unihashes, expire_unihashes,
    find_equivalent, find_outhash, find_task, find_unihash, find_unihashes, insert_outhash,
    insert_unihash, touch_unihashes,
};
use chrono::{DateTime, Utc};
use deadpool_postgres::Client;

use super::Cache;

/// What a build reports after running a task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    /// The hash method.
    pub(crate) method: String,
    /// The task's input hash.
    pub(crate) taskhash: String,
    /// The task's output hash.
    pub(crate) outhash: String,
    /// The unihash that the build used.
    pub(crate) unihash: String,
    /// Who reported, for statistics.
    pub(crate) owner: Option<String>,
    /// The recipe name.
    pub(crate) pn: Option<String>,
    /// The recipe version.
    pub(crate) pv: Option<String>,
    /// The recipe revision.
    pub(crate) pr: Option<String>,
    /// The task name.
    pub(crate) task: Option<String>,
    /// The signature data of the output hash.
    pub(crate) outhash_siginfo: Option<String>,
}

impl Cache {
    /// Looks up the unihashes of `(method, taskhash)` pairs, in order.
    pub(crate) async fn unihashes(
        &self,
        client: &Client,
        tasks: &[(String, String)],
    ) -> Result<Vec<Option<String>>, tokio_postgres::Error> {
        let mut found: Vec<Option<String>> = vec![None; tasks.len()];
        for cache in self.lookup_chain() {
            let missing: Vec<usize> = (0..tasks.len())
                .filter(|index| found.get(*index).is_some_and(Option::is_none))
                .collect();
            if missing.is_empty() {
                break;
            }
            let methods: Vec<&str> = missing
                .iter()
                .filter_map(|index| tasks.get(*index).map(|task| task.0.as_str()))
                .collect();
            let taskhashes: Vec<&str> = missing
                .iter()
                .filter_map(|index| tasks.get(*index).map(|task| task.1.as_str()))
                .collect();
            let rows = find_unihashes()
                .bind(
                    client,
                    &methods.as_slice(),
                    &taskhashes.as_slice(),
                    &cache.name.as_ref(),
                )
                .all()
                .await?;
            for row in &rows {
                let position = usize::try_from(row.position)
                    .ok()
                    .and_then(|position| position.checked_sub(1))
                    .and_then(|position| missing.get(position));
                if let Some(slot) = position.and_then(|index| found.get_mut(*index)) {
                    *slot = Some(row.unihash.clone());
                }
            }
            if !rows.is_empty() {
                touch_unihashes()
                    .bind(
                        client,
                        &methods.as_slice(),
                        &taskhashes.as_slice(),
                        &cache.name.as_ref(),
                    )
                    .await?;
            }
        }
        Ok(found)
    }

    /// Checks which unihashes this cache or its fallback knows, in order.
    pub(crate) async fn unihashes_exist(
        &self,
        client: &Client,
        unihashes: &[String],
    ) -> Result<Vec<bool>, tokio_postgres::Error> {
        let caches: Vec<&str> = self
            .lookup_chain()
            .map(|cache| cache.name.as_ref())
            .collect();
        let positions = existing_unihashes()
            .bind(client, &unihashes, &caches.as_slice())
            .all()
            .await?;
        let mut exists = vec![false; unihashes.len()];
        for position in positions {
            let index = usize::try_from(position)
                .ok()
                .and_then(|position| position.checked_sub(1));
            if let Some(slot) = index.and_then(|index| exists.get_mut(index)) {
                *slot = true;
            }
        }
        Ok(exists)
    }

    /// Looks up the unihash of a task.
    pub(crate) async fn unihash(
        &self,
        client: &Client,
        method: &str,
        taskhash: &str,
    ) -> Result<Option<String>, tokio_postgres::Error> {
        let tasks = [(method.to_owned(), taskhash.to_owned())];
        Ok(self
            .unihashes(client, &tasks)
            .await?
            .into_iter()
            .flatten()
            .next())
    }

    /// Looks up the oldest report of a task with its unihash.
    pub(crate) async fn task(
        &self,
        client: &Client,
        method: &str,
        taskhash: &str,
    ) -> Result<Option<FindTask>, tokio_postgres::Error> {
        for cache in self.lookup_chain() {
            let row = find_task()
                .bind(client, &cache.name.as_ref(), &method, &taskhash)
                .opt()
                .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        Ok(None)
    }

    /// Looks up the oldest report of an output hash.
    pub(crate) async fn outhash(
        &self,
        client: &Client,
        method: &str,
        outhash: &str,
        with_unihash: bool,
    ) -> Result<Option<FindOuthash>, tokio_postgres::Error> {
        for cache in self.lookup_chain() {
            let row = find_outhash()
                .bind(
                    client,
                    &cache.name.as_ref(),
                    &method,
                    &outhash,
                    &with_unihash,
                )
                .opt()
                .await?;
            if row.is_some() {
                return Ok(row);
            }
        }
        Ok(None)
    }

    /// Records a report and returns the unihash that the task now has.
    ///
    /// The first output hash wins: a new report adopts the unihash of the
    /// oldest other task with the same output hash, here or in the fallback.
    pub(crate) async fn report(
        &self,
        client: &Client,
        report: &Report,
    ) -> Result<String, tokio_postgres::Error> {
        let inserted = insert_outhash()
            .bind(
                client,
                &self.name.as_ref(),
                &report.method.as_str(),
                &report.taskhash.as_str(),
                &report.outhash.as_str(),
                &report.owner.as_deref(),
                &report.pn.as_deref(),
                &report.pv.as_deref(),
                &report.pr.as_deref(),
                &report.task.as_deref(),
                &report.outhash_siginfo.as_deref(),
            )
            .await?;
        if inserted > 0 {
            let mut unihash = None;
            for cache in self.lookup_chain() {
                unihash = find_equivalent()
                    .bind(
                        client,
                        &cache.name.as_ref(),
                        &report.method.as_str(),
                        &report.outhash.as_str(),
                        &report.taskhash.as_str(),
                    )
                    .opt()
                    .await?;
                if unihash.is_some() {
                    break;
                }
            }
            let unihash = unihash.unwrap_or_else(|| report.unihash.clone());
            insert_unihash()
                .bind(
                    client,
                    &self.name.as_ref(),
                    &report.method.as_str(),
                    &report.taskhash.as_str(),
                    &unihash.as_str(),
                )
                .await?;
        }
        let current = find_unihash()
            .bind(
                client,
                &self.name.as_ref(),
                &report.method.as_str(),
                &report.taskhash.as_str(),
            )
            .opt()
            .await?;
        Ok(current.unwrap_or_else(|| report.unihash.clone()))
    }

    /// Answers a report without recording it, for clients that may only
    /// read.
    pub(crate) async fn report_read_only(
        &self,
        client: &Client,
        report: &Report,
    ) -> Result<String, tokio_postgres::Error> {
        let row = self
            .outhash(client, &report.method, &report.outhash, true)
            .await?;
        Ok(row
            .and_then(|row| row.unihash)
            .unwrap_or_else(|| report.unihash.clone()))
    }

    /// Records that a task is equivalent to a unihash, unless the task has
    /// one already, and returns the task's unihash.
    pub(crate) async fn report_equivalent(
        &self,
        client: &Client,
        method: &str,
        taskhash: &str,
        unihash: &str,
    ) -> Result<String, tokio_postgres::Error> {
        insert_unihash()
            .bind(client, &self.name.as_ref(), &method, &taskhash, &unihash)
            .await?;
        let current = find_unihash()
            .bind(client, &self.name.as_ref(), &method, &taskhash)
            .opt()
            .await?;
        Ok(current.unwrap_or_else(|| unihash.to_owned()))
    }

    /// Deletes unihashes that weren't used since `cutoff`, and the reports
    /// of tasks without unihashes. Returns how many unihashes were deleted.
    pub(super) async fn expire_unihashes(
        &self,
        client: &Client,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, tokio_postgres::Error> {
        let expired = expire_unihashes()
            .bind(client, &self.name.as_ref(), &cutoff.fixed_offset())
            .await?;
        delete_orphan_outhashes()
            .bind(client, &self.name.as_ref())
            .await?;
        Ok(expired)
    }
}
