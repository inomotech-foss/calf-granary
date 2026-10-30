//! Finding the files in a directory tree that belong in a cache.

use core::mem;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::path::PathBuf;
use std::time::SystemTime;
use std::{fs, io, slice};

use crate::yocto::Kind;

/// How long after a directory changes its listing can still miss entries,
/// because file systems store modification times with limited precision.
const RACY: Duration = Duration::from_secs(2);

/// A file that belongs in a cache.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The key in the cache.
    pub(crate) key: String,
    /// The local file.
    pub(crate) path: PathBuf,
    /// When the file was last modified.
    pub(crate) modified: SystemTime,
    /// Whether the cache has the file.
    cached: AtomicBool,
}

/// Finds the files in a directory that belong in a cache. It keeps what it
/// read, so later scans only list the directories that changed.
#[derive(Debug)]
pub(crate) struct Scanner {
    /// The directory, such as `SSTATE_DIR` or `DL_DIR`.
    root: Dir,
    /// What the directory holds.
    kind: Kind,
    /// Only files modified at or after this time are found.
    since: Option<SystemTime>,
}

/// What a scan read from one directory.
#[derive(Debug)]
struct Dir {
    /// The directory.
    path: PathBuf,
    /// The key prefix of the files in the directory.
    prefix: String,
    /// The modification time of the directory when it was read.
    modified: Option<SystemTime>,
    /// When the scan started to read the directory.
    read_at: SystemTime,
    /// The subdirectories, sorted by path.
    subdirs: Vec<Self>,
    /// The files that belong in the cache, sorted by key.
    files: Vec<Entry>,
}

/// The files of a directory tree.
#[derive(Debug)]
struct Files<'a> {
    /// The files of the current directory.
    files: slice::Iter<'a, Entry>,
    /// The subdirectories still to visit, at each depth.
    dirs: Vec<slice::Iter<'a, Dir>>,
}

impl Entry {
    /// Whether the cache has the file.
    pub(crate) fn is_cached(&self) -> bool {
        self.cached.load(Ordering::Relaxed)
    }

    /// Records whether the cache has the file.
    pub(crate) fn set_cached(&self, cached: bool) {
        self.cached.store(cached, Ordering::Relaxed);
    }
}

impl Scanner {
    /// Lists the files that were found by the last scan.
    pub(crate) fn files(&self) -> impl Iterator<Item = &Entry> {
        Files {
            files: self.root.files.iter(),
            dirs: vec![self.root.subdirs.iter()],
        }
        .filter(|file| self.since.is_none_or(|since| file.modified >= since))
    }

    /// Creates a scanner that hasn't read anything yet.
    pub(crate) const fn new(root: PathBuf, kind: Kind, since: Option<SystemTime>) -> Self {
        Self {
            root: Dir::new(root, String::new()),
            kind,
            since,
        }
    }

    /// Reads the directories that changed since the last scan.
    pub(crate) fn scan(&mut self) -> io::Result<()> {
        self.root.scan(self.kind)
    }
}

impl Dir {
    /// Whether the directory is unchanged since it was read.
    fn is_current(&self, modified: SystemTime) -> bool {
        self.modified == Some(modified)
            && modified
                .checked_add(RACY)
                .is_some_and(|settled| settled < self.read_at)
    }

    /// A directory that wasn't read yet.
    const fn new(path: PathBuf, prefix: String) -> Self {
        Self {
            path,
            prefix,
            modified: None,
            read_at: SystemTime::UNIX_EPOCH,
            subdirs: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Lists the directory again. Subdirectories keep what they read, and
    /// unchanged files stay cached.
    fn read(&mut self, kind: Kind, modified: SystemTime) -> io::Result<()> {
        self.read_at = SystemTime::now();
        self.modified = Some(modified);
        let mut subdirs = Vec::new();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                if kind == Kind::Sstate {
                    subdirs.push((entry.path(), name));
                }
            } else if metadata.is_file() && kind.wants(&name) {
                files.push(Entry {
                    key: format!("{}{name}", self.prefix),
                    path: entry.path(),
                    modified: metadata.modified()?,
                    cached: AtomicBool::new(false),
                });
            }
        }

        subdirs.sort_unstable();
        let mut old_dirs = mem::take(&mut self.subdirs).into_iter().peekable();
        for (path, name) in subdirs {
            while old_dirs.next_if(|dir| dir.path < path).is_some() {}
            let dir = old_dirs
                .next_if(|dir| dir.path == path)
                .unwrap_or_else(|| Self::new(path, format!("{}{name}/", self.prefix)));
            self.subdirs.push(dir);
        }

        files.sort_unstable_by(|a, b| a.key.cmp(&b.key));
        let mut old_files = self.files.iter().peekable();
        for file in &files {
            while old_files.next_if(|known| known.key < file.key).is_some() {}
            if let Some(known) = old_files.next_if(|known| known.key == file.key) {
                file.set_cached(known.is_cached() && known.modified == file.modified);
            }
        }
        self.files = files;
        Ok(())
    }

    /// Reads the directories of the tree that changed.
    fn scan(&mut self, kind: Kind) -> io::Result<()> {
        let modified = fs::metadata(&self.path)?.modified()?;
        if !self.is_current(modified) {
            self.read(kind, modified)?;
        }
        for dir in &mut self.subdirs {
            dir.scan(kind)?;
        }
        Ok(())
    }
}

impl<'a> Iterator for Files<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<&'a Entry> {
        loop {
            if let Some(file) = self.files.next() {
                return Some(file);
            }
            let dir = loop {
                let dirs = self.dirs.last_mut()?;
                if let Some(dir) = dirs.next() {
                    break dir;
                }
                self.dirs.pop();
            };
            self.files = dir.files.iter();
            self.dirs.push(dir.subdirs.iter());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::{env, process};

    use super::*;

    /// Sets the modification time of a file or directory.
    fn touch(path: &Path, modified: SystemTime) {
        fs::File::open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    /// Scans and returns the keys that were found.
    fn keys(scanner: &mut Scanner) -> Vec<String> {
        scanner.scan().unwrap();
        let mut keys: Vec<String> = scanner.files().map(|file| file.key.clone()).collect();
        keys.sort();
        keys
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
        let mut scanner = Scanner::new(root.clone(), Kind::Sstate, None);
        assert_eq!(
            keys(&mut scanner),
            [format!("ab/cd/{first}")],
            "finds sstate"
        );
        for file in scanner.files() {
            file.set_cached(true);
        }

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
        let cached: Vec<bool> = scanner.files().map(Entry::is_cached).collect();
        assert_eq!(cached, [true, false], "unchanged files stay cached");

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
