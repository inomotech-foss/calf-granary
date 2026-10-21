// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct StartTaskParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub kind: T1,
    pub requested_by: T2,
    pub worker: T3,
}
#[derive(Debug)]
pub struct ClaimTaskParams<T1: crate::StringSql> {
    pub worker: T1,
    pub lease_seconds: f64,
}
#[derive(Debug)]
pub struct RenewLeaseParams<T1: crate::StringSql> {
    pub lease_seconds: f64,
    pub id: i64,
    pub worker: T1,
}
#[derive(Debug)]
pub struct CompleteTaskParams<T1: crate::StringSql> {
    pub id: i64,
    pub worker: T1,
}
#[derive(Debug)]
pub struct FailTaskParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub max_attempts: i32,
    pub retry_seconds: f64,
    pub failure: T1,
    pub id: i64,
    pub worker: T2,
}
#[derive(Debug)]
pub struct ReleaseTaskParams<T1: crate::StringSql> {
    pub id: i64,
    pub worker: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EnqueueDueSchedules {
    pub id: i64,
    pub kind: String,
}
pub struct EnqueueDueSchedulesBorrowed<'a> {
    pub id: i64,
    pub kind: &'a str,
}
impl<'a> From<EnqueueDueSchedulesBorrowed<'a>> for EnqueueDueSchedules {
    fn from(EnqueueDueSchedulesBorrowed { id, kind }: EnqueueDueSchedulesBorrowed<'a>) -> Self {
        Self {
            id,
            kind: kind.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimTask {
    pub id: i64,
    pub kind: String,
    pub attempts: i32,
}
pub struct ClaimTaskBorrowed<'a> {
    pub id: i64,
    pub kind: &'a str,
    pub attempts: i32,
}
impl<'a> From<ClaimTaskBorrowed<'a>> for ClaimTask {
    fn from(ClaimTaskBorrowed { id, kind, attempts }: ClaimTaskBorrowed<'a>) -> Self {
        Self {
            id,
            kind: kind.into(),
            attempts,
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
pub struct EnqueueDueSchedulesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<EnqueueDueSchedulesBorrowed, tokio_postgres::Error>,
    mapper: fn(EnqueueDueSchedulesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> EnqueueDueSchedulesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(EnqueueDueSchedulesBorrowed) -> R,
    ) -> EnqueueDueSchedulesQuery<'c, 'a, 's, C, R, N> {
        EnqueueDueSchedulesQuery {
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
pub struct ClaimTaskQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ClaimTaskBorrowed, tokio_postgres::Error>,
    mapper: fn(ClaimTaskBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ClaimTaskQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ClaimTaskBorrowed) -> R) -> ClaimTaskQuery<'c, 'a, 's, C, R, N> {
        ClaimTaskQuery {
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
pub struct StartTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn start_task() -> StartTaskStmt {
    StartTaskStmt(
        "INSERT INTO task (kind, requested_by, state, attempts, worker) VALUES ($1::text, \
         $2::text, 'running', 1, $3::text) RETURNING id",
        None,
    )
}
impl StartTaskStmt {
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
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        kind: &'a T1,
        requested_by: &'a T2,
        worker: &'a T3,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        I64Query {
            client,
            params: [kind, requested_by, worker],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        StartTaskParams<T1, T2, T3>,
        I64Query<'c, 'a, 's, C, i64, 3>,
        C,
    > for StartTaskStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a StartTaskParams<T1, T2, T3>,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        self.bind(client, &params.kind, &params.requested_by, &params.worker)
    }
}
pub struct EnqueueDueSchedulesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn enqueue_due_schedules() -> EnqueueDueSchedulesStmt {
    EnqueueDueSchedulesStmt(
        "WITH due AS ( UPDATE schedule SET next_due = now() + make_interval(secs => \
         every_seconds) WHERE next_due <= now() RETURNING kind ) INSERT INTO task (kind, \
         requested_by) SELECT kind, 'schedule' FROM due RETURNING id, kind",
        None,
    )
}
impl EnqueueDueSchedulesStmt {
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
    ) -> EnqueueDueSchedulesQuery<'c, 'a, 's, C, EnqueueDueSchedules, 0> {
        EnqueueDueSchedulesQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<EnqueueDueSchedulesBorrowed, tokio_postgres::Error> {
                Ok(EnqueueDueSchedulesBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                })
            },
            mapper: |it| EnqueueDueSchedules::from(it),
        }
    }
}
pub struct ClaimTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn claim_task() -> ClaimTaskStmt {
    ClaimTaskStmt(
        "UPDATE task SET state = 'running', attempts = attempts + 1, worker = $1::text, \
         lease_expires_at = now() + make_interval(secs => $2::float8) WHERE id = ( SELECT id FROM \
         task WHERE (state = 'pending' AND due_at <= now()) OR (state = 'running' AND \
         lease_expires_at < now()) ORDER BY due_at LIMIT 1 FOR UPDATE SKIP LOCKED ) RETURNING id, \
         kind, attempts",
        None,
    )
}
impl ClaimTaskStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        worker: &'a T1,
        lease_seconds: &'a f64,
    ) -> ClaimTaskQuery<'c, 'a, 's, C, ClaimTask, 2> {
        ClaimTaskQuery {
            client,
            params: [worker, lease_seconds],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ClaimTaskBorrowed, tokio_postgres::Error> {
                    Ok(ClaimTaskBorrowed {
                        id: row.try_get(0)?,
                        kind: row.try_get(1)?,
                        attempts: row.try_get(2)?,
                    })
                },
            mapper: |it| ClaimTask::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ClaimTaskParams<T1>,
        ClaimTaskQuery<'c, 'a, 's, C, ClaimTask, 2>,
        C,
    > for ClaimTaskStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ClaimTaskParams<T1>,
    ) -> ClaimTaskQuery<'c, 'a, 's, C, ClaimTask, 2> {
        self.bind(client, &params.worker, &params.lease_seconds)
    }
}
pub struct RenewLeaseStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn renew_lease() -> RenewLeaseStmt {
    RenewLeaseStmt(
        "UPDATE task SET lease_expires_at = now() + make_interval(secs => $1::float8) WHERE id = \
         $2::bigint AND worker = $3::text AND state = 'running'",
        None,
    )
}
impl RenewLeaseStmt {
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
        lease_seconds: &'a f64,
        id: &'a i64,
        worker: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[lease_seconds, id, worker]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RenewLeaseParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RenewLeaseStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RenewLeaseParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.lease_seconds, &params.id, &params.worker))
    }
}
pub struct CompleteTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn complete_task() -> CompleteTaskStmt {
    CompleteTaskStmt(
        "UPDATE task SET state = 'done', finished_at = now(), lease_expires_at = NULL, failure = \
         NULL WHERE id = $1::bigint AND worker = $2::text AND state = 'running'",
        None,
    )
}
impl CompleteTaskStmt {
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
        id: &'a i64,
        worker: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id, worker]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CompleteTaskParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CompleteTaskStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CompleteTaskParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.id, &params.worker))
    }
}
pub struct FailTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn fail_task() -> FailTaskStmt {
    FailTaskStmt(
        "UPDATE task SET state = CASE WHEN attempts >= $1::integer THEN 'failed' ELSE 'pending' \
         END, due_at = now() + make_interval(secs => $2::float8), finished_at = CASE WHEN \
         attempts >= $1::integer THEN now() END, lease_expires_at = NULL, failure = $3::text \
         WHERE id = $4::bigint AND worker = $5::text AND state = 'running'",
        None,
    )
}
impl FailTaskStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        max_attempts: &'a i32,
        retry_seconds: &'a f64,
        failure: &'a T1,
        id: &'a i64,
        worker: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[max_attempts, retry_seconds, failure, id, worker])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        FailTaskParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for FailTaskStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a FailTaskParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.max_attempts,
            &params.retry_seconds,
            &params.failure,
            &params.id,
            &params.worker,
        ))
    }
}
pub struct ReleaseTaskStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn release_task() -> ReleaseTaskStmt {
    ReleaseTaskStmt(
        "UPDATE task SET state = 'pending', attempts = attempts - 1, lease_expires_at = NULL \
         WHERE id = $1::bigint AND worker = $2::text AND state = 'running'",
        None,
    )
}
impl ReleaseTaskStmt {
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
        id: &'a i64,
        worker: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id, worker]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        ReleaseTaskParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for ReleaseTaskStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a ReleaseTaskParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.id, &params.worker))
    }
}
