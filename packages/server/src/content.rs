//! Deduplicated content: blobs made of chunks in object storage.
//!
//! Postgres is the source of truth. Chunks and blobs are shared by every
//! cache and identified by the SHA-256 of their plain content. Object keys
//! are random, so a race between two writers of the same chunk leaves one
//! object that the loser deletes.

mod chunker;
mod collect;
mod read;
mod tier;
mod upload;
mod write;
mod zstd;

use core::time::Duration;

use aws_lc_rs::error::Unspecified;
use aws_lc_rs::rand;
use object_store::path::Path;

pub(crate) use self::collect::{CollectError, CollectStats};
pub(crate) use self::read::Encoding;
pub(crate) use self::tier::{Placement, TierError, TierStats};
pub(crate) use self::write::WriteError;
use crate::hex::Hex;
use crate::store::{StoreName, Stores};

/// The prefix of chunk objects within a store.
const CHUNK_PREFIX: &str = "chunks";

/// Content in object storage, indexed in Postgres.
#[derive(Debug)]
pub(crate) struct Content {
    /// The stores that hold chunk objects.
    stores: Stores,
    /// How long unreferenced chunks and blobs are kept, so that uploads in
    /// progress can still reference them.
    grace: Duration,
}

impl Content {
    /// Wraps the stores.
    pub(crate) const fn new(stores: Stores, grace: Duration) -> Self {
        Self { stores, grace }
    }

    /// The names of the configured stores.
    pub(crate) fn store_names(&self) -> impl Iterator<Item = &StoreName> {
        self.stores.iter().map(|(name, _)| name)
    }
}

/// The object key of a new chunk.
fn new_object_key() -> Result<Path, Unspecified> {
    let mut random = [0u8; 16];
    rand::fill(&mut random)?;
    Ok(Path::from_iter([CHUNK_PREFIX, &Hex(&random).to_string()]))
}
