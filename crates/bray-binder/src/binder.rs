use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundPatternId, BoundReferenceTarget,
    BoundUnitKey, BoundUnitView,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_symbols::{
    AnySymbolId, ConstantSymbolId, LocalBindingSymbolId, LocalScopeBoundary, LocalScopeId,
    SymbolFactKind, SymbolName, TypeData, TypeId,
};

use crate::BinderFactContext;
use crate::unit::{
    BoundUnitConstructionError, BoundUnitConstructionResult, BoundUnitLocalBuilder,
    BoundUnitLocalCheckpoint,
};

/// The semantic category whose rules govern one binding operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingContext {
    Expression,
    TypeExpression,
    ConstantExpression,
    PredicateExpression,
    Pattern(PatternBindingMode),
    CallableBody,
    ContractClause,
    TrustedBoundary,
}

/// The operation performed by one pattern binder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PatternBindingMode {
    Declaration,
    Assignment,
    MatchObserve,
    MatchConsume,
}

/// One contextual expectation active while binding a nested construct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectedContext {
    Type(TypeId),
    Semantic(ExpectedSemanticKind),
}

/// A non-type semantic category expected from a nested construct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectedSemanticKind {
    Value,
    Type,
    Constant,
    Predicate,
    Pattern,
}

/// The category of one active source-level control target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlTargetKind {
    Callable,
    Loop,
    Generator,
    Block,
}

/// One exact target available to return, break, or continue binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlTarget {
    kind: ControlTargetKind,
    syntax: SyntaxAnchor,
    expected_value: Option<TypeId>,
}

impl ControlTarget {
    pub(crate) const fn new(
        kind: ControlTargetKind,
        syntax: SyntaxAnchor,
        expected_value: Option<TypeId>,
    ) -> Self {
        Self {
            kind,
            syntax,
            expected_value,
        }
    }

    pub(crate) const fn kind(self) -> ControlTargetKind {
        self.kind
    }

    pub(crate) const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }

    pub(crate) const fn expected_value(self) -> Option<TypeId> {
        self.expected_value
    }
}

/// A semantic fact observation that can invalidate one bound unit.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BinderDependency {
    /// One exact symbol-owned semantic fact.
    Symbol {
        /// The exact symbol whose fact was observed.
        symbol: AnySymbolId,
        /// The category of observed symbol fact.
        kind: SymbolFactKind,
    },
    /// One target-profile fact represented by its compiler-known constant.
    Target(ConstantSymbolId),
    /// One nested or otherwise required checked semantic unit.
    Unit(BoundUnitKey),
}

/// How an abandoned candidate classifies facts observed after its checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbandonedDependencyRelevance {
    Relevant,
    ProvenIrrelevant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BinderCheckpoint {
    unit: BoundUnitLocalCheckpoint,
    diagnostics: usize,
    expected_contexts: Box<[ExpectedContext]>,
    control_targets: Box<[ControlTarget]>,
    known_value_type_log: usize,
    contextual_pattern_bindings: usize,
    dependency_log: usize,
}

/// Mutable state owned exclusively by one binding operation.
#[derive(Debug)]
pub(crate) struct Binder<'facts, C: BinderFactContext + ?Sized> {
    facts: &'facts C,
    binding_context: BindingContext,
    unit: BoundUnitLocalBuilder,
    diagnostics: Vec<Diagnostic>,
    expected_contexts: Vec<ExpectedContext>,
    control_targets: Vec<ControlTarget>,
    known_value_types: BTreeMap<BoundReferenceTarget, TypeId>,
    known_value_type_log: Vec<(BoundReferenceTarget, Option<TypeId>)>,
    contextual_pattern_bindings: Vec<(
        LocalScopeId,
        SymbolName,
        LocalBindingSymbolId,
        BoundPatternId,
    )>,
    dependencies: BTreeSet<BinderDependency>,
    dependency_log: Vec<BinderDependency>,
}

impl<'facts, C: BinderFactContext + ?Sized> Binder<'facts, C> {
    pub(crate) fn new(
        facts: &'facts C,
        binding_context: BindingContext,
        unit: BoundUnitLocalBuilder,
    ) -> Self {
        Self {
            facts,
            binding_context,
            unit,
            diagnostics: Vec::new(),
            expected_contexts: Vec::new(),
            control_targets: Vec::new(),
            known_value_types: BTreeMap::new(),
            known_value_type_log: Vec::new(),
            contextual_pattern_bindings: Vec::new(),
            dependencies: BTreeSet::new(),
            dependency_log: Vec::new(),
        }
    }

    pub(crate) const fn facts(&self) -> &'facts C {
        self.facts
    }

    pub(crate) const fn binding_context(&self) -> BindingContext {
        self.binding_context
    }

    pub(crate) const fn unit_mut(&mut self) -> &mut BoundUnitLocalBuilder {
        &mut self.unit
    }

    pub(crate) const fn unit(&self) -> &BoundUnitLocalBuilder {
        &self.unit
    }

    pub(crate) fn unit_view(&self) -> BoundUnitView<'_> {
        self.unit.view()
    }

    pub(crate) fn expression_is_recovered(&self, expression: BoundExpressionId) -> bool {
        self.node_is_recovered(expression.into())
    }

    pub(crate) fn pattern_is_recovered(&self, pattern: BoundPatternId) -> bool {
        self.node_is_recovered(pattern.into())
    }

    pub(crate) fn block_is_recovered(&self, block: BoundBlockId) -> bool {
        self.node_is_recovered(block.into())
    }

    fn node_is_recovered(&self, node: AnyBoundNodeId) -> bool {
        self.unit_view()
            .node_is_recovered(node)
            .is_none_or(|is_recovered| is_recovered)
    }

    pub(crate) fn push_expected(&mut self, expected: ExpectedContext) {
        self.expected_contexts.push(expected);
    }

    pub(crate) fn pop_expected(&mut self) -> Option<ExpectedContext> {
        self.expected_contexts.pop()
    }

    pub(crate) fn expected(&self) -> Option<ExpectedContext> {
        self.expected_contexts.last().copied()
    }

    pub(crate) fn push_control_target(&mut self, target: ControlTarget) {
        self.control_targets.push(target);
    }

    pub(crate) fn pop_control_target(&mut self) -> Option<ControlTarget> {
        self.control_targets.pop()
    }

    pub(crate) fn control_target(&self) -> Option<ControlTarget> {
        self.control_targets.last().copied()
    }

    pub(crate) fn control_target_of_kind(&self, kind: ControlTargetKind) -> Option<ControlTarget> {
        self.control_targets
            .iter()
            .rev()
            .copied()
            .find(|target| target.kind() == kind)
    }

    pub(crate) fn record_value_type(&mut self, target: BoundReferenceTarget, ty: TypeId) {
        let Ok(data) = self.facts.semantic_values().type_data(ty) else {
            return;
        };

        if matches!(data.as_ref(), TypeData::Error) {
            return;
        }

        let previous = self.known_value_types.insert(target, ty);

        if previous != Some(ty) {
            self.known_value_type_log.push((target, previous));
        }
    }

    pub(crate) fn value_type(&self, target: BoundReferenceTarget) -> Option<TypeId> {
        self.known_value_types.get(&target).copied()
    }

    pub(crate) fn record_contextual_pattern_binding(
        &mut self,
        scope: LocalScopeId,
        name: SymbolName,
        binding: LocalBindingSymbolId,
        pattern: BoundPatternId,
    ) {
        self.contextual_pattern_bindings
            .push((scope, name, binding, pattern));
    }

    pub(crate) fn contextual_pattern_binding(
        &self,
        mut scope: LocalScopeId,
        name: &str,
    ) -> Result<Option<(LocalBindingSymbolId, BoundPatternId)>, BoundUnitConstructionError> {
        loop {
            if !self.unit.local_symbols_named(scope, name)?.is_empty()
                || !self.unit.surface_symbols_named(scope, name)?.is_empty()
            {
                return Ok(None);
            }

            if let Some((_, _, binding, pattern)) =
                self.contextual_pattern_bindings.iter().rev().find(
                    |(candidate_scope, candidate_name, _, _)| {
                        *candidate_scope == scope && candidate_name.as_str() == name
                    },
                )
            {
                return Ok(Some((*binding, *pattern)));
            }

            if self.unit.scope_boundary(scope)? == LocalScopeBoundary::Callable {
                return Ok(None);
            }

            let Some(parent) = self.unit.scope_parent(scope)? else {
                return Ok(None);
            };

            scope = parent;
        }
    }

    pub(crate) fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn record_dependency(&mut self, dependency: BinderDependency) {
        // The ordered set owns canonical publication order while the trail independently owns
        // rollback history, so a newly observed immutable key must be retained in both.
        if self.dependencies.insert(dependency.clone()) {
            self.dependency_log.push(dependency);
        }
    }

    pub(crate) fn checkpoint(&self) -> BinderCheckpoint {
        BinderCheckpoint {
            unit: self.unit.checkpoint(),
            diagnostics: self.diagnostics.len(),
            // Candidates may pop an enclosing context or replace one at the same depth. These
            // shallow Copy-only stacks therefore need exact snapshots while arena state uses
            // compact lengths and rollback trails.
            expected_contexts: self.expected_contexts.clone().into_boxed_slice(),
            control_targets: self.control_targets.clone().into_boxed_slice(),
            known_value_type_log: self.known_value_type_log.len(),
            contextual_pattern_bindings: self.contextual_pattern_bindings.len(),
            dependency_log: self.dependency_log.len(),
        }
    }

    pub(crate) fn rollback(
        &mut self,
        checkpoint: BinderCheckpoint,
        dependency_relevance: AbandonedDependencyRelevance,
    ) -> bool {
        if checkpoint.diagnostics > self.diagnostics.len()
            || checkpoint.known_value_type_log > self.known_value_type_log.len()
            || checkpoint.contextual_pattern_bindings > self.contextual_pattern_bindings.len()
            || checkpoint.dependency_log > self.dependency_log.len()
        {
            return false;
        }

        if !self.unit.rollback(checkpoint.unit) {
            return false;
        }

        self.diagnostics.truncate(checkpoint.diagnostics);

        self.expected_contexts = checkpoint.expected_contexts.into_vec();
        self.control_targets = checkpoint.control_targets.into_vec();

        for (target, previous) in self
            .known_value_type_log
            .drain(checkpoint.known_value_type_log..)
            .rev()
        {
            match previous {
                Some(ty) => {
                    self.known_value_types.insert(target, ty);
                }
                None => {
                    self.known_value_types.remove(&target);
                }
            }
        }

        self.contextual_pattern_bindings
            .truncate(checkpoint.contextual_pattern_bindings);

        if dependency_relevance == AbandonedDependencyRelevance::ProvenIrrelevant {
            for dependency in self.dependency_log.drain(checkpoint.dependency_log..) {
                self.dependencies.remove(&dependency);
            }
        }

        true
    }

    pub(crate) fn candidate_context_is_balanced(&self, checkpoint: &BinderCheckpoint) -> bool {
        *checkpoint.expected_contexts == self.expected_contexts
            && *checkpoint.control_targets == self.control_targets
    }

    pub(crate) fn finish(self) -> Result<BinderOutput, BoundUnitConstructionError> {
        let unit = self.unit.finish()?;
        let diagnostics = DiagnosticBag::from(self.diagnostics);

        let dependencies = self
            .dependencies
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(BinderOutput {
            unit,
            diagnostics,
            dependencies,
        })
    }
}

/// Completed binding state used to create a bound unit.
#[derive(Debug)]
pub(crate) struct BinderOutput {
    unit: BoundUnitConstructionResult,
    diagnostics: DiagnosticBag,
    dependencies: Box<[BinderDependency]>,
}

impl BinderOutput {
    pub(crate) const fn unit(&self) -> &BoundUnitConstructionResult {
        &self.unit
    }

    pub(crate) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(crate) const fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        BoundUnitConstructionResult,
        DiagnosticBag,
        Box<[BinderDependency]>,
    ) {
        (self.unit, self.diagnostics, self.dependencies)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundReferenceTarget;
    use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_symbols::{AnyLocalSymbolId, LocalSymbolRegionId, SymbolFactKind, TypeData};

    use super::{
        AbandonedDependencyRelevance, Binder, BinderDependency, BindingContext, ControlTarget,
        ControlTargetKind, ExpectedContext, ExpectedSemanticKind,
    };
    use crate::BinderFactContext;
    use crate::fact::test_support::{TestContext, TestFixture as FactFixture};
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn binders_expose_typed_inputs_and_nested_expectations() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(20));

        let mut binder = Binder::new(&facts, BindingContext::CallableBody, unit);

        let target = ControlTarget::new(
            ControlTargetKind::Callable,
            unit_fixture.first,
            Some(fact_fixture.declared_type),
        );

        binder.push_expected(ExpectedContext::Semantic(ExpectedSemanticKind::Value));
        binder.push_expected(ExpectedContext::Type(fact_fixture.declared_type));
        binder.push_control_target(target);

        assert_eq!(binder.binding_context(), BindingContext::CallableBody);

        assert_eq!(
            binder.facts().semantic_values().id(),
            fact_fixture.semantic_values.id()
        );

        assert_eq!(
            binder.expected(),
            Some(ExpectedContext::Type(fact_fixture.declared_type))
        );

        assert_eq!(binder.control_target(), Some(target));
        assert_eq!(target.kind(), ControlTargetKind::Callable);
        assert_eq!(target.syntax(), unit_fixture.first);
        assert_eq!(target.expected_value(), Some(fact_fixture.declared_type));

        assert_eq!(
            binder.pop_expected(),
            Some(ExpectedContext::Type(fact_fixture.declared_type))
        );

        assert_eq!(binder.pop_control_target(), Some(target));
    }

    #[test]
    fn rollback_discards_candidate_state_and_reuses_deterministic_slots() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(21));

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let root = binder.unit_mut().root_scope();
        let checkpoint = binder.checkpoint();

        let abandoned = push_binding(binder.unit_mut(), root, unit_fixture.first, false);

        assert_eq!(binder.unit_mut().activate_local(root, abandoned), Ok(()));

        let Ok(known_type) = facts.semantic_values().intern_type(TypeData::tuple([])) else {
            panic!("known test type must be available");
        };

        let abandoned_target = BoundReferenceTarget::Local(abandoned.into());

        binder.record_value_type(abandoned_target, known_type);

        assert_eq!(binder.value_type(abandoned_target), Some(known_type));

        binder.push_expected(ExpectedContext::Type(fact_fixture.declared_type));

        binder.push_control_target(ControlTarget::new(
            ControlTargetKind::Loop,
            unit_fixture.first,
            None,
        ));

        binder.add_diagnostic(diagnostic(0));
        binder.record_dependency(BinderDependency::Target(fact_fixture.constant));

        assert!(binder.rollback(checkpoint, AbandonedDependencyRelevance::ProvenIrrelevant));

        assert_eq!(binder.expected(), None);
        assert_eq!(binder.control_target(), None);
        assert_eq!(binder.value_type(abandoned_target), None);

        let reused = push_binding(binder.unit_mut(), root, unit_fixture.first, false);

        assert_eq!(reused, abandoned);
        assert_eq!(binder.unit_mut().activate_local(root, reused), Ok(()));

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binder must freeze: {error:?}"),
        };

        assert!(result.diagnostics().is_empty());
        assert!(result.dependencies().is_empty());

        let Some(scope) = result.unit().local_symbols().scope(root) else {
            panic!("root scope must be published");
        };

        assert_eq!(
            scope.local_symbols_named("value"),
            &[AnyLocalSymbolId::from(reused)]
        );
    }

    #[test]
    fn observed_dependencies_are_deduplicated_sorted_and_retained_when_relevant() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(22));

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let symbol = fact_fixture.constant.into();
        let symbol_dependency = BinderDependency::Symbol {
            symbol,
            kind: SymbolFactKind::ConstantDeclaredType,
        };

        let target_dependency = BinderDependency::Target(fact_fixture.constant);
        let candidate_dependency = BinderDependency::Symbol {
            symbol,
            kind: SymbolFactKind::ConstantDefinition,
        };

        binder.record_dependency(target_dependency.clone());
        binder.record_dependency(symbol_dependency.clone());
        binder.record_dependency(target_dependency.clone());

        let checkpoint = binder.checkpoint();

        binder.add_diagnostic(diagnostic(1));
        binder.record_dependency(candidate_dependency.clone());

        assert!(binder.rollback(checkpoint, AbandonedDependencyRelevance::Relevant));

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binder must freeze: {error:?}"),
        };

        assert_eq!(
            result.dependencies(),
            &[symbol_dependency, candidate_dependency, target_dependency]
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn binders_are_task_local_and_outputs_are_shareable() {
        fn assert_send<T: Send>() {}
        fn assert_send_sync<T: Send + Sync>() {}

        type TestBinder = Binder<'static, TestContext<'static>>;

        assert_send::<TestBinder>();
        assert_send_sync::<super::BinderOutput>();
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
