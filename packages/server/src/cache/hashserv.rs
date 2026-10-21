//! Hash equivalence data in hashserv caches.

use calf_granary_server_db::queries::hashserv::{
    HashservReport, existing_unihashes, expire_unihashes, find_equivalent, find_outhash, find_task,
    find_unihash, find_unihashes, insert_outhash, insert_unihash, touch_unihashes,
};
use chrono::{DateTime, FixedOffset, Utc};
use deadpool_postgres::Client;

use super::Cache;
use crate::sha256::{InvalidSha256, Sha256};

/// What a build reports after running a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report<'a> {
    /// The hash method.
    pub(crate) method: &'a str,
    /// The task's input hash.
    pub(crate) taskhash: Sha256,
    /// The task's output hash.
    pub(crate) outhash: Sha256,
    /// The unihash that the build used.
    pub(crate) unihash: Sha256,
    /// Who reported, for statistics.
    pub(crate) owner: Option<&'a str>,
    /// The recipe name.
    pub(crate) pn: Option<&'a str>,
    /// The recipe version.
    pub(crate) pv: Option<&'a str>,
    /// The recipe revision.
    pub(crate) pr: Option<&'a str>,
    /// The task name.
    pub(crate) task: Option<&'a str>,
    /// The signature data of the output hash.
    pub(crate) outhash_siginfo: Option<&'a str>,
}

/// A recorded report with the unihash its task has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredReport {
    /// The task's input hash.
    pub(crate) taskhash: Sha256,
    /// The task's output hash.
    pub(crate) outhash: Sha256,
    /// The task's unihash.
    pub(crate) unihash: Sha256,
    /// When the report arrived.
    pub(crate) created_at: DateTime<FixedOffset>,
    /// Who reported.
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

impl TryFrom<HashservReport> for StoredReport {
    type Error = InvalidSha256;

    fn try_from(row: HashservReport) -> Result<Self, Self::Error> {
        Ok(Self {
            taskhash: row.taskhash.as_slice().try_into()?,
            outhash: row.outhash.as_slice().try_into()?,
            unihash: row.unihash.as_slice().try_into()?,
            created_at: row.created_at,
            owner: row.owner,
            pn: row.pn,
            pv: row.pv,
            pr: row.pr,
            task: row.task,
            outhash_siginfo: row.outhash_siginfo,
        })
    }
}

/// Errors from hash equivalence queries.
#[derive(Debug, thiserror::Error)]
pub(crate) enum HashservError {
    /// A query failed.
    #[error("a hash equivalence query failed")]
    Database(#[from] tokio_postgres::Error),
    /// The database returned a hash that isn't a SHA-256 digest.
    #[error("the database returned an invalid hash")]
    Digest(#[from] InvalidSha256),
}

impl Cache {
    /// Looks up the unihashes of `(method, taskhash)` pairs, in order.
    pub(crate) async fn unihashes(
        &self,
        client: &Client,
        tasks: &[(&str, Sha256)],
    ) -> Result<Vec<Option<Sha256>>, HashservError> {
        let mut found: Vec<Option<Sha256>> = vec![None; tasks.len()];
        let mut missing = Vec::with_capacity(tasks.len());
        let mut methods = Vec::with_capacity(tasks.len());
        let mut taskhashes = Vec::with_capacity(tasks.len());
        for cache in self.lookup_chain() {
            missing.clear();
            methods.clear();
            taskhashes.clear();
            for (index, (slot, (method, taskhash))) in found.iter().zip(tasks).enumerate() {
                if slot.is_none() {
                    missing.push(index);
                    methods.push(*method);
                    taskhashes.push(taskhash.as_bytes().as_slice());
                }
            }
            if missing.is_empty() {
                break;
            }
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
                    *slot = Some(row.unihash.as_slice().try_into()?);
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
        unihashes: &[Sha256],
    ) -> Result<Vec<bool>, tokio_postgres::Error> {
        let caches: Vec<&str> = self
            .lookup_chain()
            .map(|cache| cache.name.as_ref())
            .collect();
        let unihashes_bytes: Vec<&[u8]> = unihashes
            .iter()
            .map(|unihash| unihash.as_bytes().as_slice())
            .collect();
        let positions = existing_unihashes()
            .bind(client, &unihashes_bytes.as_slice(), &caches.as_slice())
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
        taskhash: Sha256,
    ) -> Result<Option<Sha256>, HashservError> {
        Ok(self
            .unihashes(client, &[(method, taskhash)])
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
        taskhash: Sha256,
    ) -> Result<Option<StoredReport>, HashservError> {
        for cache in self.lookup_chain() {
            let row = find_task()
                .bind(
                    client,
                    &cache.name.as_ref(),
                    &method,
                    &taskhash.as_bytes().as_slice(),
                )
                .opt()
                .await?;
            if let Some(row) = row {
                return Ok(Some(row.try_into()?));
            }
        }
        Ok(None)
    }

    /// Looks up the oldest report of an output hash with its unihash.
    pub(crate) async fn outhash(
        &self,
        client: &Client,
        method: &str,
        outhash: Sha256,
    ) -> Result<Option<StoredReport>, HashservError> {
        for cache in self.lookup_chain() {
            let row = find_outhash()
                .bind(
                    client,
                    &cache.name.as_ref(),
                    &method,
                    &outhash.as_bytes().as_slice(),
                )
                .opt()
                .await?;
            if let Some(row) = row {
                return Ok(Some(row.try_into()?));
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
        report: &Report<'_>,
    ) -> Result<Sha256, HashservError> {
        let method = report.method;
        let taskhash = report.taskhash.as_bytes().as_slice();
        let outhash = report.outhash.as_bytes().as_slice();
        let mut equivalent = None;
        for cache in self.lookup_chain() {
            equivalent = find_equivalent()
                .bind(client, &cache.name.as_ref(), &method, &outhash, &taskhash)
                .opt()
                .await?;
            if equivalent.is_some() {
                break;
            }
        }
        let unihash = equivalent
            .as_deref()
            .unwrap_or(report.unihash.as_bytes().as_slice());
        insert_unihash()
            .bind(client, &self.name.as_ref(), &method, &taskhash, &unihash)
            .await?;
        insert_outhash()
            .bind(
                client,
                &self.name.as_ref(),
                &method,
                &taskhash,
                &outhash,
                &report.owner,
                &report.pn,
                &report.pv,
                &report.pr,
                &report.task,
                &report.outhash_siginfo,
            )
            .await?;
        self.current_unihash(client, method, report.taskhash, report.unihash)
            .await
    }

    /// Answers a report without recording it, for clients that may only
    /// read.
    pub(crate) async fn report_read_only(
        &self,
        client: &Client,
        report: &Report<'_>,
    ) -> Result<Sha256, HashservError> {
        let row = self.outhash(client, report.method, report.outhash).await?;
        Ok(row.map_or(report.unihash, |row| row.unihash))
    }

    /// Records that a task is equivalent to a unihash, unless the task has
    /// one already, and returns the task's unihash.
    pub(crate) async fn report_equivalent(
        &self,
        client: &Client,
        method: &str,
        taskhash: Sha256,
        unihash: Sha256,
    ) -> Result<Sha256, HashservError> {
        insert_unihash()
            .bind(
                client,
                &self.name.as_ref(),
                &method,
                &taskhash.as_bytes().as_slice(),
                &unihash.as_bytes().as_slice(),
            )
            .await?;
        self.current_unihash(client, method, taskhash, unihash)
            .await
    }

    /// The unihash that a task has in this cache, or `default`.
    async fn current_unihash(
        &self,
        client: &Client,
        method: &str,
        taskhash: Sha256,
        default: Sha256,
    ) -> Result<Sha256, HashservError> {
        let current = find_unihash()
            .bind(
                client,
                &self.name.as_ref(),
                &method,
                &taskhash.as_bytes().as_slice(),
            )
            .opt()
            .await?;
        Ok(match current {
            Some(current) => current.as_slice().try_into()?,
            None => default,
        })
    }

    /// Deletes unihashes that weren't used since `cutoff`, with the reports
    /// of their tasks. Returns how many unihashes were deleted.
    pub(super) async fn expire_unihashes(
        &self,
        client: &Client,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, tokio_postgres::Error> {
        expire_unihashes()
            .bind(client, &self.name.as_ref(), &cutoff.fixed_offset())
            .await
    }
}
