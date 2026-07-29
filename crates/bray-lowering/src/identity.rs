use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_bound_tree::{BoundUnit, BoundUnitKey, BoundUnitRoot};
use bray_ir::{MirTargetFacts, MirUnitKind};
use bray_runtime_interface::ProtectedAsyncFrameId;
use bray_symbols::CallableExecution;

const ASYNC_FRAME_IDENTITY_REVISION: u32 = 1;

/// Selects the MIR representation category for one executable bound unit.
pub fn executable_unit_kind(unit: &BoundUnit, target: &MirTargetFacts) -> MirUnitKind {
    let execution = match unit.root() {
        BoundUnitRoot::CallableBody { execution, .. }
        | BoundUnitRoot::AnonymousCallable { execution, .. } => Some(execution),
        BoundUnitRoot::Expression(_) | BoundUnitRoot::ExpressionSequence(_) => None,
    };

    match execution {
        Some(CallableExecution::Asynchronous) => {
            MirUnitKind::ProtectedAsyncFrame(protected_frame_identity(unit.key(), target))
        }
        Some(CallableExecution::Synchronous) | None => MirUnitKind::Synchronous,
    }
}

fn protected_frame_identity(
    key: &BoundUnitKey,
    target: &MirTargetFacts,
) -> ProtectedAsyncFrameId {
    // This is the target-specific template identity. The concrete codegen instance
    // adds its specialization arguments and selected implementation witnesses.
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.protected-async-frame");
    hasher.write_u32(ASYNC_FRAME_IDENTITY_REVISION);
    key.hash(&mut hasher);
    target.hash(&mut hasher);

    ProtectedAsyncFrameId::new(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_testing::test_bound_unit;

    use super::{executable_unit_kind, protected_frame_identity};

    #[test]
    fn protected_frame_identity_is_stable_for_equal_unit_keys() {
        let first = test_bound_unit(7);
        let second = test_bound_unit(7);
        let target = bray_testing::test_mir_target();

        assert_eq!(
            protected_frame_identity(first.key(), &target),
            protected_frame_identity(second.key(), &target)
        );
    }

    #[test]
    fn protected_frame_identity_includes_target_and_runtime_abi_facts() {
        let unit = test_bound_unit(7);
        let target = bray_testing::test_mir_target();

        let alternate_abi = bray_ir::MirTargetFacts::new(
            target.profile().clone(),
            RuntimeAbiVersion::new(
                target.runtime_abi().major(),
                target.runtime_abi().minor().saturating_add(1),
            ),
        );

        assert_ne!(
            protected_frame_identity(unit.key(), &target),
            protected_frame_identity(unit.key(), &alternate_abi)
        );
    }

    #[test]
    fn synchronous_test_units_select_synchronous_mir() {
        let unit = test_bound_unit(8);
        let target = bray_testing::test_mir_target();

        assert_eq!(
            executable_unit_kind(&unit, &target),
            bray_ir::MirUnitKind::Synchronous
        );
    }
}
