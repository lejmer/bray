pub(crate) fn contains_symbol(contents: &str, symbol: &str) -> bool {
    symbol_tokens(contents).any(|token| token == symbol)
}

pub(crate) fn symbol_tokens(contents: &str) -> impl Iterator<Item = &str> {
    contents
        .split_whitespace()
        .flat_map(|token| [token, token.strip_prefix('_').unwrap_or(token)])
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
