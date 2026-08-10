use std::fs;
use std::hash::Hasher;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest as _, Sha256};

/// Computes the SHA-256 digest of one file without loading it into memory.
pub fn sha256_file(path: &Path) -> io::Result<[u8; 32]> {
    let file = fs::File::open(path)?;

    sha256_reader(file)
}

/// Computes the SHA-256 digest of one byte stream without loading it into memory.
pub fn sha256_reader(mut reader: impl Read) -> io::Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];

    loop {
        let length = reader.read(&mut buffer)?;

        if length == 0 {
            break;
        }

        hasher.update(&buffer[..length]);
    }

    Ok(hasher.finalize().into())
}

/// Incremental BLAKE3 state with platform-independent primitive encoding.
pub struct StableDigestHasher(blake3::Hasher);

impl StableDigestHasher {
    /// Creates an empty stable digest.
    pub fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    /// Finalizes the complete 256-bit digest.
    pub fn finalize(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

impl Default for StableDigestHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for StableDigestHasher {
    fn finish(&self) -> u64 {
        let digest = self.0.clone().finalize();
        let mut prefix = [0; 8];

        prefix.copy_from_slice(&digest.as_bytes()[..8]);

        u64::from_le_bytes(prefix)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&value.to_le_bytes());
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.write_i64(i64::try_from(value).unwrap_or_else(|_| {
            if value.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::hash::Hasher;

    use super::{StableDigestHasher, sha256_file};

    #[test]
    fn file_sha256_is_computed_incrementally() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("temporary directory must be available");
        };

        let path = directory.path().join("input");

        if let Err(error) = fs::write(&path, b"bray") {
            panic!("test input must be written: {error}");
        }

        let Ok(digest) = sha256_file(&path) else {
            panic!("test input must be readable");
        };

        assert_eq!(
            digest,
            [
                0x87, 0x7f, 0xba, 0x91, 0x41, 0xff, 0x29, 0x30, 0xdb, 0x14, 0xe9, 0x78, 0x16, 0xa1,
                0xf9, 0xac, 0x5e, 0x57, 0x88, 0xb7, 0x7a, 0x13, 0xda, 0xf3, 0x30, 0xa4, 0x56, 0xf8,
                0x68, 0x12, 0x79, 0xb9,
            ]
        );
    }

    #[test]
    fn primitive_encoding_is_explicitly_little_endian() {
        let mut encoded = StableDigestHasher::new();
        let mut bytes = StableDigestHasher::new();

        encoded.write_u32(0x1234_5678);
        bytes.write(&[0x78, 0x56, 0x34, 0x12]);

        assert_eq!(encoded.finalize(), bytes.finalize());
    }
}
