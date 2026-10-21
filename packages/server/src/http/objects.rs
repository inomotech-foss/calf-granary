//! Reading and writing objects in caches.

mod hashserv;
mod metrics;
mod nix;

use core::error::Error;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use deadpool_postgres::{Client, Pool};
use serde::{Deserialize, Serialize};

use self::hashserv::HashservStats;
use self::metrics::{Lookup, ObjectMetrics, Upload};
use super::credential;
use crate::access::{AccessLog, Accessed};
use crate::cache::{Cache, Caches, Hit, ObjectKey, Protocol, PutError, Stored};
use crate::content::{Content, Encoding, WriteError};
use crate::token::{Grant, TokenKey};

/// The most keys one missing-keys request may ask about.
const MAX_KEYS: usize = 10_000;

/// Shared state of the object routes.
#[derive(Clone, Debug)]
struct Objects {
    /// The caches.
    caches: Arc<Caches>,
    /// The content of the entries.
    content: Arc<Content>,
    /// Postgres connections.
    pool: Pool,
    /// Where reads are recorded.
    access: Arc<AccessLog>,
    /// The key that verifies write tokens.
    key: TokenKey,
    /// Records reads and writes.
    metrics: ObjectMetrics,
    /// Request statistics of hashserv caches.
    hashserv: Arc<HashservStats>,
}

/// A question about which keys a cache lacks.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissingRequest {
    /// The keys.
    keys: Vec<String>,
}

/// The keys that a cache lacks.
#[derive(Debug, Serialize)]
struct MissingResponse {
    /// The keys that neither the cache nor its fallback has.
    missing: Vec<String>,
}

impl Objects {
    /// Looks up an entry and records the read.
    async fn fetch(&self, client: &Client, found: &Cache, key: &str) -> Result<Hit, StatusCode> {
        let cache = found.name();
        let object = ObjectKey::try_from(key).map_err(|_invalid| StatusCode::BAD_REQUEST)?;
        match found.get(client, &object).await {
            Ok(Some(hit)) => {
                let lookup = if hit.fallback {
                    Lookup::Fallback
                } else {
                    Lookup::Hit
                };
                self.metrics.lookup(cache, lookup);
                self.access.record(Accessed {
                    cache: hit.cache.clone(),
                    key: object,
                });
                Ok(hit)
            }
            Ok(None) => {
                self.metrics.lookup(cache, Lookup::Miss);
                Err(StatusCode::NOT_FOUND)
            }
            Err(err) => {
                self.metrics.lookup(cache, Lookup::Error);
                tracing::error!(
                    error = &err as &dyn Error,
                    cache = cache.as_ref(),
                    key,
                    "failed to look up an entry"
                );
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }

    /// Checks that the request carries a granary token that grants the
    /// cache.
    fn authorize(&self, headers: &HeaderMap, cache: &str) -> Result<Grant, StatusCode> {
        let token = credential(headers).ok_or(StatusCode::UNAUTHORIZED)?;
        let grant = self.key.verify(&token).map_err(|err| {
            tracing::info!(error = &err as &dyn Error, "rejected a write token");
            StatusCode::UNAUTHORIZED
        })?;
        if grant.allows(cache) {
            Ok(grant)
        } else {
            tracing::info!(sub = grant.sub, cache, "rejected a write outside the grant");
            Err(StatusCode::FORBIDDEN)
        }
    }

    /// Takes a database connection.
    async fn client(&self) -> Result<Client, StatusCode> {
        self.pool.get().await.map_err(|err| {
            tracing::error!(
                error = &err as &dyn Error,
                "failed to get a database connection"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })
    }
}

/// Builds the router for reading and writing objects.
pub(super) fn router(
    caches: Arc<Caches>,
    content: Arc<Content>,
    pool: Pool,
    access: Arc<AccessLog>,
    key: TokenKey,
) -> Router {
    Router::new()
        .route(
            "/{cache}/{*key}",
            get(get_object).head(head_object).put(put_object),
        )
        .route("/_api/nix/{cache}/upload", put(nix::upload))
        .route("/_api/nix/{cache}/missing", post(nix::missing))
        .route("/_api/nix/{cache}/public-key", get(nix::public_key))
        .route("/_hashserv/{cache}", get(hashserv::connect))
        .route("/_api/files/{cache}/missing", post(missing))
        .layer(DefaultBodyLimit::disable())
        .with_state(Objects {
            caches,
            content,
            pool,
            access,
            key,
            metrics: ObjectMetrics::new(),
            hashserv: Arc::new(HashservStats::new()),
        })
}

/// Streams an object, from the fallback cache if needed.
async fn get_object(State(state): State<Objects>, path: Path<(String, String)>) -> Response {
    let Path((cache, key)) = &path;
    let Some(found) = state.caches.get(cache) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Protocol::Nix(settings) = found.protocol() {
        return nix::get(&state, found, settings, key, false).await;
    }
    serve_file(&state, found, key, false).await
}

/// Returns the headers of an object, from the fallback cache if needed.
async fn head_object(State(state): State<Objects>, path: Path<(String, String)>) -> Response {
    let Path((cache, key)) = &path;
    let Some(found) = state.caches.get(cache) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Protocol::Nix(settings) = found.protocol() {
        return nix::get(&state, found, settings, key, true).await;
    }
    serve_file(&state, found, key, true).await
}

/// Serves the plain content of an entry, from the fallback cache if needed.
async fn serve_file(state: &Objects, cache: &Cache, key: &str, head: bool) -> Response {
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let hit = match state.fetch(&client, cache, key).await {
        Ok(hit) => hit,
        Err(status) => return status.into_response(),
    };
    if head {
        return object_headers(&hit).into_response();
    }
    match state
        .content
        .read(&client, hit.blob_id, Encoding::Plain)
        .await
    {
        Ok(stream) => {
            state.metrics.served(cache.name(), hit.size);
            (object_headers(&hit), Body::from_stream(stream)).into_response()
        }
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to read an object");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Stores an object unless the cache already has it.
///
/// Needs a granary token that grants the cache. Answers `201 Created` for new
/// objects and `204 No Content` for existing ones.
async fn put_object(
    State(state): State<Objects>,
    Path((cache, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> StatusCode {
    let grant = match state.authorize(&headers, &cache) {
        Ok(grant) => grant,
        Err(status) => return status,
    };
    let Some(found) = state.caches.get(&cache) else {
        return StatusCode::NOT_FOUND;
    };
    match found.protocol() {
        Protocol::Files => {}
        Protocol::Nix(_) => return nix::put(&state, found, &key, body).await,
        Protocol::Hashserv => return StatusCode::METHOD_NOT_ALLOWED,
    }
    let Ok(object) = ObjectKey::try_from(key.as_str()) else {
        return StatusCode::BAD_REQUEST;
    };
    match found
        .put(
            &state.content,
            &state.pool,
            &object,
            body.into_data_stream(),
        )
        .await
    {
        Ok(Stored::New { size }) => {
            tracing::debug!(sub = grant.sub, cache, key, size, "stored an object");
            state.metrics.upload(found.name(), Upload::Stored);
            state.metrics.stored(found.name(), size);
            StatusCode::CREATED
        }
        Ok(Stored::Existing) => {
            state.metrics.upload(found.name(), Upload::Existing);
            StatusCode::NO_CONTENT
        }
        Err(err @ PutError::Write(WriteError::Body(_))) => {
            state.metrics.upload(found.name(), Upload::Aborted);
            tracing::info!(error = &err as &dyn Error, cache, key, "upload aborted");
            StatusCode::BAD_REQUEST
        }
        Err(err) => {
            state.metrics.upload(found.name(), Upload::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache,
                key,
                "failed to store an object"
            );
            StatusCode::BAD_GATEWAY
        }
    }
}

/// Answers which keys neither a files cache nor its fallback has.
async fn missing(
    State(state): State<Objects>,
    Path(cache): Path<String>,
    Json(request): Json<MissingRequest>,
) -> Response {
    let Some(found) = state.caches.get(&cache) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !matches!(found.protocol(), Protocol::Files) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if request.keys.len() > MAX_KEYS {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Ok(keys) = request
        .keys
        .iter()
        .map(|key| ObjectKey::try_from(key.as_str()))
        .collect::<Result<Vec<_>, _>>()
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    match found.missing(&client, &keys).await {
        Ok(missing) => Json(MissingResponse { missing }).into_response(),
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to look up missing keys");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// The headers that describe an object.
fn object_headers(hit: &Hit) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(CONTENT_LENGTH, HeaderValue::from(hit.size));
    let last_modified = hit
        .created_at
        .to_utc()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    if let Ok(value) = HeaderValue::from_str(&last_modified) {
        headers.insert(LAST_MODIFIED, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("\"{}\"", hit.hash)) {
        headers.insert(ETAG, value);
    }
    headers
}
