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
}
