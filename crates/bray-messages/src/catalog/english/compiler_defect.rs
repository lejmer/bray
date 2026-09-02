pub(crate) const INTERNAL_COMPILER_ERROR: &str = "internal compiler error";
const INTERNAL_COMPILER_ERROR_PREFIX: &str = "internal compiler error: ";

pub(crate) fn format_internal_compiler_error(detail: impl AsRef<str>) -> String {
    let detail = detail.as_ref();

    if detail == INTERNAL_COMPILER_ERROR || detail.starts_with(INTERNAL_COMPILER_ERROR_PREFIX) {
        detail.to_owned()
    } else {
        format!("{INTERNAL_COMPILER_ERROR_PREFIX}{detail}")
    }
}
