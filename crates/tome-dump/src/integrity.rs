//! SHA1 verification for dump files.
//!
//! Wikipedia publishes a `*-sha1sums.txt` file alongside each dump. We
//! stream-hash the dump file and compare against the expected hex digest;
//! the file never lives entirely in memory.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha1::{Digest, Sha1};
use tome_core::{Result, TomeError};

const HASH_BUF_SIZE: usize = 1024 * 1024; // 1 MiB chunks

/// Compute the SHA1 of `path` and return its lowercase hex digest.
pub fn sha1_hex(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(HASH_BUF_SIZE, file);
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; HASH_BUF_SIZE];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify the file at `path` hashes to `expected_hex` (case-insensitive).
pub fn verify_sha1(path: &Path, expected_hex: &str) -> Result<()> {
    let actual = sha1_hex(path)?;
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(TomeError::Integrity(format!(
            "sha1 mismatch: expected {expected_hex}, got {actual}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn known_sha1_of_empty_string() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"").unwrap();
        f.flush().unwrap();
        // SHA1 of empty input is the well-known constant.
        assert_eq!(
            sha1_hex(f.path()).unwrap(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn known_sha1_of_abc() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"abc").unwrap();
        f.flush().unwrap();
        assert_eq!(
            sha1_hex(f.path()).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn verify_succeeds_on_match() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello").unwrap();
        f.flush().unwrap();
        verify_sha1(f.path(), "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d").unwrap();
    }

    #[test]
    fn verify_succeeds_case_insensitive() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello").unwrap();
        f.flush().unwrap();
        verify_sha1(f.path(), "AAF4C61DDCC5E8A2DABEDE0F3B482CD9AEA9434D").unwrap();
    }

    #[test]
    fn verify_fails_on_mismatch() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello").unwrap();
        f.flush().unwrap();
        let err = verify_sha1(f.path(), "0000000000000000000000000000000000000000").unwrap_err();
        assert!(matches!(err, TomeError::Integrity(_)));
    }
}
