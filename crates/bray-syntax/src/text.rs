use bray_source::TextRange;

pub(crate) fn assert_text_len_matches_range(text: &str, range: TextRange) {
    let expected_len = match usize::try_from(range.len().bytes()) {
        Ok(len) => len,
        Err(_) => panic!("text range length does not fit usize"),
    };

    assert_eq!(text.len(), expected_len);
}
