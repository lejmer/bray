use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::ExecutionProperty;

/// Checked declaration assumptions and body observations used to verify execution guarantees.
///
/// Expression conditions describe the callable's entry inputs. They cease to describe runtime
/// state after an operation changes those inputs. Absent symbolic meaning supplies no evidence.
pub struct ExecutionGuaranteeInput {
    conditions: bray_symbols::CallableConditionSet,
    result_ordinal: bray_symbols::SymbolOrdinal,
    unknown_observation_start: Option<u32>,
    source_observation_count: Option<u32>,
    expressions: std::collections::BTreeMap<
        bray_bound_tree::BoundExpressionId,
        bray_symbols::ConstantTermId,
    >,
    calls: std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, ExecutionCallInput>,
    propagation_payloads:
        std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, bray_symbols::SymbolOrdinal>,
    scalar_observations: std::collections::BTreeSet<bray_bound_tree::BoundExpressionId>,
    local_observations: std::collections::BTreeMap<
        bray_symbols::LocalBindingSymbolId,
        bray_symbols::ConstantTermId,
    >,
    lifecycle: std::collections::BTreeMap<
        (
            bray_symbols::TypeId,
            bray_symbols::TypeAssociatedLifecycleSlot,
        ),
        ExecutionLifecycleInput,
    >,
    cleanup_dependencies: std::collections::BTreeMap<
        bray_symbols::TypeId,
        std::collections::BTreeSet<bray_symbols::TypeId>,
    >,
    storage_observations: std::collections::BTreeMap<
        bray_bound_tree::StorageIdentityId,
        bray_symbols::ConstantTermId,
    >,
    cleanup_calls:
        std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, bray_symbols::TypeId>,
    projection_dependencies: std::collections::BTreeSet<bray_bound_tree::CallableProofDependency>,
    projections_verified: bool,
    inert_calls: std::collections::BTreeSet<bray_bound_tree::BoundExpressionId>,
}

impl ExecutionGuaranteeInput {
    /// Retains normalized declaration conditions and checked symbolic expression meanings.
    pub fn new(
        conditions: bray_symbols::CallableConditionSet,
        result_ordinal: bray_symbols::SymbolOrdinal,
        expressions: std::collections::BTreeMap<
            bray_bound_tree::BoundExpressionId,
            bray_symbols::ConstantTermId,
        >,
        mut calls: std::collections::BTreeMap<
            bray_bound_tree::BoundExpressionId,
            ExecutionCallInput,
        >,
        local_observations: std::collections::BTreeMap<
            bray_symbols::LocalBindingSymbolId,
            bray_symbols::ConstantTermId,
        >,
        propagations: impl IntoIterator<Item = bray_bound_tree::BoundExpressionId>,
    ) -> Self {
        let mut next = result_ordinal
            .raw()
            .checked_add(1)
            .and_then(|start| start.checked_add(u32::try_from(local_observations.len()).ok()?));

        let source_observation_count = next;

        for call in calls.values_mut() {
            call.observation_start = next.map(bray_symbols::SymbolOrdinal::new);

            next = next.and_then(|next| {
                next.checked_add(u32::try_from(call.arguments.len()).ok()?.checked_mul(2)?)?
                    .checked_add(1)
            });
        }

        let mut propagation_payloads = std::collections::BTreeMap::new();

        for expression in propagations {
            if let Some(ordinal) = next {
                propagation_payloads.insert(expression, bray_symbols::SymbolOrdinal::new(ordinal));
                next = ordinal.checked_add(1);
            }
        }

        Self {
            conditions,
            result_ordinal,
            unknown_observation_start: next,
            source_observation_count,
            expressions,
            calls,
            propagation_payloads,
            scalar_observations: Default::default(),
            local_observations,
            lifecycle: Default::default(),
            cleanup_dependencies: Default::default(),
            storage_observations: Default::default(),
            cleanup_calls: Default::default(),
            projection_dependencies: Default::default(),
            projections_verified: false,
            inert_calls: Default::default(),
        }
    }

    /// Retains selected lifecycle contracts and the symbolic roots of owned storage.
    pub fn with_lifecycle(
        mut self,
        lifecycle: std::collections::BTreeMap<
            (
                bray_symbols::TypeId,
                bray_symbols::TypeAssociatedLifecycleSlot,
            ),
            ExecutionLifecycleInput,
        >,
        cleanup_dependencies: std::collections::BTreeMap<
            bray_symbols::TypeId,
            std::collections::BTreeSet<bray_symbols::TypeId>,
        >,
        storage_observations: std::collections::BTreeMap<
            bray_bound_tree::StorageIdentityId,
            bray_symbols::ConstantTermId,
        >,
        cleanup_calls: std::collections::BTreeMap<
            bray_bound_tree::BoundExpressionId,
            bray_symbols::TypeId,
        >,
    ) -> Self {
        self.lifecycle = lifecycle;
        self.cleanup_dependencies = cleanup_dependencies;
        self.storage_observations = storage_observations;
        self.cleanup_calls = cleanup_calls;

        self
    }

    pub(crate) fn needs_completion_check(&self) -> bool {
        !self.cleanup_calls.is_empty()
            || self
                .lifecycle
                .keys()
                .any(|(_, slot)| *slot == bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)
    }

    /// Resolves observing pattern bindings to the storage paths they alias.
    pub fn with_resolved_storage(
        mut self,
        values: &bray_symbols::SemanticValueStore,
        storage: &bray_bound_tree::StoragePlan,
    ) -> Result<Self, bray_symbols::SemanticValueStoreError> {
        use bray_bound_tree::{StorageBinding, StorageBindingTarget};

        let mut aliases = std::collections::BTreeMap::new();

        for (target, binding) in storage.bindings() {
            let (StorageBindingTarget::Local(local), StorageBinding::Access(access)) =
                (target, binding)
            else {
                continue;
            };

            let Some(alias) = self.local_observation(*local) else {
                continue;
            };

            let Some(subject) = storage
                .root_identity(*access)
                .and_then(|identity| self.storage_observation(identity))
            else {
                continue;
            };

            let Some(projections) = storage.resolved_projections(*access) else {
                continue;
            };

            if let Some(subject) = crate::constant::shape::storage_path_observation(
                values,
                subject,
                projections,
                self.projections_verified,
            )? {
                aliases.insert(alias, subject);
            }
        }

        let mut expressions = std::collections::BTreeMap::new();

        for (expression, term) in self.expressions {
            let mut remaining = crate::contract::MAX_CONDITION_STEPS;

            let resolved = crate::contract::rewrite_condition_with_budget(
                values,
                term,
                &mut remaining,
                |term, _| {
                    Ok(match aliases.get(&term) {
                        Some(subject) => std::ops::ControlFlow::Break(Some(*subject)),
                        None => std::ops::ControlFlow::Continue(()),
                    })
                },
            )?;

            if let Some(term) = resolved {
                expressions.insert(expression, term);
            }
        }

        self.expressions = expressions;

        Ok(self)
    }

    /// Retains implementation evidence for implicit storage projections and checked inert calls.
    pub fn with_implicit_execution(
        mut self,
        dependencies: Option<Vec<bray_bound_tree::CallableProofDependency>>,
        memory: &bray_bound_tree::CheckedMemoryOperations,
    ) -> Self {
        self.projections_verified = dependencies.is_some();
        self.projection_dependencies = dependencies.into_iter().flatten().collect();

        self.inert_calls = memory
            .operations()
            .iter()
            .filter(|operation| {
                !memory.is_recovered()
                    && matches!(
                        operation.kind(),
                        bray_bound_tree::CheckedMemoryOperationKind::BorrowFrom { .. }
                            | bray_bound_tree::CheckedMemoryOperationKind::UninitPointer { .. }
                    )
            })
            .map(|operation| operation.expression())
            .collect();

        self
    }

    pub(crate) const fn projections_verified(&self) -> bool {
        self.projections_verified
    }

    pub(crate) fn projection_dependencies(
        &self,
    ) -> &std::collections::BTreeSet<bray_bound_tree::CallableProofDependency> {
        &self.projection_dependencies
    }

    pub(crate) fn is_inert_call(&self, expression: bray_bound_tree::BoundExpressionId) -> bool {
        self.inert_calls.contains(&expression)
    }

    pub(crate) fn cleanup_call(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Option<bray_symbols::TypeId> {
        self.cleanup_calls.get(&expression).copied()
    }

    pub(crate) fn finalizer(&self, ty: bray_symbols::TypeId) -> Option<&ExecutionLifecycleInput> {
        self.lifecycle(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)
    }

    pub(crate) fn lifecycle(
        &self,
        ty: bray_symbols::TypeId,
        slot: bray_symbols::TypeAssociatedLifecycleSlot,
    ) -> Option<&ExecutionLifecycleInput> {
        self.lifecycle.get(&(ty, slot))
    }

    pub(crate) fn cleanup_dependencies(
        &self,
        ty: bray_symbols::TypeId,
    ) -> impl Iterator<Item = bray_symbols::TypeId> + '_ {
        self.cleanup_dependencies
            .get(&ty)
            .into_iter()
            .flatten()
            .copied()
    }

    pub(crate) fn requires_completion_plan(&self, ty: bray_symbols::TypeId) -> bool {
        let mut pending = self.cleanup_dependencies(ty).collect::<Vec<_>>();
        let mut visited = std::collections::BTreeSet::new();

        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }

            if self.finalizer(ty).is_some() {
                return true;
            }

            pending.extend(self.cleanup_dependencies(ty));
        }

        false
    }

    pub(crate) fn storage_observation(
        &self,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> Option<bray_symbols::ConstantTermId> {
        self.storage_observations.get(&identity).copied()
    }

    pub(crate) fn unknown_observation(
        &self,
        operation: crate::analysis::AnalysisObservationSite,
        original: bray_symbols::SymbolOrdinal,
    ) -> Option<bray_symbols::SymbolOrdinal> {
        let width = self.source_observation_count?;

        if original.raw() >= width {
            return None;
        }

        let operation = u32::try_from(operation.to_index()?).ok()?;
        let offset = operation.checked_mul(width)?.checked_add(original.raw())?;

        self.unknown_observation_start?
            .checked_add(offset)
            .map(bray_symbols::SymbolOrdinal::new)
    }

    pub(crate) fn propagation_payload(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Option<bray_symbols::SymbolOrdinal> {
        self.propagation_payloads.get(&expression).copied()
    }

    /// Marks scalar value expressions whose observations survive later storage mutation.
    pub fn with_scalar_observations(
        mut self,
        expressions: impl IntoIterator<Item = bray_bound_tree::BoundExpressionId>,
    ) -> Self {
        self.scalar_observations.extend(expressions);

        self
    }

    pub(crate) fn is_scalar_observation(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> bool {
        self.scalar_observations.contains(&expression)
    }

    pub(crate) fn local_observation(
        &self,
        binding: bray_symbols::LocalBindingSymbolId,
    ) -> Option<bray_symbols::ConstantTermId> {
        self.local_observations.get(&binding).copied()
    }

    pub(crate) const fn conditions(&self) -> &bray_symbols::CallableConditionSet {
        &self.conditions
    }

    pub(crate) const fn result_ordinal(&self) -> bray_symbols::SymbolOrdinal {
        self.result_ordinal
    }

    pub(crate) fn expression(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Option<bray_symbols::ConstantTermId> {
        self.expressions.get(&expression).copied()
    }

    pub(crate) fn call(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Option<&ExecutionCallInput> {
        self.calls.get(&expression)
    }

    pub(crate) fn dependency_count(&self) -> usize {
        use bray_symbols::CallableConditions;

        self.calls
            .values()
            .fold(self.scalar_observations.len(), |count, call| {
                count
                    .saturating_add(call.conditions.execution_guarantees().len())
                    .saturating_add(call.conditions.normal_completion_postconditions().len())
                    .saturating_add(call.conditions.guarded_postconditions().len())
                    .saturating_add(1)
            })
    }
}

/// One selected implicit lifecycle callable and its instantiated declaration promises.
/// Completion checking validates the selected implementation before using these promises.
pub struct ExecutionLifecycleInput {
    callable: bray_symbols::CallableInstanceData,
    conditions: bray_symbols::CallableConditionSet,
    result: bray_symbols::TypeId,
    execution: bray_symbols::CallableExecution,
}

impl ExecutionLifecycleInput {
    /// Retains the receiver-normalized contract of the selected lifecycle body.
    pub const fn new(
        callable: bray_symbols::CallableInstanceData,
        conditions: bray_symbols::CallableConditionSet,
        result: bray_symbols::TypeId,
        execution: bray_symbols::CallableExecution,
    ) -> Self {
        Self {
            callable,
            conditions,
            result,
            execution,
        }
    }

    pub(crate) const fn execution(&self) -> bray_symbols::CallableExecution {
        self.execution
    }

    /// Returns the selected instance whose implementation supplies proof evidence.
    pub const fn callable(&self) -> bray_symbols::CallableInstanceData {
        self.callable
    }

    /// Returns promises normalized for the selected receiver and generic arguments.
    pub const fn conditions(&self) -> &bray_symbols::CallableConditionSet {
        &self.conditions
    }

    /// Returns the selected body's completion type, before any async frame wrapping.
    pub const fn result(&self) -> bray_symbols::TypeId {
        self.result
    }
}

/// A selected synchronous call's instantiated declaration contract and receiver-first value inputs.
/// Its promises are candidates for dependency checking, not implementation evidence.
pub struct ExecutionCallInput {
    conditions: bray_symbols::CallableConditionSet,
    arguments: Box<[ExecutionCallArgument]>,
    observation_start: Option<bray_symbols::SymbolOrdinal>,
    borrowed_arguments: std::collections::BTreeSet<bray_bound_tree::BoundExpressionId>,
}

/// An explicit evaluated input or an independently checked inert default.
#[derive(Clone, Copy)]
pub enum ExecutionCallArgument {
    /// An expression occurrence in the caller's evaluation order.
    Expression(bray_bound_tree::BoundExpressionId),
    /// A checked constant term with no runtime observation dependencies.
    Constant(bray_symbols::ConstantTermId),
}

impl ExecutionCallInput {
    /// Retains generic-normalized conditions and explicit inputs in declaration order.
    pub fn new(
        conditions: bray_symbols::CallableConditionSet,
        arguments: impl IntoIterator<Item = ExecutionCallArgument>,
        borrowed_arguments: impl IntoIterator<Item = bray_bound_tree::BoundExpressionId>,
    ) -> Self {
        Self {
            conditions,
            arguments: arguments.into_iter().collect(),
            observation_start: None,
            borrowed_arguments: borrowed_arguments.into_iter().collect(),
        }
    }

    pub(crate) const fn conditions(&self) -> &bray_symbols::CallableConditionSet {
        &self.conditions
    }

    pub(crate) fn arguments(&self) -> &[ExecutionCallArgument] {
        &self.arguments
    }

    pub(crate) fn observation_ordinal(&self, index: usize) -> Option<bray_symbols::SymbolOrdinal> {
        (index <= self.arguments.len().checked_mul(2)?).then_some(())?;

        self.observation_start?
            .raw()
            .checked_add(u32::try_from(index).ok()?)
            .map(bray_symbols::SymbolOrdinal::new)
    }

    pub(crate) fn borrows_argument(&self, expression: bray_bound_tree::BoundExpressionId) -> bool {
        self.borrowed_arguments.contains(&expression)
    }
}

/// Checks an execution-property declaration against the language vocabulary.
///
/// Recognizing a property creates a proof obligation, not proof of a callable's behavior.
pub fn check_execution_property(
    name: &str,
    source: SourceSpan,
) -> Result<ExecutionProperty, Diagnostic> {
    ExecutionProperty::from_name(name).ok_or_else(|| {
        Diagnostic::new(
            DiagnosticId::new(source.range().start().bytes()),
            DiagnosticKind::CheckingUnknownExecutionProperty,
            SeverityKind::Error,
        )
        .with_primary_span(source)
        .with_arg(DiagnosticArg::referenced_name(name))
    })
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextSize};
    use bray_symbols::ExecutionProperty;

    use super::check_execution_property;

    #[test]
    fn execution_property_checking_preserves_the_unknown_identifier() {
        let source = SourceSpan::empty(SourceId::new(0), TextSize::ZERO);

        assert_eq!(
            check_execution_property("pure", source),
            Ok(ExecutionProperty::Pure)
        );

        assert_eq!(
            check_execution_property("total", source),
            Ok(ExecutionProperty::Total)
        );

        let error = check_execution_property("constant", source).unwrap_err();

        assert_eq!(
            error.kind(),
            bray_diagnostics::DiagnosticKind::CheckingUnknownExecutionProperty
        );

        assert_eq!(error.primary_span(), Some(source));

        assert!(
            error
                .args()
                .contains(&bray_diagnostics::DiagnosticArg::referenced_name(
                    "constant"
                ))
        );
    }
}
