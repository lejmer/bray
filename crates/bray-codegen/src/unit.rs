use std::sync::Arc;

use bray_ir::{MirUnit, MirUnitKey};

/// Stable structural identity of one partitioned codegen unit.
///
/// The partition revision identifies the policy that selected canonical MIR membership. Later
/// concrete-instance partitioning extends that typed membership without accepting an unrelated
/// caller-supplied identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenUnitKey {
    partition_revision: u32,
    mir_units: Arc<[MirUnitKey]>,
}

impl CodegenUnitKey {
    /// Returns the codegen partition-policy revision.
    pub const fn partition_revision(&self) -> u32 {
        self.partition_revision
    }

    /// Returns canonical MIR semantic keys that structurally identify this unit.
    pub fn mir_units(&self) -> &[MirUnitKey] {
        &self.mir_units
    }
}

/// One immutable validated MIR work item supplied to a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenUnit {
    key: CodegenUnitKey,
    mir_units: Arc<[MirUnit]>,
}

impl CodegenUnit {
    /// Creates a codegen unit and derives its key from canonical non-empty MIR membership.
    pub fn try_new(
        partition_revision: u32,
        mir_units: impl IntoIterator<Item = MirUnit>,
    ) -> Result<Self, CodegenUnitBuildError> {
        let mut mir_units: Vec<_> = mir_units.into_iter().collect();

        if mir_units.is_empty() {
            return Err(CodegenUnitBuildError::Empty);
        }

        mir_units.sort_unstable_by(|left, right| left.key().cmp(right.key()));

        if mir_units
            .windows(2)
            .any(|pair| pair[0].key() == pair[1].key())
        {
            return Err(CodegenUnitBuildError::DuplicateMirUnit);
        }

        // The structural key retains Arc-backed semantic keys independently of MIR storage.
        let membership = mir_units.iter().map(|unit| unit.key().clone()).collect();

        Ok(Self {
            key: CodegenUnitKey {
                partition_revision,
                mir_units: membership,
            },
            mir_units: mir_units.into(),
        })
    }

    /// Returns the stable structural unit key.
    pub const fn key(&self) -> &CodegenUnitKey {
        &self.key
    }

    /// Returns validated MIR units in canonical semantic-key order.
    pub fn mir_units(&self) -> &[MirUnit] {
        &self.mir_units
    }
}

/// A contract violation that prevents creation of a codegen unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenUnitBuildError {
    /// The partition contains no definitions.
    Empty,
    /// Two MIR units have the same canonical semantic key.
    DuplicateMirUnit,
}

#[cfg(test)]
mod tests {
    use bray_testing::test_mir_unit;

    use super::{CodegenUnit, CodegenUnitBuildError};

    #[test]
    fn units_reject_empty_and_duplicate_mir_membership() {
        assert_eq!(
            CodegenUnit::try_new(1, []),
            Err(CodegenUnitBuildError::Empty)
        );

        let first = test_mir_unit(4);
        let duplicate = first.clone();

        assert_eq!(
            CodegenUnit::try_new(1, [first, duplicate]),
            Err(CodegenUnitBuildError::DuplicateMirUnit)
        );
    }

    #[test]
    fn units_derive_equal_keys_independently_of_input_order() {
        let first = CodegenUnit::try_new(1, [test_mir_unit(8), test_mir_unit(4)]);
        let second = CodegenUnit::try_new(1, [test_mir_unit(4), test_mir_unit(8)]);

        assert_eq!(first, second);
    }
}
