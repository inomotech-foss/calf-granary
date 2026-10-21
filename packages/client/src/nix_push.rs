//! The `nix push` command.

use clap::Args;
use futures::{StreamExt as _, TryStreamExt as _, stream};
use reqwest::{Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::io::ReaderStream;

use crate::nix::{self, NixError, PathInfo};
use crate::target::Target;

/// The header that carries the metadata of an uploaded NAR.
const NAR_INFO_HEADER: &str = "x-granary-nar-info";

/// Command-line arguments of `nix push`.
#[derive(Debug, Args)]
pub(crate) struct PushArgs {
    /// The Nix cache.
    #[command(flatten)]
    target: Target,
    /// The store paths to push, with everything they reference.
    #[arg(required = true)]
    paths: Vec<String>,
}

/// The answer to a missing-paths request.
#[derive(Debug, Deserialize)]
struct Missing {
    /// The hash parts of the store paths that the cache lacks.
    missing: Vec<String>,
}

/// The metadata of an uploaded NAR.
#[derive(Debug, Serialize)]
struct UploadInfo<'a> {
    /// The full store path.
    store_path: &'a str,
    /// The full store paths that the path references.
    references: &'a [String],
    /// The full store path of the derivation that built the path.
    deriver: Option<&'a str>,
    /// The system that built the path, which Nix doesn't report.
    system: Option<&'a str>,
    /// The content address of a content-addressed path.
    ca: Option<&'a str>,
    /// The SHA-256 of the NAR.
    nar_hash: &'a str,
    /// The size of the NAR.
    nar_size: u64,
}

impl PushArgs {
    /// Uploads the closure of the paths, except what the cache has.
    pub(crate) async fn run(self) -> Result<(), PushError> {
        let closure = nix::closure(&self.paths).await?;
        let hashes: Vec<&str> = closure
            .keys()
            .filter_map(|path| nix::hash_part(path))
            .collect();
        let http = Client::new();
        let missing: Missing = http
            .post(
                self.target
                    .url(&format!("_api/nix/{}/missing", self.target.cache())),
            )
            .json(&json!({ "hashes": hashes }))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(PushError::Missing)?
            .json()
            .await
            .map_err(PushError::Missing)?;
        tracing::info!(
            closure = closure.len(),
            missing = missing.missing.len(),
            "checked the cache"
        );
        let uploads = closure
            .iter()
            .filter(|(path, _)| {
                nix::hash_part(path).is_some_and(|hash| missing.missing.iter().any(|m| m == hash))
            })
            .map(|(path, info)| self.upload(&http, path, info));
        stream::iter(uploads)
            .buffer_unordered(self.target.jobs())
            .try_collect::<Vec<()>>()
            .await?;
        Ok(())
    }

    /// Uploads the NAR of one store path.
    async fn upload(&self, http: &Client, path: &str, info: &PathInfo) -> Result<(), PushError> {
        let metadata = UploadInfo {
            store_path: path,
            references: &info.references,
            deriver: info.deriver.as_deref(),
            system: None,
            ca: info.ca.as_deref(),
            nar_hash: &info.nar_hash,
            nar_size: info.nar_size,
        };
        let metadata = serde_json::to_string(&metadata).map_err(PushError::Metadata)?;
        let mut dump = nix::dump(path)?;
        let stdout = dump.stdout.take().ok_or(PushError::Dump)?;
        let response = http
            .put(
                self.target
                    .url(&format!("_api/nix/{}/upload", self.target.cache())),
            )
            .bearer_auth(self.target.token())
            .header(NAR_INFO_HEADER, metadata)
            .body(Body::wrap_stream(ReaderStream::new(stdout)))
            .send()
            .await
            .map_err(|source| PushError::Upload {
                path: path.to_owned(),
                source,
            })?;
        let dumped = dump
            .wait()
            .await
            .map_err(|err| PushError::Nix(NixError::Spawn(err)))?;
        if !dumped.success() {
            return Err(PushError::Dump);
        }
        match response.status() {
            StatusCode::CREATED => tracing::info!(path, "uploaded"),
            StatusCode::NO_CONTENT => tracing::info!(path, "already cached"),
            status => {
                return Err(PushError::Rejected {
                    path: path.to_owned(),
                    status,
                });
            }
        }
        Ok(())
    }
}

/// Errors from pushing store paths.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PushError {
    /// `nix store dump-path` failed.
    #[error("failed to dump a store path")]
    Dump,
    /// The upload metadata can't be encoded.
    #[error("failed to encode the upload metadata")]
    Metadata(#[source] serde_json::Error),
    /// Asking which paths are missing failed.
    #[error("failed to ask the cache which paths it lacks")]
    Missing(#[source] reqwest::Error),
    /// Nix failed.
    #[error(transparent)]
    Nix(#[from] NixError),
    /// The cache rejected an upload.
    #[error("the cache rejected {path} with {status}")]
    Rejected {
        /// The store path.
        path: String,
        /// The response status.
        status: StatusCode,
    },
    /// Sending an upload failed.
    #[error("failed to upload {path}")]
    Upload {
        /// The store path.
        path: String,
        /// The error from the HTTP client.
        #[source]
        source: reqwest::Error,
    },
}
