// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct InsertNixPathParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::ArraySql<Item = T4>,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
> {
    pub cache: T1,
    pub key: T2,
    pub store_path: T3,
    pub refs: T5,
    pub deriver: Option<T6>,
    pub system: Option<T7>,
    pub ca: Option<T8>,
}
#[derive(Debug)]
pub struct FindNixPathParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub cache: T1,
    pub key: T2,
}
#[derive(Debug)]
pub struct MissingNixPathsParams<
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
> {
    pub hashes: T2,
    pub caches: T4,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FindNixPath {
    pub store_path: String,
    pub refs: Vec<String>,
    pub deriver: Option<String>,
    pub system: Option<String>,
    pub ca: Option<String>,
    pub blob_id: i64,
    pub hash: Vec<u8>,
    pub size: i64,
    pub file_size: Option<i64>,
}
pub struct FindNixPathBorrowed<'a> {
    pub store_path: &'a str,
    pub refs: crate::ArrayIterator<'a, &'a str>,
    pub deriver: Option<&'a str>,
    pub system: Option<&'a str>,
    pub ca: Option<&'a str>,
    pub blob_id: i64,
    pub hash: &'a [u8],
    pub size: i64,
    pub file_size: Option<i64>,
}
impl<'a> From<FindNixPathBorrowed<'a>> for FindNixPath {
    fn from(
        FindNixPathBorrowed {
            store_path,
            refs,
            deriver,
            system,
            ca,
            blob_id,
            hash,
            size,
            file_size,
        }: FindNixPathBorrowed<'a>,
    ) -> Self {
        Self {
            store_path: store_path.into(),
            refs: refs.map(|v| v.into()).collect(),
            deriver: deriver.map(|v| v.into()),
            system: system.map(|v| v.into()),
            ca: ca.map(|v| v.into()),
            blob_id,
            hash: hash.into(),
            size,
            file_size,
        }
    }
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct FindNixPathQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<FindNixPathBorrowed, tokio_postgres::Error>,
    mapper: fn(FindNixPathBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FindNixPathQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(FindNixPathBorrowed) -> R,
    ) -> FindNixPathQuery<'c, 'a, 's, C, R, N> {
        FindNixPathQuery {
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&str, tokio_postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
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
pub struct InsertNixPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_nix_path() -> InsertNixPathStmt {
    InsertNixPathStmt(
        "INSERT INTO nix_path (cache, key, store_path, refs, deriver, system, ca) VALUES ( \
         $1::text, $2::text, $3::text, $4::text[], $5::text, $6::text, $7::text )",
        None,
    )
}
impl InsertNixPathStmt {
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
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
        T5: crate::ArraySql<Item = T4>,
        T6: crate::StringSql,
        T7: crate::StringSql,
        T8: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        key: &'a T2,
        store_path: &'a T3,
        refs: &'a T5,
        deriver: &'a Option<T6>,
        system: &'a Option<T7>,
        ca: &'a Option<T8>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[cache, key, store_path, refs, deriver, system, ca])
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::ArraySql<Item = T4>,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        InsertNixPathParams<T1, T2, T3, T4, T5, T6, T7, T8>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for InsertNixPathStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a InsertNixPathParams<T1, T2, T3, T4, T5, T6, T7, T8>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.cache,
            &params.key,
            &params.store_path,
            &params.refs,
            &params.deriver,
            &params.system,
            &params.ca,
        ))
    }
}
pub struct FindNixPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_nix_path() -> FindNixPathStmt {
    FindNixPathStmt(
        "SELECT nix_path.store_path, nix_path.refs, nix_path.deriver, nix_path.system, \
         nix_path.ca, entry.blob_id, blob.hash, blob.size, ( SELECT CASE WHEN \
         bool_and(chunk.compression = 'zstd') THEN sum(chunk.stored_size)::bigint END FROM \
         blob_chunk JOIN chunk ON chunk.id = blob_chunk.chunk_id WHERE blob_chunk.blob_id = \
         blob.id ) AS file_size FROM nix_path JOIN entry USING (cache, key) JOIN blob ON blob.id \
         = entry.blob_id WHERE nix_path.cache = $1::text AND nix_path.key = $2::text",
        None,
    )
}
impl FindNixPathStmt {
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
    ) -> FindNixPathQuery<'c, 'a, 's, C, FindNixPath, 2> {
        FindNixPathQuery {
            client,
            params: [cache, key],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<FindNixPathBorrowed, tokio_postgres::Error> {
                    Ok(FindNixPathBorrowed {
                        store_path: row.try_get(0)?,
                        refs: row.try_get(1)?,
                        deriver: row.try_get(2)?,
                        system: row.try_get(3)?,
                        ca: row.try_get(4)?,
                        blob_id: row.try_get(5)?,
                        hash: row.try_get(6)?,
                        size: row.try_get(7)?,
                        file_size: row.try_get(8)?,
                    })
                },
            mapper: |it| FindNixPath::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindNixPathParams<T1, T2>,
        FindNixPathQuery<'c, 'a, 's, C, FindNixPath, 2>,
        C,
    > for FindNixPathStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindNixPathParams<T1, T2>,
    ) -> FindNixPathQuery<'c, 'a, 's, C, FindNixPath, 2> {
        self.bind(client, &params.cache, &params.key)
    }
}
pub struct MissingNixPathsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn missing_nix_paths() -> MissingNixPathsStmt {
    MissingNixPathsStmt(
        "SELECT hash FROM unnest($1::text[]) AS requested (hash) WHERE NOT EXISTS ( SELECT FROM \
         nix_path WHERE nix_path.key = requested.hash AND nix_path.cache = any($2::text[]) )",
        None,
    )
}
impl MissingNixPathsStmt {
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
        T1: crate::StringSql,
        T2: crate::ArraySql<Item = T1>,
        T3: crate::StringSql,
        T4: crate::ArraySql<Item = T3>,
    >(
        &'s self,
        client: &'c C,
        hashes: &'a T2,
        caches: &'a T4,
    ) -> StringQuery<'c, 'a, 's, C, String, 2> {
        StringQuery {
            client,
            params: [hashes, caches],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        MissingNixPathsParams<T1, T2, T3, T4>,
        StringQuery<'c, 'a, 's, C, String, 2>,
        C,
    > for MissingNixPathsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a MissingNixPathsParams<T1, T2, T3, T4>,
    ) -> StringQuery<'c, 'a, 's, C, String, 2> {
        self.bind(client, &params.hashes, &params.caches)
    }
}
