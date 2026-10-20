// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RecordAccessesParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::ArraySql<Item = T2>,
> {
    pub bucket: T1,
    pub accessed_at: chrono::DateTime<chrono::FixedOffset>,
    pub keys: T3,
}
#[derive(Debug)]
pub struct LastAccessesParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::ArraySql<Item = T2>,
> {
    pub bucket: T1,
    pub keys: T3,
}
#[derive(Debug)]
pub struct ForgetAccessesBeforeParams<T1: crate::StringSql> {
    pub bucket: T1,
    pub cutoff: chrono::DateTime<chrono::FixedOffset>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct LastAccesses {
    pub key: String,
    pub last_access: chrono::DateTime<chrono::FixedOffset>,
}
pub struct LastAccessesBorrowed<'a> {
    pub key: &'a str,
    pub last_access: chrono::DateTime<chrono::FixedOffset>,
}
impl<'a> From<LastAccessesBorrowed<'a>> for LastAccesses {
    fn from(LastAccessesBorrowed { key, last_access }: LastAccessesBorrowed<'a>) -> Self {
        Self {
            key: key.into(),
            last_access,
        }
    }
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct LastAccessesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<LastAccessesBorrowed, tokio_postgres::Error>,
    mapper: fn(LastAccessesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LastAccessesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(LastAccessesBorrowed) -> R,
    ) -> LastAccessesQuery<'c, 'a, 's, C, R, N> {
        LastAccessesQuery {
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
pub struct RecordAccessesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_accesses() -> RecordAccessesStmt {
    RecordAccessesStmt(
        "INSERT INTO access (bucket, key, last_access) SELECT $1::text, key, $2::timestamptz FROM \
         unnest($3::text[]) AS key ON CONFLICT (bucket, key) DO UPDATE SET last_access = \
         greatest(access.last_access, excluded.last_access)",
        None,
    )
}
impl RecordAccessesStmt {
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
        T3: crate::ArraySql<Item = T2>,
    >(
        &'s self,
        client: &'c C,
        bucket: &'a T1,
        accessed_at: &'a chrono::DateTime<chrono::FixedOffset>,
        keys: &'a T3,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[bucket, accessed_at, keys]).await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::ArraySql<Item = T2>,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordAccessesParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordAccessesStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordAccessesParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.bucket, &params.accessed_at, &params.keys))
    }
}
pub struct LastAccessesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn last_accesses() -> LastAccessesStmt {
    LastAccessesStmt(
        "SELECT key, last_access FROM access WHERE bucket = $1::text AND key = any($2::text[])",
        None,
    )
}
impl LastAccessesStmt {
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
        T2: crate::StringSql,
        T3: crate::ArraySql<Item = T2>,
    >(
        &'s self,
        client: &'c C,
        bucket: &'a T1,
        keys: &'a T3,
    ) -> LastAccessesQuery<'c, 'a, 's, C, LastAccesses, 2> {
        LastAccessesQuery {
            client,
            params: [bucket, keys],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<LastAccessesBorrowed, tokio_postgres::Error> {
                    Ok(LastAccessesBorrowed {
                        key: row.try_get(0)?,
                        last_access: row.try_get(1)?,
                    })
                },
            mapper: |it| LastAccesses::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::ArraySql<Item = T2>,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        LastAccessesParams<T1, T2, T3>,
        LastAccessesQuery<'c, 'a, 's, C, LastAccesses, 2>,
        C,
    > for LastAccessesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a LastAccessesParams<T1, T2, T3>,
    ) -> LastAccessesQuery<'c, 'a, 's, C, LastAccesses, 2> {
        self.bind(client, &params.bucket, &params.keys)
    }
}
pub struct ForgetAccessesBeforeStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn forget_accesses_before() -> ForgetAccessesBeforeStmt {
    ForgetAccessesBeforeStmt(
        "DELETE FROM access WHERE bucket = $1::text AND last_access < $2::timestamptz",
        None,
    )
}
impl ForgetAccessesBeforeStmt {
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
        bucket: &'a T1,
        cutoff: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[bucket, cutoff]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        ForgetAccessesBeforeParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for ForgetAccessesBeforeStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a ForgetAccessesBeforeParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.bucket, &params.cutoff))
    }
}
