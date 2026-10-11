use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, IndexTarget, SelectedOperation,
    SemanticSelection,
};

use super::flow::ExecutionFlowDomain;
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn index_in_bounds(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> bool {
        if !matches!(
            self.semantics.selections().expression(expression),
            Some(SemanticSelection::Operation(SelectedOperation::Index {
                target: IndexTarget::ArrayElement | IndexTarget::SliceElement,
                ..
            }))
        ) {
            return false;
        }

        let Some(BoundExpression::Structured(index)) = self.request.view().expression(expression)
        else {
            return false;
        };

        let [receiver, selector] = index.operands() else {
            return false;
        };

        let Some(ty) = self.semantics.types().expression(*selector) else {
            return false;
        };

        if crate::representation::type_representation(self.request, ty.ty())
            != Some(bray_compiler_known::RepresentationRole::ScalarUsize)
        {
            return false;
        }

        let receiver = self.value(state, *receiver);
        let selector = self.value(state, *selector);

        if referent(&receiver) == &ExecutionCondition::Unknown
            || selector == ExecutionCondition::Unknown
        {
            return false;
        }

        let equalities = ExecutionCondition::equalities(&state.assumptions, None);

        let assumptions = state
            .assumptions
            .iter()
            .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
            .collect();

        self.sequence_lengths.iter().any(|length| {
            let Some(observed) = state.entries.get(&(*length).into()).and_then(|entry| {
                entry.arguments.get(&crate::ExecutionPlace::argument(
                    bray_symbols::SymbolOrdinal::new(0),
                ))
            }) else {
                return false;
            };

            if referent(&receiver) != referent(observed) {
                return false;
            }

            let length = self.value(state, *length);

            if length == ExecutionCondition::Unknown {
                return false;
            }

            // Each candidate length needs its own condition while the selector stays shared.
            ExecutionCondition::operation(BoundOperator::Less, vec![selector.clone(), length])
                .with_equalities(&equalities)
                .prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
        })
    }
}

fn referent(mut condition: &ExecutionCondition) -> &ExecutionCondition {
    while let ExecutionCondition::Borrowed(value) = condition {
        condition = value;
    }

    condition
}
