//! Uploading Yocto shared state and downloads.

use core::error::Error;
use core::time::Duration;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use clap::{Args, Subcommand};
use futures::{FutureExt as _, StreamExt as _, TryStreamExt as _, stream};
use reqwest::{Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::task;
use tokio_util::io::ReaderStream;
use url::Url;

use crate::scan::{self, Scanner};
use crate::target::{Connection, Target};

/// How many keys one missing-keys request asks about.
const MISSING_BATCH: usize = 1000;

/// The endings of sstate files that mirrors serve. Others are temporary.
const SSTATE_SUFFIXES: [&str; 4] = [".tar.zst", ".tgz", ".siginfo", ".sig"];

/// The endings of download files that only matter locally.
const LOCAL_SUFFIXES: [&str; 3] = [".done", ".lock", ".tmp"];

/// A Yocto command.
#[derive(Debug, Subcommand)]
pub(crate) enum YoctoCommand {
    /// Upload the sstate files that the cache lacks.
    PushSstate(FilesArgs),
    /// Upload the downloaded sources that the cache lacks.
    PushDownloads(FilesArgs),
}

/// Command-line arguments of the push commands.
#[derive(Debug, Args)]
pub(crate) struct FilesArgs {
    /// The cache.
    #[command(flatten)]
    target: Target,
    /// The directory, such as `SSTATE_DIR` or `DL_DIR`.
    #[arg(long)]
    dir: PathBuf,
    /// Only consider files modified at or after this Unix time, such as the
    /// start of the build.
    #[arg(long)]
    since: Option<u64>,
}

/// What a directory holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `SSTATE_DIR`, with files in subdirectories.
    Sstate,
    /// `DL_DIR`, where mirrors serve the top-level files.
    Downloads,
}

impl Kind {
    /// Whether a file belongs in the cache.
    pub(crate) fn wants(self, name: &str) -> bool {
        match self {
            Self::Sstate => {
                name.starts_with("sstate:")
                    && SSTATE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
            }
            Self::Downloads => {
                !name.starts_with('.')
                    && !LOCAL_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
                    && !is_temporary(name)
            }
        }
    }
}

/// The body of a missing-keys request.
#[derive(Debug, Serialize)]
struct MissingRequest<'a> {
    /// The keys to ask about.
    keys: &'a [String],
}

/// The answer to a missing-keys request.
#[derive(Debug, Deserialize)]
struct Missing {
    /// The keys that the cache lacks.
    missing: HashSet<String>,
}

impl YoctoCommand {
    /// Uploads the files that the cache lacks.
    pub(crate) async fn run(self) -> Result<(), FilesError> {
        let (args, kind) = match self {
            Self::PushSstate(args) => (args, Kind::Sstate),
            Self::PushDownloads(args) => (args, Kind::Downloads),
        };
        let since = args
            .since
            .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));
        let mut scanner = Scanner::new(args.dir, kind, since);
        task::block_in_place(|| scanner.scan()).map_err(FilesError::Walk)?;
        push(
            args.target.connection(),
            args.target.cache(),
            scanner.files(),
        )
        .await
    }
}

/// Errors from pushing files.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FilesError {
    /// Some files weren't uploaded.
    #[error("failed to upload {0} files")]
    Failed(usize),
    /// Asking which files are missing failed.
    #[error("failed to ask the cache which files it lacks")]
    Missing(#[source] reqwest::Error),
    /// A file can't be opened.
    #[error("failed to open {}", path.display())]
    Open {
        /// The file.
        path: PathBuf,
        /// The error from the operating system.
        #[source]
        source: io::Error,
    },
    /// The cache rejected an upload.
    #[error("the cache rejected {key} with {status}")]
    Rejected {
        /// The key.
        key: String,
        /// The response status.
        status: StatusCode,
    },
    /// Sending an upload failed.
    #[error("failed to upload {key}")]
    Upload {
        /// The key.
        key: String,
        /// The error from the HTTP client.
        #[source]
        source: reqwest::Error,
    },
    /// Listing the files failed.
    #[error("failed to list the files")]
    Walk(#[source] io::Error),
}

/// Whether a name is one that Python's `tempfile.mkstemp` picks. Git
/// fetches write mirror tarballs to such a file before renaming it.
fn is_temporary(name: &str) -> bool {
    name.strip_prefix("tmp").is_some_and(|random| {
        random.len() == 8
            && random
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    })
}

/// Asks the cache which files of a batch it lacks, and returns those with
/// their keys. Marks the others as cached.
async fn missing<'a>(
    http: &Client,
    url: &Url,
    token: &str,
    batch: Vec<scan::File<'a>>,
) -> Result<Vec<(scan::File<'a>, String)>, FilesError> {
    let keys: Vec<String> = batch.iter().map(|file| file.key()).collect();
    let answer: Missing = http
        .post(url.clone())
        .bearer_auth(token)
        .json(&MissingRequest { keys: &keys })
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(FilesError::Missing)?
        .json()
        .await
        .map_err(FilesError::Missing)?;
    tracing::info!(
        files = batch.len(),
        missing = answer.missing.len(),
        "checked the cache"
    );
    Ok(batch
        .into_iter()
        .zip(keys)
        .filter(|(file, key)| {
            let missing = answer.missing.contains(key);
            file.set_cached(!missing);
            missing
        })
        .collect())
}

/// Uploads the files that `cache` lacks and marks them as cached. A file
/// that fails is logged and skipped, so it doesn't hold up the others.
pub(crate) async fn push<'a, I>(
    connection: &Connection,
    cache: &str,
    files: I,
) -> Result<(), FilesError>
where
    I: IntoIterator<Item = scan::File<'a>, IntoIter: Send>,
{
    let http = Client::new();
    let missing_url = connection.url(["_api", "files", cache, "missing"]);
    let base = connection.url([cache]);
    let failed = stream::iter(files)
        .chunks(MISSING_BATCH)
        .then(|batch| missing(&http, &missing_url, connection.token(), batch))
        .map_ok(|batch| stream::iter(batch).map(Ok))
        .try_flatten()
        .map_ok(|(file, key)| upload(&http, &base, connection.token(), file, key).map(Ok))
        .try_buffer_unordered(connection.jobs())
        .try_fold(0usize, async |failed, uploaded: Result<(), FilesError>| {
            Ok(match uploaded {
                Ok(()) => failed,
                Err(err) => {
                    tracing::warn!(error = &err as &dyn Error, "upload failed");
                    failed.saturating_add(1)
                }
            })
        })
        .await?;
    if failed > 0 {
        return Err(FilesError::Failed(failed));
    }
    Ok(())
}

/// Uploads one file.
///
/// `base` is the URL of the cache.
async fn upload(
    http: &Client,
    base: &Url,
    token: &str,
    file: scan::File<'_>,
    key: String,
) -> Result<(), FilesError> {
    let mut url = base.clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.extend(key.split('/'));
    }
    let path = file.path();
    let contents = File::open(&path)
        .await
        .map_err(|source| FilesError::Open { path, source })?;
    let response = http
        .put(url)
        .bearer_auth(token)
        .body(Body::wrap_stream(ReaderStream::new(contents)))
        .send()
        .await
        .map_err(|source| FilesError::Upload {
            key: key.clone(),
            source,
        })?;
    match response.status() {
        StatusCode::CREATED => tracing::info!(key, "uploaded"),
        StatusCode::NO_CONTENT => tracing::info!(key, "already cached"),
        status => return Err(FilesError::Rejected { key, status }),
    }
    file.set_cached(true);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sstate_files() {
        for name in [
            "sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:abcd_populate_sysroot.tar.zst",
            "sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:abcd_populate_sysroot.tar.zst.\
             siginfo",
        ] {
            assert!(Kind::Sstate.wants(name), "{name} is uploaded");
        }
        for name in [
            "sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:abcd_package.tar.zst.Xa8bQ2",
            "index-x86_64",
        ] {
            assert!(!Kind::Sstate.wants(name), "{name} is skipped");
        }
    }

    #[test]
    fn download_files() {
        assert!(
            Kind::Downloads.wants("zlib-1.3.1.tar.xz"),
            "sources are uploaded"
        );
        assert!(
            Kind::Downloads.wants("git2_github.com.madler.zlib.git.tar.gz"),
            "mirror tarballs are uploaded"
        );
        assert!(
            Kind::Downloads.wants("tmpwatch-2.11.tar.bz2"),
            "only temporary names are skipped"
        );
        for name in [
            "zlib-1.3.1.tar.xz.done",
            "zlib-1.3.1.tar.xz.lock",
            ".hidden",
            "tmpa8b_q2z0",
        ] {
            assert!(!Kind::Downloads.wants(name), "{name} is skipped");
        }
    }
}
