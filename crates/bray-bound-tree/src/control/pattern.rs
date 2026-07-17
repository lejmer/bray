use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    ConstantValueId, StructFieldSymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
};

use crate::{BoundExpressionId, BoundPatternId, BoundUnitId};

/// One runtime test selected for a checked pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternTest {
    /// The pattern succeeds without a runtime test.
    Always,
    /// The subject must equal one canonical constant value.
    Constant(ConstantValueId),
    /// The subject must be nullable-absent.
    NullableAbsent,
    /// The subject must be nullable-present.
    NullablePresent,
    /// The subject must have one exact active union variant.
    UnionVariant(UnionVariantSymbolId),
}

/// One checked projection from a pattern subject to a direct child pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternProjection {
    /// The child receives the same subject as its parent.
    Identity,
    /// The child receives one named product field.
    ProductField(StructFieldSymbolId),
    /// The child receives one tuple element.
    TupleElement(SymbolOrdinal),
    /// The child receives one fixed-array element.
    ArrayElement(SymbolOrdinal),
    /// The child receives one fixed-array element counted from the end.
    ArrayElementFromEnd(SymbolOrdinal),
    /// The child is a source `..` marker and receives no runtime value.
    Remainder,
    /// The child receives the present value of a nullable subject.
    NullableValue,
    /// The child receives the value behind owned indirection.
    OwnedTarget,
    /// The child receives one field of the proven active union payload.
    UnionPayloadField {
        /// The variant proven active by the parent pattern.
        variant: UnionVariantSymbolId,
        /// The selected payload field.
        field: UnionPayloadFieldSymbolId,
    },
}

/// The projection selected for one direct child pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedPatternChildProjection {
    child: BoundPatternId,
    projection: PatternProjection,
}

impl CheckedPatternChildProjection {
    /// Creates one direct child projection.
    pub const fn new(child: BoundPatternId, projection: PatternProjection) -> Self {
        Self { child, projection }
    }

    /// Returns the direct child receiving the projected subject.
    pub const fn child(self) -> BoundPatternId {
        self.child
    }

    /// Returns the exact checked projection operation.
    pub const fn projection(self) -> PatternProjection {
        self.projection
    }
}

/// The lowering-facing semantic decision for one pattern.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CheckedPatternResolution {
    /// Pattern checking selected an exact test and direct child projections.
    Resolved {
        /// Whether the pattern covers every value of its checked subject type.
        is_irrefutable: bool,
        /// The runtime test selected for this pattern.
        test: PatternTest,
        /// Direct child projections in source-semantic order.
        projections: Arc<[CheckedPatternChildProjection]>,
    },
    /// Earlier recovery prevented sound test or projection selection.
    Recovered,
}

/// Durable lowering-facing facts for one exact checked pattern.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedPatternFacts {
    pattern: BoundPatternId,
    resolution: CheckedPatternResolution,
}

impl CheckedPatternFacts {
    /// Creates resolved pattern facts with projections in source-semantic order.
    pub fn resolved(
        pattern: BoundPatternId,
        is_irrefutable: bool,
        test: PatternTest,
        projections: impl IntoIterator<Item = CheckedPatternChildProjection>,
    ) -> Self {
        Self {
            pattern,
            resolution: CheckedPatternResolution::Resolved {
                is_irrefutable,
                test,
                projections: shared_slice(projections),
            },
        }
    }

    /// Creates an explicit recovery fact without inventing a valid test or projection.
    pub const fn recovered(pattern: BoundPatternId) -> Self {
        Self {
            pattern,
            resolution: CheckedPatternResolution::Recovered,
        }
    }

    /// Returns the exact pattern described by this fact.
    pub const fn pattern(&self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the checked semantic decision or explicit recovery state.
    pub const fn resolution(&self) -> &CheckedPatternResolution {
        &self.resolution
    }

    pub(super) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.pattern.unit() == unit
            && match &self.resolution {
                CheckedPatternResolution::Resolved { projections, .. } => projections
                    .iter()
                    .all(|projection| projection.child().unit() == unit),
                CheckedPatternResolution::Recovered => true,
            }
    }
}

/// The checked coverage result for one match expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MatchExhaustiveness {
    /// Every possible subject value is definitely handled.
    Exhaustive,
    /// At least one subject region is definitely not handled.
    NonExhaustive,
    /// Recovery prevented a sound coverage decision.
    Recovered,
}

/// Durable coverage facts for one exact match expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedMatchFacts {
    expression: BoundExpressionId,
    exhaustiveness: MatchExhaustiveness,
}

impl CheckedMatchFacts {
    /// Creates one checked match coverage fact.
    pub const fn new(expression: BoundExpressionId, exhaustiveness: MatchExhaustiveness) -> Self {
        Self {
            expression,
            exhaustiveness,
        }
    }

    /// Returns the exact match expression described by this fact.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the checked coverage decision.
    pub const fn exhaustiveness(self) -> MatchExhaustiveness {
        self.exhaustiveness
    }

    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        self.expression.unit() == unit
    }
}
