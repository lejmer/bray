use std::path::Path;

use tempfile::TempDir;

/// Creates a test directory beneath `target/test-tmp` while keeping this tool free of test-only repository crates.
pub(crate) fn temporary_directory() -> TempDir {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bray-llvm-toolchain must live two levels beneath the repository root")
        .join("target/test-tmp");

    std::fs::create_dir_all(&parent).expect("test temporary root must be created");

    tempfile::tempdir_in(parent).expect("test temporary directory must be created")
}
