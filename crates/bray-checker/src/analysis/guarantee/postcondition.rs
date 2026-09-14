use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpressionId, BoundReferenceTarget, SelectedCall};
use bray_symbols::ReceiverMode;

use super::flow::{ExecutionFlowDomain, ExecutionState};
use crate::{
    CheckerRequestContext, ExecutionCompletionContract, ExecutionCondition, ExecutionPlace,
};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn receiver_post_state(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &SelectedCall,
        contracts: &[&ExecutionCompletionContract],
    ) -> BTreeMap<ExecutionPlace, ExecutionCondition> {
        let mut inputs = BTreeMap::new();

        let Some(receiver) = call.receiver().filter(|receiver| {
            matches!(
                receiver.mode(),
                ReceiverMode::Shared | ReceiverMode::Mutable
            )
        }) else {
            return inputs;
        };

        let input = BoundReferenceTarget::Surface(receiver.parameter().into());

        if !contracts
            .iter()
            .flat_map(|contract| &contract.postconditions)
            .any(|(condition, _)| condition.inputs().iter().any(|place| place.root == input))
        {
            return inputs;
        }

        let Some(place) = crate::execution_guarantees::expression_place(
            self.request.unit(),
            self.semantics,
            receiver.expression(),
        ) else {
            return inputs;
        };

        let value = ExecutionCondition::PostState(expression, input);

        // The caller's receiver and callee predicate refer to the same normal-exit observation.
        state.assign(place, value.clone());
        inputs.insert(input.into(), value);

        inputs
    }
}
