use std::path::{Path, PathBuf};

pub(super) fn root() -> Result<PathBuf, String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "LLVM toolchain manifest directory has no workspace ancestor".to_owned())
}
