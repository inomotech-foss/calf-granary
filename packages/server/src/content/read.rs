//! Reading content.

use std::sync::Arc;

use bytes::Bytes;
use calf_granary_server_db::client::GenericClient;
use calf_granary_server_db::queries::content::blob_chunks;
use futures::stream::BoxStream;
use futures::{StreamExt as _, TryStreamExt as _, stream};
use object_store::path::Path;
use object_store::{GetResult, ObjectStore, ObjectStoreExt as _};

use super::{Content, UNCOMPRESSED};

impl Content {
    /// Streams the plain content of a blob.
    ///
    /// The chunk list is read up front, so the stream doesn't hold a
    /// database connection.
    pub(crate) async fn read<C>(
        &self,
        client: &C,
        blob_id: i64,
    ) -> Result<BoxStream<'static, Result<Bytes, object_store::Error>>, ReadError>
    where
        C: GenericClient + Sync,
    {
        let mut objects: Vec<(Arc<dyn ObjectStore>, Path)> = Vec::new();
        for chunk in blob_chunks().bind(client, &blob_id).all().await? {
            if chunk.compression != UNCOMPRESSED {
                return Err(ReadError::Compression(chunk.compression));
            }
            let store = self
                .stores
                .get(&chunk.store)
                .ok_or_else(|| ReadError::UnknownStore(chunk.store.clone()))?;
            objects.push((Arc::clone(store), Path::from(chunk.object_key)));
        }
        Ok(stream::iter(objects)
            .then(async |(store, key)| store.get(&key).await.map(GetResult::into_stream))
            .try_flatten()
            .boxed())
    }
}

/// Errors from reading content.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// A chunk uses a compression that can't be read yet.
    #[error("unsupported chunk compression {0:?}")]
    Compression(String),
    /// Looking up the chunks failed.
    #[error("failed to look up the chunks")]
    Database(#[from] tokio_postgres::Error),
    /// A chunk is in a store that isn't configured.
    #[error("the store {0} isn't configured")]
    UnknownStore(String),
}
