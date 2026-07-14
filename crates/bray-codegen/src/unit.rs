use std::sync::Arc;

use bray_base::shared_str;
use bray_ir::MirUnit;

/// Stable structural identity of one partitioned codegen unit.
///
/// The partition revision identifies the policy that selected unit membership. The opaque
/// identity is derived from canonical concrete-instance membership by the codegen partitioner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenUnitKey {
    partition_revision: u32,
    identity: Arc<str>,
}

impl CodegenUnitKey {
    /// Creates a key unless its partition-derived identity is empty.
    pub fn try_new(partition_revision: u32, identity: impl Into<Arc<str>>) -> Option<Self> {
        let identity = shared_str(identity);

        if identity.is_empty() {
            return None;
        }

        Some(Self {
            partition_revision,
            identity,
        })
    }

    /// Returns the codegen partition-policy revision.
    pub const fn partition_revision(&self) -> u32 {
        self.partition_revision
    }

    /// Returns the canonical partition-derived unit identity.
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// One immutable validated MIR work item supplied to a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenUnit {
    key: CodegenUnitKey,
    mir_units: Arc<[MirUnit]>,
}

impl CodegenUnit {
    /// Creates a codegen unit with non-empty, unique MIR units in canonical key order.
    pub fn try_new(
        key: CodegenUnitKey,
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

        Ok(Self {
            key,
            mir_units: mir_units.into(),
        })
    }

    /// Returns the stable partition-derived unit key.
    pub const fn key(&self) -> &CodegenUnitKey {
        &self.key
    }

    /// Returns validated MIR units in canonical semantic-key order.
    pub fn mir_units(&self) -> &[MirUnit] {
        &self.mir_units
    }
}

/// A contract violation that prevents codegen-unit publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenUnitBuildError {
    /// The partition contains no definitions.
    Empty,
    /// Two MIR units have the same canonical semantic key.
    DuplicateMirUnit,
}

#[cfg(test)]
mod tests {
    use bray_ir::{MirUnit, MirUnitBuilder};
    use bray_testing::test_bound_unit;

    use super::{CodegenUnit, CodegenUnitBuildError, CodegenUnitKey};

    #[test]
    fn units_reject_empty_and_duplicate_mir_membership() {
        let key = key();

        assert_eq!(
            CodegenUnit::try_new(key.clone(), []),
            Err(CodegenUnitBuildError::Empty)
        );

        let first = mir_unit(4);
        let duplicate = first.clone();

        assert_eq!(
            CodegenUnit::try_new(key, [first, duplicate]),
            Err(CodegenUnitBuildError::DuplicateMirUnit)
        );
    }

    #[test]
    fn units_canonicalize_mir_membership_independently_of_input_order() {
        let first = CodegenUnit::try_new(key(), [mir_unit(8), mir_unit(4)]);
        let second = CodegenUnit::try_new(key(), [mir_unit(4), mir_unit(8)]);

        assert_eq!(first, second);
    }

    fn key() -> CodegenUnitKey {
        let Some(key) = CodegenUnitKey::try_new(1, "package.main.0") else {
            panic!("test codegen unit key must be valid");
        };

        key
    }

    fn mir_unit(unit: u32) -> MirUnit {
        let bound = test_bound_unit(unit);
        let source = bound.key().source();
        let mut builder = MirUnitBuilder::new(bound.identity());

        let Ok(entry) = builder.push_block(source) else {
            panic!("test MIR block must be valid");
        };

        let Ok(unit) = builder.finish(entry) else {
            panic!("test MIR unit must be valid");
        };

        unit
    }
}
