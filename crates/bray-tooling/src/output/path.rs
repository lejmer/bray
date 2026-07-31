use std::path::Path;

pub(crate) fn path_to_output_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
