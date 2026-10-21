// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RecordPruneRunParams<T1: crate::StringSql> {
    pub task_id: i64,
    pub cache: T1,
    pub scanned: i64,
    pub deleted: i64,
    pub freed_bytes: i64,
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct RecordPruneRunStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_prune_run() -> RecordPruneRunStmt {
    RecordPruneRunStmt(
        "INSERT INTO prune_run (task_id, cache, scanned, deleted, freed_bytes) VALUES \
         ($1::bigint, $2::text, $3::bigint, $4::bigint, $5::bigint) ON CONFLICT (task_id, cache) \
         DO UPDATE SET scanned = excluded.scanned, deleted = excluded.deleted, freed_bytes = \
         excluded.freed_bytes, finished_at = now()",
        None,
    )
}
impl RecordPruneRunStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        task_id: &'a i64,
        cache: &'a T1,
        scanned: &'a i64,
        deleted: &'a i64,
        freed_bytes: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[task_id, cache, scanned, deleted, freed_bytes])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordPruneRunParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordPruneRunStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordPruneRunParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.task_id,
            &params.cache,
            &params.scanned,
            &params.deleted,
            &params.freed_bytes,
        ))
    }
}
