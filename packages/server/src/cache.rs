//! Caches and the objects in them.

use core::borrow::Borrow;
use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use object_store::path::{Path, PathPart};
use object_store::{GetOptions, GetResult, ObjectStore};
use serde::Deserialize;

/// The name of a cache.
///
/// Names consist of lowercase ASCII letters, digits and dashes and start with
/// a letter or digit. They appear as the first path segment of object URLs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct CacheName(String);

impl Borrow<str> for CacheName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CacheName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CacheName {
    type Error = InvalidCacheName;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let valid = name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(name))
        } else {
            Err(InvalidCacheName { name })
        }
    }
}

/// A cache name with characters outside the allowed set.
#[derive(Debug, thiserror::Error)]
#[error("invalid cache name {name:?}")]
pub(crate) struct InvalidCacheName {
    /// The rejected name.
    name: String,
}

/// The key of an object within a cache.
///
/// Keys are `/`-separated paths without empty, `.` or `..` segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObjectKey(Path);

impl TryFrom<&str> for ObjectKey {
    type Error = InvalidObjectKey;

    fn try_from(key: &str) -> Result<Self, Self::Error> {
        let valid = !key.is_empty()
            && key
                .split('/')
                .all(|segment| !matches!(segment, "" | "." | ".."));
        if valid {
            Ok(Self(key.split('/').map(PathPart::from).collect()))
        } else {
            Err(InvalidObjectKey {
                key: key.to_owned(),
            })
        }
    }
}

/// An object key with empty, `.` or `..` segments.
#[derive(Debug, thiserror::Error)]
#[error("invalid object key {key:?}")]
pub(crate) struct InvalidObjectKey {
    /// The rejected key.
    key: String,
}

/// A cache backed by object storage.
#[derive(Debug)]
pub(crate) struct Cache {
    /// Where the objects are stored.
    store: Arc<dyn ObjectStore>,
    /// The cache to read from when an object is missing here.
    fallback: Option<Arc<Self>>,
}

impl Cache {
    /// Creates a cache.
    pub(crate) const fn new(store: Arc<dyn ObjectStore>, fallback: Option<Arc<Self>>) -> Self {
        Self { store, fallback }
    }

    /// Fetches an object, trying the fallback cache when it's missing here.
    ///
    /// With `head`, only the metadata is fetched. Returns `None` if neither
    /// cache has the object.
    pub(crate) async fn get(
        &self,
        key: &ObjectKey,
        head: bool,
    ) -> Result<Option<GetResult>, object_store::Error> {
        if let Some(result) = self.get_here(key, head).await? {
            return Ok(Some(result));
        }
        match &self.fallback {
            Some(fallback) => fallback.get_here(key, head).await,
            None => Ok(None),
        }
    }

    /// Fetches an object from this cache only.
    async fn get_here(
        &self,
        key: &ObjectKey,
        head: bool,
    ) -> Result<Option<GetResult>, object_store::Error> {
        let options = GetOptions::new().with_head(head);
        match self.store.get_opts(&key.0, options).await {
            Ok(result) => Ok(Some(result)),
            Err(object_store::Error::NotFound { path: _, source: _ }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// All caches by name.
#[derive(Debug, Default)]
pub(crate) struct Caches(BTreeMap<CacheName, Arc<Cache>>);

impl Caches {
    /// Creates the set of caches.
    pub(crate) const fn new(caches: BTreeMap<CacheName, Arc<Cache>>) -> Self {
        Self(caches)
    }

    /// Looks up a cache by name.
    pub(crate) fn get(&self, name: &str) -> Option<&Cache> {
        self.0.get(name).map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_names() {
        for name in ["sstate-main", "0", "downloads"] {
            assert!(
                CacheName::try_from(name.to_owned()).is_ok(),
                "{name} is valid"
            );
        }
        for name in ["", "-main", "Main", "sstate/main", "sstate_main"] {
            assert!(
                CacheName::try_from(name.to_owned()).is_err(),
                "{name} is invalid"
            );
        }
    }

    #[test]
    fn object_keys() {
        let key = ObjectKey::try_from(
            "universal/ab/cd/sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:\
             abcd_populate_sysroot.tar.zst",
        )
        .unwrap();
        assert_eq!(key.0.parts().count(), 4, "segments are kept");

        let encoded = ObjectKey::try_from("git2/foo~1.0%40bar.tar.gz").unwrap();
        assert_eq!(
            encoded.0.as_ref(),
            "git2/foo%7E1.0%2540bar.tar.gz",
            "unsafe characters are encoded"
        );

        for invalid in ["", "/a", "a/", "a//b", "a/./b", "a/../b", ".."] {
            assert!(
                ObjectKey::try_from(invalid).is_err(),
                "{invalid} is invalid"
            );
        }
    }
}
