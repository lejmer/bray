use std::path::Path;

use bray_target::NativeTarget;

use super::fixtures::NULLABLE_STATE_FIXTURE;
use super::repeatable::audit_repeatable_fixture;

pub(super) fn audit_nullable_state_queries(
    root: &Path,
    target: NativeTarget,
    runtime: &Path,
) -> Result<(), String> {
    audit_repeatable_fixture(
        root,
        target,
        runtime,
        "bray-native-nullable-state-",
        NULLABLE_STATE_FIXTURE,
        0,
        "nullable state queries",
        &[],
    )
}
