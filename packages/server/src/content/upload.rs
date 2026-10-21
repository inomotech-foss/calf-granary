//! Multipart uploads that clean up after themselves.

use core::error::Error;

use bytes::Bytes;
use object_store::WriteMultipart;
use tokio::runtime::Handle;

/// A multipart upload that is aborted if it's dropped before it finishes.
///
/// Dropping happens when the client disconnects, because the request handler
/// is dropped with it.
#[derive(Debug)]
pub(super) struct Upload {
    /// The writer, until the upload finishes.
    writer: Option<WriteMultipart>,
}

impl Upload {
    /// Wraps a writer.
    pub(super) const fn new(writer: WriteMultipart) -> Self {
        Self {
            writer: Some(writer),
        }
    }

    /// Queues data, waiting until fewer than `concurrency` parts are in
    /// flight.
    pub(super) async fn put(
        &mut self,
        bytes: Bytes,
        concurrency: usize,
    ) -> Result<(), object_store::Error> {
        if let Some(writer) = &mut self.writer {
            writer.wait_for_capacity(concurrency).await?;
            writer.put(bytes);
        }
        Ok(())
    }

    /// Uploads the remaining data and completes the upload.
    pub(super) async fn finish(mut self) -> Result<(), object_store::Error> {
        match self.writer.take() {
            Some(writer) => writer.finish().await.map(drop),
            None => Ok(()),
        }
    }
}

impl Drop for Upload {
    fn drop(&mut self) {
        let Some(writer) = self.writer.take() else {
            return;
        };
        let Ok(runtime) = Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
            if let Err(err) = writer.abort().await {
                tracing::warn!(
                    error = &err as &dyn Error,
                    "failed to abort an unfinished upload"
                );
            }
        });
    }
}
