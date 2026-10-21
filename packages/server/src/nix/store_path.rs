//! Store paths and their hash parts.

use core::fmt;

use super::STORE_DIR;

/// The length of the hash part of a store path.
const HASH_LEN: usize = 32;

/// The longest name part of a store path.
const MAX_NAME_LEN: usize = 211;

/// The hash part of a store path, which names its narinfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StorePathHash([u8; HASH_LEN]);

impl StorePathHash {
    /// The hash part as text.
    pub(crate) fn as_str(&self) -> &str {
        str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl AsRef<str> for StorePathHash {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for StorePathHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for StorePathHash {
    type Error = InvalidStorePathHash;

    fn try_from(hash: &str) -> Result<Self, Self::Error> {
        let bytes: [u8; HASH_LEN] = hash
            .as_bytes()
            .try_into()
            .map_err(|_length| InvalidStorePathHash)?;
        let valid = bytes.iter().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && !b"eotu".contains(byte))
        });
        if valid {
            Ok(Self(bytes))
        } else {
            Err(InvalidStorePathHash)
        }
    }
}

/// Text that isn't the hash part of a store path.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("invalid store path hash")]
pub(crate) struct InvalidStorePathHash;

/// A path in the Nix store, such as `/nix/store/<hash>-hello-2.12`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StorePath {
    /// The hash part.
    hash: StorePathHash,
    /// The base name, `<hash>-<name>`.
    base_name: String,
}

impl StorePath {
    /// Parses a base name such as `<hash>-hello-2.12`.
    pub(crate) fn from_base_name(base_name: String) -> Result<Self, InvalidStorePath> {
        let parsed = base_name
            .split_once('-')
            .filter(|(_hash, name)| is_valid_name(name))
            .and_then(|(hash, _name)| StorePathHash::try_from(hash).ok());
        match parsed {
            Some(hash) => Ok(Self { hash, base_name }),
            None => Err(InvalidStorePath { path: base_name }),
        }
    }

    /// Joins a hash part and a name part.
    pub(crate) fn new(hash: StorePathHash, name: &str) -> Result<Self, InvalidStorePath> {
        let mut base_name =
            String::with_capacity(HASH_LEN.saturating_add(1).saturating_add(name.len()));
        base_name.push_str(hash.as_str());
        base_name.push('-');
        base_name.push_str(name);
        if is_valid_name(name) {
            Ok(Self { hash, base_name })
        } else {
            Err(InvalidStorePath { path: base_name })
        }
    }

    /// The hash part.
    pub(crate) const fn hash(&self) -> &StorePathHash {
        &self.hash
    }

    /// The base name, without the store directory.
    pub(crate) fn base_name(&self) -> &str {
        &self.base_name
    }

    /// The name part, after the hash part.
    pub(crate) fn name(&self) -> &str {
        self.base_name
            .split_once('-')
            .map_or_else(Default::default, |(_hash, name)| name)
    }
}

impl TryFrom<String> for StorePath {
    type Error = InvalidStorePath;

    /// Parses a full path such as `/nix/store/<hash>-hello-2.12`.
    fn try_from(mut path: String) -> Result<Self, Self::Error> {
        let prefix = path
            .strip_prefix(STORE_DIR)
            .and_then(|rest| rest.strip_prefix('/'))
            .map(|base_name| path.len().saturating_sub(base_name.len()));
        let Some(prefix) = prefix else {
            return Err(InvalidStorePath { path });
        };
        path.drain(..prefix);
        Self::from_base_name(path)
    }
}

impl fmt::Display for StorePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{STORE_DIR}/{}", self.base_name)
    }
}

/// Text that isn't a valid store path.
#[derive(Debug, thiserror::Error)]
#[error("invalid store path {path:?}")]
pub(crate) struct InvalidStorePath {
    /// The rejected text.
    path: String,
}

/// Whether text is a valid name part of a store path.
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+-._?=".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "0mdqa9w1p6cmli6976v4wi0sw9r4p5pr";

    #[test]
    fn parses_store_paths() {
        let path = StorePath::try_from(format!("/nix/store/{HASH}-hello-2.12")).unwrap();
        assert_eq!(path.hash().as_str(), HASH, "the hash part");
        assert_eq!(path.name(), "hello-2.12", "the name part");
        assert_eq!(
            path.base_name(),
            format!("{HASH}-hello-2.12"),
            "the base name"
        );
        assert_eq!(
            StorePath::new(*path.hash(), path.name()).unwrap(),
            path,
            "the parts join again"
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
                StorePath::try_from(path.clone()).is_err(),
                "{path} is invalid"
            );
        }
    }
}
