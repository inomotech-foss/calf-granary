//! zstd frames, with one reused context per thread.

use core::cell::RefCell;
use std::io;

use zstd::bulk::{Compressor, Decompressor};

/// The zstd level of new chunks.
pub(super) const LEVEL: i32 = 8;

thread_local! {
    /// The compression context of this thread.
    static COMPRESSOR: RefCell<Option<Compressor<'static>>> = const { RefCell::new(None) };
    /// The decompression context of this thread.
    static DECOMPRESSOR: RefCell<Option<Decompressor<'static>>> = const { RefCell::new(None) };
}

/// Compresses data into one frame.
pub(super) fn compress(data: &[u8]) -> io::Result<Vec<u8>> {
    COMPRESSOR.with_borrow_mut(|compressor| {
        let compressor = match compressor {
            Some(compressor) => compressor,
            None => compressor.insert(Compressor::new(LEVEL)?),
        };
        compressor.compress(data)
    })
}

/// Decompresses a frame of at most `capacity` bytes.
pub(super) fn decompress(frame: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    DECOMPRESSOR.with_borrow_mut(|decompressor| {
        let decompressor = match decompressor {
            Some(decompressor) => decompressor,
            None => decompressor.insert(Decompressor::new()?),
        };
        decompressor.decompress(frame, capacity)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let data = b"granary granary granary granary".repeat(100);
        for _ in 0..2 {
            let frame = compress(&data).unwrap();
            assert!(frame.len() < data.len(), "repetition compresses");
            assert_eq!(
                decompress(&frame, data.len()).unwrap(),
                data,
                "the context is reusable"
            );
        }
    }
}
