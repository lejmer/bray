use crate::{BoundNodeKind, BoundUnitId, BoundUnitKind};

/// A contract violation that prevents publication of a checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedUnitBuildError {
    /// The semantic key does not describe the checked-unit category being constructed.
    UnitKindMismatch {
        /// The category required by the checked-unit wrapper.
        expected: BoundUnitKind,
        /// The category carried by the semantic unit key.
        actual: BoundUnitKind,
    },
    /// The category-specific root does not name a node in the owned bound tree.
    MissingRoot {
        /// The unit owning the immutable bound tree.
        unit: BoundUnitId,
        /// The exact category of root required by the checked unit.
        kind: BoundNodeKind,
    },
    /// The local snapshot key does not correspond to the semantic unit key.
    LocalSymbolRegionMismatch,
    /// A nested-unit key is not an anonymous callable directly enclosed by this unit.
    InvalidNestedUnit {
        /// The position of the invalid nested-unit key.
        index: usize,
    },
    /// Nested-unit keys are not unique and ordered by canonical source identity.
    NonCanonicalNestedUnits {
        /// The first position that is not strictly ordered after its predecessor.
        index: usize,
    },
}
