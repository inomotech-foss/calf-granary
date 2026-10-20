//! The configuration file.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::{fs, io, path};

use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::prefix::PrefixStore;
use serde::Deserialize;
use toml::de::Error as TomlError;

use crate::cache::{Cache, CacheName, Caches};

/// The configuration file.
///
/// Object storage endpoints and credentials come from the standard `AWS_*`
/// environment variables.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    /// The caches by name.
    caches: BTreeMap<CacheName, CacheConfig>,
}

impl Config {
    /// Reads and parses the configuration file.
    pub(crate) fn load(path: &path::Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Connects every cache to its bucket.
    ///
    /// Fails if a fallback names an unknown cache or a cache that has a
    /// fallback itself.
    pub(crate) fn caches(&self) -> Result<Caches, ConfigError> {
        let mut caches = BTreeMap::new();
        for (name, cache) in self
            .caches
            .iter()
            .filter(|(_, cache)| cache.fallback.is_none())
        {
            caches.insert(name.clone(), Arc::new(cache.connect(name, None)?));
        }
        for (name, cache) in &self.caches {
            let Some(fallback) = &cache.fallback else {
                continue;
            };
            let fallback = caches
                .get(fallback)
                .cloned()
                .ok_or_else(|| ConfigError::Fallback {
                    cache: name.clone(),
                    fallback: fallback.clone(),
                })?;
            caches.insert(name.clone(), Arc::new(cache.connect(name, Some(fallback))?));
        }
        Ok(Caches::new(caches))
    }
}

/// The configuration of one cache.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheConfig {
    /// The bucket that holds the objects.
    bucket: String,
    /// A key prefix within the bucket.
    prefix: Option<String>,
    /// The cache to read from when an object is missing.
    fallback: Option<CacheName>,
}

impl CacheConfig {
    /// Creates the client for the bucket.
    fn connect(
        &self,
        name: &CacheName,
        fallback: Option<Arc<Cache>>,
    ) -> Result<Cache, ConfigError> {
        let store = AmazonS3Builder::from_env()
            .with_bucket_name(&self.bucket)
            .build()
            .map_err(|source| ConfigError::Storage {
                cache: name.clone(),
                source,
            })?;
        let prefix = self.prefix.as_deref().map(Path::from).unwrap_or_default();
        let store: Arc<dyn ObjectStore> = if prefix.as_ref().is_empty() {
            Arc::new(store)
        } else {
            Arc::new(PrefixStore::new(store, prefix.clone()))
        };
        Ok(Cache::new(
            self.bucket.as_str().into(),
            prefix,
            store,
            fallback,
        ))
    }
}

/// Errors from loading the configuration.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    /// A fallback names a missing cache or one with its own fallback.
    #[error("cache {cache} falls back to {fallback}, which is missing or has a fallback itself")]
    Fallback {
        /// The cache with the fallback.
        cache: CacheName,
        /// The fallback cache.
        fallback: CacheName,
    },
    /// The file isn't valid.
    #[error("failed to parse {}", path.display())]
    Parse {
        /// The configuration file.
        path: path::PathBuf,
        /// The parser error.
        #[source]
        source: TomlError,
    },
    /// The file can't be read.
    #[error("failed to read {}", path.display())]
    Read {
        /// The configuration file.
        path: path::PathBuf,
        /// The error from the operating system.
        #[source]
        source: io::Error,
    },
    /// The object storage client can't be created.
    #[error("failed to set up the storage of cache {cache}")]
    Storage {
        /// The cache.
        cache: CacheName,
        /// The error from the storage client.
        #[source]
        source: object_store::Error,
    },
}
