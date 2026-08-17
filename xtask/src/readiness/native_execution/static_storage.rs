use std::path::Path;

use bray_target::NativeTarget;

use super::core::audit_repeatable_fixture;

const STATIC_STORAGE_FIXTURE: &str = "xtask/fixtures/native-execution/static_storage.bray";

pub(super) fn audit_static_storage(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-static-storage-",
        STATIC_STORAGE_FIXTURE,
        42,
        "Bray-owned static storage",
        &[],
    )
}
