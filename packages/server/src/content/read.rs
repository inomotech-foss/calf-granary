//! Reading content.

use std::io;
use std::sync::Arc;

use async_compression::Level;
use async_compression::tokio::bufread::{ZstdDecoder, ZstdEncoder};
use bytes::Bytes;
use calf_granary_server_db::client::GenericClient;
use calf_granary_server_db::queries::content::{blob_chunks, zstd_size};
use calf_granary_server_db::types::ChunkCompression;
use futures::stream::{self, BoxStream};
use futures::{StreamExt as _, TryStreamExt as _};
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt as _};
use tokio::task::{self, JoinError};
use tokio_util::io::{ReaderStream, StreamReader};

use super::{Content, zstd};

/// The largest chunk that is converted in memory. Larger ones, which only
/// whole content has, are converted as a stream.
const MAX_BULK_SIZE: usize = 4 * 1024 * 1024;

/// How a blob's bytes are delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// The plain content.
    Plain,
    /// One zstd frame per chunk, which zstd decoders read as one stream.
    Zstd,
}

impl From<ChunkCompression> for Encoding {
    fn from(compression: ChunkCompression) -> Self {
        match compression {
            ChunkCompression::Plain => Self::Plain,
            ChunkCompression::Zstd => Self::Zstd,
        }
    }
}

/// A chunk to read.
struct Part {
    /// The store that holds the object.
    store: Arc<dyn ObjectStore>,
    /// The object.
    key: Path,
    /// How the object is stored.
    stored: Encoding,
    /// The plain size in bytes.
    size: usize,
    /// How the chunk is delivered.
    encoding: Encoding,
}

impl Part {
    /// Streams the chunk in its encoding.
    ///
    /// Chunks that are stored in the requested encoding are streamed as they
    /// are. Small ones are converted whole, large ones as a stream.
    async fn open(self) -> Result<BoxStream<'static, Result<Bytes, ChunkError>>, ChunkError> {
        let object = self.store.get(&self.key).await.map_err(ChunkError::Store)?;
        if self.stored == self.encoding {
            return Ok(object.into_stream().map_err(ChunkError::Store).boxed());
        }
        if self.size > MAX_BULK_SIZE {
            let reader = StreamReader::new(object.into_stream().map_err(io::Error::other));
            let converted = match self.stored {
                Encoding::Zstd => ReaderStream::new(ZstdDecoder::new(reader)).boxed(),
                Encoding::Plain => ReaderStream::new(ZstdEncoder::with_quality(
                    reader,
                    Level::Precise(zstd::LEVEL),
                ))
                .boxed(),
            };
            return Ok(converted.map_err(ChunkError::Convert).boxed());
        }
        let stored = object.bytes().await.map_err(ChunkError::Store)?;
        let converted = task::spawn_blocking(move || match self.stored {
            Encoding::Zstd => zstd::decompress(&stored, self.size),
            Encoding::Plain => zstd::compress(&stored),
        })
        .await
        .map_err(ChunkError::Task)?
        .map_err(ChunkError::Convert)?;
        Ok(stream::once(async { Ok(Bytes::from(converted)) }).boxed())
    }
}

impl Content {
    /// The size of a blob in the zstd encoding, if every chunk is stored as
    /// zstd and the frames are served as they are.
    pub(crate) async fn zstd_size<C>(
        &self,
        client: &C,
        blob_id: i64,
    ) -> Result<Option<u64>, tokio_postgres::Error>
    where
        C: GenericClient + Sync,
    {
        let size = zstd_size().bind(client, &blob_id).one().await?;
        Ok(size.and_then(|size| u64::try_from(size).ok()))
    }

    /// Streams the content of a blob in the encoding.
    ///
    /// The chunk list is read up front, so the stream doesn't hold a
    /// database connection.
    pub(crate) async fn read<C>(
        &self,
        client: &C,
        blob_id: i64,
        encoding: Encoding,
    ) -> Result<BoxStream<'static, Result<Bytes, ChunkError>>, ReadError>
    where
        C: GenericClient + Sync,
    {
        let mut parts = Vec::new();
        for chunk in blob_chunks().bind(client, &blob_id).all().await? {
            let store = self
                .stores
                .get(&chunk.store)
                .ok_or_else(|| ReadError::UnknownStore(chunk.store.clone()))?;
            parts.push(Part {
                store: Arc::clone(store),
                key: Path::from(chunk.object_key),
                stored: chunk.compression.into(),
                encoding,
                size: usize::try_from(chunk.size).map_err(|_overflow| ReadError::TooLarge)?,
            });
        }
        Ok(stream::iter(parts).then(Part::open).try_flatten().boxed())
    }
}

/// Errors from reading a blob's chunk list.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// Looking up the chunks failed.
    #[error("failed to look up the chunks")]
    Database(#[from] tokio_postgres::Error),
    /// A chunk is larger than memory can hold.
    #[error("a chunk is too large")]
    TooLarge,
    /// A chunk is in a store that isn't configured.
    #[error("the store {0} isn't configured")]
    UnknownStore(String),
}

/// Errors from streaming a chunk.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ChunkError {
    /// The chunk can't be compressed, or isn't a valid zstd frame of the
    /// recorded size.
    #[error("failed to convert a chunk")]
    Convert(#[source] io::Error),
    /// Object storage failed.
    #[error("failed to read a chunk")]
    Store(#[source] object_store::Error),
    /// A conversion task panicked.
    #[error("the conversion task failed")]
    Task(#[source] JoinError),
}
