use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{LocalBindingSymbolId, TypeId};

use crate::{BoundExpressionId, BoundPatternId, BoundPatternTarget, BoundUnitId, BoundUnitKind};

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternCheckEntry {
    pattern: BoundPatternId,
    input_type: TypeId,
    refutability: PatternRefutability,
    target: Option<BoundPatternTarget>,
    is_recovered: bool,
}

impl PatternCheckEntry {
    /// Creates one checked pattern result.
    pub const fn new(
        pattern: BoundPatternId,
        input_type: TypeId,
        refutability: PatternRefutability,
        target: Option<BoundPatternTarget>,
        is_recovered: bool,
    ) -> Self {
        Self {
            pattern,
            input_type,
            refutability,
            target,
            is_recovered,
        }
    }

    /// Returns the exact pattern occurrence.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the type matched by this pattern.
    pub const fn input_type(self) -> TypeId {
        self.input_type
    }

    /// Returns whether this pattern can fail.
    pub const fn refutability(self) -> PatternRefutability {
        self.refutability
    }

    /// Returns the declaration or local selected by this pattern.
    pub const fn target(self) -> Option<BoundPatternTarget> {
        self.target
    }

    /// Returns whether checking used conservative recovery.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// The checked type assigned to one pattern-introduced local binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternBindingTypeEntry {
    binding: LocalBindingSymbolId,
    ty: TypeId,
    is_recovered: bool,
}

impl PatternBindingTypeEntry {
    /// Creates one checked pattern-binding type.
    pub const fn new(binding: LocalBindingSymbolId, ty: TypeId, is_recovered: bool) -> Self {
        Self {
            binding,
            ty,
            is_recovered,
        }
    }

    /// Returns the exact local binding.
    pub const fn binding(self) -> LocalBindingSymbolId {
        self.binding
    }

    /// Returns the binding's checked type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns whether checking used conservative recovery.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// Coverage established for one match expression.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Immutable checked pattern and match-coverage facts for one bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedPatternFacts {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    patterns: Arc<[PatternCheckEntry]>,
    binding_types: Arc<[PatternBindingTypeEntry]>,
    matches: Arc<[MatchCoverageEntry]>,
}

impl CheckedPatternFacts {
    /// Creates one complete pattern fact table.
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

    /// Returns the exact bound unit described by these facts.
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
