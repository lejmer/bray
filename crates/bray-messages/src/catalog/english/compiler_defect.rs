pub(crate) const INTERNAL_COMPILER_ERROR: &str = "internal compiler error";

pub(crate) fn format_internal_compiler_error(detail: impl AsRef<str>) -> String {
    let detail = detail.as_ref();

    let already_labeled = detail
        .strip_prefix(INTERNAL_COMPILER_ERROR)
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(": "));

    if already_labeled {
        detail.to_owned()
    } else {
        format!("{INTERNAL_COMPILER_ERROR}: {detail}")
    }
}
