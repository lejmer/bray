pub(crate) const INTERNAL_COMPILER_ERROR: &str = "internal compiler error";

pub(crate) fn format_internal_compiler_error(detail: impl AsRef<str>) -> String {
    format!("{INTERNAL_COMPILER_ERROR}: {}", detail.as_ref())
}
