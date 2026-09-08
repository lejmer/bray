use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundPatternId, BoundReferenceTarget,
    BoundUnitKey, BoundUnitView,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_symbols::{
    AnySymbolId, ConstantSymbolId, LocalBindingSymbolId, LocalScopeBoundary, LocalScopeId,
    SymbolName, SymbolQueryKind, TypeData, TypeId,
};

use crate::BindingQueryContext;
use crate::unit::{
    BoundUnitConstructionError, BoundUnitConstructionResult, BoundUnitLocalBuilder,
    BoundUnitLocalCheckpoint,
};

/// The operation performed by one pattern binder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PatternBindingMode {
    Declaration,
    Assignment,
    MatchObserve,
    MatchConsume,
}

/// The category of one active source-level control target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlTargetKind {
    Callable,
    Loop,
    GeneratorIteration,
    GeneratorRegion,
    Block,
}

/// One exact target available to return, break, or continue binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ControlTarget {
    kind: ControlTargetKind,
    syntax: SyntaxAnchor,
}

impl ControlTarget {
    pub(crate) const fn new(kind: ControlTargetKind, syntax: SyntaxAnchor) -> Self {
        Self { kind, syntax }
    }

    pub(crate) const fn kind(self) -> ControlTargetKind {
        self.kind
    }

    pub(crate) const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }
}

/// A semantic dependency observation that can invalidate one bound unit.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BinderDependency {
    /// One exact symbol-owned semantic query.
    Symbol {
        /// The exact symbol whose query was observed.
        symbol: AnySymbolId,
        /// The category of the observed symbol query.
        kind: SymbolQueryKind,
    },
    /// One target property represented by its compiler-known constant.
    Target(ConstantSymbolId),
    /// One nested or otherwise required checked semantic unit.
    Unit(BoundUnitKey),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BinderCheckpoint {
    unit: BoundUnitLocalCheckpoint,
    diagnostics: usize,
    control_targets: Box<[ControlTarget]>,
    known_value_type_log: usize,
    contextual_pattern_bindings: usize,
    dependency_log: usize,
}

/// Mutable state owned exclusively by one binding operation.
#[derive(Debug)]
pub(crate) struct Binder<'binding_context, C: BindingQueryContext + ?Sized> {
    binding_context: &'binding_context C,
    unit: BoundUnitLocalBuilder,
    diagnostics: Vec<Diagnostic>,
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

impl<'binding_context, C: BindingQueryContext + ?Sized> Binder<'binding_context, C> {
    pub(crate) fn new(binding_context: &'binding_context C, unit: BoundUnitLocalBuilder) -> Self {
        Self {
            binding_context,
            unit,
            diagnostics: Vec::new(),
            control_targets: Vec::new(),
            known_value_types: BTreeMap::new(),
            known_value_type_log: Vec::new(),
            contextual_pattern_bindings: Vec::new(),
            dependencies: BTreeSet::new(),
            dependency_log: Vec::new(),
        }
    }

    pub(crate) const fn binding_context(&self) -> &'binding_context C {
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

    pub(crate) fn children_are_recovered(
        &self,
        operands: &[BoundExpressionId],
        blocks: &[BoundBlockId],
    ) -> bool {
        operands
            .iter()
            .any(|operand| self.expression_is_recovered(*operand))
            || blocks.iter().any(|block| self.block_is_recovered(*block))
    }

    fn node_is_recovered(&self, node: AnyBoundNodeId) -> bool {
        self.unit_view()
            .node_is_recovered(node)
            .is_none_or(|is_recovered| is_recovered)
    }

    pub(crate) fn push_control_target(&mut self, target: ControlTarget) {
        self.control_targets.push(target);
    }

    pub(crate) fn pop_control_target(&mut self) -> Option<ControlTarget> {
        self.control_targets.pop()
    }

    #[cfg(test)]
    pub(crate) fn control_target(&self) -> Option<ControlTarget> {
        self.control_targets.last().copied()
    }

    pub(crate) fn control_target_of_kind(&self, kind: ControlTargetKind) -> Option<ControlTarget> {
        self.control_target_of_kinds(&[kind])
    }

    pub(crate) fn control_target_of_kinds(
        &self,
        kinds: &[ControlTargetKind],
    ) -> Option<ControlTarget> {
        self.control_targets
            .iter()
            .rev()
            .copied()
            .find(|target| kinds.contains(&target.kind()))
    }

    pub(crate) fn record_value_type(
        &mut self,
        target: BoundReferenceTarget,
        ty: TypeId,
    ) -> Result<(), bray_symbols::SemanticValueStoreError> {
        let data = self.binding_context.semantic_values().type_data(ty)?;

        if matches!(data.as_ref(), TypeData::Error) {
            return Ok(());
        }

        let previous = self.known_value_types.insert(target, ty);

        if previous != Some(ty) {
            self.known_value_type_log.push((target, previous));
        }

        Ok(())
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
        // The set defines publication order while the log lets failed transactions undo inserts.
        if self.dependencies.insert(dependency.clone()) {
            self.dependency_log.push(dependency);
        }
    }

    pub(crate) fn checkpoint(&self) -> BinderCheckpoint {
        BinderCheckpoint {
            unit: self.unit.checkpoint(),
            diagnostics: self.diagnostics.len(),
            control_targets: self.control_targets.clone().into_boxed_slice(),
            known_value_type_log: self.known_value_type_log.len(),
            contextual_pattern_bindings: self.contextual_pattern_bindings.len(),
            dependency_log: self.dependency_log.len(),
        }
    }

    pub(crate) fn rollback(&mut self, checkpoint: BinderCheckpoint) -> bool {
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

        for dependency in self.dependency_log.drain(checkpoint.dependency_log..) {
            self.dependencies.remove(&dependency);
        }

        true
    }

    pub(crate) fn transaction_context_is_balanced(&self, checkpoint: &BinderCheckpoint) -> bool {
        *checkpoint.control_targets == self.control_targets
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
            contract_inputs: None,
        })
    }
}

/// Completed binding state used to create a bound unit.
#[derive(Debug)]
pub(crate) struct BinderOutput {
    unit: BoundUnitConstructionResult,
    diagnostics: DiagnosticBag,
    dependencies: Box<[BinderDependency]>,
    contract_inputs: Option<(
        bray_symbols::CallableTypeTemplate,
        Vec<LocalBindingSymbolId>,
    )>,
}

impl BinderOutput {
    pub(crate) fn with_contract_inputs(
        mut self,
        signature: bray_symbols::CallableTypeTemplate,
        parameters: Vec<LocalBindingSymbolId>,
    ) -> Self {
        self.contract_inputs = Some((signature, parameters));

        self
    }

    pub(crate) const fn unit(&self) -> &BoundUnitConstructionResult {
        &self.unit
    }

    #[cfg(test)]
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
        Option<(
            bray_symbols::CallableTypeTemplate,
            Vec<LocalBindingSymbolId>,
        )>,
    ) {
        (
            self.unit,
            self.diagnostics,
            self.dependencies,
            self.contract_inputs,
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundReferenceTarget;
    use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_symbols::{
        AnyLocalSymbolId, LocalSymbolRegionId, SemanticValueStore, SemanticValueStoreError,
        SymbolQueryKind, TypeData,
    };

    use super::{Binder, BinderDependency, ControlTarget, ControlTargetKind};
    use crate::BindingQueryContext;
    use crate::query::test_support::{TestContext, TestFixture as QueryFixture};
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn binders_expose_typed_inputs_and_control_targets() {
        let query_fixture = QueryFixture::new();
        let binding_context = query_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(20));

        let mut binder = Binder::new(&binding_context, unit);

        let target = ControlTarget::new(ControlTargetKind::Callable, unit_fixture.first);

        binder.push_control_target(target);

        assert_eq!(
            binder.binding_context().semantic_values().id(),
            query_fixture.semantic_values.id()
        );

        assert_eq!(binder.control_target(), Some(target));
        assert_eq!(target.kind(), ControlTargetKind::Callable);
        assert_eq!(target.syntax(), unit_fixture.first);

        assert_eq!(binder.pop_control_target(), Some(target));
    }

    #[test]
    fn rollback_discards_candidate_state_and_reuses_deterministic_slots() {
        let query_fixture = QueryFixture::new();
        let binding_context = query_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(21));

        let mut binder = Binder::new(&binding_context, unit);

        let root = binder.unit_mut().root_scope();
        let checkpoint = binder.checkpoint();

        let abandoned = push_binding(binder.unit_mut(), root, unit_fixture.first, false);

        assert_eq!(binder.unit_mut().activate_local(root, abandoned), Ok(()));

        let Ok(known_type) = binding_context
            .semantic_values()
            .intern_type(TypeData::tuple([]))
        else {
            panic!("known test type must be available");
        };

        let abandoned_target = BoundReferenceTarget::Local(abandoned.into());

        binder
            .record_value_type(abandoned_target, known_type)
            .unwrap_or_else(|error| panic!("known test type must resolve: {error:?}"));

        assert_eq!(binder.value_type(abandoned_target), Some(known_type));

        binder.push_control_target(ControlTarget::new(
            ControlTargetKind::Loop,
            unit_fixture.first,
        ));

        binder.add_diagnostic(diagnostic(0));
        binder.record_dependency(BinderDependency::Target(query_fixture.constant));

        assert!(binder.rollback(checkpoint));

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
    fn recording_value_types_preserves_foreign_store_identity() {
        let query_fixture = QueryFixture::new();
        let binding_context = query_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(22));
        let mut binder = Binder::new(&binding_context, unit);

        let foreign = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("foreign store must initialize: {error:?}"));

        let foreign_type = foreign
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("foreign type must intern: {error:?}"));

        assert_eq!(
            binder.record_value_type(
                BoundReferenceTarget::Surface(query_fixture.constant.into()),
                foreign_type,
            ),
            Err(SemanticValueStoreError::ForeignId {
                expected: binding_context.semantic_values().id(),
                actual: foreign.id(),
            })
        );
    }

    #[test]
    fn observed_dependencies_are_deduplicated_sorted_and_rolled_back_with_failed_work() {
        let query_fixture = QueryFixture::new();
        let binding_context = query_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(22));

        let mut binder = Binder::new(&binding_context, unit);

        let symbol = query_fixture.constant.into();

        let symbol_dependency = BinderDependency::Symbol {
            symbol,
            kind: SymbolQueryKind::ConstantDeclaredType,
        };

        let target_dependency = BinderDependency::Target(query_fixture.constant);

        let candidate_dependency = BinderDependency::Symbol {
            symbol,
            kind: SymbolQueryKind::ConstantDefinition,
        };

        binder.record_dependency(target_dependency.clone());
        binder.record_dependency(symbol_dependency.clone());
        binder.record_dependency(target_dependency.clone());

        let checkpoint = binder.checkpoint();

        binder.add_diagnostic(diagnostic(1));
        binder.record_dependency(candidate_dependency.clone());

        assert!(binder.rollback(checkpoint));

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binder must freeze: {error:?}"),
        };

        assert_eq!(
            result.dependencies(),
            &[symbol_dependency, target_dependency]
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
