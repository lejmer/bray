pub(crate) fn contains_symbol(contents: &str, symbol: &str) -> bool {
    contents
        .split_whitespace()
        .any(|token| token == symbol || token.strip_prefix('_') == Some(symbol))
}

#[cfg(test)]
mod tests {
    use super::contains_symbol;

    #[test]
    fn symbol_matching_accepts_mach_o_prefix_without_partial_names() {
        assert!(contains_symbol(
            "0000 _bray_platform_file_write",
            "bray_platform_file_write"
        ));

        assert!(!contains_symbol(
            "0000 bray_platform_file_write_extra",
            "bray_platform_file_write"
        ));
    }
}
