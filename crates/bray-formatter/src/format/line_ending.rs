pub(super) fn detect(source_text: &str) -> &'static str {
    let bytes = source_text.as_bytes();

    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte != b'\n' {
            continue;
        }

        return if index > 0 && bytes[index - 1] == b'\r' {
            "\r\n"
        } else {
            "\n"
        };
    }

    "\n"
}

pub(super) fn contains(text: &str) -> bool {
    text.as_bytes().contains(&b'\n')
}

pub(super) fn count(text: &str) -> u8 {
    text.bytes()
        .filter(|byte| *byte == b'\n')
        .fold(0_u8, |count, _| count.saturating_add(1))
}
