//! Reading and writing objects in caches.

mod metrics;

use core::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use deadpool_postgres::{Client, Pool};

use self::metrics::{Lookup, ObjectMetrics, Upload};
use super::bearer;
use crate::access::{AccessLog, Accessed};
use crate::cache::{Caches, Hit, ObjectKey, PutError, Stored};
use crate::content::{Content, WriteError};
use crate::hex;
use crate::token::TokenKey;

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
}

impl Objects {
    /// Looks up an entry and records the read.
    async fn fetch(
        &self,
        client: &Client,
        Path((cache, key)): &Path<(String, String)>,
    ) -> Result<Hit, StatusCode> {
        let found = self.caches.get(cache).ok_or(StatusCode::NOT_FOUND)?;
        let object =
            ObjectKey::try_from(key.as_str()).map_err(|_invalid| StatusCode::BAD_REQUEST)?;
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
                    cache,
                    key,
                    "failed to look up an entry"
                );
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
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
        .layer(DefaultBodyLimit::disable())
        .with_state(Objects {
            caches,
            content,
            pool,
            access,
            key,
            metrics: ObjectMetrics::new(),
        })
}

/// Streams an object, from the fallback cache if needed.
async fn get_object(State(state): State<Objects>, path: Path<(String, String)>) -> Response {
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let hit = match state.fetch(&client, &path).await {
        Ok(hit) => hit,
        Err(status) => return status.into_response(),
    };
    match state.content.read(&client, hit.blob_id).await {
        Ok(stream) => {
            let Path((cache, _)) = &path;
            state.metrics.served(cache, hit.size);
            (object_headers(&hit), Body::from_stream(stream)).into_response()
        }
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to read an object");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Returns the headers of an object, from the fallback cache if needed.
async fn head_object(State(state): State<Objects>, path: Path<(String, String)>) -> Response {
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    match state.fetch(&client, &path).await {
        Ok(hit) => object_headers(&hit).into_response(),
        Err(status) => status.into_response(),
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
    let Some(token) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    let grant = match state.key.verify(token) {
        Ok(grant) => grant,
        Err(err) => {
            tracing::info!(error = &err as &dyn Error, "rejected a write token");
            return StatusCode::UNAUTHORIZED;
        }
    };
    if !grant.allows(&cache) {
        tracing::info!(sub = grant.sub, cache, "rejected a write outside the grant");
        return StatusCode::FORBIDDEN;
    }
    let Some(found) = state.caches.get(&cache) else {
        return StatusCode::NOT_FOUND;
    };
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
            state.metrics.upload(&cache, Upload::Stored);
            state.metrics.stored(&cache, size);
            StatusCode::CREATED
        }
        Ok(Stored::Existing) => {
            state.metrics.upload(&cache, Upload::Existing);
            StatusCode::NO_CONTENT
        }
        Err(err @ PutError::Write(WriteError::Body(_))) => {
            state.metrics.upload(&cache, Upload::Aborted);
            tracing::info!(error = &err as &dyn Error, cache, key, "upload aborted");
            StatusCode::BAD_REQUEST
        }
        Err(err) => {
            state.metrics.upload(&cache, Upload::Error);
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
    if let Ok(value) = HeaderValue::from_str(&format!("\"{}\"", hex::encode(&hit.hash))) {
        headers.insert(ETAG, value);
    }
    headers
}
