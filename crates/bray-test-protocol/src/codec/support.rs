use std::io::{Read, Write};

use crate::TestErrorTypeIdentity;

pub(super) const PROTOCOL_VERSION: u32 = 1;
pub(super) const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_COLLECTION_ITEMS: usize = 1_000_000;
pub(super) const MAX_STRING_BYTES: usize = 1024 * 1024;

/// Returns the wire revision shared by test catalogs and runner messages.
pub const fn protocol_version() -> u32 {
    PROTOCOL_VERSION
}

/// Failure to encode or decode a bounded test protocol record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestProtocolError {
    /// The underlying protocol stream rejected an operation.
    Io,
    /// The record does not satisfy the canonical encoding contract.
    Malformed,
    /// The record uses an unsupported protocol format version.
    UnsupportedVersion(u32),
    /// A declared or accumulated allocation exceeds a protocol bound.
    ResourceLimit,
}

pub(super) struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    pub(super) fn new(magic: &[u8; 8]) -> Self {
        let mut bytes = Vec::with_capacity(128);

        bytes.extend_from_slice(magic);
        bytes.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());

        Self { bytes }
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn bytes(&mut self, value: &[u8]) -> Result<(), TestProtocolError> {
        self.length(value.len())?;
        self.bytes.extend_from_slice(value);

        Ok(())
    }

    pub(super) fn string(&mut self, value: &str) -> Result<(), TestProtocolError> {
        if value.len() > MAX_STRING_BYTES {
            return Err(TestProtocolError::ResourceLimit);
        }

        self.bytes(value.as_bytes())
    }

    pub(super) fn length(&mut self, value: usize) -> Result<(), TestProtocolError> {
        let value = u32::try_from(value).map_err(|_| TestProtocolError::ResourceLimit)?;

        self.u32(value);

        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<u8>, TestProtocolError> {
        if self.bytes.len() > MAX_FRAME_BYTES {
            return Err(TestProtocolError::ResourceLimit);
        }

        Ok(self.bytes)
    }
}

pub(super) struct Decoder<'bytes> {
    bytes: &'bytes [u8],
    position: usize,
}

impl<'bytes> Decoder<'bytes> {
    pub(super) fn new(bytes: &'bytes [u8], magic: &[u8; 8]) -> Result<Self, TestProtocolError> {
        if bytes.len() > MAX_FRAME_BYTES || bytes.len() < 12 || &bytes[..8] != magic {
            return Err(TestProtocolError::Malformed);
        }

        let version = u32::from_le_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| TestProtocolError::Malformed)?,
        );

        if version != PROTOCOL_VERSION {
            return Err(TestProtocolError::UnsupportedVersion(version));
        }

        Ok(Self {
            bytes,
            position: 12,
        })
    }

    pub(super) fn u8(&mut self) -> Result<u8, TestProtocolError> {
        let value = *self
            .bytes
            .get(self.position)
            .ok_or(TestProtocolError::Malformed)?;

        self.position += 1;

        Ok(value)
    }

    pub(super) fn u32(&mut self) -> Result<u32, TestProtocolError> {
        let bytes = self.take(4)?;

        Ok(u32::from_le_bytes(
            bytes.try_into().map_err(|_| TestProtocolError::Malformed)?,
        ))
    }

    pub(super) fn u64(&mut self) -> Result<u64, TestProtocolError> {
        let bytes = self.take(8)?;

        Ok(u64::from_le_bytes(
            bytes.try_into().map_err(|_| TestProtocolError::Malformed)?,
        ))
    }

    pub(super) fn length(&mut self) -> Result<usize, TestProtocolError> {
        let length = usize::try_from(self.u32()?).map_err(|_| TestProtocolError::ResourceLimit)?;

        if length > MAX_COLLECTION_ITEMS {
            return Err(TestProtocolError::ResourceLimit);
        }

        Ok(length)
    }

    pub(super) fn bytes(&mut self) -> Result<&'bytes [u8], TestProtocolError> {
        let length = usize::try_from(self.u32()?).map_err(|_| TestProtocolError::ResourceLimit)?;

        if length > MAX_FRAME_BYTES {
            return Err(TestProtocolError::ResourceLimit);
        }

        self.take(length)
    }

    pub(super) fn string(&mut self) -> Result<&'bytes str, TestProtocolError> {
        let bytes = self.bytes()?;

        if bytes.len() > MAX_STRING_BYTES {
            return Err(TestProtocolError::ResourceLimit);
        }

        std::str::from_utf8(bytes).map_err(|_| TestProtocolError::Malformed)
    }

    pub(super) fn finish(self) -> Result<(), TestProtocolError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(TestProtocolError::Malformed)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'bytes [u8], TestProtocolError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(TestProtocolError::ResourceLimit)?;

        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(TestProtocolError::Malformed)?;

        self.position = end;

        Ok(bytes)
    }
}

pub(super) fn encode_optional_error_type(
    encoder: &mut Encoder,
    error_type: Option<&TestErrorTypeIdentity>,
) -> Result<(), TestProtocolError> {
    match error_type {
        Some(error_type) => {
            encoder.u8(1);
            encoder.string(error_type.as_str())?;
        }
        None => encoder.u8(0),
    }

    Ok(())
}

pub(super) fn decode_optional_error_type(
    decoder: &mut Decoder<'_>,
) -> Result<Option<TestErrorTypeIdentity>, TestProtocolError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => TestErrorTypeIdentity::try_new(decoder.string()?)
            .map(Some)
            .ok_or(TestProtocolError::Malformed),
        _ => Err(TestProtocolError::Malformed),
    }
}

pub(super) fn write_frame(
    writer: &mut impl Write,
    payload: &[u8],
) -> Result<(), TestProtocolError> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(TestProtocolError::ResourceLimit);
    }

    let length = u32::try_from(payload.len()).map_err(|_| TestProtocolError::ResourceLimit)?;

    writer
        .write_all(&length.to_le_bytes())
        .and_then(|()| writer.write_all(payload))
        .map_err(|_| TestProtocolError::Io)
}

pub(super) fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, TestProtocolError> {
    let mut length = [0; 4];

    reader
        .read_exact(&mut length)
        .map_err(|_| TestProtocolError::Io)?;

    let length = usize::try_from(u32::from_le_bytes(length))
        .map_err(|_| TestProtocolError::ResourceLimit)?;

    if length > MAX_FRAME_BYTES {
        return Err(TestProtocolError::ResourceLimit);
    }

    let mut payload = vec![0; length];

    reader
        .read_exact(&mut payload)
        .map_err(|_| TestProtocolError::Io)?;

    Ok(payload)
}
