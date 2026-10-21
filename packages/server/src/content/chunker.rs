//! Content-defined chunking with `FastCDC`.
//!
//! Cut points depend only on nearby bytes, so an insertion early in a file
//! leaves the later chunks unchanged. Changing the sizes moves every cut
//! point and ends deduplication with existing chunks.

use bytes::{Bytes, BytesMut};
use fastcdc::v2020::FastCDC;

/// The smallest chunk, except for the last one.
const MIN_SIZE: usize = 16 * 1024;

/// The average chunk size the cut points aim for.
const AVG_SIZE: usize = 64 * 1024;

/// The largest chunk.
pub(super) const MAX_SIZE: usize = 256 * 1024;

/// Splits a byte stream into chunks.
#[derive(Debug, Default)]
pub(super) struct Chunker {
    /// Bytes that aren't part of a finished chunk yet.
    buffer: BytesMut,
}

impl Chunker {
    /// Adds bytes and returns the chunks that are certain now.
    ///
    /// The last cut point in the buffer may still move when more bytes
    /// arrive, so the bytes after the second to last one are kept.
    pub(super) fn push(&mut self, bytes: &[u8]) -> Vec<Bytes> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() < MAX_SIZE.saturating_mul(2) {
            return Vec::new();
        }
        let mut lengths = self.lengths();
        lengths.pop();
        self.split(&lengths)
    }

    /// Returns the remaining chunks.
    pub(super) fn finish(mut self) -> Vec<Bytes> {
        let lengths = self.lengths();
        self.split(&lengths)
    }

    /// The lengths of the chunks in the buffer.
    fn lengths(&self) -> Vec<usize> {
        FastCDC::new(&self.buffer, MIN_SIZE, AVG_SIZE, MAX_SIZE)
            .map(|chunk| chunk.length)
            .collect()
    }

    /// Takes chunks of the given lengths from the front of the buffer.
    fn split(&mut self, lengths: &[usize]) -> Vec<Bytes> {
        lengths
            .iter()
            .map(|length| self.buffer.split_to(*length).freeze())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use core::iter;

    use super::*;

    fn data(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        iter::repeat_with(|| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state.to_be_bytes()[0]
        })
        .take(len)
        .collect()
    }

    fn chunk(bytes: &[u8], piece: usize) -> Vec<Bytes> {
        let mut chunker = Chunker::default();
        let mut chunks = Vec::new();
        for part in bytes.chunks(piece) {
            chunks.extend(chunker.push(part));
        }
        chunks.extend(chunker.finish());
        chunks
    }

    #[test]
    fn chunks_reassemble_the_input() {
        let input = data(3 * 1024 * 1024, 1);
        let chunks = chunk(&input, 10_000);
        assert_eq!(chunks.concat(), input, "no bytes are lost or reordered");
        assert!(
            chunks.iter().all(|chunk| chunk.len() <= MAX_SIZE),
            "no chunk exceeds the maximum"
        );
        assert!(chunks.len() > 10, "large input is split");
    }

    #[test]
    fn cut_points_ignore_how_bytes_arrive() {
        let input = data(2 * 1024 * 1024, 2);
        assert_eq!(
            chunk(&input, 1000),
            chunk(&input, 300_000),
            "the read size doesn't matter"
        );
    }

    #[test]
    fn insertions_keep_later_chunks() {
        let input = data(2 * 1024 * 1024, 3);
        let mut changed = b"inserted".to_vec();
        changed.extend_from_slice(&input);
        let original = chunk(&input, 65_536);
        let shifted = chunk(&changed, 65_536);
        let shared = shifted
            .iter()
            .filter(|chunk| original.contains(chunk))
            .count();
        assert!(
            shared >= original.len().saturating_sub(2),
            "only the chunks near the change differ"
        );
    }
}
