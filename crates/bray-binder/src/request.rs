use std::collections::BTreeSet;

use bray_bound_tree::{BoundUnitKey, BoundUnitView};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_symbols::{AnySymbolId, ConstantSymbolId, SymbolFactKind, TypeId};

use crate::BinderFactContext;
use crate::unit::{
    BoundUnitConstructionError, BoundUnitConstructionResult, BoundUnitLocalBuilder,
    BoundUnitLocalCheckpoint,
};

/// The semantic category whose rules govern one binding request.
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

/// The operation performed by one pattern-binding request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PatternBindingMode {
    Declaration,
    Assignment,
    Match,
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
pub(crate) enum BinderDependency {
    Symbol {
        symbol: AnySymbolId,
        kind: SymbolFactKind,
    },
    Target(ConstantSymbolId),
    Unit(BoundUnitKey),
}

/// How an abandoned candidate classifies facts observed after its checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbandonedDependencyDisposition {
    RetainObserved,
    DiscardProvenIrrelevant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BinderRequestCheckpoint {
    unit: BoundUnitLocalCheckpoint,
    diagnostics: usize,
    expected_contexts: usize,
    control_targets: usize,
    dependency_log: usize,
}

/// Mutable state owned exclusively by one binding request.
#[derive(Debug)]
pub(crate) struct BinderRequestContext<'facts, C: BinderFactContext + ?Sized> {
    facts: &'facts C,
    binding_context: BindingContext,
    unit: BoundUnitLocalBuilder,
    diagnostics: Vec<Diagnostic>,
    expected_contexts: Vec<ExpectedContext>,
    control_targets: Vec<ControlTarget>,
    dependencies: BTreeSet<BinderDependency>,
    dependency_log: Vec<BinderDependency>,
}

impl<'facts, C: BinderFactContext + ?Sized> BinderRequestContext<'facts, C> {
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

    pub(crate) fn unit_view(&self) -> BoundUnitView<'_> {
        self.unit.view()
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

    pub(crate) fn checkpoint(&self) -> BinderRequestCheckpoint {
        BinderRequestCheckpoint {
            unit: self.unit.checkpoint(),
            diagnostics: self.diagnostics.len(),
            expected_contexts: self.expected_contexts.len(),
            control_targets: self.control_targets.len(),
            dependency_log: self.dependency_log.len(),
        }
    }

    pub(crate) fn rollback(
        &mut self,
        checkpoint: BinderRequestCheckpoint,
        dependencies: AbandonedDependencyDisposition,
    ) -> bool {
        if checkpoint.diagnostics > self.diagnostics.len()
            || checkpoint.expected_contexts > self.expected_contexts.len()
            || checkpoint.control_targets > self.control_targets.len()
            || checkpoint.dependency_log > self.dependency_log.len()
        {
            return false;
        }

        if !self.unit.rollback(checkpoint.unit) {
            return false;
        }

        self.diagnostics.truncate(checkpoint.diagnostics);
        self.expected_contexts
            .truncate(checkpoint.expected_contexts);
        self.control_targets.truncate(checkpoint.control_targets);

        if dependencies == AbandonedDependencyDisposition::DiscardProvenIrrelevant {
            for dependency in self.dependency_log.drain(checkpoint.dependency_log..) {
                self.dependencies.remove(&dependency);
            }
        }

        true
    }

    pub(crate) fn finish(self) -> Result<BinderRequestResult, BoundUnitConstructionError> {
        let unit = self.unit.finish()?;
        let diagnostics = DiagnosticBag::from(self.diagnostics);
        let dependencies = self
            .dependencies
            .into_iter()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(BinderRequestResult {
            unit,
            diagnostics,
            dependencies,
        })
    }
}

/// Frozen task-local output awaiting category-specific checked-unit assembly.
#[derive(Debug)]
pub(crate) struct BinderRequestResult {
    unit: BoundUnitConstructionResult,
    diagnostics: DiagnosticBag,
    dependencies: Box<[BinderDependency]>,
}

impl BinderRequestResult {
    pub(crate) const fn unit(&self) -> &BoundUnitConstructionResult {
        &self.unit
    }

    pub(crate) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(crate) const fn dependencies(&self) -> &[BinderDependency] {
        &self.dependencies
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_symbols::{AnyLocalSymbolId, LocalSymbolRegionId, SymbolFactKind};

    use super::{
        AbandonedDependencyDisposition, BinderDependency, BinderRequestContext, BindingContext,
        ControlTarget, ControlTargetKind, ExpectedContext, ExpectedSemanticKind,
    };
    use crate::BinderFactContext;
    use crate::fact::test_support::{TestContext, TestFixture as FactFixture};
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn request_contexts_expose_typed_inputs_and_nested_expectations() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(20));
        let mut request = BinderRequestContext::new(&facts, BindingContext::CallableBody, unit);
        let target = ControlTarget::new(
            ControlTargetKind::Callable,
            unit_fixture.first,
            Some(fact_fixture.declared_type),
        );

        request.push_expected(ExpectedContext::Semantic(ExpectedSemanticKind::Value));
        request.push_expected(ExpectedContext::Type(fact_fixture.declared_type));
        request.push_control_target(target);

        assert_eq!(request.binding_context(), BindingContext::CallableBody);
        assert_eq!(
            request.facts().semantic_values().id(),
            fact_fixture.semantic_values.id()
        );
        assert_eq!(
            request.expected(),
            Some(ExpectedContext::Type(fact_fixture.declared_type))
        );
        assert_eq!(request.control_target(), Some(target));
        assert_eq!(target.kind(), ControlTargetKind::Callable);
        assert_eq!(target.syntax(), unit_fixture.first);
        assert_eq!(target.expected_value(), Some(fact_fixture.declared_type));
        assert_eq!(
            request.pop_expected(),
            Some(ExpectedContext::Type(fact_fixture.declared_type))
        );
        assert_eq!(request.pop_control_target(), Some(target));
    }

    #[test]
    fn rollback_discards_candidate_state_and_reuses_deterministic_slots() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(21));
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);
        let root = request.unit_mut().root_scope();
        let checkpoint = request.checkpoint();
        let abandoned = push_binding(request.unit_mut(), root, unit_fixture.first, false);

        assert_eq!(request.unit_mut().activate_local(root, abandoned), Ok(()));

        request.push_expected(ExpectedContext::Type(fact_fixture.declared_type));
        request.push_control_target(ControlTarget::new(
            ControlTargetKind::Loop,
            unit_fixture.first,
            None,
        ));
        request.add_diagnostic(diagnostic(0));
        request.record_dependency(BinderDependency::Target(fact_fixture.constant));

        assert!(request.rollback(
            checkpoint,
            AbandonedDependencyDisposition::DiscardProvenIrrelevant
        ));
        assert_eq!(request.expected(), None);
        assert_eq!(request.control_target(), None);

        let reused = push_binding(request.unit_mut(), root, unit_fixture.first, false);

        assert_eq!(reused, abandoned);
        assert_eq!(request.unit_mut().activate_local(root, reused), Ok(()));

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("request must freeze: {error:?}"),
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
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);
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

        request.record_dependency(target_dependency.clone());
        request.record_dependency(symbol_dependency.clone());
        request.record_dependency(target_dependency.clone());

        let checkpoint = request.checkpoint();

        request.add_diagnostic(diagnostic(1));
        request.record_dependency(candidate_dependency.clone());

        assert!(request.rollback(checkpoint, AbandonedDependencyDisposition::RetainObserved));

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("request must freeze: {error:?}"),
        };

        assert_eq!(
            result.dependencies(),
            &[symbol_dependency, candidate_dependency, target_dependency]
        );
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn request_contexts_are_task_local_and_results_are_shareable() {
        fn assert_send<T: Send>() {}
        fn assert_send_sync<T: Send + Sync>() {}

        type TestRequest = BinderRequestContext<'static, TestContext<'static>>;

        assert_send::<TestRequest>();
        assert_send_sync::<super::BinderRequestResult>();
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
