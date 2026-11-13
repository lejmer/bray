const MAX_CHARACTERS: usize = 80;

/// Formats source embedded in prose, without bounding the surrounding message.
pub(crate) fn format_code_snippet(source: &str) -> String {
    let mut characters = source.char_indices().enumerate().peekable();

    while let Some((ordinal, (offset, character))) = characters.next() {
        if matches!(character, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
            || ordinal == MAX_CHARACTERS - 1 && characters.peek().is_some()
        {
            return format!("{}…", source[..offset].trim_end());
        }
    }

    source.to_owned()
}

#[cfg(test)]
mod tests {
    use super::format_code_snippet;

    #[test]
    fn snippets_preserve_short_code_and_bound_unicode_and_line_breaks() {
        for (source, expected) in [
            (String::new(), String::new()),
            ("inspect(&owner)".to_owned(), "inspect(&owner)".to_owned()),
            ("x".repeat(80), "x".repeat(80)),
            ("λ".repeat(81), format!("{}…", "λ".repeat(79))),
            ("func()\r\n{\r\n}".to_owned(), "func()…".to_owned()),
            ("func()\u{2028}{}".to_owned(), "func()…".to_owned()),
            ("\nbody".to_owned(), "…".to_owned()),
        ] {
            let snippet = format_code_snippet(&source);

            assert_eq!(snippet, expected);
            assert!(snippet.chars().count() <= 80);
        }
    }
}
