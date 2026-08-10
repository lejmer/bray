use std::fmt::Write;

/// Formats bytes as contiguous lowercase hexadecimal text.
pub fn lowercase_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));

    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }

    text
}

/// Returns whether every byte is a lowercase hexadecimal digit.
pub fn is_lowercase_hex(text: &str) -> bool {
    text.bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Decodes exact-width lowercase hexadecimal text.
pub fn decode_lowercase_hex<const BYTE_COUNT: usize>(text: &str) -> Option<[u8; BYTE_COUNT]> {
    if text.len() != BYTE_COUNT.checked_mul(2)? || !is_lowercase_hex(text) {
        return None;
    }

    let mut bytes = [0_u8; BYTE_COUNT];

    for (destination, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        *destination = decode_hex_digit(pair[0])? << 4 | decode_hex_digit(pair[1])?;
    }

    Some(bytes)
}

const fn decode_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lowercase_hex_preserves_every_byte_with_fixed_width() {
        assert_eq!(super::lowercase_hex(&[0, 10, 255]), "000aff");
    }

    #[test]
    fn lowercase_hex_validation_rejects_other_text() {
        assert!(super::is_lowercase_hex("0123456789abcdef"));
        assert!(!super::is_lowercase_hex("ABCDEF"));
        assert!(!super::is_lowercase_hex("not-hex"));
    }

    #[test]
    fn lowercase_hex_decoding_requires_exact_width_and_canonical_digits() {
        assert_eq!(super::decode_lowercase_hex::<3>("000aff"), Some([0, 10, 255]));
        assert_eq!(super::decode_lowercase_hex::<3>("000a"), None);
        assert_eq!(super::decode_lowercase_hex::<3>("000aFF"), None);
    }
}
