//! The Nix binary cache protocol over HTTP.

use core::error::Error;
use core::str;
use std::io;

use async_compression::tokio::bufread::XzEncoder;
use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse as _, Response};
use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt as _, TryStreamExt as _, stream};
use serde::{Deserialize, Serialize};
use tokio_util::io::{ReaderStream, StreamReader};

use super::Objects;
use super::metrics::{Lookup, Upload};
use crate::access::Accessed;
use crate::cache::{Cache, NarCompression, NixCache, NixPutError, ObjectKey, Protocol, Stored};
use crate::content::{Encoding, WriteError};
use crate::nix::{NarInfo, PathInfo, StorePath, StorePathHash, UploadedNarInfo, parse_sha256};

/// The largest narinfo that `nix copy` may upload, and the largest metadata
/// of an upload.
const MAX_NARINFO_SIZE: usize = 1024 * 1024;

/// The usual size of a rendered narinfo, which most fit in.
const NARINFO_CAPACITY: usize = 1024;

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
    /// Reads the metadata line at the start of an upload body.
    ///
    /// Returns the metadata and the bytes after the line, which start the
    /// NAR.
    async fn read<S>(body: &mut S) -> Option<(Self, Bytes)>
    where
        S: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
    {
        let mut buffer = BytesMut::new();
        loop {
            let chunk = body.try_next().await.ok()??;
            let start = buffer.len();
            buffer.extend_from_slice(&chunk);
            let newline = buffer.get(start..)?.iter().position(|byte| *byte == b'\n');
            if let Some(newline) = newline {
                let line = buffer.split_to(start.saturating_add(newline).saturating_add(1));
                let info =
                    serde_json::from_slice(line.get(..line.len().saturating_sub(1))?).ok()?;
                return Some((info, buffer.freeze()));
            }
            if buffer.len() > MAX_NARINFO_SIZE {
                return None;
            }
        }
    }

    /// Validates the metadata.
    fn into_path_info(self) -> Option<PathInfo> {
        let parse = |path: String| StorePath::try_from(path).ok();
        Some(PathInfo {
            store_path: parse(self.store_path)?,
            references: self
                .references
                .into_iter()
                .map(parse)
                .collect::<Option<_>>()?,
            deriver: match self.deriver {
                Some(deriver) => Some(parse(deriver)?),
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
        return text_response("text/x-nix-cache-info", nix.cache_info(), head);
    }
    if let Some(hash) = key.strip_suffix(".narinfo") {
        return narinfo(state, cache, nix, hash, head).await;
    }
    if let Some(hash) = key
        .strip_prefix("nar/")
        .and_then(|file| file.strip_suffix(".nar.zst"))
        .and_then(|hash| StorePathHash::try_from(hash).ok())
    {
        return nar(state, cache, &hash, head).await;
    }
    if let Some(compression) = key.strip_prefix("nar/").and_then(NarCompression::of_file) {
        return nar_file(state, cache, key, compression, head).await;
    }
    super::serve_file(state, cache, key, head).await
}

/// Serves a NAR file that `nix copy` uploaded, compressed as its name says.
///
/// Nix keeps the narinfos it uploads and may download from their URLs
/// instead of the ones the cache serves.
async fn nar_file(
    state: &Objects,
    cache: &Cache,
    key: &str,
    compression: NarCompression,
    head: bool,
) -> Response {
    let Ok(object) = ObjectKey::try_from(key.to_owned()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let hit = match cache.get(&client, &object).await {
        Ok(Some(hit)) => hit,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to look up a NAR file");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let content_type = HeaderValue::from_static("application/x-nix-nar");
    if head {
        return ([(CONTENT_TYPE, content_type)], ()).into_response();
    }
    let encoding = match compression {
        NarCompression::Zstd => Encoding::Zstd,
        NarCompression::None | NarCompression::Xz => Encoding::Plain,
    };
    let stream = match state.content.read(&client, hit.blob_id, encoding).await {
        Ok(stream) => stream,
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to read a NAR file");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let body = if compression == NarCompression::Xz {
        let plain = StreamReader::new(stream.map_err(io::Error::other));
        Body::from_stream(ReaderStream::new(XzEncoder::new(plain)))
    } else {
        Body::from_stream(stream)
    };
    ([(CONTENT_TYPE, content_type)], body).into_response()
}

/// Serves the signed narinfo of a store path.
async fn narinfo(
    state: &Objects,
    cache: &Cache,
    nix: &NixCache,
    hash: &str,
    head: bool,
) -> Response {
    let Ok(hash) = StorePathHash::try_from(hash) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client = match state.client().await {
        Ok(client) => client,
        Err(status) => return status.into_response(),
    };
    let name = cache.name();
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
                key: ObjectKey::from(&hash),
            });
            let mut text = String::with_capacity(NARINFO_CAPACITY);
            let info = NarInfo {
                path: &hit.info,
                file_size: hit.file_size,
            };
            if info.render(&nix.key, &mut text).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            text_response("text/x-nix-narinfo", text, head)
        }
        Ok(None) => {
            state.metrics.lookup(name, Lookup::Miss);
            StatusCode::NOT_FOUND.into_response()
        }
        Err(err) => {
            state.metrics.lookup(name, Lookup::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache = name.as_ref(),
                "failed to look up a store path"
            );
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// Streams the NAR of a store path as zstd frames.
async fn nar(state: &Objects, cache: &Cache, hash: &StorePathHash, head: bool) -> Response {
    let key = ObjectKey::from(hash);
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
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/x-nix-nar"),
    );
    match state.content.zstd_size(&client, hit.blob_id).await {
        Ok(Some(size)) => {
            headers.insert(CONTENT_LENGTH, HeaderValue::from(size));
        }
        Ok(None) => {}
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to size a NAR");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    if head {
        return (headers, ()).into_response();
    }
    match state
        .content
        .read(&client, hit.blob_id, Encoding::Zstd)
        .await
    {
        Ok(stream) => {
            state.metrics.served(cache.name(), hit.size);
            (headers, Body::from_stream(stream)).into_response()
        }
        Err(err) => {
            tracing::error!(error = &err as &dyn Error, "failed to read a NAR");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Answers an upload from `nix copy` to a Nix cache.
///
/// NAR files come first and narinfos after them. Other files, such as build
/// logs, are kept as they are, except under the names of store paths. The
/// caller checked the token.
pub(super) async fn put(state: &Objects, cache: &Cache, key: &str, body: Body) -> StatusCode {
    let name = cache.name();
    let stored = if key == "nix-cache-info" {
        return StatusCode::NO_CONTENT;
    } else if key.ends_with(".narinfo") {
        let Ok(text) = to_bytes(body, MAX_NARINFO_SIZE).await else {
            return StatusCode::PAYLOAD_TOO_LARGE;
        };
        let Some(info) = str::from_utf8(&text).ok().and_then(UploadedNarInfo::parse) else {
            return StatusCode::BAD_REQUEST;
        };
        if key.strip_suffix(".narinfo") != Some(info.path.store_path.hash().as_ref()) {
            return StatusCode::BAD_REQUEST;
        }
        cache.put_narinfo(&state.pool, &info).await
    } else {
        if StorePathHash::try_from(key).is_ok() {
            return StatusCode::BAD_REQUEST;
        }
        let Ok(object) = ObjectKey::try_from(key.to_owned()) else {
            return StatusCode::BAD_REQUEST;
        };
        match key.strip_prefix("nar/").map(NarCompression::of_file) {
            Some(Some(compression)) => {
                cache
                    .put_nar_file(
                        &state.content,
                        &state.pool,
                        &object,
                        compression,
                        body.into_data_stream(),
                    )
                    .await
            }
            Some(None) => return StatusCode::UNSUPPORTED_MEDIA_TYPE,
            None => {
                return match cache
                    .put(
                        &state.content,
                        &state.pool,
                        &object,
                        body.into_data_stream(),
                    )
                    .await
                {
                    Ok(Stored::New { size: _ }) => StatusCode::CREATED,
                    Ok(Stored::Existing) => StatusCode::NO_CONTENT,
                    Err(err) => {
                        tracing::error!(
                            error = &err as &dyn Error,
                            cache = name.as_ref(),
                            key,
                            "failed to store a file"
                        );
                        StatusCode::BAD_GATEWAY
                    }
                };
            }
        }
    };
    match stored {
        Ok(Stored::New { size }) => {
            state.metrics.upload(name, Upload::Stored);
            state.metrics.stored(name, size);
            StatusCode::CREATED
        }
        Ok(Stored::Existing) => {
            state.metrics.upload(name, Upload::Existing);
            StatusCode::NO_CONTENT
        }
        Err(
            err @ (NixPutError::Mismatch
            | NixPutError::MissingNar
            | NixPutError::Write(WriteError::Body(_))),
        ) => {
            state.metrics.upload(name, Upload::Aborted);
            tracing::info!(
                error = &err as &dyn Error,
                cache = name.as_ref(),
                key,
                "rejected an upload"
            );
            StatusCode::BAD_REQUEST
        }
        Err(err) => {
            state.metrics.upload(name, Upload::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache = name.as_ref(),
                key,
                "failed to store an upload"
            );
            StatusCode::BAD_GATEWAY
        }
    }
}

/// Stores a store path from its uncompressed NAR.
///
/// The body is the metadata as one line of JSON, then the NAR. Answers `201
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
    let mut body = body.into_data_stream();
    let Some((info, start)) = UploadInfo::read(&mut body).await else {
        return StatusCode::BAD_REQUEST;
    };
    let Some(info) = info.into_path_info() else {
        return StatusCode::BAD_REQUEST;
    };
    let path = &info.store_path;
    let nar = stream::iter([Ok(start)]).chain(body);
    match found
        .put_nix_path(&state.content, &state.pool, &info, nar)
        .await
    {
        Ok(Stored::New { size }) => {
            tracing::debug!(sub = grant.sub, cache, %path, size, "stored a store path");
            state.metrics.upload(found.name(), Upload::Stored);
            state.metrics.stored(found.name(), size);
            StatusCode::CREATED
        }
        Ok(Stored::Existing) => {
            state.metrics.upload(found.name(), Upload::Existing);
            StatusCode::NO_CONTENT
        }
        Err(err @ (NixPutError::Mismatch | NixPutError::Write(WriteError::Body(_)))) => {
            state.metrics.upload(found.name(), Upload::Aborted);
            tracing::info!(error = &err as &dyn Error, cache, %path, "rejected a NAR");
            StatusCode::BAD_REQUEST
        }
        Err(err) => {
            state.metrics.upload(found.name(), Upload::Error);
            tracing::error!(
                error = &err as &dyn Error,
                cache,
                %path,
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
        Some(Protocol::Nix(nix)) => {
            text_response("text/plain", nix.key.public_key().to_owned(), false)
        }
        Some(Protocol::Files | Protocol::Hashserv) | None => StatusCode::NOT_FOUND.into_response(),
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
fn text_response<T>(content_type: &'static str, text: T, head: bool) -> Response
where
    T: Into<Bytes>,
{
    let text = text.into();
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(CONTENT_LENGTH, HeaderValue::from(text.len()));
    if head {
        (headers, ()).into_response()
    } else {
        (headers, text).into_response()
    }
}
