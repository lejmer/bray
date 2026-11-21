use std::borrow::Cow;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, ReachedAccess, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::BoundExpressionId;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn captured_memory_operand<'entry>(
        &self,
        execution: &'entry crate::analysis::guarantee::ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<&'entry bray_bound_tree::StorageCapturedValue> {
        let invocation = operation.expression().into();
        let selection = self.execution.semantics.selections().expression(operation.expression())?;
        let input = selection.inputs().find(|input| input.expression() == Some(argument))?;
        let ordinal = selection.invocation_input_ordinal(invocation, input.root())?;

        execution.admitted_call(invocation)?.argument_storage(ordinal)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_uninit_target(
        &self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<Target> {
        let selection = self.execution.semantics.selections().expression(operation.expression())?;
        let input = selection.inputs().find(|input| input.expression() == Some(argument))?;
        let values = self.request.semantic_values();

        let target = if let bray_symbols::TypeData::Borrow { target, .. } = *values.type_data(input.ty(values)) {
            let capture = self.captured_memory_operand(execution, operation, argument)?;

            state.memory.captured_value_target(capture.binding()?, target)?
        } else if selection.stages_inputs() {
            self.invocation_place(operation.expression().into(), input.root()).map(Target::storage)?
        } else {
            self.expression_target(state, argument)?
        };

        Some(target.project(&[bray_bound_tree::StorageProjection::UninitPayload]))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn nullable_is_absent(
        &self,
        execution: &crate::analysis::guarantee::ExecutionState,
        expression: BoundExpressionId,
        ty: bray_symbols::TypeId,
    ) -> bool {
        matches!(
            self.request.semantic_values().type_data(ty).as_ref(),
            bray_symbols::TypeData::Nullable(_)
        ) && matches!((self.execution.operand_value(execution, expression)).kind(), crate::ExecutionConditionKind::Literal(value)
                if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn assignment_destination<'memory>(
        &self,
        memory: &'memory crate::analysis::storage_flow::memory::State,
        expression: BoundExpressionId,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Cow<'memory, ReachedAccess>> {
        if let Some(target) = self.input.direct_reference_target(self.storage, access) {
            return Some(Cow::Owned(ReachedAccess { address: bray_bound_tree::StorageAddressValue::one(target), loan: None }));
        }

        let address = memory.assignment_destination(expression)?;

        (!address.address.is_unknown()).then_some(Cow::Borrowed(address))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn expression_is_borrowed(
        &self,
        expression: BoundExpressionId,
    ) -> bool {
        self.execution
            .semantics
            .types()
            .expression(expression)
            .is_some_and(|ty| {
                matches!(
                    self.request.semantic_values().type_data(ty.ty()).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                )
            })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn expression_target(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Target> {
        self.expression_result_storage(expression)
            .map(Target::storage)
            .or_else(|| {
                self.storage.node_plans(expression.into()).find_map(|plan| {
                    self.input
                        .reference_target(self.request.semantic_values(), &state.memory, self.storage, plan.access())
                })
            })
            .or_else(|| self.argument_storage(expression).map(Target::storage))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn argument_storage(
        &self,
        expression: BoundExpressionId,
    ) -> Option<Place> {
        self.expression_result_storage(expression).or_else(|| {
            self.input.expression_places(expression).first().cloned()
        })
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn expression_result_storage(
        &self,
        expression: BoundExpressionId,
    ) -> Option<Place> {
        self.storage.value_slot(expression).map(Place::root)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn expression_reference(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Source> {
        if let Some(slot) = self.expression_result_storage(expression) {
            // Completed evaluated operands use their produced slot, never the consumed source.
            return state.memory.has_stored_value(&slot)
                .then(|| Source::Value(Target::storage(slot)));
        }

        let borrowed = self.expression_is_borrowed(expression);

        if borrowed
            && let Some(capability) = self
                .storage
                .node_plans(expression.into())
                .filter_map(|plan| self.storage.borrow_for_plan(plan))
                .find(|capability| state.memory.has_borrow(*capability))
        {
            // A newly formed borrow names the aggregate, not a reference stored inside it.
            return Some(Source::Borrow(capability));
        }

        if let Some(target) = self.storage.node_plans(expression.into()).find_map(|plan| {
            let target = self
                .input
                .reference_target(self.request.semantic_values(), &state.memory, self.storage, plan.access())?;

            state.memory.has_value(&target).then_some(target)
        }) {
            return Some(Source::Value(target));
        }

        if let Some(slot) = self
            .argument_storage(expression)
            .filter(|storage| state.memory.has_stored_value(storage))
        {
            return Some(Source::Value(Target::storage(slot)));
        }

        if !borrowed {
            return None;
        }

        self.storage
            .node_plans(expression.into())
            .filter_map(|plan| {
                self.storage
                    .access(plan.access())?
                    .root()
                    .borrow_capability()
            })
            .find(|capability| state.memory.has_borrow(*capability))
            .map(Source::Borrow)
    }
}
