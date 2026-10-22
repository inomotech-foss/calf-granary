// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RecordPruneExpiryParams<T1: crate::StringSql> {
    pub task_id: i64,
    pub cache: T1,
    pub expired: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct RecordPruneCollectionParams {
    pub task_id: i64,
    pub deleted_blobs: i64,
    pub deleted_chunks: i64,
    pub freed_bytes: i64,
    pub swept_objects: i64,
    pub retired_objects: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct RecordTierRunParams {
    pub task_id: i64,
    pub moved_chunks: i64,
    pub moved_bytes: i64,
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct RecordPruneExpiryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_prune_expiry() -> RecordPruneExpiryStmt {
    RecordPruneExpiryStmt(
        "INSERT INTO prune_expiry (task_id, cache, expired) VALUES ($1::bigint, $2::text, \
         $3::bigint) ON CONFLICT (task_id, cache) DO UPDATE SET expired = excluded.expired, \
         finished_at = now()",
        None,
    )
}
impl RecordPruneExpiryStmt {
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
        expired: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[task_id, cache, expired]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordPruneExpiryParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordPruneExpiryStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordPruneExpiryParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.task_id, &params.cache, &params.expired))
    }
}
pub struct RecordPruneCollectionStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_prune_collection() -> RecordPruneCollectionStmt {
    RecordPruneCollectionStmt(
        "INSERT INTO prune_collection ( task_id, deleted_blobs, deleted_chunks, freed_bytes, \
         swept_objects, retired_objects ) VALUES ( $1::bigint, $2::bigint, $3::bigint, \
         $4::bigint, $5::bigint, $6::bigint ) ON CONFLICT (task_id) DO UPDATE SET deleted_blobs = \
         excluded.deleted_blobs, deleted_chunks = excluded.deleted_chunks, freed_bytes = \
         excluded.freed_bytes, swept_objects = excluded.swept_objects, retired_objects = \
         excluded.retired_objects, finished_at = now()",
        None,
    )
}
impl RecordPruneCollectionStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        task_id: &'a i64,
        deleted_blobs: &'a i64,
        deleted_chunks: &'a i64,
        freed_bytes: &'a i64,
        swept_objects: &'a i64,
        retired_objects: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    task_id,
                    deleted_blobs,
                    deleted_chunks,
                    freed_bytes,
                    swept_objects,
                    retired_objects,
                ],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordPruneCollectionParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordPruneCollectionStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordPruneCollectionParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.task_id,
            &params.deleted_blobs,
            &params.deleted_chunks,
            &params.freed_bytes,
            &params.swept_objects,
            &params.retired_objects,
        ))
    }
}
pub struct RecordTierRunStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_tier_run() -> RecordTierRunStmt {
    RecordTierRunStmt(
        "INSERT INTO tier_run (task_id, moved_chunks, moved_bytes) VALUES ($1::bigint, \
         $2::bigint, $3::bigint) ON CONFLICT (task_id) DO UPDATE SET moved_chunks = \
         excluded.moved_chunks, moved_bytes = excluded.moved_bytes, finished_at = now()",
        None,
    )
}
impl RecordTierRunStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        task_id: &'a i64,
        moved_chunks: &'a i64,
        moved_bytes: &'a i64,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[task_id, moved_chunks, moved_bytes])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordTierRunParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordTierRunStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordTierRunParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.task_id,
            &params.moved_chunks,
            &params.moved_bytes,
        ))
    }
}
