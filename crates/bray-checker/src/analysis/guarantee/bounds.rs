use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind, IndexTarget,
    SelectedOperation, SemanticSelection,
};

use super::flow::ExecutionFlowDomain;
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn index_in_bounds(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<bool> {
        matches!(
            self.semantics.selections().expression(expression),
            Some(SemanticSelection::Operation(SelectedOperation::Index {
                target: IndexTarget::ArrayElement
                    | IndexTarget::SliceElement
                    | IndexTarget::ArraySlice
                    | IndexTarget::Slice,
                ..
            }))
        ).then(|| self.prove_index_bounds(state, expression))
    }

    fn prove_index_bounds(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> bool {
        let Some(BoundExpression::Structured(index)) = self.request.view().expression(expression)
        else {
            panic!("checked built-in indexing retains its structured expression");
        };

        let mut ranges = super::integer::IntegerRanges::new();

        if index.operands().iter().skip(1).any(|operand| {
            self.integer_range(state, *operand, &mut ranges, &mut {
                ExecutionCondition::WORK_LIMIT
            })
            .is_none_or(|(lower, _)| lower < num_bigint::BigInt::from(0))
        }) {
            return false;
        }

        let receiver = *index
            .operands()
            .first()
            .expect("checked indexing has a receiver");

        let extent = || self.sequence_extent(state, receiver);

        let equalities = state.proofs.equalities(&state.assumptions, None);

        let assumptions = state
            .assumptions
            .iter()
            .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
            .collect();

        let mut proves = |operator, left, right| {
            let condition = ExecutionCondition::operation(operator, vec![left, right]);
            let normalized = condition.with_equalities(&equalities);

            normalized.prove_ordinary(&assumptions, Some(&state.proofs)) == Some(true)
                || self.integer_condition_truth(state, &normalized, &mut ranges) == Some(true)
                || normalized != condition
                    && self.integer_condition_truth(state, &condition, &mut ranges) == Some(true)
        };

        match index.kind() {
            BoundStructuredExpressionKind::ElementIndex => {
                let [_, selector] = index.operands() else {
                    panic!("checked element indexing has one selector");
                };

                extent().is_some_and(|extent| {
                    proves(BoundOperator::Less, self.operand_value(state, *selector), extent)
                })
            }
            BoundStructuredExpressionKind::SliceIndex => {
                let bounds = index
                    .slice_bounds()
                    .expect("checked slicing retains its bounds");

                let ordered = match (bounds.lower(), bounds.upper()) {
                    (Some(lower), Some(upper)) => proves(
                        BoundOperator::LessEqual,
                        self.operand_value(state, lower),
                        self.operand_value(state, upper),
                    ),
                    (Some(lower), None) => extent().is_some_and(|extent| {
                        proves(BoundOperator::LessEqual, self.operand_value(state, lower), extent)
                    }),
                    // A missing lower bound is zero, and checked slice bounds have type usize.
                    (None, _) => true,
                };

                ordered
                    && bounds.upper().is_none_or(|upper| {
                        extent().is_some_and(|extent| {
                            proves(BoundOperator::LessEqual, self.operand_value(state, upper), extent)
                        })
                    })
            }
            _ => unreachable!("selected built-in indexing retains its checked source form"),
        }
    }

    /// Publishes immutable extent while the normal slice still owns its evaluated operands.
    pub(super) fn publish_slice_extent(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        if !matches!(self.semantics.selections().expression(expression),
            Some(SemanticSelection::Operation(SelectedOperation::Index {
                target: IndexTarget::ArraySlice | IndexTarget::Slice, ..
            }))) {
            return;
        }

        let Some(BoundExpression::Structured(slice)) = self.request.view().expression(expression) else {
            unreachable!("checked built-in slicing retains its operands");
        };

        let bounds = slice.slice_bounds().expect("checked slicing retains its bounds");
        let receiver = slice.operands()[0];

        let upper = match bounds.upper() {
            Some(upper) => self.operand_value(state, upper),
            None => self.sequence_extent(state, receiver).unwrap_or_else(ExecutionCondition::unknown),
        };

        let extent = if let Some(lower) = bounds.lower() {
            ExecutionCondition::operation(BoundOperator::Subtract, vec![upper, self.operand_value(state, lower)])
        } else {
            upper
        };

        if let Some(length) = self.slice_length(&self.operand_value(state, expression)) {

            ExecutionCondition::value_alias(length, extent).assume(true, &mut state.assumptions);
        }
    }

    fn sequence_extent(&self, state: &ExecutionState, receiver: BoundExpressionId) -> Option<ExecutionCondition> {
        let ty = self
            .semantics
            .types()
            .expression(receiver)
            .expect("checked indexing receiver retains its type")
            .ty();

        let values = self.request.semantic_values();

        if let bray_symbols::TypeData::Array { length, .. } =
            values.type_data(values.unborrowed_type(ty)).as_ref()
        {
            return Some(if let Some(length) = values.constant_term_integer(*length) {
                let ty = crate::representation::representation_type(
                    self.request,
                    bray_compiler_known::RepresentationRole::ScalarUsize,
                );

                ExecutionCondition::new(crate::ExecutionConditionKind::Literal(std::sync::Arc::new(
                    bray_symbols::ConstantValueData::new(
                        ty,
                        bray_symbols::ConstantValueKind::Integer(length),
                    ),
                )))
            } else {
                crate::execution_guarantees::execution_condition_from_term(values, *length, &[])
                    .expect("checked array extent belongs to this semantic store")
            });
        }

        self.slice_length(&self.operand_value(state, receiver))
    }

    fn slice_length(&self, value: &ExecutionCondition) -> Option<ExecutionCondition> {
        // Indexing and the selected sequence call observe the same borrow descriptor.
        let view = value.clone().argument_value(true);

        let length = ExecutionCondition::call(self.sequence_length?, None, [],
            vec![view.descriptor(crate::execution_guarantees::ExecutionDescriptorKind::Reference)], None);

        (!length.is_unknown()).then_some(length)
    }
}
