pub(crate) fn contains_symbol(contents: &str, symbol: &str) -> bool {
    symbol_tokens(contents).any(|token| token == symbol)
}

pub(crate) fn contains_archive(contents: &str, stem: &str) -> bool {
    let names = [
        format!("lib{stem}.a"),
        format!("{stem}.a"),
        format!("{stem}.lib"),
    ];

    contents
        .split(|character: char| character.is_whitespace() || matches!(character, '(' | ')' | ':'))
        .filter_map(|token| token.rsplit(['/', '\\']).next())
        .map(|name| name.trim_matches(['"', '\'']))
        .any(|name| names.iter().any(|archive| archive == name))
}

pub(crate) fn symbol_tokens(contents: &str) -> impl Iterator<Item = &str> {
    contents
        .lines()
        .filter(|line| !line.trim_start().starts_with("<<dead>>"))
        .flat_map(str::split_whitespace)
        .flat_map(|token| [token, token.strip_prefix('_').unwrap_or(token)])
}

#[cfg(test)]
mod tests {
    use super::{contains_archive, contains_symbol};

    #[test]
    fn archive_matching_distinguishes_native_symbols_and_archive_members() {
        assert!(!contains_archive(
            "bray_runtime_event_signal\n_bray_runtime_event_creation",
            "bray_runtime_event"
        ));

        assert!(contains_archive(
            "/runtime/libbray_runtime_event.a(member.o)",
            "bray_runtime_event"
        ));

        assert!(contains_archive(
            "bray_runtime_event.lib:member.obj",
            "bray_runtime_event"
        ));

        assert!(!contains_archive("/runtime/libbray_std.a(member.o)", "std"));

        assert!(contains_archive(
            "\"C:\\native output\\bray_runtime_event.lib\":member.obj",
            "bray_runtime_event"
        ));
    }

    #[test]
    fn symbol_matching_ignores_mach_o_dead_stripped_entries() {
        let map = "# Symbols:\n0x1000 0x08 [ 1] _retained\n# Dead Stripped Symbols:\n<<dead>> 0x08 [ 2] _discarded\n";

        assert!(contains_symbol(map, "retained"));
        assert!(!contains_symbol(map, "discarded"));
    }

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
