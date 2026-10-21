//! Storage classes: the buckets that hold chunk objects.

use core::borrow::Borrow;
use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use object_store::ObjectStore;
use serde::Deserialize;

/// The name of a store.
///
/// Names consist of lowercase ASCII letters, digits and dashes and start with
/// a letter or digit. Chunks record the name of the store that holds them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct StoreName(String);

impl AsRef<str> for StoreName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for StoreName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StoreName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for StoreName {
    type Error = InvalidStoreName;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let valid = name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(name))
        } else {
            Err(InvalidStoreName { name })
        }
    }
}

/// A store name with characters outside the allowed set.
#[derive(Debug, thiserror::Error)]
#[error("invalid store name {name:?}")]
pub(crate) struct InvalidStoreName {
    /// The rejected name.
    name: String,
}

/// The configured stores by name.
#[derive(Debug)]
pub(crate) struct Stores(BTreeMap<StoreName, Arc<dyn ObjectStore>>);

impl Stores {
    /// Wraps the stores.
    pub(crate) const fn new(stores: BTreeMap<StoreName, Arc<dyn ObjectStore>>) -> Self {
        Self(stores)
    }

    /// Returns the store with the name.
    pub(crate) fn get(&self, name: &str) -> Option<&Arc<dyn ObjectStore>> {
        self.0.get(name)
    }

    /// Returns every store with its name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&StoreName, &Arc<dyn ObjectStore>)> {
        self.0.iter()
    }
}
