// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct InsertChunkParams<
    T1: crate::BytesSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub hash: T1,
    pub size: i64,
    pub compression: T2,
    pub stored_size: i64,
    pub store: T3,
    pub object_key: T4,
}
#[derive(Debug)]
pub struct InsertBlobParams<T1: crate::BytesSql> {
    pub hash: T1,
    pub size: i64,
}
#[derive(Debug)]
pub struct InsertBlobChunksParams<T1: crate::ArraySql<Item = i64>> {
    pub blob_id: i64,
    pub chunk_ids: T1,
}
#[derive(Debug)]
pub struct InsertEntryParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub cache: T1,
    pub key: T2,
    pub blob_id: i64,
}
#[derive(Debug)]
pub struct EntryExistsParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub cache: T1,
    pub key: T2,
}
#[derive(Debug)]
pub struct FindEntryParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub cache: T1,
    pub key: T2,
}
#[derive(Debug)]
pub struct RecordEntryAccessesParams<
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
> {
    pub keys: T2,
    pub cache: T3,
}
#[derive(Debug, Clone, PartialEq)]
pub struct InsertChunk {
    pub id: i64,
    pub object_key: String,
}
pub struct InsertChunkBorrowed<'a> {
    pub id: i64,
    pub object_key: &'a str,
}
impl<'a> From<InsertChunkBorrowed<'a>> for InsertChunk {
    fn from(InsertChunkBorrowed { id, object_key }: InsertChunkBorrowed<'a>) -> Self {
        Self {
            id,
            object_key: object_key.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct InsertBlob {
    pub id: i64,
    pub inserted: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FindEntry {
    pub blob_id: i64,
    pub hash: Vec<u8>,
    pub size: i64,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}
pub struct FindEntryBorrowed<'a> {
    pub blob_id: i64,
    pub hash: &'a [u8],
    pub size: i64,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}
impl<'a> From<FindEntryBorrowed<'a>> for FindEntry {
    fn from(
        FindEntryBorrowed {
            blob_id,
            hash,
            size,
            created_at,
        }: FindEntryBorrowed<'a>,
    ) -> Self {
        Self {
            blob_id,
            hash: hash.into(),
            size,
            created_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct BlobChunks {
    pub store: String,
    pub object_key: String,
    pub compression: String,
    pub size: i64,
}
pub struct BlobChunksBorrowed<'a> {
    pub store: &'a str,
    pub object_key: &'a str,
    pub compression: &'a str,
    pub size: i64,
}
impl<'a> From<BlobChunksBorrowed<'a>> for BlobChunks {
    fn from(
        BlobChunksBorrowed {
            store,
            object_key,
            compression,
            size,
        }: BlobChunksBorrowed<'a>,
    ) -> Self {
        Self {
            store: store.into(),
            object_key: object_key.into(),
            compression: compression.into(),
            size,
        }
    }
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct I64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<i64, tokio_postgres::Error>,
    mapper: fn(i64) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> I64Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(i64) -> R) -> I64Query<'c, 'a, 's, C, R, N> {
        I64Query {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct InsertChunkQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<InsertChunkBorrowed, tokio_postgres::Error>,
    mapper: fn(InsertChunkBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> InsertChunkQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(InsertChunkBorrowed) -> R,
    ) -> InsertChunkQuery<'c, 'a, 's, C, R, N> {
        InsertChunkQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct InsertBlobQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<InsertBlob, tokio_postgres::Error>,
    mapper: fn(InsertBlob) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> InsertBlobQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(InsertBlob) -> R) -> InsertBlobQuery<'c, 'a, 's, C, R, N> {
        InsertBlobQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct ChronoDateTimechronoFixedOffsetQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(
        &tokio_postgres::Row,
    ) -> Result<chrono::DateTime<chrono::FixedOffset>, tokio_postgres::Error>,
    mapper: fn(chrono::DateTime<chrono::FixedOffset>) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ChronoDateTimechronoFixedOffsetQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(chrono::DateTime<chrono::FixedOffset>) -> R,
    ) -> ChronoDateTimechronoFixedOffsetQuery<'c, 'a, 's, C, R, N> {
        ChronoDateTimechronoFixedOffsetQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<bool, tokio_postgres::Error>,
    mapper: fn(bool) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BoolQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(bool) -> R) -> BoolQuery<'c, 'a, 's, C, R, N> {
        BoolQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct FindEntryQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<FindEntryBorrowed, tokio_postgres::Error>,
    mapper: fn(FindEntryBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FindEntryQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FindEntryBorrowed) -> R) -> FindEntryQuery<'c, 'a, 's, C, R, N> {
        FindEntryQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct BlobChunksQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BlobChunksBorrowed, tokio_postgres::Error>,
    mapper: fn(BlobChunksBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BlobChunksQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BlobChunksBorrowed) -> R,
    ) -> BlobChunksQuery<'c, 'a, 's, C, R, N> {
        BlobChunksQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct TouchChunkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn touch_chunk() -> TouchChunkStmt {
    TouchChunkStmt(
        "UPDATE chunk SET touched_at = now() WHERE hash = $1::bytea AND state = 'valid' RETURNING \
         id",
        None,
    )
}
impl TouchChunkStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c C,
        hash: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct InsertChunkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_chunk() -> InsertChunkStmt {
    InsertChunkStmt(
        "INSERT INTO chunk (hash, size, compression, stored_size, store, object_key) VALUES ( \
         $1::bytea, $2::bigint, $3::text, $4::bigint, $5::text, $6::text ) ON CONFLICT (hash) \
         WHERE state = 'valid' DO UPDATE SET touched_at = now() RETURNING id, object_key",
        None,
    )
}
impl InsertChunkStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::BytesSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        hash: &'a T1,
        size: &'a i64,
        compression: &'a T2,
        stored_size: &'a i64,
        store: &'a T3,
        object_key: &'a T4,
    ) -> InsertChunkQuery<'c, 'a, 's, C, InsertChunk, 6> {
        InsertChunkQuery {
            client,
            params: [hash, size, compression, stored_size, store, object_key],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<InsertChunkBorrowed, tokio_postgres::Error> {
                    Ok(InsertChunkBorrowed {
                        id: row.try_get(0)?,
                        object_key: row.try_get(1)?,
                    })
                },
            mapper: |it| InsertChunk::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::BytesSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        InsertChunkParams<T1, T2, T3, T4>,
        InsertChunkQuery<'c, 'a, 's, C, InsertChunk, 6>,
        C,
    > for InsertChunkStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a InsertChunkParams<T1, T2, T3, T4>,
    ) -> InsertChunkQuery<'c, 'a, 's, C, InsertChunk, 6> {
        self.bind(
            client,
            &params.hash,
            &params.size,
            &params.compression,
            &params.stored_size,
            &params.store,
            &params.object_key,
        )
    }
}
pub struct InsertBlobStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_blob() -> InsertBlobStmt {
    InsertBlobStmt(
        "INSERT INTO blob (hash, size) VALUES ($1::bytea, $2::bigint) ON CONFLICT (hash) DO \
         UPDATE SET touched_at = now() RETURNING id, (xmax::text = '0') AS inserted",
        None,
    )
}
impl InsertBlobStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c C,
        hash: &'a T1,
        size: &'a i64,
    ) -> InsertBlobQuery<'c, 'a, 's, C, InsertBlob, 2> {
        InsertBlobQuery {
            client,
            params: [hash, size],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<InsertBlob, tokio_postgres::Error> {
                Ok(InsertBlob {
                    id: row.try_get(0)?,
                    inserted: row.try_get(1)?,
                })
            },
            mapper: |it| InsertBlob::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        InsertBlobParams<T1>,
        InsertBlobQuery<'c, 'a, 's, C, InsertBlob, 2>,
        C,
    > for InsertBlobStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a InsertBlobParams<T1>,
    ) -> InsertBlobQuery<'c, 'a, 's, C, InsertBlob, 2> {
        self.bind(client, &params.hash, &params.size)
    }
}
pub struct InsertBlobChunksStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_blob_chunks() -> InsertBlobChunksStmt {
    InsertBlobChunksStmt(
        "INSERT INTO blob_chunk (blob_id, seq, chunk_id) SELECT $1::bigint, ordinality - 1, \
         chunk_id FROM unnest($2::bigint[]) WITH ORDINALITY AS chunks (chunk_id, ordinality)",
        None,
    )
}
impl InsertBlobChunksStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c C,
        blob_id: &'a i64,
        chunk_ids: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[blob_id, chunk_ids]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::ArraySql<Item = i64>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        InsertBlobChunksParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for InsertBlobChunksStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a InsertBlobChunksParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.blob_id, &params.chunk_ids))
    }
}
pub struct InsertEntryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_entry() -> InsertEntryStmt {
    InsertEntryStmt(
        "INSERT INTO entry (cache, key, blob_id) VALUES ($1::text, $2::text, $3::bigint) ON \
         CONFLICT (cache, key) DO NOTHING RETURNING created_at",
        None,
    )
}
impl InsertEntryStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        key: &'a T2,
        blob_id: &'a i64,
    ) -> ChronoDateTimechronoFixedOffsetQuery<'c, 'a, 's, C, chrono::DateTime<chrono::FixedOffset>, 3>
    {
        ChronoDateTimechronoFixedOffsetQuery {
            client,
            params: [cache, key, blob_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        InsertEntryParams<T1, T2>,
        ChronoDateTimechronoFixedOffsetQuery<
            'c,
            'a,
            's,
            C,
            chrono::DateTime<chrono::FixedOffset>,
            3,
        >,
        C,
    > for InsertEntryStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a InsertEntryParams<T1, T2>,
    ) -> ChronoDateTimechronoFixedOffsetQuery<'c, 'a, 's, C, chrono::DateTime<chrono::FixedOffset>, 3>
    {
        self.bind(client, &params.cache, &params.key, &params.blob_id)
    }
}
pub struct EntryExistsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn entry_exists() -> EntryExistsStmt {
    EntryExistsStmt(
        "SELECT EXISTS ( SELECT FROM entry WHERE cache = $1::text AND key = $2::text ) AS found",
        None,
    )
}
impl EntryExistsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        key: &'a T2,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [cache, key],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        EntryExistsParams<T1, T2>,
        BoolQuery<'c, 'a, 's, C, bool, 2>,
        C,
    > for EntryExistsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a EntryExistsParams<T1, T2>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.cache, &params.key)
    }
}
pub struct FindEntryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_entry() -> FindEntryStmt {
    FindEntryStmt(
        "SELECT entry.blob_id, blob.hash, blob.size, entry.created_at FROM entry JOIN blob ON \
         blob.id = entry.blob_id WHERE entry.cache = $1::text AND entry.key = $2::text",
        None,
    )
}
impl FindEntryStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        key: &'a T2,
    ) -> FindEntryQuery<'c, 'a, 's, C, FindEntry, 2> {
        FindEntryQuery {
            client,
            params: [cache, key],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<FindEntryBorrowed, tokio_postgres::Error> {
                    Ok(FindEntryBorrowed {
                        blob_id: row.try_get(0)?,
                        hash: row.try_get(1)?,
                        size: row.try_get(2)?,
                        created_at: row.try_get(3)?,
                    })
                },
            mapper: |it| FindEntry::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindEntryParams<T1, T2>,
        FindEntryQuery<'c, 'a, 's, C, FindEntry, 2>,
        C,
    > for FindEntryStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindEntryParams<T1, T2>,
    ) -> FindEntryQuery<'c, 'a, 's, C, FindEntry, 2> {
        self.bind(client, &params.cache, &params.key)
    }
}
pub struct BlobChunksStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn blob_chunks() -> BlobChunksStmt {
    BlobChunksStmt(
        "SELECT chunk.store, chunk.object_key, chunk.compression, chunk.size FROM blob_chunk JOIN \
         chunk ON chunk.id = blob_chunk.chunk_id WHERE blob_chunk.blob_id = $1::bigint ORDER BY \
         blob_chunk.seq",
        None,
    )
}
impl BlobChunksStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        blob_id: &'a i64,
    ) -> BlobChunksQuery<'c, 'a, 's, C, BlobChunks, 1> {
        BlobChunksQuery {
            client,
            params: [blob_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BlobChunksBorrowed, tokio_postgres::Error> {
                    Ok(BlobChunksBorrowed {
                        store: row.try_get(0)?,
                        object_key: row.try_get(1)?,
                        compression: row.try_get(2)?,
                        size: row.try_get(3)?,
                    })
                },
            mapper: |it| BlobChunks::from(it),
        }
    }
}
pub struct RecordEntryAccessesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_entry_accesses() -> RecordEntryAccessesStmt {
    RecordEntryAccessesStmt(
        "UPDATE entry SET last_access = greatest(last_access, now()) FROM unnest($1::text[]) AS \
         accessed (key) WHERE entry.cache = $2::text AND entry.key = accessed.key",
        None,
    )
}
impl RecordEntryAccessesStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::ArraySql<Item = T1>,
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        keys: &'a T2,
        cache: &'a T3,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[keys, cache]).await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordEntryAccessesParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordEntryAccessesStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordEntryAccessesParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.keys, &params.cache))
    }
}
