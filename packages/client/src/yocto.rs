//! Uploading Yocto shared state and downloads.

use core::time::Duration;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use clap::{Args, Subcommand};
use futures::{StreamExt as _, TryStreamExt as _, stream};
use reqwest::{Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::task::{self, JoinError};
use tokio_util::io::ReaderStream;
use url::Url;

use crate::target::Target;

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
enum Kind {
    /// `SSTATE_DIR`, with files in subdirectories.
    Sstate,
    /// `DL_DIR`, where mirrors serve the top-level files.
    Downloads,
}

impl Kind {
    /// Whether a file belongs in the cache.
    fn wants(self, name: &str) -> bool {
        match self {
            Self::Sstate => {
                name.starts_with("sstate:")
                    && SSTATE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
            }
            Self::Downloads => {
                !name.starts_with('.')
                    && !LOCAL_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
            }
        }
    }
}

/// The body of a missing-keys request.
#[derive(Debug, Serialize)]
struct MissingRequest<'a> {
    /// The keys to ask about.
    keys: &'a [&'a str],
}

/// The answer to a missing-keys request.
#[derive(Debug, Deserialize)]
struct Missing {
    /// The keys that the cache lacks.
    missing: Vec<String>,
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
        let dir = args.dir.clone();
        let files = task::spawn_blocking(move || collect(&dir, kind, since))
            .await
            .map_err(FilesError::Task)?
            .map_err(FilesError::Walk)?;
        let http = Client::new();
        let cache = args.target.cache();
        let missing_url = args.target.url(["_api", "files", cache, "missing"]);
        let mut missing = HashSet::new();
        for batch in files.chunks(MISSING_BATCH) {
            let keys: Vec<&str> = batch.iter().map(|(key, _)| key.as_str()).collect();
            let answer: Missing = http
                .post(missing_url.clone())
                .bearer_auth(args.target.token())
                .json(&MissingRequest { keys: &keys })
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(FilesError::Missing)?
                .json()
                .await
                .map_err(FilesError::Missing)?;
            missing.extend(answer.missing);
        }
        tracing::info!(
            files = files.len(),
            missing = missing.len(),
            "checked the cache"
        );
        let base = args.target.url([cache]);
        let uploads = files
            .iter()
            .filter(|(key, _)| missing.contains(key))
            .map(|(key, path)| upload(&http, &base, args.target.token(), key, path));
        stream::iter(uploads)
            .buffer_unordered(args.target.jobs())
            .try_collect::<Vec<()>>()
            .await?;
        Ok(())
    }
}

/// Errors from pushing files.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FilesError {
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
    /// Listing the files failed to run.
    #[error("the file listing failed")]
    Task(#[source] JoinError),
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

/// Uploads one file.
///
/// `base` is the URL of the cache.
async fn upload(
    http: &Client,
    base: &Url,
    token: &str,
    key: &str,
    path: &Path,
) -> Result<(), FilesError> {
    let mut url = base.clone();
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.extend(key.split('/'));
    }
    let file = File::open(path).await.map_err(|source| FilesError::Open {
        path: path.to_owned(),
        source,
    })?;
    let response = http
        .put(url)
        .bearer_auth(token)
        .body(Body::wrap_stream(ReaderStream::new(file)))
        .send()
        .await
        .map_err(|source| FilesError::Upload {
            key: key.to_owned(),
            source,
        })?;
    match response.status() {
        StatusCode::CREATED => tracing::info!(key, "uploaded"),
        StatusCode::NO_CONTENT => tracing::info!(key, "already cached"),
        status => {
            return Err(FilesError::Rejected {
                key: key.to_owned(),
                status,
            });
        }
    }
    Ok(())
}

/// Lists the files that belong in the cache, as keys and paths.
fn collect(
    dir: &Path,
    kind: Kind,
    since: Option<SystemTime>,
) -> io::Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_owned(), String::new())];
    while let Some((current, prefix)) = pending.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                if kind == Kind::Sstate {
                    let mut key = format!("{prefix}{name}");
                    key.push('/');
                    pending.push((entry.path(), key));
                }
                continue;
            }
            let recent =
                since.is_none_or(|since| metadata.modified().is_ok_and(|time| time >= since));
            if metadata.is_file() && recent && kind.wants(&name) {
                files.push((format!("{prefix}{name}"), entry.path()));
            }
        }
    }
    files.sort();
    Ok(files)
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
        for name in [
            "zlib-1.3.1.tar.xz.done",
            "zlib-1.3.1.tar.xz.lock",
            ".hidden",
        ] {
            assert!(!Kind::Downloads.wants(name), "{name} is skipped");
        }
    }
}
