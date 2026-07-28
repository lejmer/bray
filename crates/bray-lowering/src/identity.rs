use std::hash::{Hash, Hasher};

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
    // TODO(BRA-159): Extend this target-specific template identity with the concrete
    // specialization arguments and witnesses selected during codegen-unit partitioning.
    let mut hasher = StableFrameHasher::new();

    hasher.write(b"bray.protected-async-frame");
    hasher.write_u32(ASYNC_FRAME_IDENTITY_REVISION);
    key.hash(&mut hasher);
    target.hash(&mut hasher);

    ProtectedAsyncFrameId::new(hasher.finalize())
}

struct StableFrameHasher(blake3::Hasher);

impl StableFrameHasher {
    fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    fn finalize(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

impl Hasher for StableFrameHasher {
    fn finish(&self) -> u64 {
        // `Hasher::finish` cannot consume the incremental state needed to finalize BLAKE3.
        let digest = self.0.clone().finalize();
        let mut prefix = [0; 8];

        prefix.copy_from_slice(&digest.as_bytes()[..8]);

        u64::from_le_bytes(prefix)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&value.to_le_bytes());
    }

    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes());
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    fn write_i8(&mut self, value: i8) {
        self.write(&value.to_le_bytes());
    }

    fn write_i16(&mut self, value: i16) {
        self.write(&value.to_le_bytes());
    }

    fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    fn write_i64(&mut self, value: i64) {
        self.write(&value.to_le_bytes());
    }

    fn write_i128(&mut self, value: i128) {
        self.write(&value.to_le_bytes());
    }

    fn write_isize(&mut self, value: isize) {
        self.write_i64(i64::try_from(value).unwrap_or_else(|_| {
            if value.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        }));
    }
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
