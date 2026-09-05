use std::borrow::Cow;

pub(crate) fn normalize_line_endings(bytes: &[u8]) -> Cow<'_, [u8]> {
    if !bytes.windows(2).any(|pair| pair == b"\r\n") {
        return Cow::Borrowed(bytes);
    }

    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }

    Cow::Owned(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_line_endings;

    #[test]
    fn crlf_normalization_preserves_lone_carriage_returns() {
        assert_eq!(
            normalize_line_endings(b"first\nsecond\n"),
            normalize_line_endings(b"first\r\nsecond\r\n"),
        );

        assert_eq!(normalize_line_endings(b"first\rsecond").as_ref(), b"first\rsecond");
    }
}
