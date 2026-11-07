use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, StorageAccessPurpose};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    fn staged_memory_input(
        &self,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<bray_symbols::DependencySubjectRoot> {
        let selection = self.execution.semantics.selections().expression(operation.expression())?;
        if !selection.stages_inputs() {
            return None;
        }
        selection.inputs().find(|input| input.expression() == Some(argument)).map(|input| input.root())
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_argument_target(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<Target> {
        if self.execution.semantics.selections().expression(operation.expression()).is_some_and(|selection| selection.stages_inputs()) {
            let root = self.staged_memory_input(operation, argument)?;
            return self.invocation_place(operation.expression().into(), root).map(Target::storage);
        }
        self.expression_target(state, argument)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_argument_reference(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<Source> {
        if self.execution.semantics.selections().expression(operation.expression()).is_some_and(|selection| selection.stages_inputs()) {
            let root = self.staged_memory_input(operation, argument)?;
            return self.invocation_reference(state, operation.expression().into(), root);
        }
        self.expression_reference(state, argument)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_uninit_target(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        argument: BoundExpressionId,
    ) -> Option<Target> {
        let selection = self.execution.semantics.selections().expression(operation.expression());
        if !selection.is_some_and(|selection| selection.stages_inputs()) {
            return self.uninit_target(state, argument);
        }
        let root = self.staged_memory_input(operation, argument)?;
        let selection = selection?;
        let input = selection.inputs().find(|input| input.root() == root)?;
        let borrowed = matches!(self.request.semantic_values().type_data(input.ty(self.request.semantic_values())).as_ref(),
            bray_symbols::TypeData::Borrow { .. });
        let target = if borrowed {
            let mut regions = self.memory_argument_reference(state, operation, argument)?.regions(&state.memory)?;
            if regions.len() != 1 { return None; }
            regions.pop().expect("one protected input target")
        } else {
            self.memory_argument_target(state, operation, argument)?
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
        ) && matches!(self.execution.value(execution, expression), crate::ExecutionCondition::Literal(value)
                if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn assignment_target(
        &self,
        state: &StorageFlowState,
        node: AnyBoundNodeId,
    ) -> Option<Target> {
        let access = self
            .storage
            .node_plans(node)
            .find(|plan| plan.purpose() == StorageAccessPurpose::Assignment)?
            .access();

        self.input
            .reference_target(self.request.semantic_values(), &state.memory, self.storage, access)
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

    pub(in crate::analysis::storage_flow::check::analysis) fn uninit_target(
        &self,
        state: &StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Target> {
        let borrowed = self.expression_is_borrowed(expression);

        let storage = if borrowed {
            // A selected element has one bounded location even when its offset
            // is unknown. Memory transitions retain that target's inexactness.
            let targets = self
                .expression_reference(state, expression)?
                .regions(&state.memory)?;

            if targets.len() != 1 {
                return None;
            }

            targets
                .into_iter()
                .next()
                .expect("one protected storage target")
        } else {
            self.expression_target(state, expression)?
        };

        Some(storage.project(&[bray_bound_tree::StorageProjection::UninitPayload]))
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
