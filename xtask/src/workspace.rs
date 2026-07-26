use std::path::{Path, PathBuf};

pub(crate) fn root() -> Result<PathBuf, String> {
    let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));

    xtask
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest directory has no workspace parent".to_owned())
}

pub(crate) fn io_error(action: &str, path: &Path, error: std::io::Error) -> String {
    format!("failed to {action} {}: {error}", path.display())
}
