//! Caches and the entries in them.

mod hashserv;
mod nix;

use core::borrow::Borrow;
use core::error::Error;
use core::fmt;
use core::str::FromStr;
use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use calf_granary_server_db::queries::content::{
    entry_exists, expire_entries, find_entry, insert_entry, missing_entries,
};
use chrono::{DateTime, FixedOffset, TimeDelta, Utc};
use deadpool_postgres::{Client, Pool, PoolError};
use futures::Stream;
use serde::{Deserialize, Serialize};

pub(crate) use self::hashserv::Report;
pub(crate) use self::nix::{NarCompression, NixPutError};
use crate::content::{Content, WriteError};
use crate::nix::SigningKey;
use crate::store::StoreName;

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

/// The key of an entry within a cache.
///
/// Keys are `/`-separated paths without empty, `.` or `..` segments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ObjectKey(String);

impl AsRef<str> for ObjectKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ObjectKey {
    type Error = InvalidObjectKey;

    fn try_from(key: &str) -> Result<Self, Self::Error> {
        let valid = !key.is_empty()
            && key
                .split('/')
                .all(|segment| !matches!(segment, "" | "." | ".."));
        if valid {
            Ok(Self(key.to_owned()))
        } else {
            Err(InvalidObjectKey {
                key: key.to_owned(),
            })
        }
    }
}

/// A key that isn't a valid entry key.
#[derive(Debug, thiserror::Error)]
#[error("invalid object key {key:?}")]
pub(crate) struct InvalidObjectKey {
    /// The rejected key.
    key: String,
}

/// An entry found in a cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    /// The cache that had the entry.
    pub(crate) cache: CacheName,
    /// Whether that is the fallback cache.
    pub(crate) fallback: bool,
    /// The blob with the content.
    pub(crate) blob_id: i64,
    /// The SHA-256 of the content.
    pub(crate) hash: Vec<u8>,
    /// The size of the content in bytes.
    pub(crate) size: u64,
    /// When the entry was created.
    pub(crate) created_at: DateTime<FixedOffset>,
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

/// The name of a protocol in the configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProtocolName {
    /// Plain files under arbitrary keys, as Yocto uses them.
    #[default]
    Files,
    /// A Nix binary cache.
    Nix,
    /// A hash equivalence server for Yocto builds.
    Hashserv,
}

/// The protocol that clients of a cache speak.
#[derive(Debug)]
pub(crate) enum Protocol {
    /// Plain files under arbitrary keys.
    Files,
    /// A Nix binary cache.
    Nix(NixCache),
    /// A hash equivalence server for Yocto builds.
    Hashserv,
}

impl Protocol {
    /// The name in the configuration.
    pub(crate) const fn name(&self) -> ProtocolName {
        match self {
            Self::Files => ProtocolName::Files,
            Self::Nix(_) => ProtocolName::Nix,
            Self::Hashserv => ProtocolName::Hashserv,
        }
    }
}

/// The settings of a Nix binary cache.
#[derive(Debug)]
pub(crate) struct NixCache {
    /// Signs the narinfos.
    pub(crate) key: SigningKey,
    /// The `Priority` in `nix-cache-info`.
    pub(crate) priority: u32,
}

/// A cache: a namespace of entries with a retention.
#[derive(Debug)]
pub(crate) struct Cache {
    /// The name.
    name: CacheName,
    /// The store that new chunks go to, unless the cache keeps no content.
    store: Option<StoreName>,
    /// Whether content is split into deduplicated, compressed chunks.
    chunking: bool,
    /// How long entries are kept after they were last read or written.
    retention: Option<Duration>,
    /// The protocol that clients speak.
    protocol: Protocol,
    /// The cache to read from when an entry is missing.
    fallback: Option<Arc<Self>>,
}

impl Cache {
    /// Creates a cache.
    pub(crate) const fn new(
        name: CacheName,
        store: Option<StoreName>,
        chunking: bool,
        retention: Option<Duration>,
        protocol: Protocol,
        fallback: Option<Arc<Self>>,
    ) -> Self {
        Self {
            name,
            store,
            chunking,
            retention,
            protocol,
            fallback,
        }
    }

    /// The name.
    pub(crate) const fn name(&self) -> &CacheName {
        &self.name
    }

    /// The protocol that clients speak.
    pub(crate) const fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    /// This cache and its fallback, in lookup order.
    fn lookup_chain(&self) -> impl Iterator<Item = &Self> {
        [Some(self), self.fallback.as_deref()].into_iter().flatten()
    }

    /// Returns the keys that neither this cache nor its fallback has.
    pub(crate) async fn missing(
        &self,
        client: &Client,
        keys: &[ObjectKey],
    ) -> Result<Vec<String>, tokio_postgres::Error> {
        let keys: Vec<&str> = keys.iter().map(AsRef::as_ref).collect();
        let caches: Vec<&str> = self
            .lookup_chain()
            .map(|cache| cache.name.as_ref())
            .collect();
        missing_entries()
            .bind(client, &keys.as_slice(), &caches.as_slice())
            .all()
            .await
    }

    /// The store that new chunks go to.
    fn store(&self) -> Result<&StoreName, NoStore> {
        self.store
            .as_ref()
            .ok_or_else(|| NoStore(self.name.clone()))
    }

    /// Looks up an entry, trying the fallback cache when it's missing here.
    pub(crate) async fn get(
        &self,
        client: &Client,
        key: &ObjectKey,
    ) -> Result<Option<Hit>, tokio_postgres::Error> {
        if let Some(hit) = self.get_here(client, key, false).await? {
            return Ok(Some(hit));
        }
        match &self.fallback {
            Some(fallback) => fallback.get_here(client, key, true).await,
            None => Ok(None),
        }
    }

    /// Looks up an entry in this cache only.
    async fn get_here(
        &self,
        client: &Client,
        key: &ObjectKey,
        fallback: bool,
    ) -> Result<Option<Hit>, tokio_postgres::Error> {
        let entry = find_entry()
            .bind(client, &self.name.as_ref(), &key.as_ref())
            .opt()
            .await?;
        Ok(entry.map(|entry| Hit {
            cache: self.name.clone(),
            fallback,
            blob_id: entry.blob_id,
            hash: entry.hash,
            size: u64::try_from(entry.size).unwrap_or_default(),
            created_at: entry.created_at,
        }))
    }

    /// Stores an object unless this cache already has it.
    pub(crate) async fn put<S, E>(
        &self,
        content: &Content,
        pool: &Pool,
        key: &ObjectKey,
        body: S,
    ) -> Result<Stored, PutError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin + Send,
        E: Error + Send + Sync + 'static,
    {
        let exists = entry_exists()
            .bind(&pool.get().await?, &self.name.as_ref(), &key.as_ref())
            .one()
            .await?;
        if exists {
            return Ok(Stored::Existing);
        }
        let blob = if self.chunking {
            content.write_chunked(pool, self.store()?, body).await?
        } else {
            content.write(pool, self.store()?, body).await?
        };
        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        let blob_id = content.commit(&transaction, &blob).await?;
        let created = insert_entry()
            .bind(&transaction, &self.name.as_ref(), &key.as_ref(), &blob_id)
            .opt()
            .await?;
        transaction.commit().await?;
        Ok(match created {
            Some(_) => Stored::New { size: blob.size },
            None => Stored::Existing,
        })
    }

    /// Deletes entries that weren't read or written within the retention.
    ///
    /// Returns `None` for caches without a retention.
    pub(crate) async fn expire(
        &self,
        client: &Client,
        now: DateTime<Utc>,
    ) -> Result<Option<u64>, ExpireError> {
        let Some(retention) = self.retention else {
            return Ok(None);
        };
        let cutoff = TimeDelta::from_std(retention)
            .ok()
            .and_then(|retention| now.checked_sub_signed(retention))
            .ok_or(ExpireError::Retention)?;
        if matches!(self.protocol, Protocol::Hashserv) {
            return Ok(Some(self.expire_unihashes(client, cutoff).await?));
        }
        let expired = expire_entries()
            .bind(client, &self.name.as_ref(), &cutoff.fixed_offset())
            .await?;
        Ok(Some(expired))
    }
}

/// A cache that keeps no content.
#[derive(Debug, thiserror::Error)]
#[error("cache {0} keeps no content")]
pub(crate) struct NoStore(CacheName);

/// Errors from storing an object.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PutError {
    /// Getting a database connection failed.
    #[error("failed to get a database connection")]
    Connect(#[from] PoolError),
    /// Recording the entry failed.
    #[error("failed to record the entry")]
    Database(#[from] tokio_postgres::Error),
    /// The cache keeps no content.
    #[error(transparent)]
    NoStore(#[from] NoStore),
    /// Storing the content failed.
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// Errors from expiring entries.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ExpireError {
    /// Deleting the entries failed.
    #[error("failed to delete the expired entries")]
    Database(#[from] tokio_postgres::Error),
    /// The retention reaches back further than timestamps can represent.
    #[error("the retention is too long")]
    Retention,
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
        assert_eq!(key.as_ref().split('/').count(), 4, "segments are kept");

        for invalid in ["", "/a", "a/", "a//b", "a/./b", "a/../b", ".."] {
            assert!(
                ObjectKey::try_from(invalid).is_err(),
                "{invalid} is invalid"
            );
        }
    }
}
