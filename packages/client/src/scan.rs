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

/// A file that a scan found.
#[derive(Clone, Copy, Debug)]
pub(crate) struct File<'a> {
    /// The directory of the file.
    dir: &'a Dir,
    /// The file.
    entry: &'a Entry,
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
    /// The files that belong in the cache, sorted by name.
    files: Vec<Entry>,
}

/// A file in a directory listing.
#[derive(Debug)]
struct Entry {
    /// The file name.
    name: String,
    /// When the file was last modified.
    modified: SystemTime,
    /// Whether the cache has the file.
    cached: AtomicBool,
}

/// The files of a directory tree.
#[derive(Debug)]
struct Files<'a> {
    /// The current directory.
    dir: &'a Dir,
    /// The files of the current directory still to visit.
    entries: slice::Iter<'a, Entry>,
    /// The subdirectories still to visit, at each depth.
    dirs: Vec<slice::Iter<'a, Dir>>,
}

impl Scanner {
    /// Lists the files that the last scan found.
    pub(crate) fn files(&self) -> impl Iterator<Item = File<'_>> {
        Files {
            dir: &self.root,
            entries: self.root.files.iter(),
            dirs: vec![self.root.subdirs.iter()],
        }
    }

    /// Creates a scanner that hasn't read anything yet.
    pub(crate) const fn new(root: PathBuf, kind: Kind, since: Option<SystemTime>) -> Self {
        Self {
            root: Dir::new(root, String::new()),
            kind,
            since,
        }
    }

    /// Reads the directories that changed since the last scan, and the
    /// modification times of the files that the cache doesn't have yet.
    pub(crate) fn scan(&mut self) -> io::Result<()> {
        self.root.scan(self.kind, self.since)
    }
}

impl File<'_> {
    /// Whether the cache has the file.
    pub(crate) fn is_cached(self) -> bool {
        self.entry.cached.load(Ordering::Relaxed)
    }

    /// The key in the cache.
    pub(crate) fn key(self) -> String {
        format!("{}{}", self.dir.prefix, self.entry.name)
    }

    /// When the file was last modified.
    pub(crate) const fn modified(self) -> SystemTime {
        self.entry.modified
    }

    /// The local file.
    pub(crate) fn path(self) -> PathBuf {
        self.dir.path.join(&self.entry.name)
    }

    /// Records whether the cache has the file.
    pub(crate) fn set_cached(self, cached: bool) {
        self.entry.cached.store(cached, Ordering::Relaxed);
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
    fn read(
        &mut self,
        kind: Kind,
        since: Option<SystemTime>,
        modified: SystemTime,
    ) -> io::Result<()> {
        let read_at = SystemTime::now();
        let mut subdirs = Vec::new();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                if kind == Kind::Sstate {
                    subdirs.push((entry.path(), name));
                }
            } else if kind.wants(&name) {
                let metadata = match entry.metadata() {
                    Ok(metadata) if metadata.is_file() => metadata,
                    Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                    _ => continue,
                };
                let file_modified = metadata.modified()?;
                if since.is_none_or(|since| file_modified >= since) {
                    files.push(Entry {
                        name,
                        modified: file_modified,
                        cached: AtomicBool::new(false),
                    });
                }
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

        files.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let mut old_files = self.files.iter().peekable();
        for file in &mut files {
            while old_files.next_if(|known| known.name < file.name).is_some() {}
            if let Some(known) = old_files.next_if(|known| known.name == file.name) {
                *file.cached.get_mut() =
                    known.cached.load(Ordering::Relaxed) && known.modified == file.modified;
            }
        }
        self.files = files;
        self.modified = Some(modified);
        self.read_at = read_at;
        Ok(())
    }

    /// Reads the modification times of the files that the cache doesn't
    /// have yet, because writing to a file doesn't change its directory.
    fn restat(&mut self) -> io::Result<()> {
        for entry in &mut self.files {
            if *entry.cached.get_mut() {
                continue;
            }
            match fs::metadata(self.path.join(&entry.name)) {
                Ok(metadata) => entry.modified = metadata.modified()?,
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    entry.modified = SystemTime::now();
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    /// Reads the directories of the tree that changed.
    fn scan(&mut self, kind: Kind, since: Option<SystemTime>) -> io::Result<()> {
        let modified = fs::metadata(&self.path)?.modified()?;
        if self.is_current(modified) {
            self.restat()?;
        } else {
            self.read(kind, since, modified)?;
        }
        for dir in &mut self.subdirs {
            dir.scan(kind, since)?;
        }
        Ok(())
    }
}

impl<'a> Iterator for Files<'a> {
    type Item = File<'a>;

    fn next(&mut self) -> Option<File<'a>> {
        loop {
            if let Some(entry) = self.entries.next() {
                return Some(File {
                    dir: self.dir,
                    entry,
                });
            }
            let dir = loop {
                let dirs = self.dirs.last_mut()?;
                if let Some(dir) = dirs.next() {
                    break dir;
                }
                self.dirs.pop();
            };
            self.dir = dir;
            self.entries = dir.files.iter();
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
        let mut keys: Vec<String> = scanner.files().map(File::key).collect();
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
        let cached: Vec<bool> = scanner.files().map(File::is_cached).collect();
        assert_eq!(cached, [true, false], "unchanged files stay cached");

        touch(&leaf.join(second), past);
        scanner.scan().unwrap();
        let modified: Vec<SystemTime> = scanner.files().map(File::modified).collect();
        assert_eq!(
            modified.get(1),
            Some(&past),
            "uncached files are read again"
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
