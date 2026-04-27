//! zstd wrapper for the Warm tier.

use tome_core::{Result, TomeError};

pub const DEFAULT_LEVEL: i32 = 9;

/// Maximum decompressed size we will accept. Wikipedia's largest articles are
/// well under this; any larger blob is treated as corrupt or hostile.
const MAX_DECOMPRESSED: usize = 50 * 1024 * 1024;

pub fn compress(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::bulk::compress(data, level).map_err(|e| TomeError::Storage(format!("zstd compress: {e}")))
}

pub fn decompress(compressed: &[u8]) -> Result<Vec<u8>> {
    zstd::bulk::decompress(compressed, MAX_DECOMPRESSED)
        .map_err(|e| TomeError::Storage(format!("zstd decompress: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short_text() {
        let data = b"The quick brown fox jumps over the lazy dog.";
        let compressed = compress(data, DEFAULT_LEVEL).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn round_trip_long_text_actually_compresses() {
        let data = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(500);
        let compressed = compress(data.as_bytes(), DEFAULT_LEVEL).unwrap();
        assert!(
            compressed.len() < data.len() / 2,
            "expected at least 2x compression on repetitive input, got {} -> {}",
            data.len(),
            compressed.len()
        );
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data.as_bytes());
    }

    #[test]
    fn empty_input_round_trips() {
        let data: &[u8] = &[];
        let compressed = compress(data, DEFAULT_LEVEL).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn corrupt_blob_is_rejected() {
        let err = decompress(b"not actually zstd").unwrap_err();
        assert!(matches!(err, TomeError::Storage(_)));
    }
}
