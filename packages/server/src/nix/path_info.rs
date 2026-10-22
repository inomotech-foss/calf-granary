//! What Nix knows about a store path.

use core::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::base32;
use super::store_path::StorePath;
use crate::hex;
use crate::sha256::Sha256;

/// The metadata of a store path that a cache keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathInfo {
    /// The store path.
    pub(crate) store_path: StorePath,
    /// The store paths that this one references.
    pub(crate) references: Vec<StorePath>,
    /// The derivation that built the path.
    pub(crate) deriver: Option<StorePath>,
    /// The system that built the path.
    pub(crate) system: Option<InfoValue>,
    /// The content address of content-addressed paths.
    pub(crate) ca: Option<InfoValue>,
    /// The SHA-256 of the uncompressed NAR.
    pub(crate) nar_hash: Sha256,
    /// The size of the uncompressed NAR.
    pub(crate) nar_size: u64,
}

/// A free-form narinfo value, such as `System` or `CA`.
///
/// It is not empty and has no control characters, so it stays on its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InfoValue(String);

impl TryFrom<String> for InfoValue {
    type Error = InvalidInfoValue;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() || value.chars().any(char::is_control) {
            Err(InvalidInfoValue)
        } else {
            Ok(Self(value))
        }
    }
}

impl AsRef<str> for InfoValue {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InfoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A narinfo value is empty or spans lines.
#[derive(Debug, thiserror::Error)]
#[error("a narinfo value must be one line of text")]
pub(crate) struct InvalidInfoValue;

/// Parses a SHA-256 hash as Nix prints it: `sha256:` with base32 or
/// hexadecimal digits, or `sha256-` with base64.
pub(crate) fn parse_sha256(text: &str) -> Option<Sha256> {
    if let Some(digits) = text.strip_prefix("sha256:") {
        if let Some(bytes) = base32::decode::<32>(digits) {
            return Some(bytes.into());
        }
        let mut bytes = [0; 32];
        hex::decode_into(digits, &mut bytes)?;
        return Some(bytes.into());
    }
    let mut decoded = [0; 33];
    let len = STANDARD
        .decode_slice(text.strip_prefix("sha256-")?, &mut decoded)
        .ok()?;
    decoded.get(..len)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_hash_format() {
        let empty = Sha256::of(b"");
        for text in [
            "sha256:0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73",
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
        ] {
            assert_eq!(parse_sha256(text), Some(empty), "{text}");
        }
    }

    #[test]
    fn rejects_other_hashes() {
        for text in ["", "sha256:", "sha1:0000", "md5-AAAA", "sha256-AAAA"] {
            assert_eq!(parse_sha256(text), None, "{text:?}");
        }
    }
}
