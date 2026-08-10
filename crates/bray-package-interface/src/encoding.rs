use std::sync::Arc;

use crate::InterfaceValidationError;

const COMPRESSION_LEVEL: i32 = 3;
const FRAME_HEADER_DESCRIPTOR_OFFSET: usize = 4;
const MAXIMUM_WINDOW_LOG: u32 = 20;
const MAXIMUM_WINDOW_SIZE: u64 = 1 << MAXIMUM_WINDOW_LOG;
const MINIMUM_COMPRESSION_INPUT: usize = 1024;
const MINIMUM_COMPRESSION_SAVING: usize = 64;
const WINDOW_DESCRIPTOR_OFFSET: usize = 5;

const CONTENT_CHECKSUM_FLAG: u8 = 0b0000_0100;
const DICTIONARY_IDENTIFIER_MASK: u8 = 0b0000_0011;
const RESERVED_FRAME_HEADER_MASK: u8 = 0b0001_1000;
const SINGLE_SEGMENT_FLAG: u8 = 0b0010_0000;
const WINDOW_MANTISSA_MASK: u8 = 0b0000_0111;

/// Exact encoding of one package-interface section payload.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum InterfaceSectionEncoding {
    /// Revision-one uncompressed canonical section bytes.
    Raw = 0,
    /// Revision-one deterministic Zstandard frame.
    ZstdFrame = 1,
}

impl InterfaceSectionEncoding {
    pub(crate) const fn from_wire_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Raw),
            1 => Some(Self::ZstdFrame),
            _ => None,
        }
    }

    /// Returns the stable revisioned wire identifier.
    pub const fn wire_value(self) -> u8 {
        self as u8
    }
}

pub(crate) fn encode_section(
    decoded: &[u8],
) -> Result<(InterfaceSectionEncoding, Vec<u8>), InterfaceValidationError> {
    if decoded.len() < MINIMUM_COMPRESSION_INPUT {
        return Ok((InterfaceSectionEncoding::Raw, decoded.to_vec()));
    }

    let mut compressor = zstd::bulk::Compressor::new(COMPRESSION_LEVEL)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    compressor
        .include_checksum(true)
        .and_then(|_| compressor.include_contentsize(true))
        .and_then(|_| compressor.include_dictid(false))
        .and_then(|_| compressor.long_distance_matching(false))
        .and_then(|_| compressor.window_log(MAXIMUM_WINDOW_LOG))
        .map_err(|_| InterfaceValidationError::Malformed)?;

    let compressed = compressor
        .compress(decoded)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    if compressed
        .len()
        .checked_add(MINIMUM_COMPRESSION_SAVING)
        .is_some_and(|minimum_raw_length| minimum_raw_length <= decoded.len())
    {
        return Ok((InterfaceSectionEncoding::ZstdFrame, compressed));
    }

    Ok((InterfaceSectionEncoding::Raw, decoded.to_vec()))
}

pub(crate) fn validate_zstd_frame(
    encoded: &[u8],
    decoded_length: u64,
) -> Result<(), InterfaceValidationError> {
    let frame_length = zstd::zstd_safe::find_frame_compressed_size(encoded)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    if frame_length != encoded.len() {
        return Err(InterfaceValidationError::Malformed);
    }

    validate_zstd_frame_header(encoded, decoded_length)?;

    let content_size = zstd::zstd_safe::get_frame_content_size(encoded)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    if content_size != Some(decoded_length) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn validate_zstd_frame_header(
    encoded: &[u8],
    decoded_length: u64,
) -> Result<(), InterfaceValidationError> {
    let descriptor = encoded
        .get(FRAME_HEADER_DESCRIPTOR_OFFSET)
        .copied()
        .ok_or(InterfaceValidationError::Malformed)?;

    let single_segment = descriptor & SINGLE_SEGMENT_FLAG != 0;
    let has_checksum = descriptor & CONTENT_CHECKSUM_FLAG != 0;
    let uses_dictionary = descriptor & DICTIONARY_IDENTIFIER_MASK != 0;
    let uses_reserved_bits = descriptor & RESERVED_FRAME_HEADER_MASK != 0;

    if !has_checksum || uses_dictionary || uses_reserved_bits {
        return Err(InterfaceValidationError::Malformed);
    }

    let window_size = if single_segment {
        decoded_length
    } else {
        let window_descriptor = encoded
            .get(WINDOW_DESCRIPTOR_OFFSET)
            .copied()
            .ok_or(InterfaceValidationError::Malformed)?;

        let exponent = u32::from(window_descriptor >> 3);

        let base = 1_u64
            .checked_shl(10 + exponent)
            .ok_or(InterfaceValidationError::Malformed)?;

        base.checked_add(base / 8 * u64::from(window_descriptor & WINDOW_MANTISSA_MASK))
            .ok_or(InterfaceValidationError::Malformed)?
    };

    if window_size > MAXIMUM_WINDOW_SIZE {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

pub(crate) fn decode_zstd_frame(
    encoded: &[u8],
    decoded_length: u64,
) -> Result<Arc<[u8]>, InterfaceValidationError> {
    let capacity = usize::try_from(decoded_length).map_err(|_| InterfaceValidationError::Malformed)?;

    let mut decompressor =
        zstd::bulk::Decompressor::new().map_err(|_| InterfaceValidationError::Malformed)?;

    decompressor
        .window_log_max(MAXIMUM_WINDOW_LOG)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    let decoded = decompressor
        .decompress(encoded, capacity)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    if decoded.len() != capacity {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(decoded.into())
}

#[cfg(test)]
mod tests {
    use super::{
        InterfaceSectionEncoding, decode_zstd_frame, encode_section, validate_zstd_frame,
        validate_zstd_frame_header,
    };

    #[test]
    fn canonical_policy_compresses_only_when_the_fixed_saving_is_met() {
        let small = vec![0; 1023];
        let compressible = vec![0; 4096];

        assert_eq!(
            encode_section(&small).map(|(encoding, _)| encoding),
            Ok(InterfaceSectionEncoding::Raw)
        );

        let (encoding, encoded) = encode_section(&compressible)
            .unwrap_or_else(|error| panic!("compressible section must encode: {error:?}"));

        assert_eq!(encoding, InterfaceSectionEncoding::ZstdFrame);
        assert_eq!(validate_zstd_frame(&encoded, 4096), Ok(()));

        let decoded = decode_zstd_frame(&encoded, 4096)
            .unwrap_or_else(|error| panic!("canonical frame must decode: {error:?}"));

        assert_eq!(decoded.as_ref(), compressible);
    }

    #[test]
    fn canonical_compression_is_byte_deterministic() {
        let decoded = vec![0x5a; 4096];
        let first = encode_section(&decoded);
        let second = encode_section(&decoded);

        assert_eq!(first, second);
    }

    #[test]
    fn frame_contract_rejects_missing_checksums_dictionaries_and_oversized_windows() {
        let missing_checksum = [0; 5];
        let external_dictionary = [0, 0, 0, 0, 0b0010_0101];
        let oversized_window = [0, 0, 0, 0, 0b0000_0100, 11 << 3];

        assert_eq!(
            validate_zstd_frame_header(&missing_checksum, 1),
            Err(crate::InterfaceValidationError::Malformed)
        );

        assert_eq!(
            validate_zstd_frame_header(&external_dictionary, 1),
            Err(crate::InterfaceValidationError::Malformed)
        );

        assert_eq!(
            validate_zstd_frame_header(&oversized_window, 1),
            Err(crate::InterfaceValidationError::Malformed)
        );
    }
}
