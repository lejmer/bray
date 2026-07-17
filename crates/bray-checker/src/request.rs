use bray_base::Cancellation;
use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundPatternId,
    BoundUnitKind, BoundUnitView, CheckedForIterationFacts, CheckedMatchFacts, CheckedPatternFacts,
};
use bray_symbols::{AvailableCompilerKnownSymbols, SemanticValueStore};

/// Typed inputs for whole-unit semantic checking.
#[derive(Clone, Copy)]
pub struct UnitCheckRequest<'view> {
    view: BoundUnitView<'view>,
    root: UnitCheckRoot,
    control_fact_selections: &'view ControlFactSelections,
    semantic_values: &'view SemanticValueStore,
    available_compiler_known_symbols: &'view AvailableCompilerKnownSymbols,
    cancellation: &'view dyn Cancellation,
}

/// Exact semantic selections required to publish lowering-facing control facts.
#[derive(Clone, Debug, Default)]
pub struct ControlFactSelections {
    patterns: BTreeMap<BoundPatternId, CheckedPatternFacts>,
    matches: BTreeMap<BoundExpressionId, CheckedMatchFacts>,
    for_iterations: BTreeMap<BoundExpressionId, CheckedForIterationFacts>,
}

/// A duplicate exact semantic selection in one checker request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ControlFactSelectionsError {
    /// More than one pattern selection names the same pattern.
    DuplicatePattern(BoundPatternId),
    /// More than one coverage selection names the same match expression.
    DuplicateMatch(BoundExpressionId),
    /// More than one protocol selection names the same for expression.
    DuplicateForIteration(BoundExpressionId),
}

impl ControlFactSelections {
    /// Creates empty selections for a checker request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one exact literal, product, path, variant, or other pattern selection.
    pub fn insert_pattern(
        &mut self,
        fact: CheckedPatternFacts,
    ) -> Result<(), ControlFactSelectionsError> {
        let pattern = fact.pattern();

        if self.patterns.contains_key(&pattern) {
            return Err(ControlFactSelectionsError::DuplicatePattern(pattern));
        }

        self.patterns.insert(pattern, fact);

        Ok(())
    }

    /// Records one exact match coverage decision.
    pub fn insert_match(
        &mut self,
        fact: CheckedMatchFacts,
    ) -> Result<(), ControlFactSelectionsError> {
        let expression = fact.expression();

        if self.matches.contains_key(&expression) {
            return Err(ControlFactSelectionsError::DuplicateMatch(expression));
        }

        self.matches.insert(expression, fact);

        Ok(())
    }

    /// Records one exact selected for-iteration protocol.
    pub fn insert_for_iteration(
        &mut self,
        fact: CheckedForIterationFacts,
    ) -> Result<(), ControlFactSelectionsError> {
        let expression = fact.expression();

        if self.for_iterations.contains_key(&expression) {
            return Err(ControlFactSelectionsError::DuplicateForIteration(
                expression,
            ));
        }

        self.for_iterations.insert(expression, fact);

        Ok(())
    }

    pub(crate) fn pattern(&self, pattern: BoundPatternId) -> Option<&CheckedPatternFacts> {
        self.patterns.get(&pattern)
    }

    pub(crate) fn match_facts(&self, expression: BoundExpressionId) -> Option<CheckedMatchFacts> {
        self.matches.get(&expression).copied()
    }

    pub(crate) fn for_iteration(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&CheckedForIterationFacts> {
        self.for_iterations.get(&expression)
    }
}

/// The exact root category of one independently checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitCheckRoot {
    /// A declared or anonymous callable body.
    CallableBody(BoundCallableBodyId),
    /// A declaration-owned expression.
    Expression(BoundExpressionId),
    /// An ordered declaration-owned expression sequence.
    ExpressionSequence(BoundBlockId),
}

/// Rejects an inconsistent whole-unit checker request before analysis begins.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitCheckRequestError {
    /// The root belongs to another bound unit.
    ForeignRoot,
    /// The root category does not match the independently checked unit category.
    RootKindMismatch,
}

impl UnitCheckRoot {
    pub(crate) const fn accepts(self, kind: BoundUnitKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::CallableBody(_),
                BoundUnitKind::CallableBody | BoundUnitKind::AnonymousCallable
            ) | (
                Self::Expression(_),
                BoundUnitKind::RuntimeDefault
                    | BoundUnitKind::ConstantTemplate
                    | BoundUnitKind::PredicateDefinition
            ) | (
                Self::ExpressionSequence(_),
                BoundUnitKind::Constraint | BoundUnitKind::ContractClause
            )
        )
    }

    pub(crate) const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::CallableBody(root) => AnyBoundNodeId::CallableBody(root),
            Self::Expression(root) => AnyBoundNodeId::Expression(root),
            Self::ExpressionSequence(root) => AnyBoundNodeId::Block(root),
        }
    }
}

impl<'view> UnitCheckRequest<'view> {
    /// Creates a checker request over committed read-only bound structure.
    ///
    /// Returns an error when the root belongs to another unit or its category
    /// does not match the independently checked unit.
    pub fn new(
        view: BoundUnitView<'view>,
        root: UnitCheckRoot,
        control_fact_selections: &'view ControlFactSelections,
        semantic_values: &'view SemanticValueStore,
        available_compiler_known_symbols: &'view AvailableCompilerKnownSymbols,
        cancellation: &'view dyn Cancellation,
    ) -> Result<Self, UnitCheckRequestError> {
        if root.node().unit() != view.unit() {
            return Err(UnitCheckRequestError::ForeignRoot);
        }

        if !root.accepts(view.kind()) {
            return Err(UnitCheckRequestError::RootKindMismatch);
        }

        Ok(Self {
            view,
            root,
            control_fact_selections,
            semantic_values,
            available_compiler_known_symbols,
            cancellation,
        })
    }

    /// Returns the read-only bound unit view to analyze.
    pub const fn view(self) -> BoundUnitView<'view> {
        self.view
    }

    /// Returns the exact bound root to analyze.
    pub const fn root(self) -> UnitCheckRoot {
        self.root
    }

    /// Returns exact prior semantic selections needed for control-fact publication.
    pub const fn control_fact_selections(self) -> &'view ControlFactSelections {
        self.control_fact_selections
    }

    /// Returns the canonical semantic values referenced by the bound unit.
    pub const fn semantic_values(self) -> &'view SemanticValueStore {
        self.semantic_values
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub const fn available_compiler_known_symbols(self) -> &'view AvailableCompilerKnownSymbols {
        self.available_compiler_known_symbols
    }

    /// Returns whether compilation cancellation has been requested.
    pub fn is_cancelled(self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundNodeOrigin, BoundPattern, BoundPatternKind, BoundPatternMode, BoundTreeBuilder,
        BoundUnitId, CheckedPatternFacts,
    };

    use super::{
        ControlFactSelections, ControlFactSelectionsError, UnitCheckRequest, UnitCheckRoot,
    };

    #[test]
    fn requests_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<UnitCheckRequest<'static>>();
        assert_send_sync::<UnitCheckRoot>();
    }

    #[test]
    fn selections_reject_duplicate_exact_pattern_facts() {
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(19));
        let pattern = match tree.push_pattern(BoundPattern::new(
            BoundNodeOrigin::source(crate::test_support::callable_key().source()),
            crate::test_support::error_type(),
            BoundPatternMode::Match,
            BoundPatternKind::Binding,
            [],
            [],
        )) {
            Ok(pattern) => pattern,
            Err(error) => panic!("test pattern must fit: {error:?}"),
        };
        let fact = CheckedPatternFacts::recovered(pattern);
        let mut selections = ControlFactSelections::new();

        assert_eq!(selections.insert_pattern(fact.clone()), Ok(()));
        assert_eq!(
            selections.insert_pattern(fact),
            Err(ControlFactSelectionsError::DuplicatePattern(pattern))
        );
    }
}
