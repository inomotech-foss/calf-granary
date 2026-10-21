//! Store paths and their hash parts.

use core::fmt;

use super::STORE_DIR;

/// The length of the hash part of a store path.
const HASH_LEN: usize = 32;

/// The longest name part of a store path.
const MAX_NAME_LEN: usize = 211;

/// The hash part of a store path, which names its narinfo.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StorePathHash(String);

impl AsRef<str> for StorePathHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for StorePathHash {
    type Error = InvalidStorePath;

    fn try_from(hash: &str) -> Result<Self, Self::Error> {
        let valid = hash.len() == HASH_LEN
            && hash.bytes().all(|byte| {
                byte.is_ascii_digit() || (byte.is_ascii_lowercase() && !b"eotu".contains(&byte))
            });
        if valid {
            Ok(Self(hash.to_owned()))
        } else {
            Err(InvalidStorePath {
                path: hash.to_owned(),
            })
        }
    }
}

/// A path in the Nix store, such as `/nix/store/<hash>-hello-2.12`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StorePath {
    /// The hash part.
    hash: StorePathHash,
    /// The name part.
    name: String,
}

impl StorePath {
    /// Parses a base name such as `<hash>-hello-2.12`.
    pub(crate) fn from_base_name(base_name: &str) -> Result<Self, InvalidStorePath> {
        let invalid = || InvalidStorePath {
            path: base_name.to_owned(),
        };
        let (hash, name) = base_name.split_once('-').ok_or_else(invalid)?;
        let valid_name = !name.is_empty()
            && name.len() <= MAX_NAME_LEN
            && !name.starts_with('.')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+-._?=".contains(&byte));
        if !valid_name {
            return Err(invalid());
        }
        Ok(Self {
            hash: StorePathHash::try_from(hash).map_err(|_invalid| invalid())?,
            name: name.to_owned(),
        })
    }

    /// The hash part.
    pub(crate) const fn hash(&self) -> &StorePathHash {
        &self.hash
    }

    /// The base name, without the store directory.
    pub(crate) fn base_name(&self) -> String {
        format!("{}-{}", self.hash.0, self.name)
    }
}

impl TryFrom<&str> for StorePath {
    type Error = InvalidStorePath;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        let base_name = path
            .strip_prefix(STORE_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            .ok_or_else(|| InvalidStorePath {
                path: path.to_owned(),
            })?;
        Self::from_base_name(base_name)
    }
}

impl fmt::Display for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{STORE_DIR}/{}-{}", self.hash.0, self.name)
    }
}

/// Text that isn't a valid store path or hash part.
#[derive(Debug, thiserror::Error)]
#[error("invalid store path {path:?}")]
pub(crate) struct InvalidStorePath {
    /// The rejected text.
    path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0mdqa9w1p6cmli6976v4wi0sw9r4p5pr";

    #[test]
    fn parses_store_paths() {
        let path = StorePath::try_from(format!("/nix/store/{HASH}-hello-2.12").as_str()).unwrap();
        assert_eq!(path.hash().as_ref(), HASH, "the hash part");
        assert_eq!(
            path.base_name(),
            format!("{HASH}-hello-2.12"),
            "the base name"
        );
        assert_eq!(
            path.to_string(),
            format!("/nix/store/{HASH}-hello-2.12"),
            "the full path"
        );
    }

    #[test]
    fn rejects_invalid_store_paths() {
        for path in [
            format!("/nix/store/{HASH}"),
            format!("/nix/store/{HASH}-"),
            format!("/nix/store/{HASH}-a/b"),
            format!("/nix/store/{HASH}-.hidden"),
            format!("/other/{HASH}-hello"),
            "/nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-hello".to_owned(),
            "/nix/store/short-hello".to_owned(),
        ] {
            assert!(
                StorePath::try_from(path.as_str()).is_err(),
                "{path} is invalid"
            );
        }
    }
}
