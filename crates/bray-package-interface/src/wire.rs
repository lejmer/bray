use std::array::TryFromSliceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WireDecodeError {
    Truncated,
}

pub(crate) struct WireReader<'bytes> {
    bytes: &'bytes [u8],
    position: usize,
}

impl<'bytes> WireReader<'bytes> {
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, position: 0 }
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

    pub(crate) fn read_array<const LENGTH: usize>(
        &mut self,
    ) -> Result<[u8; LENGTH], WireDecodeError> {
        let end = self
            .position
            .checked_add(LENGTH)
            .ok_or(WireDecodeError::Truncated)?;

        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(WireDecodeError::Truncated)?;

        self.position = end;

        bytes
            .try_into()
            .map_err(|_: TryFromSliceError| WireDecodeError::Truncated)
    }
}

pub(crate) struct WireEncoder {
    bytes: Vec<u8>,
}

impl WireEncoder {
    pub(crate) const fn new() -> Self {
        Self { bytes: Vec::new() }
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

    #[cfg(test)]
    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[cfg(test)]
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
        encoder.write_u16(0x1122);
        encoder.write_u32(0x3344_5566);
        encoder.write_u64(0x7788_99aa_bbcc_ddee);

        assert_eq!(
            encoder.bytes(),
            &[
                0x22, 0x11, 0x66, 0x55, 0x44, 0x33, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77,
            ]
        );

        let mut reader = WireReader::new(encoder.bytes());
        assert_eq!(reader.read_u16(), Ok(0x1122));
        assert_eq!(reader.read_u32(), Ok(0x3344_5566));
        assert_eq!(reader.read_u64(), Ok(0x7788_99aa_bbcc_ddee));
    }

    #[test]
    fn fixed_width_reads_reject_every_truncated_value() {
        for length in 0..8 {
            let mut reader = WireReader::new(&[0; 8][..length]);
            assert_eq!(reader.read_u64(), Err(WireDecodeError::Truncated));
        }
    }
}
