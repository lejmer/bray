/// Returns whether text is a canonical portable non-root relative path.
///
/// Canonical paths use `/` separators and contain no empty, current-directory,
/// parent-directory, absolute, backslash, or null components.
pub fn is_canonical_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::is_canonical_relative_path;

    #[test]
    fn portable_relative_paths_reject_host_and_traversal_forms() {
        assert!(is_canonical_relative_path("interfaces/std.brayi"));
        assert!(!is_canonical_relative_path("/interfaces/std.brayi"));
        assert!(!is_canonical_relative_path("interfaces\\std.brayi"));
        assert!(!is_canonical_relative_path("interfaces/../std.brayi"));
        assert!(!is_canonical_relative_path("interfaces//std.brayi"));
    }
}
