// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct FindUnihashesParams<
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::BytesSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
> {
    pub methods: T2,
    pub taskhashes: T4,
    pub cache: T5,
}
#[derive(Debug)]
pub struct TouchUnihashesParams<
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::BytesSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
> {
    pub methods: T2,
    pub taskhashes: T4,
    pub cache: T5,
}
#[derive(Debug)]
pub struct ExistingUnihashesParams<
    T1: crate::BytesSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
> {
    pub unihashes: T2,
    pub caches: T4,
}
#[derive(Debug)]
pub struct FindTaskParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql> {
    pub cache: T1,
    pub method: T2,
    pub taskhash: T3,
}
#[derive(Debug)]
pub struct FindOuthashParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql> {
    pub cache: T1,
    pub method: T2,
    pub outhash: T3,
}
#[derive(Debug)]
pub struct InsertOuthashParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
    T9: crate::StringSql,
    T10: crate::StringSql,
> {
    pub cache: T1,
    pub method: T2,
    pub taskhash: T3,
    pub outhash: T4,
    pub owner: Option<T5>,
    pub pn: Option<T6>,
    pub pv: Option<T7>,
    pub pr: Option<T8>,
    pub task: Option<T9>,
    pub outhash_siginfo: Option<T10>,
}
#[derive(Debug)]
pub struct FindEquivalentParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
> {
    pub cache: T1,
    pub method: T2,
    pub outhash: T3,
    pub taskhash: T4,
}
#[derive(Debug)]
pub struct InsertUnihashParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
> {
    pub cache: T1,
    pub method: T2,
    pub taskhash: T3,
    pub unihash: T4,
}
#[derive(Debug)]
pub struct FindUnihashParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql> {
    pub cache: T1,
    pub method: T2,
    pub taskhash: T3,
}
#[derive(Debug)]
pub struct ExpireUnihashesParams<T1: crate::StringSql> {
    pub cache: T1,
    pub cutoff: chrono::DateTime<chrono::FixedOffset>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FindUnihashes {
    pub position: i64,
    pub unihash: Vec<u8>,
}
pub struct FindUnihashesBorrowed<'a> {
    pub position: i64,
    pub unihash: &'a [u8],
}
impl<'a> From<FindUnihashesBorrowed<'a>> for FindUnihashes {
    fn from(FindUnihashesBorrowed { position, unihash }: FindUnihashesBorrowed<'a>) -> Self {
        Self {
            position,
            unihash: unihash.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct HashservReport {
    pub taskhash: Vec<u8>,
    pub outhash: Vec<u8>,
    pub unihash: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub owner: Option<String>,
    pub pn: Option<String>,
    pub pv: Option<String>,
    pub pr: Option<String>,
    pub task: Option<String>,
    pub outhash_siginfo: Option<String>,
}
pub struct HashservReportBorrowed<'a> {
    pub taskhash: &'a [u8],
    pub outhash: &'a [u8],
    pub unihash: &'a [u8],
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub owner: Option<&'a str>,
    pub pn: Option<&'a str>,
    pub pv: Option<&'a str>,
    pub pr: Option<&'a str>,
    pub task: Option<&'a str>,
    pub outhash_siginfo: Option<&'a str>,
}
impl<'a> From<HashservReportBorrowed<'a>> for HashservReport {
    fn from(
        HashservReportBorrowed {
            taskhash,
            outhash,
            unihash,
            created_at,
            owner,
            pn,
            pv,
            pr,
            task,
            outhash_siginfo,
        }: HashservReportBorrowed<'a>,
    ) -> Self {
        Self {
            taskhash: taskhash.into(),
            outhash: outhash.into(),
            unihash: unihash.into(),
            created_at,
            owner: owner.map(|v| v.into()),
            pn: pn.map(|v| v.into()),
            pv: pv.map(|v| v.into()),
            pr: pr.map(|v| v.into()),
            task: task.map(|v| v.into()),
            outhash_siginfo: outhash_siginfo.map(|v| v.into()),
        }
    }
}
use futures::{self, StreamExt, TryStreamExt};

use crate::client::async_::GenericClient;
pub struct FindUnihashesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<FindUnihashesBorrowed, tokio_postgres::Error>,
    mapper: fn(FindUnihashesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FindUnihashesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(FindUnihashesBorrowed) -> R,
    ) -> FindUnihashesQuery<'c, 'a, 's, C, R, N> {
        FindUnihashesQuery {
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
pub struct HashservReportQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<HashservReportBorrowed, tokio_postgres::Error>,
    mapper: fn(HashservReportBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> HashservReportQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(HashservReportBorrowed) -> R,
    ) -> HashservReportQuery<'c, 'a, 's, C, R, N> {
        HashservReportQuery {
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
pub struct Vecu8Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&[u8], tokio_postgres::Error>,
    mapper: fn(&[u8]) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> Vecu8Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&[u8]) -> R) -> Vecu8Query<'c, 'a, 's, C, R, N> {
        Vecu8Query {
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
pub struct FindUnihashesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_unihashes() -> FindUnihashesStmt {
    FindUnihashesStmt(
        "SELECT requested.position, hashserv_unihash.unihash FROM unnest($1::text[], $2::bytea[]) \
         WITH ORDINALITY AS requested (method, taskhash, position) JOIN hashserv_unihash ON \
         hashserv_unihash.cache = $3::text AND hashserv_unihash.method = requested.method AND \
         hashserv_unihash.taskhash = requested.taskhash",
        None,
    )
}
impl FindUnihashesStmt {
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
        T3: crate::BytesSql,
        T4: crate::ArraySql<Item = T3>,
        T5: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        methods: &'a T2,
        taskhashes: &'a T4,
        cache: &'a T5,
    ) -> FindUnihashesQuery<'c, 'a, 's, C, FindUnihashes, 3> {
        FindUnihashesQuery {
            client,
            params: [methods, taskhashes, cache],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<FindUnihashesBorrowed, tokio_postgres::Error> {
                    Ok(FindUnihashesBorrowed {
                        position: row.try_get(0)?,
                        unihash: row.try_get(1)?,
                    })
                },
            mapper: |it| FindUnihashes::from(it),
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
    T3: crate::BytesSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindUnihashesParams<T1, T2, T3, T4, T5>,
        FindUnihashesQuery<'c, 'a, 's, C, FindUnihashes, 3>,
        C,
    > for FindUnihashesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindUnihashesParams<T1, T2, T3, T4, T5>,
    ) -> FindUnihashesQuery<'c, 'a, 's, C, FindUnihashes, 3> {
        self.bind(client, &params.methods, &params.taskhashes, &params.cache)
    }
}
pub struct TouchUnihashesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn touch_unihashes() -> TouchUnihashesStmt {
    TouchUnihashesStmt(
        "UPDATE hashserv_unihash SET accessed_at = now() FROM unnest($1::text[], $2::bytea[]) AS \
         requested (method, taskhash) WHERE hashserv_unihash.cache = $3::text AND \
         hashserv_unihash.method = requested.method AND hashserv_unihash.taskhash = \
         requested.taskhash AND hashserv_unihash.accessed_at < now() - interval '1 hour'",
        None,
    )
}
impl TouchUnihashesStmt {
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
        T3: crate::BytesSql,
        T4: crate::ArraySql<Item = T3>,
        T5: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        methods: &'a T2,
        taskhashes: &'a T4,
        cache: &'a T5,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[methods, taskhashes, cache]).await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::BytesSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        TouchUnihashesParams<T1, T2, T3, T4, T5>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for TouchUnihashesStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a TouchUnihashesParams<T1, T2, T3, T4, T5>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.methods, &params.taskhashes, &params.cache))
    }
}
pub struct ExistingUnihashesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn existing_unihashes() -> ExistingUnihashesStmt {
    ExistingUnihashesStmt(
        "SELECT requested.position FROM unnest($1::bytea[]) WITH ORDINALITY AS requested \
         (unihash, position) WHERE EXISTS ( SELECT FROM hashserv_unihash WHERE \
         hashserv_unihash.cache = any($2::text[]) AND hashserv_unihash.unihash = \
         requested.unihash )",
        None,
    )
}
impl ExistingUnihashesStmt {
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
        T2: crate::ArraySql<Item = T1>,
        T3: crate::StringSql,
        T4: crate::ArraySql<Item = T3>,
    >(
        &'s self,
        client: &'c C,
        unihashes: &'a T2,
        caches: &'a T4,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        I64Query {
            client,
            params: [unihashes, caches],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::BytesSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ExistingUnihashesParams<T1, T2, T3, T4>,
        I64Query<'c, 'a, 's, C, i64, 2>,
        C,
    > for ExistingUnihashesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ExistingUnihashesParams<T1, T2, T3, T4>,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        self.bind(client, &params.unihashes, &params.caches)
    }
}
pub struct FindTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_task() -> FindTaskStmt {
    FindTaskStmt(
        "SELECT hashserv_outhash.taskhash, hashserv_outhash.outhash, hashserv_unihash.unihash, \
         hashserv_outhash.created_at, hashserv_outhash.owner, hashserv_outhash.pn, \
         hashserv_outhash.pv, hashserv_outhash.pr, hashserv_outhash.task, \
         hashserv_outhash.outhash_siginfo FROM hashserv_outhash JOIN hashserv_unihash USING \
         (cache, method, taskhash) WHERE hashserv_outhash.cache = $1::text AND \
         hashserv_outhash.method = $2::text AND hashserv_outhash.taskhash = $3::bytea ORDER BY \
         hashserv_outhash.created_at LIMIT 1",
        None,
    )
}
impl FindTaskStmt {
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
        T3: crate::BytesSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        taskhash: &'a T3,
    ) -> HashservReportQuery<'c, 'a, 's, C, HashservReport, 3> {
        HashservReportQuery {
            client,
            params: [cache, method, taskhash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<HashservReportBorrowed, tokio_postgres::Error> {
                Ok(HashservReportBorrowed {
                    taskhash: row.try_get(0)?,
                    outhash: row.try_get(1)?,
                    unihash: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                    owner: row.try_get(4)?,
                    pn: row.try_get(5)?,
                    pv: row.try_get(6)?,
                    pr: row.try_get(7)?,
                    task: row.try_get(8)?,
                    outhash_siginfo: row.try_get(9)?,
                })
            },
            mapper: |it| HashservReport::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindTaskParams<T1, T2, T3>,
        HashservReportQuery<'c, 'a, 's, C, HashservReport, 3>,
        C,
    > for FindTaskStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindTaskParams<T1, T2, T3>,
    ) -> HashservReportQuery<'c, 'a, 's, C, HashservReport, 3> {
        self.bind(client, &params.cache, &params.method, &params.taskhash)
    }
}
pub struct FindOuthashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_outhash() -> FindOuthashStmt {
    FindOuthashStmt(
        "SELECT hashserv_outhash.taskhash, hashserv_outhash.outhash, hashserv_unihash.unihash, \
         hashserv_outhash.created_at, hashserv_outhash.owner, hashserv_outhash.pn, \
         hashserv_outhash.pv, hashserv_outhash.pr, hashserv_outhash.task, \
         hashserv_outhash.outhash_siginfo FROM hashserv_outhash JOIN hashserv_unihash USING \
         (cache, method, taskhash) WHERE hashserv_outhash.cache = $1::text AND \
         hashserv_outhash.method = $2::text AND hashserv_outhash.outhash = $3::bytea ORDER BY \
         hashserv_outhash.created_at LIMIT 1",
        None,
    )
}
impl FindOuthashStmt {
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
        T3: crate::BytesSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        outhash: &'a T3,
    ) -> HashservReportQuery<'c, 'a, 's, C, HashservReport, 3> {
        HashservReportQuery {
            client,
            params: [cache, method, outhash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<HashservReportBorrowed, tokio_postgres::Error> {
                Ok(HashservReportBorrowed {
                    taskhash: row.try_get(0)?,
                    outhash: row.try_get(1)?,
                    unihash: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                    owner: row.try_get(4)?,
                    pn: row.try_get(5)?,
                    pv: row.try_get(6)?,
                    pr: row.try_get(7)?,
                    task: row.try_get(8)?,
                    outhash_siginfo: row.try_get(9)?,
                })
            },
            mapper: |it| HashservReport::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindOuthashParams<T1, T2, T3>,
        HashservReportQuery<'c, 'a, 's, C, HashservReport, 3>,
        C,
    > for FindOuthashStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindOuthashParams<T1, T2, T3>,
    ) -> HashservReportQuery<'c, 'a, 's, C, HashservReport, 3> {
        self.bind(client, &params.cache, &params.method, &params.outhash)
    }
}
pub struct InsertOuthashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_outhash() -> InsertOuthashStmt {
    InsertOuthashStmt(
        "INSERT INTO hashserv_outhash (cache, method, taskhash, outhash, owner, pn, pv, pr, task, \
         outhash_siginfo) VALUES ( $1::text, $2::text, $3::bytea, $4::bytea, $5::text, $6::text, \
         $7::text, $8::text, $9::text, $10::text ) ON CONFLICT DO NOTHING",
        None,
    )
}
impl InsertOuthashStmt {
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
        T3: crate::BytesSql,
        T4: crate::BytesSql,
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
        T8: crate::StringSql,
        T9: crate::StringSql,
        T10: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        taskhash: &'a T3,
        outhash: &'a T4,
        owner: &'a Option<T5>,
        pn: &'a Option<T6>,
        pv: &'a Option<T7>,
        pr: &'a Option<T8>,
        task: &'a Option<T9>,
        outhash_siginfo: &'a Option<T10>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    cache,
                    method,
                    taskhash,
                    outhash,
                    owner,
                    pn,
                    pv,
                    pr,
                    task,
                    outhash_siginfo,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        InsertOuthashParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for InsertOuthashStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a InsertOuthashParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.cache,
            &params.method,
            &params.taskhash,
            &params.outhash,
            &params.owner,
            &params.pn,
            &params.pv,
            &params.pr,
            &params.task,
            &params.outhash_siginfo,
        ))
    }
}
pub struct FindEquivalentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_equivalent() -> FindEquivalentStmt {
    FindEquivalentStmt(
        "SELECT hashserv_unihash.unihash FROM hashserv_outhash JOIN hashserv_unihash USING \
         (cache, method, taskhash) WHERE hashserv_outhash.cache = $1::text AND \
         hashserv_outhash.method = $2::text AND hashserv_outhash.outhash = $3::bytea AND \
         hashserv_outhash.taskhash != $4::bytea ORDER BY hashserv_outhash.created_at LIMIT 1",
        None,
    )
}
impl FindEquivalentStmt {
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
        T3: crate::BytesSql,
        T4: crate::BytesSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        outhash: &'a T3,
        taskhash: &'a T4,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 4> {
        Vecu8Query {
            client,
            params: [cache, method, outhash, taskhash],
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
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindEquivalentParams<T1, T2, T3, T4>,
        Vecu8Query<'c, 'a, 's, C, Vec<u8>, 4>,
        C,
    > for FindEquivalentStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindEquivalentParams<T1, T2, T3, T4>,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 4> {
        self.bind(
            client,
            &params.cache,
            &params.method,
            &params.outhash,
            &params.taskhash,
        )
    }
}
pub struct InsertUnihashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert_unihash() -> InsertUnihashStmt {
    InsertUnihashStmt(
        "INSERT INTO hashserv_unihash (cache, method, taskhash, unihash) VALUES ($1::text, \
         $2::text, $3::bytea, $4::bytea) ON CONFLICT DO NOTHING",
        None,
    )
}
impl InsertUnihashStmt {
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
        T3: crate::BytesSql,
        T4: crate::BytesSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        taskhash: &'a T3,
        unihash: &'a T4,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[cache, method, taskhash, unihash])
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::BytesSql,
    T4: crate::BytesSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        InsertUnihashParams<T1, T2, T3, T4>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for InsertUnihashStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a InsertUnihashParams<T1, T2, T3, T4>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.cache,
            &params.method,
            &params.taskhash,
            &params.unihash,
        ))
    }
}
pub struct FindUnihashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_unihash() -> FindUnihashStmt {
    FindUnihashStmt(
        "SELECT unihash FROM hashserv_unihash WHERE cache = $1::text AND method = $2::text AND \
         taskhash = $3::bytea",
        None,
    )
}
impl FindUnihashStmt {
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
        T3: crate::BytesSql,
    >(
        &'s self,
        client: &'c C,
        cache: &'a T1,
        method: &'a T2,
        taskhash: &'a T3,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 3> {
        Vecu8Query {
            client,
            params: [cache, method, taskhash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::BytesSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FindUnihashParams<T1, T2, T3>,
        Vecu8Query<'c, 'a, 's, C, Vec<u8>, 3>,
        C,
    > for FindUnihashStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FindUnihashParams<T1, T2, T3>,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 3> {
        self.bind(client, &params.cache, &params.method, &params.taskhash)
    }
}
pub struct ExpireUnihashesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn expire_unihashes() -> ExpireUnihashesStmt {
    ExpireUnihashesStmt(
        "DELETE FROM hashserv_unihash WHERE cache = $1::text AND accessed_at < $2::timestamptz",
        None,
    )
}
impl ExpireUnihashesStmt {
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
        cache: &'a T1,
        cutoff: &'a chrono::DateTime<chrono::FixedOffset>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[cache, cutoff]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        ExpireUnihashesParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for ExpireUnihashesStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a ExpireUnihashesParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.cache, &params.cutoff))
    }
}
