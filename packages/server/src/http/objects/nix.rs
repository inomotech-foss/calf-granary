//! The Nix binary cache protocol over HTTP.

use core::error::Error;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use serde::{Deserialize, Serialize};

use super::Objects;
use super::metrics::{Lookup, Upload};
use crate::access::Accessed;
use crate::cache::{Cache, NixCache, NixPutError, ObjectKey, Protocol, Stored};
use crate::content::{Encoding, WriteError};
use crate::nix::{NarInfo, PathInfo, STORE_DIR, StorePath, StorePathHash, parse_sha256};

/// The header that carries the metadata of an uploaded NAR as JSON.
const NAR_INFO_HEADER: &str = "x-granary-nar-info";

/// The most hashes one missing-paths request may ask about.
const MAX_HASHES: usize = 10_000;

/// The metadata of an uploaded NAR.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadInfo {
    /// The full store path.
    store_path: String,
    /// The full store paths that the path references.
    references: Vec<String>,
    /// The full store path of the derivation that built the path.
    deriver: Option<String>,
    /// The system that built the path.
    system: Option<String>,
    /// The content address of a content-addressed path.
    ca: Option<String>,
    /// The SHA-256 of the NAR, in any format that Nix prints.
    nar_hash: String,
    /// The size of the NAR in bytes.
    nar_size: u64,
}

impl UploadInfo {
    /// Validates the metadata.
    fn into_path_info(self) -> Option<PathInfo> {
        let parse = |path: &str| StorePath::try_from(path).ok();
        Some(PathInfo {
            store_path: parse(&self.store_path)?,
            references: self
                .references
                .iter()
                .map(|reference| parse(reference))
                .collect::<Option<_>>()?,
            deriver: match self.deriver {
                Some(deriver) => Some(parse(&deriver)?),
                None => None,
            },
            system: self.system,
            ca: self.ca,
            nar_hash: parse_sha256(&self.nar_hash)?,
            nar_size: self.nar_size,
        })
    }
}

/// A question about which store paths a cache lacks.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MissingRequest {
    /// The hash parts of the store paths.
    hashes: Vec<String>,
}

/// The store paths that a cache lacks.
#[derive(Debug, Serialize)]
pub(super) struct MissingResponse {
    /// The hash parts of the missing store paths.
    missing: Vec<String>,
}

/// Answers a read from a Nix cache.
pub(super) async fn get(
    state: &Objects,
    cache: &Cache,
    nix: &NixCache,
    key: &str,
    head: bool,
) -> Response {
    if key == "nix-cache-info" {
        let text = format!(
            "StoreDir: {STORE_DIR}\nWantMassQuery: 1\nPriority: {}\n",
            nix.priority
        );
        return text_response("text/x-nix-cache-info", text, head);
    }
    if let Some(hash) = key.strip_suffix(".narinfo") {
        return narinfo(state, cache, nix, hash, head).await;
    }
    if let Some(hash) = key
        .strip_prefix("nar/")
        .and_then(|file| file.strip_suffix(".nar.zst"))
    {
        return nar(state, cache, hash, head).await;
    }
    StatusCode::NOT_FOUND.into_response()
}

/// Serves the signed narinfo of a store path.
async fn narinfo(
    state: &Objects,
    cache: &Cache,
    nix: &NixCache,
    hash: &str,
    head: bool,
) -> Response {
    let (Ok(hash), Ok(key)) = (StorePathHash::try_from(hash), ObjectKey::try_from(hash)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let name = cache.name().as_ref();
    match cache.find_nix_path(&client, &hash).await {
        Ok(Some(hit)) => {
            let lookup = if &hit.cache == cache.name() {
                Lookup::Hit
            } else {
                Lookup::Fallback
            };
            state.metrics.lookup(name, lookup);
            state.access.record(Accessed {
                cache: hit.cache.clone(),
                key,
            });
            let info = NarInfo {
                path: hit.info,
                url: format!("nar/{}.nar.zst", hash.as_ref()),
                compression: "zstd",
                file_size: hit.file_size,
            };
            text_response("text/x-nix-narinfo", info.render(&nix.key), head)
        }
        Ok(None) => {
            state.metrics.lookup(name, Lookup::Miss);
            StatusCode::NOT_FOUND.into_response()
        }
        Err(err) => {
            state.metrics.lookup(name, Lookup::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache = name,
                "failed to look up a store path"
            );
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// Streams the NAR of a store path as zstd frames.
async fn nar(state: &Objects, cache: &Cache, hash: &str, head: bool) -> Response {
    let Ok(key) = ObjectKey::try_from(hash) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let hit = match cache.get(&client, &key).await {
        Ok(Some(hit)) => hit,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to look up a NAR");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    state.access.record(Accessed {
        cache: hit.cache.clone(),
        key,
    });
    let content_type = HeaderValue::from_static("application/x-nix-nar");
    if head {
        return ([(CONTENT_TYPE, content_type)], ()).into_response();
    }
    match state
        .content
        .read(&client, hit.blob_id, Encoding::Zstd)
        .await
    {
        Ok(stream) => {
            state.metrics.served(cache.name().as_ref(), hit.size);
            ([(CONTENT_TYPE, content_type)], Body::from_stream(stream)).into_response()
        }
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to read a NAR");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Stores a store path from its uncompressed NAR.
///
/// The metadata is JSON in the `X-Granary-Nar-Info` header. Answers `201
/// Created` for new paths and `204 No Content` for existing ones.
pub(super) async fn upload(
    State(state): State<Objects>,
    Path(cache): Path<String>,
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
    if !matches!(found.protocol(), Protocol::Nix(_)) {
        return StatusCode::NOT_FOUND;
    }
    let info = headers
        .get(NAR_INFO_HEADER)
        .and_then(|value| serde_json::from_slice::<UploadInfo>(value.as_bytes()).ok())
        .and_then(UploadInfo::into_path_info);
    let Some(info) = info else {
        return StatusCode::BAD_REQUEST;
    };
    let path = info.store_path.to_string();
    match found
        .put_nix_path(&state.content, &state.pool, &info, body.into_data_stream())
        .await
    {
        Ok(Stored::New { size }) => {
            tracing::debug!(sub = grant.sub, cache, path, size, "stored a store path");
            state.metrics.upload(&cache, Upload::Stored);
            state.metrics.stored(&cache, size);
            StatusCode::CREATED
        }
        Ok(Stored::Existing) => {
            state.metrics.upload(&cache, Upload::Existing);
            StatusCode::NO_CONTENT
        }
        Err(err @ (NixPutError::Mismatch | NixPutError::Write(WriteError::Body(_)))) => {
            state.metrics.upload(&cache, Upload::Aborted);
            tracing::info!(error = &err as &dyn Error, cache, path, "rejected a NAR");
            StatusCode::BAD_REQUEST
        }
        Err(err) => {
            state.metrics.upload(&cache, Upload::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache,
                path,
                "failed to store a store path"
            );
            StatusCode::BAD_GATEWAY
        }
    }
}

/// Serves the public key of a Nix cache, for `trusted-public-keys`.
pub(super) async fn public_key(
    State(state): State<Objects>,
    Path(cache): Path<String>,
) -> Response {
    match state.caches.get(&cache).map(Cache::protocol) {
        Some(Protocol::Nix(nix)) => text_response("text/plain", nix.key.public_key(), false),
        Some(Protocol::Files) | None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Answers which store paths neither the cache nor its fallback has.
pub(super) async fn missing(
    State(state): State<Objects>,
    Path(cache): Path<String>,
    Json(request): Json<MissingRequest>,
) -> Response {
    let Some(found) = state.caches.get(&cache) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !matches!(found.protocol(), Protocol::Nix(_)) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if request.hashes.len() > MAX_HASHES {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Ok(hashes) = request
        .hashes
        .iter()
        .map(|hash| StorePathHash::try_from(hash.as_str()))
        .collect::<Result<Vec<_>, _>>()
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    match found.missing_nix_paths(&client, &hashes).await {
        Ok(missing) => Json(MissingResponse { missing }).into_response(),
        Err(err) => {
            tracing::error!(
                error = &err as &dyn Error,
                "failed to look up missing paths"
            );
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// A text response, without the body for HEAD requests.
fn text_response(content_type: &'static str, text: String, head: bool) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(CONTENT_LENGTH, HeaderValue::from(text.len()));
    if head {
        (headers, ()).into_response()
    } else {
        (headers, text).into_response()
    }
}
