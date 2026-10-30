//! Uploading Yocto shared state and downloads.

use core::time::Duration;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io};

use clap::{Args, Subcommand};
use futures::{StreamExt as _, TryStreamExt as _, stream};
use reqwest::{Body, Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio::task;
use tokio_util::io::ReaderStream;
use url::Url;

use crate::target::{Connection, Target};

/// How many keys one missing-keys request asks about.
const MISSING_BATCH: usize = 1000;

/// The endings of sstate files that mirrors serve. Others are temporary.
const SSTATE_SUFFIXES: [&str; 4] = [".tar.zst", ".tgz", ".siginfo", ".sig"];

/// The endings of download files that only matter locally.
const LOCAL_SUFFIXES: [&str; 3] = [".done", ".lock", ".tmp"];

/// How long after a directory changes its listing can still miss entries,
/// because file systems store modification times with limited precision.
const RACY: Duration = Duration::from_secs(2);

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
        let mut scanner = Scanner::new(args.dir.clone(), kind, since);
        let files = task::block_in_place(|| scanner.scan()).map_err(FilesError::Walk)?;
        push(args.target.connection(), args.target.cache(), &files).await
    }
}

/// A file that belongs in a cache.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    /// The key in the cache.
    pub(crate) key: String,
    /// The local file.
    pub(crate) path: PathBuf,
    /// When the file was last modified.
    pub(crate) modified: SystemTime,
}

/// Finds the files in a directory that belong in a cache. It remembers what
/// it read, so later scans only list the directories that changed.
#[derive(Debug)]
pub(crate) struct Scanner {
    /// The directory, such as `SSTATE_DIR` or `DL_DIR`.
    root: PathBuf,
    /// What the directory holds.
    kind: Kind,
    /// Only files modified at or after this time are found.
    since: Option<SystemTime>,
    /// What the last scan read, by directory.
    listings: HashMap<PathBuf, Listing>,
}

/// What a scan read from one directory.
#[derive(Debug)]
struct Listing {
    /// The modification time of the directory.
    modified: SystemTime,
    /// When the scan started to read the directory.
    read_at: SystemTime,
    /// The subdirectories, with the key prefix of their files.
    subdirs: Vec<(PathBuf, String)>,
    /// The files that belong in the cache.
    files: Vec<Entry>,
}

impl Scanner {
    /// Creates a scanner that hasn't read anything yet.
    pub(crate) fn new(root: PathBuf, kind: Kind, since: Option<SystemTime>) -> Self {
        Self {
            root,
            kind,
            since,
            listings: HashMap::new(),
        }
    }

    /// Reads one directory.
    fn read(&self, dir: &Path, prefix: &str, modified: SystemTime) -> io::Result<Listing> {
        let mut listing = Listing {
            modified,
            read_at: SystemTime::now(),
            subdirs: Vec::new(),
            files: Vec::new(),
        };
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                if self.kind == Kind::Sstate {
                    listing
                        .subdirs
                        .push((entry.path(), format!("{prefix}{name}/")));
                }
            } else if metadata.is_file() && self.kind.wants(&name) {
                listing.files.push(Entry {
                    key: format!("{prefix}{name}"),
                    path: entry.path(),
                    modified: metadata.modified()?,
                });
            }
        }
        Ok(listing)
    }

    /// Lists the files that belong in the cache, sorted by key.
    pub(crate) fn scan(&mut self) -> io::Result<Vec<Entry>> {
        let mut listings = HashMap::new();
        let mut files = Vec::new();
        let mut pending = vec![(self.root.clone(), String::new())];
        while let Some((dir, prefix)) = pending.pop() {
            let modified = fs::metadata(&dir)?.modified()?;
            let listing = match self.listings.remove(&dir) {
                Some(listing) if listing.is_current(modified) => listing,
                _ => self.read(&dir, &prefix, modified)?,
            };
            pending.extend(listing.subdirs.iter().cloned());
            files.extend(
                listing
                    .files
                    .iter()
                    .filter(|file| self.since.is_none_or(|since| file.modified >= since))
                    .cloned(),
            );
            listings.insert(dir, listing);
        }
        self.listings = listings;
        files.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(files)
    }
}

impl Listing {
    /// Whether the directory is unchanged since it was read.
    fn is_current(&self, modified: SystemTime) -> bool {
        modified == self.modified
            && self
                .modified
                .checked_add(RACY)
                .is_some_and(|settled| settled < self.read_at)
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

/// Uploads the files that `cache` lacks.
pub(crate) async fn push(
    connection: &Connection,
    cache: &str,
    files: &[Entry],
) -> Result<(), FilesError> {
    let http = Client::new();
    let missing_url = connection.url(["_api", "files", cache, "missing"]);
    let mut missing = HashSet::new();
    for batch in files.chunks(MISSING_BATCH) {
        let keys: Vec<&str> = batch.iter().map(|file| file.key.as_str()).collect();
        let answer: Missing = http
            .post(missing_url.clone())
            .bearer_auth(connection.token())
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
    let base = connection.url([cache]);
    let uploads = files
        .iter()
        .filter(|file| missing.contains(&file.key))
        .map(|file| upload(&http, &base, connection.token(), &file.key, &file.path));
    stream::iter(uploads)
        .buffer_unordered(connection.jobs())
        .try_collect::<Vec<()>>()
        .await?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::{env, process};

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

    /// Sets the modification time of a file or directory.
    fn touch(path: &Path, modified: SystemTime) {
        fs::File::open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    #[test]
    fn scans_changed_directories() {
        let root = env::temp_dir().join(format!("granary-scan-{}", process::id()));
        let leaf = root.join("ab/cd");
        fs::create_dir_all(&leaf).unwrap();
        let first = "sstate:a:x:1:r0:x:11:abcd_package.tar.zst";
        let second = "sstate:b:x:1:r0:x:11:abcd_package.tar.zst";
        fs::write(leaf.join(first), "").unwrap();
        fs::write(leaf.join(format!("{second}.Xa8bQ2")), "").unwrap();
        let past = SystemTime::now() - Duration::from_hours(1);
        for dir in [&root, &root.join("ab"), &leaf] {
            touch(dir, past);
        }
        let keys = |scanner: &mut Scanner| -> Vec<String> {
            scanner
                .scan()
                .unwrap()
                .into_iter()
                .map(|file| file.key)
                .collect()
        };
        let mut scanner = Scanner::new(root.clone(), Kind::Sstate, None);
        assert_eq!(
            keys(&mut scanner),
            [format!("ab/cd/{first}")],
            "finds sstate"
        );

        fs::rename(leaf.join(format!("{second}.Xa8bQ2")), leaf.join(second)).unwrap();
        touch(&leaf, past);
        assert_eq!(
            keys(&mut scanner),
            [format!("ab/cd/{first}")],
            "unchanged directories aren't read again"
        );
        touch(&leaf, past + Duration::from_secs(1));
        assert_eq!(
            keys(&mut scanner),
            [format!("ab/cd/{first}"), format!("ab/cd/{second}")],
            "changed directories are read again"
        );

        let mut recent = Scanner::new(root.clone(), Kind::Sstate, Some(SystemTime::now()));
        touch(
            &leaf.join(second),
            SystemTime::now() + Duration::from_secs(1),
        );
        assert_eq!(
            keys(&mut recent),
            [format!("ab/cd/{second}")],
            "old files are skipped"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
