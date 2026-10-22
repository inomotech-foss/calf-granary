//! The Yocto hash equivalence protocol over WebSocket.
//!
//! Clients connect to `wss://<server>/_hashserv/<cache>`. Reads need no
//! token. Reports need `auth` with any username and a granary token that
//! grants the cache as the password.

mod stats;

use core::error::Error;
use core::time::Duration;
use std::borrow::Cow;
use std::time::Instant;

use axum::extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use deadpool_postgres::Client;
use futures::FutureExt as _;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use tokio::time;

pub(crate) use self::stats::HashservStats;
use super::Objects;
use crate::cache::{Cache, HashservError, Protocol, Report, StoredReport};
use crate::sha256::Sha256;

/// The protocol name in the first line of a connection.
const PROTOCOL: &str = "OEHASHEQUIV";

/// The most stream lines answered with one query.
const STREAM_BATCH: usize = 1000;

/// How long a failed `auth` waits before it answers, as upstream does.
const AUTH_DELAY: Duration = Duration::from_secs(1);

/// The answer that starts and ends stream mode.
const STREAM_OK: Utf8Bytes = Utf8Bytes::from_static("ok");

/// A command and its payload, as one JSON object with one field.
#[derive(Debug, Deserialize)]
enum Request<'a> {
    /// Checks that the server is alive.
    #[serde(rename = "ping")]
    Ping(IgnoredAny),
    /// Looks up a task.
    #[serde(rename = "get", borrow)]
    Get(GetRequest<'a>),
    /// Looks up an output hash.
    #[serde(rename = "get-outhash", borrow)]
    GetOuthash(GetOuthashRequest<'a>),
    /// Starts stream mode for unihash lookups.
    #[serde(rename = "get-stream")]
    GetStream(IgnoredAny),
    /// Starts stream mode for unihash existence checks.
    #[serde(rename = "exists-stream")]
    ExistsStream(IgnoredAny),
    /// Reports a task's output hash.
    #[serde(rename = "report", borrow)]
    Report(ReportRequest<'a>),
    /// Reports that a task is equivalent to a unihash.
    #[serde(rename = "report-equiv", borrow)]
    ReportEquiv(ReportEquivRequest<'a>),
    /// Logs in.
    #[serde(rename = "auth", borrow)]
    Auth(AuthRequest<'a>),
    /// Describes a user.
    #[serde(rename = "get-user", borrow)]
    GetUser(GetUserRequest<'a>),
    /// Acts as another user.
    #[serde(rename = "become-user")]
    BecomeUser(IgnoredAny),
    /// Returns the request statistics.
    #[serde(rename = "get-stats")]
    GetStats(IgnoredAny),
    /// Resets the request statistics.
    #[serde(rename = "reset-stats")]
    ResetStats(IgnoredAny),
    /// Waits for a backfill, which granary doesn't have.
    #[serde(rename = "backfill-wait")]
    BackfillWait(IgnoredAny),
    /// Commands of the upstream server that granary leaves out, because tiers
    /// and retention replace them.
    #[serde(
        rename = "remove",
        alias = "clean-unused",
        alias = "delete-user",
        alias = "gc-mark",
        alias = "gc-mark-stream",
        alias = "gc-status",
        alias = "gc-sweep",
        alias = "get-all-users",
        alias = "get-db-query-columns",
        alias = "get-db-usage",
        alias = "new-user",
        alias = "refresh-token",
        alias = "set-user-perms"
    )]
    Unsupported(IgnoredAny),
}

impl Request<'_> {
    /// The command name, for statistics.
    const fn command(&self) -> &'static str {
        match self {
            Self::Ping(_) => "ping",
            Self::Get(_) => "get",
            Self::GetOuthash(_) => "get-outhash",
            Self::GetStream(_) => "get-stream",
            Self::ExistsStream(_) => "exists-stream",
            Self::Report(_) => "report",
            Self::ReportEquiv(_) => "report-equiv",
            Self::Auth(_) => "auth",
            Self::GetUser(_) => "get-user",
            Self::BecomeUser(_) => "become-user",
            Self::GetStats(_) => "get-stats",
            Self::ResetStats(_) => "reset-stats",
            Self::BackfillWait(_) => "backfill-wait",
            Self::Unsupported(_) => "unsupported",
        }
    }
}

/// A request to look up a task.
#[derive(Debug, Deserialize)]
struct GetRequest<'a> {
    /// The hash method.
    #[serde(borrow)]
    method: Cow<'a, str>,
    /// The task's input hash.
    taskhash: Sha256,
    /// Whether to include the task's output hash report.
    #[serde(default)]
    all: bool,
}

/// A request to look up an output hash.
#[derive(Debug, Deserialize)]
struct GetOuthashRequest<'a> {
    /// The hash method.
    #[serde(borrow)]
    method: Cow<'a, str>,
    /// The output hash.
    outhash: Sha256,
    /// Whether to include the unihash.
    #[serde(default = "default_true")]
    with_unihash: bool,
}

/// A report of a task's output hash.
#[derive(Debug, Deserialize)]
struct ReportRequest<'a> {
    /// The task's input hash.
    taskhash: Sha256,
    /// The hash method.
    #[serde(borrow)]
    method: Cow<'a, str>,
    /// The task's output hash.
    outhash: Sha256,
    /// The unihash the build used.
    unihash: Sha256,
    /// The recipe name.
    #[serde(rename = "PN", borrow)]
    pn: Option<Cow<'a, str>>,
    /// The recipe version.
    #[serde(rename = "PV", borrow)]
    pv: Option<Cow<'a, str>>,
    /// The recipe revision.
    #[serde(rename = "PR", borrow)]
    pr: Option<Cow<'a, str>>,
    /// The task name.
    #[serde(borrow)]
    task: Option<Cow<'a, str>>,
    /// The signature data of the output hash.
    #[serde(borrow)]
    outhash_siginfo: Option<Cow<'a, str>>,
}

/// A report that a task is equivalent to a unihash.
#[derive(Debug, Deserialize)]
struct ReportEquivRequest<'a> {
    /// The task's input hash.
    taskhash: Sha256,
    /// The hash method.
    #[serde(borrow)]
    method: Cow<'a, str>,
    /// The unihash.
    unihash: Sha256,
}

/// A login.
#[derive(Debug, Deserialize)]
struct AuthRequest<'a> {
    /// Any name. Granary identifies clients by their token.
    #[serde(borrow)]
    username: Cow<'a, str>,
    /// A granary token.
    #[serde(borrow)]
    token: Cow<'a, str>,
}

/// A request to describe a user.
#[derive(Debug, Deserialize)]
struct GetUserRequest<'a> {
    /// The user, or the logged in user if missing.
    #[serde(borrow)]
    username: Option<Cow<'a, str>>,
}

/// The unihash of a task.
#[derive(Debug, Serialize)]
struct UnihashReply<'a> {
    /// The task's input hash.
    taskhash: Sha256,
    /// The hash method.
    method: &'a str,
    /// The unihash.
    unihash: Sha256,
}

/// A recorded report in the upstream format.
#[derive(Debug, Serialize)]
struct ReportReply<'a> {
    /// The hash method.
    method: &'a str,
    /// The task's input hash.
    taskhash: Sha256,
    /// The task's output hash.
    outhash: Sha256,
    /// When the report arrived, in RFC 3339.
    created: String,
    /// Who reported.
    owner: Option<String>,
    /// The recipe name.
    #[serde(rename = "PN")]
    pn: Option<String>,
    /// The recipe version.
    #[serde(rename = "PV")]
    pv: Option<String>,
    /// The recipe revision.
    #[serde(rename = "PR")]
    pr: Option<String>,
    /// The task name.
    task: Option<String>,
    /// The signature data of the output hash.
    outhash_siginfo: Option<String>,
    /// The task's unihash, if asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    unihash: Option<Sha256>,
}

impl<'a> ReportReply<'a> {
    /// The reply for a recorded report, with or without its unihash.
    fn new(method: &'a str, report: StoredReport, with_unihash: bool) -> Self {
        Self {
            method,
            taskhash: report.taskhash,
            outhash: report.outhash,
            created: report.created_at.to_rfc3339(),
            owner: report.owner,
            pn: report.pn,
            pv: report.pv,
            pr: report.pr,
            task: report.task,
            outhash_siginfo: report.outhash_siginfo,
            unihash: with_unihash.then_some(report.unihash),
        }
    }
}

/// The answer to `auth` and `get-user`.
#[derive(Debug, Serialize)]
struct UserReply<'a> {
    /// Whether the login worked, only for `auth`.
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<bool>,
    /// The name the client gave.
    username: &'a str,
    /// What the user may do.
    permissions: &'static [&'static str],
}

/// The answer to `ping`.
#[derive(Debug, Serialize)]
struct PingReply {
    /// Always true.
    alive: bool,
}

/// The answer to `backfill-wait`.
#[derive(Debug, Serialize)]
struct BackfillReply {
    /// Always zero.
    tasks: u32,
}

/// An `invoke-error` answer.
#[derive(Debug, Serialize)]
struct InvokeErrorReply<'a> {
    /// The error.
    #[serde(rename = "invoke-error")]
    error: InvokeError<'a>,
}

/// The body of an `invoke-error` answer.
#[derive(Debug, Serialize)]
struct InvokeError<'a> {
    /// What went wrong.
    message: &'a str,
}

/// What a session ends with when it can't answer.
#[derive(Debug)]
enum Failure {
    /// Answer with an `invoke-error` and close, as upstream does.
    Invoke(Cow<'static, str>),
    /// Close without an answer.
    Close,
}

impl From<tokio_postgres::Error> for Failure {
    fn from(err: tokio_postgres::Error) -> Self {
        HashservError::from(err).into()
    }
}

impl From<HashservError> for Failure {
    fn from(err: HashservError) -> Self {
        tracing::error!(error = &err as &dyn Error, "hash equivalence query failed");
        Self::Close
    }
}

/// The kind of stream that a client asked for.
#[derive(Clone, Copy, Debug)]
enum StreamKind {
    /// `<method> <taskhash>` lines answered with unihashes.
    Unihashes,
    /// Unihash lines answered with `true` or `false`.
    Exists,
}

/// The login of a session.
#[derive(Debug)]
struct Login {
    /// The name the client gave.
    username: String,
    /// The token's subject.
    subject: String,
    /// Whether the token grants reports to the cache.
    can_report: bool,
}

/// Answers the commands of one connection.
struct Handler<'a> {
    /// The route state.
    state: &'a Objects,
    /// The hashserv cache.
    cache: &'a Cache,
    /// The login, after `auth`.
    login: Option<Login>,
}

/// One client connection.
struct Session<'a> {
    /// The connection.
    socket: WebSocket,
    /// Answers the commands.
    handler: Handler<'a>,
}

impl Handler<'_> {
    /// Takes a database connection.
    async fn client(&self) -> Result<Client, Failure> {
        self.state.client().await.map_err(|_status| Failure::Close)
    }

    /// Answers `get`.
    async fn get(&self, client: &Client, request: &GetRequest<'_>) -> Result<Utf8Bytes, Failure> {
        let method = request.method.as_ref();
        if request.all {
            let row = self.cache.task(client, method, request.taskhash).await?;
            return json(&row.map(|row| ReportReply::new(method, row, true)));
        }
        let unihash = self.cache.unihash(client, method, request.taskhash).await?;
        json(&unihash.map(|unihash| UnihashReply {
            taskhash: request.taskhash,
            method,
            unihash,
        }))
    }

    /// Answers `get-outhash`.
    async fn get_outhash(
        &self,
        client: &Client,
        request: &GetOuthashRequest<'_>,
    ) -> Result<Utf8Bytes, Failure> {
        let method = request.method.as_ref();
        let row = self.cache.outhash(client, method, request.outhash).await?;
        json(&row.map(|row| ReportReply::new(method, row, request.with_unihash)))
    }

    /// Answers `report`, recording it only for clients that may report.
    async fn report(
        &self,
        client: &Client,
        request: &ReportRequest<'_>,
    ) -> Result<Utf8Bytes, Failure> {
        let mut report = Report {
            method: &request.method,
            taskhash: request.taskhash,
            outhash: request.outhash,
            unihash: request.unihash,
            owner: None,
            pn: request.pn.as_deref(),
            pv: request.pv.as_deref(),
            pr: request.pr.as_deref(),
            task: request.task.as_deref(),
            outhash_siginfo: request.outhash_siginfo.as_deref(),
        };
        let unihash = match &self.login {
            Some(login) if login.can_report => {
                report.owner = Some(&login.subject);
                self.cache.report(client, &report).await?
            }
            Some(_) | None => self.cache.report_read_only(client, &report).await?,
        };
        json(&UnihashReply {
            taskhash: report.taskhash,
            method: report.method,
            unihash,
        })
    }

    /// Answers `report-equiv` for clients that may report.
    async fn report_equiv(
        &self,
        client: &Client,
        request: &ReportEquivRequest<'_>,
    ) -> Result<Utf8Bytes, Failure> {
        self.require_report("@report")?;
        let unihash = self
            .cache
            .report_equivalent(client, &request.method, request.taskhash, request.unihash)
            .await?;
        json(&UnihashReply {
            taskhash: request.taskhash,
            method: &request.method,
            unihash,
        })
    }

    /// Answers `auth` with a granary token as the password.
    async fn auth(&mut self, request: AuthRequest<'_>) -> Result<Utf8Bytes, Failure> {
        let Ok(grant) = self.state.key.verify(&request.token) else {
            time::sleep(AUTH_DELAY).await;
            return Err(Failure::Invoke(
                format!("Unable to authenticate as {}", request.username).into(),
            ));
        };
        let can_report = grant.allows(self.cache.name().as_ref());
        let login = Login {
            username: request.username.into_owned(),
            subject: grant.sub,
            can_report,
        };
        tracing::debug!(
            sub = login.subject,
            cache = self.cache.name().as_ref(),
            "hashserv login"
        );
        let reply = json(&UserReply {
            result: Some(true),
            username: &login.username,
            permissions: permissions(Some(&login)),
        });
        self.login = Some(login);
        reply
    }

    /// Answers `get-user` for the logged in user.
    fn get_user(&self, request: &GetUserRequest<'_>) -> Result<Utf8Bytes, Failure> {
        let Some(login) = &self.login else {
            return Err(Failure::Invoke(
                "Anonymous user is not allowed to access permissions(s) @user-admin".into(),
            ));
        };
        if request
            .username
            .as_ref()
            .is_some_and(|name| name != &login.username)
        {
            return json(&());
        }
        json(&UserReply {
            result: None,
            username: &login.username,
            permissions: permissions(Some(login)),
        })
    }

    /// Fails unless the session may report, naming the missing permission.
    fn require_report(&self, permission: &str) -> Result<(), Failure> {
        match &self.login {
            Some(login) if login.can_report => Ok(()),
            Some(login) => Err(Failure::Invoke(
                format!(
                    "User {} is not allowed to access permissions(s) {permission}",
                    login.username
                )
                .into(),
            )),
            None => Err(Failure::Invoke(
                format!("Anonymous user is not allowed to access permissions(s) {permission}")
                    .into(),
            )),
        }
    }
}

impl Session<'_> {
    /// Speaks the protocol until the client leaves.
    async fn run(mut self) {
        if !self.handshake().await {
            return;
        }
        let mut lines = Vec::new();
        while let Some(line) = self.recv().await {
            let Ok(request) = serde_json::from_str::<Request<'_>>(&line) else {
                self.send(invoke_error("Invalid request")).await;
                return;
            };
            let start = Instant::now();
            let streams = matches!(request, Request::GetStream(_) | Request::ExistsStream(_));
            let command = request.command();
            let result = self.dispatch(request, &mut lines).await;
            if !streams {
                self.handler.state.hashserv.record(
                    self.handler.cache.name(),
                    command,
                    start.elapsed().as_secs_f64(),
                );
            }
            match result {
                Ok(Some(reply)) => {
                    if !self.send(reply).await {
                        return;
                    }
                }
                Ok(None) => {}
                Err(Failure::Invoke(message)) => {
                    self.send(invoke_error(&message)).await;
                    return;
                }
                Err(Failure::Close) => return,
            }
        }
    }

    /// Reads the protocol line and the headers.
    async fn handshake(&mut self) -> bool {
        let Some(line) = self.recv().await else {
            return false;
        };
        let mut words = line.split_whitespace();
        let (Some(PROTOCOL), Some(version)) = (words.next(), words.next()) else {
            return false;
        };
        if !matches!(version, "1.1" | "1.0.1") {
            return false;
        }
        let mut needs_headers = false;
        loop {
            let Some(header) = self.recv().await else {
                return false;
            };
            if header.is_empty() {
                break;
            }
            if let Some((tag, value)) = header.split_once(':')
                && tag.trim().eq_ignore_ascii_case("needs-headers")
            {
                needs_headers = value.trim() == "true";
            }
        }
        !needs_headers || self.send(Utf8Bytes::default()).await
    }

    /// Answers one command, or returns `None` after a stream.
    ///
    /// `lines` is a buffer for stream lines.
    async fn dispatch(
        &mut self,
        request: Request<'_>,
        lines: &mut Vec<Utf8Bytes>,
    ) -> Result<Option<Utf8Bytes>, Failure> {
        let handler = &mut self.handler;
        let reply = match request {
            Request::Ping(_) => json(&PingReply { alive: true })?,
            Request::Get(request) => handler.get(&handler.client().await?, &request).await?,
            Request::GetOuthash(request) => {
                handler
                    .get_outhash(&handler.client().await?, &request)
                    .await?
            }
            Request::GetStream(_) => {
                self.stream(StreamKind::Unihashes, lines).await?;
                return Ok(None);
            }
            Request::ExistsStream(_) => {
                self.stream(StreamKind::Exists, lines).await?;
                return Ok(None);
            }
            Request::Report(request) => handler.report(&handler.client().await?, &request).await?,
            Request::ReportEquiv(request) => {
                handler
                    .report_equiv(&handler.client().await?, &request)
                    .await?
            }
            Request::Auth(request) => handler.auth(request).await?,
            Request::GetUser(request) => handler.get_user(&request)?,
            Request::BecomeUser(_) => {
                return Err(Failure::Invoke("granary has no users to become".into()));
            }
            Request::GetStats(_) => json(&handler.state.hashserv.get(handler.cache.name()))?,
            Request::ResetStats(_) => {
                handler.require_report("@db-admin")?;
                json(&handler.state.hashserv.reset(handler.cache.name()))?
            }
            Request::BackfillWait(_) => json(&BackfillReply { tasks: 0 })?,
            Request::Unsupported(_) => {
                return Err(Failure::Invoke(
                    "this command isn't supported, granary manages users and retention".into(),
                ));
            }
        };
        Ok(Some(reply))
    }

    /// Runs stream mode until the client sends `END`.
    ///
    /// Each batch of lines takes a connection only for its query, so idle
    /// streams hold none.
    async fn stream(
        &mut self,
        kind: StreamKind,
        lines: &mut Vec<Utf8Bytes>,
    ) -> Result<(), Failure> {
        if !self.send(Utf8Bytes::from_static("\"ok\"")).await {
            return Err(Failure::Close);
        }
        loop {
            lines.clear();
            let first = self.recv().await.ok_or(Failure::Close)?;
            let mut ended = is_end(&first);
            if !ended {
                lines.push(first);
            }
            while !ended && lines.len() < STREAM_BATCH {
                let Some(line) = self.try_recv() else {
                    break;
                };
                if is_end(&line) {
                    ended = true;
                } else {
                    lines.push(line);
                }
            }
            match kind {
                StreamKind::Unihashes => self.answer_unihashes(lines).await?,
                StreamKind::Exists => self.answer_exists(lines).await?,
            }
            if ended {
                return if self.send(STREAM_OK).await {
                    Ok(())
                } else {
                    Err(Failure::Close)
                };
            }
        }
    }

    /// Answers `<method> <taskhash>` lines with unihashes, or empty lines.
    async fn answer_unihashes(&mut self, lines: &[Utf8Bytes]) -> Result<(), Failure> {
        let tasks = lines
            .iter()
            .map(|line| {
                let mut words = line.split_whitespace();
                match (words.next(), words.next(), words.next()) {
                    (Some(method), Some(taskhash), None) => {
                        Ok(taskhash.parse().ok().map(|taskhash| (method, taskhash)))
                    }
                    _ => Err(Failure::Close),
                }
            })
            .collect::<Result<Vec<Option<(&str, Sha256)>>, _>>()?;
        let valid: Vec<(&str, Sha256)> = tasks.iter().flatten().copied().collect();
        let client = self.handler.client().await?;
        let mut found = self
            .handler
            .cache
            .unihashes(&client, &valid)
            .await?
            .into_iter();
        drop(client);
        for task in tasks {
            let answer = task
                .and_then(|_task| found.next().flatten())
                .map_or_else(Utf8Bytes::default, |unihash| unihash.to_string().into());
            if !self.send(answer).await {
                return Err(Failure::Close);
            }
        }
        Ok(())
    }

    /// Answers unihash lines with `true` or `false`.
    async fn answer_exists(&mut self, lines: &[Utf8Bytes]) -> Result<(), Failure> {
        let unihashes: Vec<Option<Sha256>> = lines.iter().map(|line| line.parse().ok()).collect();
        let valid: Vec<Sha256> = unihashes.iter().flatten().copied().collect();
        let client = self.handler.client().await?;
        let mut exists = self
            .handler
            .cache
            .unihashes_exist(&client, &valid)
            .await?
            .into_iter();
        drop(client);
        for unihash in unihashes {
            let found = unihash.is_some() && exists.next().unwrap_or(false);
            let answer = Utf8Bytes::from_static(if found { "true" } else { "false" });
            if !self.send(answer).await {
                return Err(Failure::Close);
            }
        }
        Ok(())
    }

    /// Waits for the next line, or returns `None` once the client is gone.
    async fn recv(&mut self) -> Option<Utf8Bytes> {
        loop {
            match self.socket.recv().await? {
                Ok(Message::Text(text)) => return Some(text),
                Ok(Message::Binary(bytes)) => return Utf8Bytes::try_from(bytes).ok(),
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(Message::Close(_)) | Err(_) => return None,
            }
        }
    }

    /// Returns a line that has arrived already, without waiting.
    fn try_recv(&mut self) -> Option<Utf8Bytes> {
        match self.socket.recv().now_or_never()?? {
            Ok(Message::Text(text)) => Some(text),
            Ok(Message::Binary(bytes)) => Utf8Bytes::try_from(bytes).ok(),
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Close(_)) | Err(_) => None,
        }
    }

    /// Sends a line, returning whether the client got it.
    async fn send(&mut self, line: Utf8Bytes) -> bool {
        self.socket.send(Message::Text(line)).await.is_ok()
    }
}

/// Upgrades a request to a hash equivalence session.
pub(super) async fn connect(
    State(state): State<Objects>,
    Path(cache): Path<String>,
    upgrade: WebSocketUpgrade,
) -> Response {
    let is_hashserv = state
        .caches
        .get(&cache)
        .is_some_and(|found| matches!(found.protocol(), Protocol::Hashserv));
    if !is_hashserv {
        return StatusCode::NOT_FOUND.into_response();
    }
    upgrade.on_upgrade(async move |socket| {
        if let Some(found) = state.caches.get(&cache) {
            Session {
                socket,
                handler: Handler {
                    state: &state,
                    cache: found,
                    login: None,
                },
            }
            .run()
            .await;
        }
    })
}

/// The default of `with_unihash`.
const fn default_true() -> bool {
    true
}

/// Encodes a reply as one line of JSON.
fn json<T: Serialize>(reply: &T) -> Result<Utf8Bytes, Failure> {
    let encoded = serde_json::to_vec(reply).map_err(|_unencodable| Failure::Close)?;
    Utf8Bytes::try_from(encoded).map_err(|_invalid| Failure::Close)
}

/// An `invoke-error` line, or an empty line if it can't be encoded.
fn invoke_error(message: &str) -> Utf8Bytes {
    json(&InvokeErrorReply {
        error: InvokeError { message },
    })
    .unwrap_or_default()
}

/// Whether a stream line ends stream mode.
fn is_end(line: &str) -> bool {
    line.is_empty() || line == "END"
}

/// The permissions of a session.
fn permissions(login: Option<&Login>) -> &'static [&'static str] {
    if login.is_some_and(|login| login.can_report) {
        &["@read", "@report"]
    } else {
        &["@read"]
    }
}
