//! The `watch` command.

use core::error::Error;
use core::time::Duration;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use tokio::{task, time};

use crate::nix::{self, NixError};
use crate::nix_push::{self, PushError};
use crate::scan::Scanner;
use crate::target::Connection;
use crate::yocto::{self, FilesError, Kind};

/// Command-line arguments of `watch`.
#[derive(Debug, Args)]
pub(crate) struct WatchArgs {
    /// The server.
    #[command(flatten)]
    connection: Connection,
    /// Seconds between scans.
    #[arg(long, default_value_t = 120)]
    interval: u64,
    /// Seconds that a file must stay unmodified before it's uploaded, so
    /// that files still being written are skipped.
    #[arg(long, default_value_t = 60)]
    settle: u64,
    /// The Nix cache to push new store paths to.
    #[arg(long, requires = "known_paths")]
    nix_cache: Option<String>,
    /// A file that lists the store paths that existed before the job, one
    /// per line.
    #[arg(long)]
    known_paths: Option<PathBuf>,
    /// The cache to upload sstate files to.
    #[arg(long, requires = "sstate_dir")]
    sstate_cache: Option<String>,
    /// The `SSTATE_DIR`.
    #[arg(long)]
    sstate_dir: Option<PathBuf>,
    /// The cache to upload downloads to.
    #[arg(long, requires = "downloads_dir")]
    downloads_cache: Option<String>,
    /// The `DL_DIR`.
    #[arg(long)]
    downloads_dir: Option<PathBuf>,
    /// Only consider files modified at or after this Unix time, such as the
    /// start of the build.
    #[arg(long)]
    since: Option<u64>,
}

/// New store paths to push.
#[derive(Debug)]
struct NixWatch {
    /// The cache.
    cache: String,
    /// The store paths that were seen, and whether the cache has them.
    paths: HashMap<String, bool>,
}

/// New files in a directory to upload.
#[derive(Debug)]
struct FilesWatch {
    /// The cache.
    cache: String,
    /// The directory.
    scanner: Scanner,
}

impl WatchArgs {
    /// Uploads what's new every interval until the process is stopped.
    #[expect(clippy::infinite_loop, reason = "the action stops it with a signal")]
    pub(crate) async fn run(self) -> Result<(), WatchError> {
        let mut nix = match (self.nix_cache, self.known_paths) {
            (Some(cache), Some(file)) => Some(NixWatch {
                cache,
                paths: File::open(&file)
                    .and_then(|known| {
                        BufReader::new(known)
                            .lines()
                            .map(|path| Ok((path?, false)))
                            .collect()
                    })
                    .map_err(|source| WatchError::KnownPaths { file, source })?,
            }),
            _ => None,
        };
        let since = self
            .since
            .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));
        let mut files: Vec<FilesWatch> = [
            (self.sstate_cache, self.sstate_dir, Kind::Sstate),
            (self.downloads_cache, self.downloads_dir, Kind::Downloads),
        ]
        .into_iter()
        .filter_map(|(cache, dir, kind)| {
            Some(FilesWatch {
                cache: cache?,
                scanner: Scanner::new(dir?, kind, since),
            })
        })
        .collect();
        let settle = Duration::from_secs(self.settle);
        loop {
            time::sleep(Duration::from_secs(self.interval.max(1))).await;
            if let Some(nix) = &mut nix
                && let Err(err) = nix.pass(&self.connection).await
            {
                tracing::warn!(error = &err as &dyn Error, "pushing store paths failed");
            }
            for watch in &mut files {
                if let Err(err) = watch.pass(&self.connection, settle).await {
                    tracing::warn!(
                        error = &err as &dyn Error,
                        cache = watch.cache,
                        "uploading files failed"
                    );
                }
            }
        }
    }
}

impl NixWatch {
    /// Pushes the store paths that appeared since the last pass.
    async fn pass(&mut self, connection: &Connection) -> Result<(), WatchError> {
        let mut new = Vec::new();
        nix::each_path(|path| {
            let derivation = Path::new(path).extension().is_some_and(|ext| ext == "drv");
            if !derivation && !self.paths.contains_key(path) {
                new.push(path.to_owned());
            }
        })
        .await?;
        if new.is_empty() {
            return Ok(());
        }
        let closure = nix_push::push(connection, &self.cache, &new, |path| {
            self.paths.get(path).copied().unwrap_or(false)
        })
        .await?;
        self.paths.extend(new.into_iter().map(|path| (path, false)));
        for (path, _) in closure.iter() {
            if let Some(cached) = self.paths.get_mut(path) {
                *cached = true;
            }
        }
        Ok(())
    }
}

impl FilesWatch {
    /// Uploads the settled files that the cache doesn't have yet.
    async fn pass(&mut self, connection: &Connection, settle: Duration) -> Result<(), WatchError> {
        task::block_in_place(|| self.scanner.scan()).map_err(FilesError::Walk)?;
        let settled = SystemTime::now().checked_sub(settle).unwrap_or(UNIX_EPOCH);
        let new = self
            .scanner
            .files()
            .filter(|file| !file.is_cached() && file.modified <= settled);
        yocto::push(connection, &self.cache, new).await?;
        Ok(())
    }
}

/// Errors from watching.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WatchError {
    /// Uploading files failed.
    #[error(transparent)]
    Files(#[from] FilesError),
    /// The file of known store paths can't be read.
    #[error("failed to read {}", file.display())]
    KnownPaths {
        /// The file.
        file: PathBuf,
        /// The error from the operating system.
        #[source]
        source: io::Error,
    },
    /// Listing the store paths failed.
    #[error(transparent)]
    Nix(#[from] NixError),
    /// Pushing store paths failed.
    #[error(transparent)]
    Push(#[from] PushError),
}
