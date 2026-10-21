//! Caches and the objects in them.

mod prune;
mod upload;

use core::borrow::Borrow;
use core::error::Error;
use core::fmt;
use core::str::FromStr;
use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use futures::{Stream, StreamExt as _};
use object_store::path::{Path, PathPart};
use object_store::{
    GetOptions, GetResult, ObjectStore, PutMultipartOptions, PutOptions, PutPayload, WriteMultipart,
};
use serde::{Deserialize, Serialize};

pub(crate) use self::prune::{PruneError, PruneStats};
use self::upload::Upload;

/// The size of the parts of multipart uploads. Smaller objects are uploaded
/// in one request.
const PART_SIZE: usize = 8 * 1024 * 1024;

/// How many parts of one object are uploaded at the same time.
const PART_CONCURRENCY: usize = 4;

/// The name of a cache.
///
/// Names consist of lowercase ASCII letters, digits and dashes and start with
/// a letter or digit. They appear as the first path segment of object URLs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct CacheName(String);

impl AsRef<str> for CacheName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

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

impl FromStr for CacheName {
    type Err = InvalidCacheName;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        name.to_owned().try_into()
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

/// An object identified by its bucket and its full key in the bucket.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StoredObject {
    /// The bucket.
    pub(crate) bucket: Arc<str>,
    /// The key in the bucket, including the cache's prefix.
    pub(crate) key: Path,
}

/// An object found in a cache.
#[derive(Debug)]
pub(crate) struct Hit {
    /// Where the object was found.
    pub(crate) object: StoredObject,
    /// Whether the fallback cache had the object.
    pub(crate) fallback: bool,
    /// The object's metadata and, unless only the metadata was requested,
    /// its content.
    pub(crate) result: GetResult,
}

/// What storing an object did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stored {
    /// The object was stored.
    New {
        /// The object's size in bytes.
        size: u64,
    },
    /// The cache already had the object.
    Existing,
}

/// A cache backed by object storage.
#[derive(Debug)]
pub(crate) struct Cache {
    /// The bucket that holds the objects.
    bucket: Arc<str>,
    /// The key prefix of the objects in the bucket.
    prefix: Path,
    /// How long objects are kept after they were last read or written.
    /// Objects are kept forever without it.
    retention: Option<Duration>,
    /// The bucket, limited to the prefix.
    store: Arc<dyn ObjectStore>,
    /// The cache to read from when an object is missing here.
    fallback: Option<Arc<Self>>,
}

impl Cache {
    /// Creates a cache.
    ///
    /// The store must already be limited to `prefix`.
    pub(crate) const fn new(
        bucket: Arc<str>,
        prefix: Path,
        retention: Option<Duration>,
        store: Arc<dyn ObjectStore>,
        fallback: Option<Arc<Self>>,
    ) -> Self {
        Self {
            bucket,
            prefix,
            retention,
            store,
            fallback,
        }
    }

    /// Fetches an object, trying the fallback cache when it's missing here.
    ///
    /// With `head`, only the metadata is fetched. Returns `None` if neither
    /// cache has the object.
    pub(crate) async fn get(
        &self,
        key: &ObjectKey,
        head: bool,
    ) -> Result<Option<Hit>, object_store::Error> {
        if let Some(hit) = self.get_here(key, head).await? {
            return Ok(Some(hit));
        }
        match &self.fallback {
            Some(fallback) => Ok(fallback.get_here(key, head).await?.map(|hit| Hit {
                fallback: true,
                ..hit
            })),
            None => Ok(None),
        }
    }

    /// Fetches an object from this cache only.
    async fn get_here(
        &self,
        key: &ObjectKey,
        head: bool,
    ) -> Result<Option<Hit>, object_store::Error> {
        let options = GetOptions::new().with_head(head);
        match self.store.get_opts(&key.0, options).await {
            Ok(result) => Ok(Some(Hit {
                object: self.object(key),
                fallback: false,
                result,
            })),
            Err(object_store::Error::NotFound { path: _, source: _ }) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Stores an object unless this cache already has it.
    ///
    /// Objects that fit in one part are uploaded in one request, larger ones
    /// in parts. An unfinished multipart upload is aborted.
    pub(crate) async fn put<S, E>(&self, key: &ObjectKey, mut body: S) -> Result<Stored, PutError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
        E: Error + Send + Sync + 'static,
    {
        if self
            .get_here(key, true)
            .await
            .map_err(PutError::Store)?
            .is_some()
        {
            return Ok(Stored::Existing);
        }
        let mut parts = Vec::new();
        let mut size = 0usize;
        while size < PART_SIZE {
            let Some(chunk) = body.next().await else {
                let payload: PutPayload = parts.into_iter().collect();
                self.store
                    .put_opts(&key.0, payload, PutOptions::default())
                    .await
                    .map_err(PutError::Store)?;
                return Ok(Stored::New {
                    size: u64::try_from(size).unwrap_or(u64::MAX),
                });
            };
            let chunk = chunk.map_err(|source| PutError::Body(Box::new(source)))?;
            size = size.saturating_add(chunk.len());
            parts.push(chunk);
        }

        let upload = self
            .store
            .put_multipart_opts(&key.0, PutMultipartOptions::default())
            .await
            .map_err(PutError::Store)?;
        let mut upload = Upload::new(WriteMultipart::new_with_chunk_size(upload, PART_SIZE));
        for part in parts {
            upload
                .put(part, PART_CONCURRENCY)
                .await
                .map_err(PutError::Store)?;
        }
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|source| PutError::Body(Box::new(source)))?;
            size = size.saturating_add(chunk.len());
            upload
                .put(chunk, PART_CONCURRENCY)
                .await
                .map_err(PutError::Store)?;
        }
        upload.finish().await.map_err(PutError::Store)?;
        Ok(Stored::New {
            size: u64::try_from(size).unwrap_or(u64::MAX),
        })
    }

    /// Identifies an object of this cache in its bucket.
    fn object(&self, key: &ObjectKey) -> StoredObject {
        StoredObject {
            bucket: Arc::clone(&self.bucket),
            key: self.full_key(&key.0),
        }
    }

    /// Prepends the prefix to a location in the store.
    fn full_key(&self, location: &Path) -> Path {
        self.prefix.parts().chain(location.parts()).collect()
    }
}

/// Errors from storing an object.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PutError {
    /// Reading the object from the client failed.
    #[error("failed to read the object")]
    Body(#[source] Box<dyn Error + Send + Sync>),
    /// Object storage failed.
    #[error("failed to store the object")]
    Store(#[source] object_store::Error),
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

    /// Returns every cache with its name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&CacheName, &Cache)> {
        self.0.iter().map(|(name, cache)| (name, cache.as_ref()))
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
