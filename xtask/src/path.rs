pub(crate) fn slash_separated(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
