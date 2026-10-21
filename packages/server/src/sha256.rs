//! SHA-256 digests.

use core::fmt;
use core::str::FromStr;

use aws_lc_rs::digest::{self, SHA256};
use serde::de::{self, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::hex::{self, Hex};

/// A SHA-256 digest.
///
/// It displays and parses as 64 lowercase hexadecimal digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Sha256([u8; 32]);

impl Sha256 {
    /// The digest of `data`.
    pub(crate) fn of(data: &[u8]) -> Self {
        Self::from_digest(&digest::digest(&SHA256, data))
    }

    /// Takes the bytes of a SHA-256 digest.
    fn from_digest(digest: &digest::Digest) -> Self {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(digest.as_ref());
        Self(bytes)
    }

    /// The digest bytes.
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for Sha256 {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for Sha256 {
    type Error = InvalidSha256;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        bytes.try_into().map(Self).map_err(|_length| InvalidSha256)
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Hex(&self.0).fmt(f)
    }
}

impl FromStr for Sha256 {
    type Err = InvalidSha256;

    /// Parses 64 lowercase hexadecimal digits.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return Err(InvalidSha256);
        }
        let mut bytes = [0; 32];
        hex::decode_into(text, &mut bytes).ok_or(InvalidSha256)?;
        Ok(Self(bytes))
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Parses the text form.
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = Sha256;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("64 lowercase hexadecimal digits")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Sha256, E> {
                v.parse()
                    .map_err(|_invalid| E::invalid_value(Unexpected::Str(v), &self))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

/// Computes a SHA-256 digest of data that arrives in parts.
#[derive(Clone)]
pub(crate) struct Hasher(digest::Context);

impl Hasher {
    /// Starts an empty digest.
    pub(crate) fn new() -> Self {
        Self(digest::Context::new(&SHA256))
    }

    /// Adds data.
    pub(crate) fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    /// Returns the digest of the data so far.
    pub(crate) fn finish(self) -> Sha256 {
        Sha256::from_digest(&self.0.finish())
    }
}

impl fmt::Debug for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hasher").finish_non_exhaustive()
    }
}

/// Bytes or text that aren't a SHA-256 digest.
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("not a SHA-256 digest")]
pub(crate) struct InvalidSha256;

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "000fa5ff000fa5ff000fa5ff000fa5ff000fa5ff000fa5ff000fa5ff000fa5ff";

    #[test]
    fn round_trips_text() {
        let digest: Sha256 = TEXT.parse().unwrap();
        assert_eq!(digest.as_bytes()[..4], [0x00, 0x0F, 0xA5, 0xFF], "bytes");
        assert_eq!(digest.to_string(), TEXT, "text");
    }

    #[test]
    fn rejects_other_text() {
        for text in [
            TEXT.trim_start_matches('0'),
            &TEXT.to_uppercase(),
            &TEXT.replace('0', "g"),
        ] {
            assert!(text.parse::<Sha256>().is_err(), "{text} is rejected");
        }
    }
}
