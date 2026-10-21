//! The configuration file.

use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::{fs, io, path};

use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use object_store::prefix::PrefixStore;
use reqwest::Client;
use serde::Deserialize;
use toml::de::Error as TomlError;

use crate::cache::{Cache, CacheName, Caches};
use crate::content::Content;
use crate::duration::ConfigDuration;
use crate::exchange::Exchange;
use crate::oidc::{Issuer, IssuerConfig};
use crate::store::{StoreName, Stores};
use crate::tiers::Tiers;
use crate::token::TokenKey;

/// How long unreferenced content is kept unless configured.
const DEFAULT_COLLECTION_GRACE: Duration = Duration::from_hours(24);

/// How long issued tokens are valid unless configured.
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_hours(6);

/// How long fetching an issuer's keys may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// The configuration file.
///
/// Object storage endpoints and credentials come from the standard `AWS_*`
/// environment variables.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    /// The stores that hold chunk objects, by name.
    stores: BTreeMap<StoreName, StoreConfig>,
    /// The caches by name.
    caches: BTreeMap<CacheName, CacheConfig>,
    /// The issuers whose ID tokens are accepted, by name.
    #[serde(default)]
    issuers: BTreeMap<String, IssuerConfig>,
    /// The tiers that map ID tokens to the caches they may write.
    #[serde(default)]
    tiers: Tiers,
    /// How long issued tokens are valid, 6 hours by default.
    token_lifetime: Option<ConfigDuration>,
    /// How long unreferenced chunks and blobs are kept, 24 hours by default.
    ///
    /// Uploads must finish within this time.
    collection_grace: Option<ConfigDuration>,
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

    /// Connects every store to its bucket.
    pub(crate) fn content(&self) -> Result<Content, ConfigError> {
        let mut stores = BTreeMap::new();
        for (name, store) in &self.stores {
            stores.insert(name.clone(), store.connect(name)?);
        }
        let grace = self
            .collection_grace
            .map_or(DEFAULT_COLLECTION_GRACE, Into::into);
        Ok(Content::new(Stores::new(stores), grace))
    }

    /// Sets up the caches.
    ///
    /// Fails if a cache names an unknown store, or if a fallback names an
    /// unknown cache or a cache that has a fallback itself.
    pub(crate) fn caches(&self) -> Result<Caches, ConfigError> {
        if let Some((name, cache)) = self
            .caches
            .iter()
            .find(|(_, cache)| !self.stores.contains_key(&cache.store))
        {
            return Err(ConfigError::Store {
                cache: name.clone(),
                store: cache.store.clone(),
            });
        }
        let mut caches = BTreeMap::new();
        for (name, cache) in self
            .caches
            .iter()
            .filter(|(_, cache)| cache.fallback.is_none())
        {
            caches.insert(name.clone(), Arc::new(cache.build(name, None)));
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
            caches.insert(name.clone(), Arc::new(cache.build(name, Some(fallback))));
        }
        Ok(Caches::new(caches))
    }

    /// Sets up the token exchange.
    ///
    /// Fails if a tier names an unknown cache.
    pub(crate) fn exchange(self, key: TokenKey) -> Result<Exchange, ConfigError> {
        for tier in self.tiers.iter() {
            if let Some(cache) = tier
                .caches
                .iter()
                .find(|cache| !self.caches.contains_key(*cache))
            {
                return Err(ConfigError::TierCache {
                    tier: tier.name.clone(),
                    cache: cache.clone(),
                });
            }
        }
        let client = Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(ConfigError::HttpClient)?;
        let issuers = self
            .issuers
            .into_values()
            .map(|issuer| Issuer::new(issuer, client.clone()))
            .collect();
        let lifetime = self
            .token_lifetime
            .map_or(DEFAULT_TOKEN_LIFETIME, Into::into);
        Ok(Exchange::new(issuers, self.tiers, key, lifetime))
    }
}

/// The configuration of one store.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreConfig {
    /// The bucket that holds the objects.
    bucket: String,
    /// A key prefix within the bucket.
    prefix: Option<String>,
}

impl StoreConfig {
    /// Creates the client for the bucket.
    fn connect(&self, name: &StoreName) -> Result<Arc<dyn ObjectStore>, ConfigError> {
        let store = AmazonS3Builder::from_env()
            .with_bucket_name(&self.bucket)
            .build()
            .map_err(|source| ConfigError::Storage {
                store: name.clone(),
                source,
            })?;
        Ok(match self.prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => {
                Arc::new(PrefixStore::new(store, Path::from(prefix)))
            }
            _ => Arc::new(store),
        })
    }
}

/// The configuration of one cache.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheConfig {
    /// The store that new chunks go to.
    store: StoreName,
    /// How long entries are kept after they were last read or written.
    retention: Option<ConfigDuration>,
    /// The cache to read from when an entry is missing.
    fallback: Option<CacheName>,
}

impl CacheConfig {
    /// Creates the cache.
    fn build(&self, name: &CacheName, fallback: Option<Arc<Cache>>) -> Cache {
        Cache::new(
            name.clone(),
            self.store.clone(),
            self.retention.map(Into::into),
            fallback,
        )
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
    /// The HTTP client for fetching issuer keys can't be created.
    #[error("failed to create the HTTP client")]
    HttpClient(#[source] reqwest::Error),
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
    /// A cache names an unknown store.
    #[error("cache {cache} names the unknown store {store}")]
    Store {
        /// The cache.
        cache: CacheName,
        /// The unknown store.
        store: StoreName,
    },
    /// A tier names an unknown cache.
    #[error("tier {tier} names the unknown cache {cache}")]
    TierCache {
        /// The tier.
        tier: String,
        /// The unknown cache.
        cache: CacheName,
    },
    /// The object storage client can't be created.
    #[error("failed to set up the store {store}")]
    Storage {
        /// The store.
        store: StoreName,
        /// The error from the storage client.
        #[source]
        source: object_store::Error,
    },
}
