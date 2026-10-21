//! What Nix knows about a store path.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::base32;
use super::store_path::StorePath;
use crate::hex;

/// The length of a SHA-256 hash.
const SHA256_LEN: usize = 32;

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
    pub(crate) system: Option<String>,
    /// The content address of content-addressed paths.
    pub(crate) ca: Option<String>,
    /// The SHA-256 of the uncompressed NAR.
    pub(crate) nar_hash: Vec<u8>,
    /// The size of the uncompressed NAR.
    pub(crate) nar_size: u64,
}

/// Parses a SHA-256 hash as Nix prints it: `sha256:` with base32 or
/// hexadecimal digits, or `sha256-` with base64.
pub(crate) fn parse_sha256(text: &str) -> Option<Vec<u8>> {
    let hash = if let Some(digits) = text.strip_prefix("sha256:") {
        match digits.len() {
            52 => base32::decode(digits, SHA256_LEN)?,
            64 => hex::decode(digits)?,
            _ => return None,
        }
    } else {
        STANDARD.decode(text.strip_prefix("sha256-")?).ok()?
    };
    (hash.len() == SHA256_LEN).then_some(hash)
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::digest::{SHA256, digest};

    use super::*;

    #[test]
    fn parses_every_hash_format() {
        let empty = digest(&SHA256, b"").as_ref().to_vec();
        for text in [
            "sha256:0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73",
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
        ] {
            assert_eq!(parse_sha256(text), Some(empty.clone()), "{text}");
        }
    }

    #[test]
    fn rejects_other_hashes() {
        for text in ["", "sha256:", "sha1:0000", "md5-AAAA", "sha256-AAAA"] {
            assert_eq!(parse_sha256(text), None, "{text:?}");
        }
    }
}
