use std::hash::Hasher;

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
    use std::hash::Hasher;

    use super::StableDigestHasher;

    #[test]
    fn primitive_encoding_is_explicitly_little_endian() {
        let mut encoded = StableDigestHasher::new();
        let mut bytes = StableDigestHasher::new();

        encoded.write_u32(0x1234_5678);
        bytes.write(&[0x78, 0x56, 0x34, 0x12]);

        assert_eq!(encoded.finalize(), bytes.finalize());
    }
}
