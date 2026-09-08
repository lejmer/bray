use bray_ir::MirBlockKind;

use super::super::SyntheticLoweringError;

pub(super) fn lifecycle_operation_block_kind(
    role: bray_ir::MirGeneratedLifecycleRole,
    builder: &bray_ir::MirUnitBuilder,
) -> Result<MirBlockKind, SyntheticLoweringError> {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) => {
            Ok(MirBlockKind::LifecycleResolution)
        }
        bray_ir::MirGeneratedLifecycleRole::Destroy
        | bray_ir::MirGeneratedLifecycleRole::Abandon(
            bray_ir::MirAbandonmentAction::Destroy | bray_ir::MirAbandonmentAction::Destructor,
        ) => Ok(if builder.protected_frame().is_some() {
            MirBlockKind::LifecycleResolution
        } else {
            MirBlockKind::Ordinary
        }),
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
