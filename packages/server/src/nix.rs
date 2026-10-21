//! The Nix binary cache protocol.

mod base32;
mod narinfo;
mod path_info;
mod signing;
mod store_path;

pub(crate) use self::narinfo::{NarInfo, UploadedNarInfo};
pub(crate) use self::path_info::{PathInfo, parse_sha256};
#[cfg(test)]
pub(crate) use self::signing::tests as signing_tests;
pub(crate) use self::signing::{SigningKey, SigningKeyError};
pub(crate) use self::store_path::{InvalidStorePath, StorePath, StorePathHash};

/// The store directory that every cache serves.
pub(crate) const STORE_DIR: &str = "/nix/store";
