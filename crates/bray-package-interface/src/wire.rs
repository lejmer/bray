use std::array::TryFromSliceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WireDecodeError {
    Truncated {
        offset: usize,
        expected_length: usize,
        actual_length: usize,
    },
    TrailingBytes {
        offset: usize,
        count: usize,
    },
}

impl WireDecodeError {
    pub(crate) const fn offset(self) -> usize {
        match self {
            Self::Truncated { offset, .. } | Self::TrailingBytes { offset, .. } => offset,
        }
    }
}

pub(crate) struct WireReader<'bytes> {
    bytes: &'bytes [u8],
    position: usize,
}

impl<'bytes> WireReader<'bytes> {
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, WireDecodeError> {
        Ok(u8::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, WireDecodeError> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, WireDecodeError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_u64(&mut self) -> Result<u64, WireDecodeError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<&'bytes [u8], WireDecodeError> {
        let Some(remaining) = self.bytes.get(self.position..) else {
            return Err(WireDecodeError::Truncated {
                offset: self.position,
                expected_length: length,
                actual_length: 0,
            });
        };

        let bytes = remaining.get(..length).ok_or(WireDecodeError::Truncated {
            offset: self.position,
            expected_length: length,
            actual_length: remaining.len(),
        })?;

        self.position += length;

        Ok(bytes)
    }

    pub(crate) const fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    pub(crate) const fn position(&self) -> usize {
        self.position
    }

    pub(crate) fn finish(self) -> Result<(), WireDecodeError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(WireDecodeError::TrailingBytes {
                offset: self.position,
                count: self.bytes.len() - self.position,
            })
        }
    }

    pub(crate) fn read_array<const LENGTH: usize>(
        &mut self,
    ) -> Result<[u8; LENGTH], WireDecodeError> {
        let offset = self.position;
        let bytes = self.read_bytes(LENGTH)?;

        bytes
            .try_into()
            .map_err(|_: TryFromSliceError| WireDecodeError::Truncated {
                offset,
                expected_length: LENGTH,
                actual_length: bytes.len(),
            })
    }
}

pub(crate) struct WireEncoder {
    bytes: Vec<u8>,
}

impl WireEncoder {
    pub(crate) const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn write_u8(&mut self, value: u8) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(crate) fn write_count(&mut self, count: usize) {
        let count = u32::try_from(count)
            .unwrap_or_else(|_| unreachable!("validated artifact counts fit the wire format"));

        self.write_u32(count);
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{WireDecodeError, WireEncoder, WireReader};

    #[test]
    fn fixed_width_values_use_little_endian_encoding() {
        let mut encoder = WireEncoder::new();

        encoder.write_u8(0x11);
        encoder.write_u16(0x2233);
        encoder.write_u32(0x3344_5566);
        encoder.write_u64(0x7788_99aa_bbcc_ddee);

        assert_eq!(
            encoder.bytes(),
            &[
                0x11, 0x33, 0x22, 0x66, 0x55, 0x44, 0x33, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88,
                0x77,
            ]
        );

        let mut reader = WireReader::new(encoder.bytes());

        assert_eq!(reader.read_u8(), Ok(0x11));
        assert_eq!(reader.read_u16(), Ok(0x2233));
        assert_eq!(reader.read_u32(), Ok(0x3344_5566));
        assert_eq!(reader.read_u64(), Ok(0x7788_99aa_bbcc_ddee));
    }

    #[test]
    fn fixed_width_reads_reject_every_truncated_value() {
        for length in 0..8 {
            let mut reader = WireReader::new(&[0; 8][..length]);

            assert_eq!(
                reader.read_u64(),
                Err(WireDecodeError::Truncated {
                    offset: 0,
                    expected_length: 8,
                    actual_length: length,
                })
            );
        }
    }

    #[test]
    fn length_delimited_reads_require_exact_consumption() {
        let mut reader = WireReader::new(&[1, 2, 3]);

        assert_eq!(reader.read_bytes(2), Ok(&[1, 2][..]));
        assert_eq!(
            reader.finish(),
            Err(WireDecodeError::TrailingBytes {
                offset: 2,
                count: 1,
            })
        );

        let mut reader = WireReader::new(&[1, 2, 3]);

        assert_eq!(reader.read_bytes(3), Ok(&[1, 2, 3][..]));
        assert_eq!(reader.finish(), Ok(()));
    }
}
