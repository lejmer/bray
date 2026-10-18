use bray_ir::MirBlockKind;

pub(super) fn lifecycle_operation_block_kind(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> MirBlockKind {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy
        | bray_ir::MirGeneratedLifecycleRole::Quiesce => MirBlockKind::Ordinary,
        bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::TaskCancellation) => {
            MirBlockKind::CleanupBroadcast
        }
        bray_ir::MirGeneratedLifecycleRole::StaticFinalize
        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => panic!(
            "synthetic lowering contract violation: UnsupportedLifecycleRole {value:?}",
            value = role
        ),
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{MirCleanupPhase, MirGeneratedLifecycleRole};

    use super::lifecycle_operation_block_kind;

    #[test]
    fn unsupported_lifecycle_roles_panic_with_the_exact_role() {
        let role = MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution);

        let panic = std::panic::catch_unwind(|| lifecycle_operation_block_kind(role)).unwrap_err();

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("UnsupportedLifecycleRole"));
        assert!(message.contains(&format!("{role:?}")));
    }
}
