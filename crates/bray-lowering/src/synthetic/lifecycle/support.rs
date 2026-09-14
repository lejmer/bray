use bray_ir::MirBlockKind;

use super::super::SyntheticLoweringError;

pub(super) fn lifecycle_operation_block_kind(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> Result<MirBlockKind, SyntheticLoweringError> {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy => Ok(MirBlockKind::Ordinary),
        bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::TaskCancellation) => {
            Ok(MirBlockKind::CleanupBroadcast)
        }
        bray_ir::MirGeneratedLifecycleRole::Finalize
        | bray_ir::MirGeneratedLifecycleRole::StaticFinalize
        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => Err(SyntheticLoweringError::UnsupportedLifecycleRole(role)),
    }
}

pub(super) fn lifecycle_phase(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> bray_bound_tree::LifecyclePhase {
    use bray_bound_tree::LifecyclePhase;
    use bray_ir::{MirCleanupPhase, MirGeneratedLifecycleRole};

    match role {
        MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize => {
            LifecyclePhase::Finalize
        }
        MirGeneratedLifecycleRole::Destroy => LifecyclePhase::Destroy,
        MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation) => {
            LifecyclePhase::Cancel
        }
        MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution) => {
            LifecyclePhase::Resolve
        }
    }
}
