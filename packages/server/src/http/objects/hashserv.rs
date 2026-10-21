//! The Yocto hash equivalence protocol over WebSocket.
//!
//! Clients connect to `wss://<server>/_hashserv/<cache>`. Reads need no
//! token. Reports need `auth` with any username and a granary token that
//! grants the cache as the password.

mod stats;

use core::error::Error;
use core::time::Duration;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use chrono::{DateTime, FixedOffset};
use deadpool_postgres::Client;
use futures::FutureExt as _;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::time;

pub(crate) use self::stats::HashservStats;
use super::Objects;
use crate::cache::{Cache, Protocol, Report};

/// The protocol name in the first line of a connection.
const PROTOCOL: &str = "OEHASHEQUIV";

/// The most stream lines answered with one query.
const STREAM_BATCH: usize = 1000;

/// How long a failed `auth` waits before it answers, as upstream does.
const AUTH_DELAY: Duration = Duration::from_secs(1);

/// Commands of the upstream server that granary leaves out, because tiers
/// and retention replace them.
const UNSUPPORTED: [&str; 13] = [
    "clean-unused",
    "delete-user",
    "gc-mark",
    "gc-mark-stream",
    "gc-status",
    "gc-sweep",
    "get-all-users",
    "get-db-query-columns",
    "get-db-usage",
    "new-user",
    "refresh-token",
    "remove",
    "set-user-perms",
];

/// A request to look up a task.
#[derive(Debug, Deserialize)]
struct GetRequest {
    /// The hash method.
    method: String,
    /// The task's input hash.
    taskhash: String,
    /// Whether to include the task's output hash report.
    #[serde(default)]
    all: bool,
}

/// A request to look up an output hash.
#[derive(Debug, Deserialize)]
struct GetOuthashRequest {
    /// The hash method.
    method: String,
    /// The output hash.
    outhash: String,
    /// Whether to include the unihash.
    #[serde(default = "default_true")]
    with_unihash: bool,
}

/// A report of a task's output hash.
#[derive(Debug, Deserialize)]
struct ReportRequest {
    /// The task's input hash.
    taskhash: String,
    /// The hash method.
    method: String,
    /// The task's output hash.
    outhash: String,
    /// The unihash the build used.
    unihash: String,
    /// Who reports.
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
}

/// A report that a task is equivalent to a unihash.
#[derive(Debug, Deserialize)]
struct ReportEquivRequest {
    /// The task's input hash.
    taskhash: String,
    /// The hash method.
    method: String,
    /// The unihash.
    unihash: String,
}

/// A login.
#[derive(Debug, Deserialize)]
struct AuthRequest {
    /// Any name. Granary identifies clients by their token.
    username: String,
    /// A granary token.
    token: String,
}

/// What a session ends with when it can't answer.
#[derive(Debug)]
enum Failure {
    /// Answer with an `invoke-error` and close, as upstream does.
    Invoke(String),
    /// Close without an answer.
    Close,
}

impl From<tokio_postgres::Error> for Failure {
    fn from(err: tokio_postgres::Error) -> Self {
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
    /// Answers `get`.
    async fn get(&self, client: &Client, request: GetRequest) -> Result<Value, Failure> {
        if request.all {
            let row = self
                .cache
                .task(client, &request.method, &request.taskhash)
                .await?;
            return Ok(row.map_or(Value::Null, |row| {
                let mut reply = report_fields(
                    &request.method,
                    &request.taskhash,
                    &row.outhash,
                    row.created_at,
                    [
                        row.owner,
                        row.pn,
                        row.pv,
                        row.pr,
                        row.task,
                        row.outhash_siginfo,
                    ],
                );
                reply.insert("unihash".to_owned(), row.unihash.into());
                Value::Object(reply)
            }));
        }
        let unihash = self
            .cache
            .unihash(client, &request.method, &request.taskhash)
            .await?;
        Ok(unihash.map_or(Value::Null, |unihash| {
            json!({ "taskhash": request.taskhash, "method": request.method, "unihash": unihash })
        }))
    }

    /// Answers `get-outhash`.
    async fn get_outhash(
        &self,
        client: &Client,
        request: GetOuthashRequest,
    ) -> Result<Value, Failure> {
        let row = self
            .cache
            .outhash(
                client,
                &request.method,
                &request.outhash,
                request.with_unihash,
            )
            .await?;
        Ok(row.map_or(Value::Null, |row| {
            let mut reply = report_fields(
                &request.method,
                &row.taskhash,
                &request.outhash,
                row.created_at,
                [
                    row.owner,
                    row.pn,
                    row.pv,
                    row.pr,
                    row.task,
                    row.outhash_siginfo,
                ],
            );
            if request.with_unihash {
                reply.insert("unihash".to_owned(), row.unihash.into());
            }
            Value::Object(reply)
        }))
    }

    /// Answers `report`, recording it only for clients that may report.
    async fn report(&self, client: &Client, request: ReportRequest) -> Result<Value, Failure> {
        check_unihash(&request.unihash)?;
        let mut report = Report {
            method: request.method,
            taskhash: request.taskhash,
            outhash: request.outhash,
            unihash: request.unihash,
            owner: request.owner,
            pn: request.pn,
            pv: request.pv,
            pr: request.pr,
            task: request.task,
            outhash_siginfo: request.outhash_siginfo,
        };
        let unihash = match &self.login {
            Some(login) if login.can_report => {
                report.owner = Some(login.subject.clone());
                self.cache.report(client, &report).await?
            }
            Some(_) | None => self.cache.report_read_only(client, &report).await?,
        };
        Ok(json!({ "taskhash": report.taskhash, "method": report.method, "unihash": unihash }))
    }

    /// Answers `report-equiv` for clients that may report.
    async fn report_equiv(
        &self,
        client: &Client,
        request: ReportEquivRequest,
    ) -> Result<Value, Failure> {
        if !self.login.as_ref().is_some_and(|login| login.can_report) {
            return Err(Failure::Invoke(format!(
                "{} is not allowed to access permissions(s) @report",
                self.who()
            )));
        }
        check_unihash(&request.unihash)?;
        let unihash = self
            .cache
            .report_equivalent(client, &request.method, &request.taskhash, &request.unihash)
            .await?;
        Ok(json!({ "taskhash": request.taskhash, "method": request.method, "unihash": unihash }))
    }

    /// Answers `auth` with a granary token as the password.
    async fn auth(&mut self, request: AuthRequest) -> Result<Value, Failure> {
        let Ok(grant) = self.state.key.verify(&request.token) else {
            time::sleep(AUTH_DELAY).await;
            return Err(Failure::Invoke(format!(
                "Unable to authenticate as {}",
                request.username
            )));
        };
        let login = Login {
            username: request.username,
            subject: grant.sub.clone(),
            can_report: grant.allows(self.cache.name().as_ref()),
        };
        let reply = json!({
            "result": true,
            "username": login.username,
            "permissions": permissions(Some(&login)),
        });
        tracing::debug!(
            sub = login.subject,
            cache = self.cache.name().as_ref(),
            "hashserv login"
        );
        self.login = Some(login);
        Ok(reply)
    }

    /// Answers `get-user` for the logged in user.
    fn get_user(&self, payload: &Value) -> Result<Value, Failure> {
        let Some(login) = &self.login else {
            return Err(Failure::Invoke(
                "Anonymous user is not allowed to access permissions(s) @user-admin".to_owned(),
            ));
        };
        let asked = payload.get("username").and_then(Value::as_str);
        if asked.is_some_and(|name| name != login.username) {
            return Ok(Value::Null);
        }
        Ok(json!({ "username": login.username, "permissions": permissions(Some(login)) }))
    }

    /// Who the session acts as, for error messages.
    fn who(&self) -> String {
        self.login.as_ref().map_or_else(
            || "Anonymous user".to_owned(),
            |login| format!("User {}", login.username),
        )
    }
}

impl Session<'_> {
    /// Speaks the protocol until the client leaves.
    async fn run(mut self) {
        if !self.handshake().await {
            return;
        }
        while let Some(line) = self.recv().await {
            let Ok(Value::Object(request)) = serde_json::from_str::<Value>(&line) else {
                return;
            };
            let Some((command, payload)) = request.into_iter().next() else {
                return;
            };
            let start = Instant::now();
            let streams = matches!(command.as_str(), "get-stream" | "exists-stream");
            let result = self.dispatch(&command, payload).await;
            if !streams {
                self.handler.state.hashserv.record(
                    self.handler.cache.name().as_ref(),
                    &command,
                    start.elapsed().as_secs_f64(),
                );
            }
            match result {
                Ok(Some(reply)) => {
                    if !self.send(reply.to_string()).await {
                        return;
                    }
                }
                Ok(None) => {}
                Err(Failure::Invoke(message)) => {
                    let reply = json!({ "invoke-error": { "message": message } });
                    self.send(reply.to_string()).await;
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
        !needs_headers || self.send(String::new()).await
    }

    /// Answers one command, or returns `None` after a stream.
    async fn dispatch(&mut self, command: &str, payload: Value) -> Result<Option<Value>, Failure> {
        let client = self
            .handler
            .state
            .client()
            .await
            .map_err(|_status| Failure::Close)?;
        let reply = match command {
            "ping" => json!({ "alive": true }),
            "get" => self.handler.get(&client, parse(payload)?).await?,
            "get-outhash" => self.handler.get_outhash(&client, parse(payload)?).await?,
            "get-stream" => {
                self.stream(&client, StreamKind::Unihashes).await?;
                return Ok(None);
            }
            "exists-stream" => {
                self.stream(&client, StreamKind::Exists).await?;
                return Ok(None);
            }
            "report" => self.handler.report(&client, parse(payload)?).await?,
            "report-equiv" => self.handler.report_equiv(&client, parse(payload)?).await?,
            "auth" => self.handler.auth(parse(payload)?).await?,
            "get-user" => self.handler.get_user(&payload)?,
            "become-user" => {
                return Err(Failure::Invoke("granary has no users to become".to_owned()));
            }
            "get-stats" => self
                .handler
                .state
                .hashserv
                .get(self.handler.cache.name().as_ref()),
            "reset-stats" => {
                if !self
                    .handler
                    .login
                    .as_ref()
                    .is_some_and(|login| login.can_report)
                {
                    return Err(Failure::Invoke(format!(
                        "{} is not allowed to access permissions(s) @db-admin",
                        self.handler.who()
                    )));
                }
                self.handler
                    .state
                    .hashserv
                    .reset(self.handler.cache.name().as_ref())
            }
            "backfill-wait" => json!({ "tasks": 0 }),
            unsupported if UNSUPPORTED.contains(&unsupported) => {
                return Err(Failure::Invoke(format!(
                    "{unsupported} isn't supported, granary manages users and retention"
                )));
            }
            _ => return Err(Failure::Close),
        };
        Ok(Some(reply))
    }

    /// Runs stream mode until the client sends `END`.
    async fn stream(&mut self, client: &Client, kind: StreamKind) -> Result<(), Failure> {
        if !self.send("\"ok\"".to_owned()).await {
            return Err(Failure::Close);
        }
        loop {
            let first = self.recv().await.ok_or(Failure::Close)?;
            let mut lines = Vec::new();
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
            let answers = match kind {
                StreamKind::Unihashes => {
                    let tasks = lines
                        .iter()
                        .map(|line| {
                            let mut words = line.split_whitespace();
                            match (words.next(), words.next(), words.next()) {
                                (Some(method), Some(taskhash), None) => {
                                    Ok((method.to_owned(), taskhash.to_owned()))
                                }
                                _ => Err(Failure::Close),
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.handler
                        .cache
                        .unihashes(client, &tasks)
                        .await?
                        .into_iter()
                        .map(Option::unwrap_or_default)
                        .collect::<Vec<_>>()
                }
                StreamKind::Exists => self
                    .handler
                    .cache
                    .unihashes_exist(client, &lines)
                    .await?
                    .into_iter()
                    .map(|exists| exists.to_string())
                    .collect(),
            };
            for answer in answers {
                if !self.send(answer).await {
                    return Err(Failure::Close);
                }
            }
            if ended {
                return if self.send("ok".to_owned()).await {
                    Ok(())
                } else {
                    Err(Failure::Close)
                };
            }
        }
    }

    /// Waits for the next line, or returns `None` once the client is gone.
    async fn recv(&mut self) -> Option<String> {
        loop {
            match self.socket.recv().await? {
                Ok(Message::Text(text)) => return Some(text.to_string()),
                Ok(Message::Binary(bytes)) => return String::from_utf8(bytes.to_vec()).ok(),
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(Message::Close(_)) | Err(_) => return None,
            }
        }
    }

    /// Returns a line that has arrived already, without waiting.
    fn try_recv(&mut self) -> Option<String> {
        match self.socket.recv().now_or_never()?? {
            Ok(Message::Text(text)) => Some(text.to_string()),
            Ok(Message::Binary(bytes)) => String::from_utf8(bytes.to_vec()).ok(),
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Close(_)) | Err(_) => None,
        }
    }

    /// Sends a line, returning whether the client got it.
    async fn send(&mut self, line: String) -> bool {
        self.socket.send(Message::Text(line.into())).await.is_ok()
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

/// Parses a command payload, closing the connection if it's invalid.
fn parse<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, Failure> {
    serde_json::from_value(payload).map_err(|_invalid| Failure::Close)
}

/// Rejects unihashes that aren't 64 lowercase hexadecimal digits.
fn check_unihash(unihash: &str) -> Result<(), Failure> {
    let valid = unihash.len() == 64
        && unihash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(Failure::Invoke("Invalid unihash".to_owned()))
    }
}

/// Whether a stream line ends stream mode.
fn is_end(line: &str) -> bool {
    line.is_empty() || line == "END"
}

/// The permissions of a session.
fn permissions(login: Option<&Login>) -> Vec<&'static str> {
    if login.is_some_and(|login| login.can_report) {
        vec!["@read", "@report"]
    } else {
        vec!["@read"]
    }
}

/// The fields of an output hash report in the upstream format.
fn report_fields(
    method: &str,
    taskhash: &str,
    outhash: &str,
    created: DateTime<FixedOffset>,
    [owner, pn, pv, pr, task, siginfo]: [Option<String>; 6],
) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("method".to_owned(), method.into());
    fields.insert("taskhash".to_owned(), taskhash.into());
    fields.insert("outhash".to_owned(), outhash.into());
    fields.insert("created".to_owned(), created.to_rfc3339().into());
    fields.insert("owner".to_owned(), owner.into());
    fields.insert("PN".to_owned(), pn.into());
    fields.insert("PV".to_owned(), pv.into());
    fields.insert("PR".to_owned(), pr.into());
    fields.insert("task".to_owned(), task.into());
    fields.insert("outhash_siginfo".to_owned(), siginfo.into());
    fields
}
