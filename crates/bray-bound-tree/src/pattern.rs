use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    ConstantTermId, ConstantValueId, LocalBindingSymbolId, StructFieldSymbolId, StructSymbolId,
    SymbolOrdinal, TypeId, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use crate::{
    BoundExpressionId, BoundPatternId, BoundPatternLiteral, BoundPatternTarget, BoundUnitId,
    BoundUnitKind,
};

/// The value or storage operation selected for one checked pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternOperation {
    /// Inspect the subject without extracting ownership.
    Observe,
    /// Borrow the matched storage through shared access.
    SharedBorrow,
    /// Borrow the matched storage through exclusive mutable access.
    MutableBorrow,
    /// Move matched values from the subject.
    Consume,
    /// Copy matched values from the subject.
    Copy,
    /// Recovery prevented a reliable operation choice.
    Recovered,
}

/// One structural condition tested or established by a checked pattern.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternPredicate {
    /// The subject equals one source literal.
    Literal(PatternLiteralPredicate),
    /// The subject equals one checked open or closed constant term.
    Constant(ConstantTermId),
    /// The nullable subject is absent.
    NullableAbsent,
    /// The nullable subject is present.
    NullablePresent,
    /// The named union has one active variant.
    ActiveUnionVariant(UnionVariantSymbolId),
    /// The subject has one named product shape.
    ProductShape(StructSymbolId),
    /// The subject has one tuple arity.
    TupleShape(u32),
    /// The subject has one fixed array length.
    ArrayShape(u32),
    /// The subject is available through owned indirection.
    OwnedTarget,
}

/// A checked source literal and its canonical typed value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternLiteralPredicate {
    literal: BoundPatternLiteral,
    value: ConstantValueId,
}

impl PatternLiteralPredicate {
    /// Creates one checked literal predicate.
    pub const fn new(literal: BoundPatternLiteral, value: ConstantValueId) -> Self {
        Self { literal, value }
    }

    /// Returns the source-correlated literal.
    pub const fn literal(self) -> BoundPatternLiteral {
        self.literal
    }

    /// Returns the canonical typed literal value.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}

/// One checked structural step from a pattern subject to a nested subject.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PatternProjection {
    /// A named product field.
    ProductField(StructFieldSymbolId),
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// A field of one active union payload.
    ActiveUnionPayloadField {
        /// The variant proven active before projecting the field.
        variant: UnionVariantSymbolId,
        /// The selected payload field.
        field: UnionPayloadFieldSymbolId,
    },
    /// A fixed array or slice element counted from the start.
    ElementFromStart(SymbolOrdinal),
    /// A fixed array or slice element counted from the end.
    ElementFromEnd(SymbolOrdinal),
    /// The present value of a nullable subject.
    NullableValue,
    /// The target owned through an indirection layer.
    OwnedTarget,
}

/// Whether a checked pattern can fail for a value of its input type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PatternRefutability {
    /// Every value of the input type matches.
    Irrefutable,
    /// At least one value of the input type does not match.
    Refutable,
    /// Recovery prevented a reliable classification.
    Recovered,
}

/// The checked semantic result for one pattern occurrence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PatternCheckEntry {
    pattern: BoundPatternId,
    input_type: TypeId,
    operation: PatternOperation,
    refutability: PatternRefutability,
    target: Option<BoundPatternTarget>,
    test: Option<PatternPredicate>,
    projection: Option<PatternProjection>,
    refinement: Option<PatternPredicate>,
}

impl PatternCheckEntry {
    /// Creates one checked pattern result.
    pub const fn new(
        pattern: BoundPatternId,
        input_type: TypeId,
        operation: PatternOperation,
        refutability: PatternRefutability,
        target: Option<BoundPatternTarget>,
    ) -> Self {
        Self {
            pattern,
            input_type,
            operation,
            refutability,
            target,
            test: None,
            projection: None,
            refinement: None,
        }
    }

    /// Returns this result with its local structural test.
    pub const fn with_test(mut self, test: Option<PatternPredicate>) -> Self {
        self.test = test;

        self
    }

    /// Returns this result with its incoming structural projection.
    pub const fn with_projection(mut self, projection: Option<PatternProjection>) -> Self {
        self.projection = projection;

        self
    }

    /// Returns this result with the condition established by a successful match.
    pub const fn with_refinement(mut self, refinement: Option<PatternPredicate>) -> Self {
        self.refinement = refinement;

        self
    }

    /// Returns the exact pattern occurrence.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the type matched by this pattern.
    pub const fn input_type(self) -> TypeId {
        self.input_type
    }

    /// Returns the selected value or storage operation.
    pub const fn operation(self) -> PatternOperation {
        self.operation
    }

    /// Returns whether this pattern can fail.
    pub const fn refutability(self) -> PatternRefutability {
        self.refutability
    }

    /// Returns the declaration or local selected by this pattern.
    pub const fn target(self) -> Option<BoundPatternTarget> {
        self.target
    }

    /// Returns the structural condition tested directly by this pattern.
    pub const fn test(self) -> Option<PatternPredicate> {
        self.test
    }

    /// Returns how this pattern reaches its subject from its parent.
    pub const fn projection(self) -> Option<PatternProjection> {
        self.projection
    }

    /// Returns the condition established when this pattern succeeds.
    pub const fn refinement(self) -> Option<PatternPredicate> {
        self.refinement
    }

    /// Returns whether checking used conservative recovery.
    pub const fn is_recovered(self) -> bool {
        matches!(self.refutability, PatternRefutability::Recovered)
    }
}

/// The checked type assigned to one pattern-introduced local binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PatternBindingTypeEntry {
    binding: LocalBindingSymbolId,
    ty: TypeId,
    operation: PatternOperation,
    projection: Option<PatternProjection>,
    is_recovered: bool,
}

impl PatternBindingTypeEntry {
    /// Creates one checked pattern-binding type.
    pub const fn new(
        binding: LocalBindingSymbolId,
        ty: TypeId,
        operation: PatternOperation,
        is_recovered: bool,
    ) -> Self {
        Self {
            binding,
            ty,
            operation,
            projection: None,
            is_recovered,
        }
    }

    /// Returns this binding with its source projection.
    pub const fn with_projection(mut self, projection: Option<PatternProjection>) -> Self {
        self.projection = projection;

        self
    }

    /// Returns the exact local binding.
    pub const fn binding(self) -> LocalBindingSymbolId {
        self.binding
    }

    /// Returns the binding's checked type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the operation that produces this binding.
    pub const fn operation(self) -> PatternOperation {
        self.operation
    }

    /// Returns how the binding is projected from its pattern subject.
    pub const fn projection(self) -> Option<PatternProjection> {
        self.projection
    }

    /// Returns whether checking used conservative recovery.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// Coverage established for one match expression.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MatchCoverageEntry {
    expression: BoundExpressionId,
    unreachable_arms: Arc<[u32]>,
    is_exhaustive: bool,
    is_recovered: bool,
}

impl MatchCoverageEntry {
    /// Creates one match coverage result.
    pub fn new(
        expression: BoundExpressionId,
        unreachable_arms: impl IntoIterator<Item = u32>,
        is_exhaustive: bool,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            unreachable_arms: shared_slice(unreachable_arms),
            is_exhaustive,
            is_recovered,
        }
    }

    /// Returns the exact match expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns source-order arm indexes proven unreachable.
    pub fn unreachable_arms(&self) -> &[u32] {
        &self.unreachable_arms
    }

    /// Returns whether all values of the subject type are covered.
    pub const fn is_exhaustive(&self) -> bool {
        self.is_exhaustive
    }

    /// Returns whether checking used conservative recovery.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// Immutable checked pattern and match-coverage patterns for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedPatterns {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    patterns: Arc<[PatternCheckEntry]>,
    binding_types: Arc<[PatternBindingTypeEntry]>,
    matches: Arc<[MatchCoverageEntry]>,
}

impl CheckedPatterns {
    /// Creates one complete pattern analysis table.
    pub fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        patterns: impl IntoIterator<Item = PatternCheckEntry>,
        binding_types: impl IntoIterator<Item = PatternBindingTypeEntry>,
        matches: impl IntoIterator<Item = MatchCoverageEntry>,
    ) -> Self {
        let mut patterns = patterns.into_iter().collect::<Vec<_>>();
        let mut binding_types = binding_types.into_iter().collect::<Vec<_>>();
        let mut matches = matches.into_iter().collect::<Vec<_>>();

        patterns.sort_unstable_by_key(|entry| entry.pattern());
        binding_types.sort_unstable_by_key(|entry| entry.binding());
        matches.sort_unstable_by_key(MatchCoverageEntry::expression);

        Self {
            unit,
            kind,
            patterns: patterns.into(),
            binding_types: binding_types.into(),
            matches: matches.into(),
        }
    }

    /// Returns the exact bound unit described by these patterns.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the checked bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns checked pattern results in pattern ID order.
    pub fn patterns(&self) -> &[PatternCheckEntry] {
        &self.patterns
    }

    /// Returns checked local binding types in local identity order.
    pub fn binding_types(&self) -> &[PatternBindingTypeEntry] {
        &self.binding_types
    }

    /// Returns match coverage results in expression ID order.
    pub fn matches(&self) -> &[MatchCoverageEntry] {
        &self.matches
    }

    /// Returns the result for one exact pattern occurrence.
    pub fn pattern(&self, pattern: BoundPatternId) -> Option<PatternCheckEntry> {
        self.patterns
            .binary_search_by_key(&pattern, |entry| entry.pattern())
            .ok()
            .map(|index| self.patterns[index])
    }

    /// Returns the checked type for one pattern-introduced binding.
    pub fn binding_type(&self, binding: LocalBindingSymbolId) -> Option<PatternBindingTypeEntry> {
        self.binding_types
            .binary_search_by_key(&binding, |entry| entry.binding())
            .ok()
            .map(|index| self.binding_types[index])
    }

    /// Returns coverage for one exact match expression.
    pub fn match_coverage(&self, expression: BoundExpressionId) -> Option<&MatchCoverageEntry> {
        self.matches
            .binary_search_by_key(&expression, |entry| entry.expression())
            .ok()
            .map(|index| &self.matches[index])
    }

    /// Returns whether any pattern or match required recovery.
    pub fn is_recovered(&self) -> bool {
        self.patterns.iter().any(|entry| entry.is_recovered())
            || self.binding_types.iter().any(|entry| entry.is_recovered())
            || self.matches.iter().any(|entry| entry.is_recovered())
    }
}
