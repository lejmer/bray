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
    ) -> bool {
        if !matches!(
            self.semantics.selections().expression(expression),
            Some(SemanticSelection::Operation(SelectedOperation::Index {
                target: IndexTarget::ArrayElement
                    | IndexTarget::SliceElement
                    | IndexTarget::ArraySlice
                    | IndexTarget::Slice,
                ..
            }))
        ) {
            return false;
        }

        let Some(BoundExpression::Structured(index)) = self.request.view().expression(expression)
        else {
            panic!("checked built-in indexing retains its structured expression");
        };

        let mut ranges = super::predicate::IntegerRanges::new();

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

        let equalities = ExecutionCondition::equalities(&state.assumptions, None);

        let assumptions = state
            .assumptions
            .iter()
            .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
            .collect();

        let proves = |operator, left, right| {
            ExecutionCondition::operation(operator, vec![left, right])
                .with_equalities(&equalities)
                .prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
        };

        match index.kind() {
            BoundStructuredExpressionKind::ElementIndex => {
                let [_, selector] = index.operands() else {
                    panic!("checked element indexing has one selector");
                };

                self.index_value_in_bounds(
                    state,
                    receiver,
                    self.value(state, *selector).with_equalities(&equalities),
                    true,
                    &proves,
                    &mut { ExecutionCondition::WORK_LIMIT },
                )
            }
            BoundStructuredExpressionKind::SliceIndex => {
                let bounds = index
                    .slice_bounds()
                    .expect("checked slicing retains its bounds");

                let ordered = match (bounds.lower(), bounds.upper()) {
                    (Some(lower), Some(upper)) => proves(
                        BoundOperator::LessEqual,
                        self.value(state, lower),
                        self.value(state, upper),
                    ),
                    (Some(lower), None) => self.index_value_in_bounds(
                        state,
                        receiver,
                        self.value(state, lower),
                        false,
                        &proves,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    ),
                    // A missing lower bound is zero, and checked slice bounds have type usize.
                    (None, _) => true,
                };

                ordered
                    && bounds.upper().is_none_or(|upper| {
                        self.index_value_in_bounds(
                            state,
                            receiver,
                            self.value(state, upper),
                            false,
                            &proves,
                            &mut { ExecutionCondition::WORK_LIMIT },
                        )
                    })
            }
            _ => unreachable!("selected built-in indexing retains its checked source form"),
        }
    }

    fn evaluated_index_input(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        state
            .expressions
            .get(&expression)
            .or_else(|| self.literals.get(&expression))
            .cloned()
            .unwrap_or(ExecutionCondition::Unknown)
    }

    fn index_value_in_bounds(
        &self,
        state: &ExecutionState,
        receiver: BoundExpressionId,
        index: ExecutionCondition,
        strict: bool,
        proves: &impl Fn(BoundOperator, ExecutionCondition, ExecutionCondition) -> bool,
        budget: &mut usize,
    ) -> bool {
        let Some(remaining) = budget.checked_sub(1) else {
            return false;
        };

        *budget = remaining;

        let operator = if strict {
            BoundOperator::Less
        } else {
            BoundOperator::LessEqual
        };

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
            let length = if let Some(length) = values.constant_term_integer(*length) {
                let ty = crate::representation::representation_type(
                    self.request,
                    bray_compiler_known::RepresentationRole::ScalarUsize,
                );

                ExecutionCondition::Literal(std::sync::Arc::new(
                    bray_symbols::ConstantValueData::new(
                        ty,
                        bray_symbols::ConstantValueKind::Integer(length),
                    ),
                ))
            } else {
                crate::execution_guarantees::execution_condition_from_term(values, *length, &[])
                    .expect("checked array extent belongs to this semantic store")
            };

            if proves(operator, index.clone(), length) {
                return true;
            }
        }

        if let Some(BoundExpression::Structured(slice)) = self.request.view().expression(receiver)
            && slice.kind() == BoundStructuredExpressionKind::SliceIndex
        {
            let bounds = slice
                .slice_bounds()
                .expect("checked slice value retains its bounds");

            let index = bounds.lower().map_or(index.clone(), |lower| {
                let lower = self.evaluated_index_input(state, lower);

                if matches!(&index, ExecutionCondition::Literal(value)
                    if value.kind() == &bray_symbols::ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(0))) {
                    lower
                } else {
                    ExecutionCondition::operation(BoundOperator::Add, vec![lower, index.clone()])
                }
            });

            // On normal continuation the inner slice exists with exactly these bounds.
            return if let Some(upper) = bounds.upper() {
                proves(operator, index, self.evaluated_index_input(state, upper))
            } else {
                self.index_value_in_bounds(
                    state,
                    slice.operands()[0],
                    index,
                    strict,
                    proves,
                    budget,
                )
            };
        }

        let receiver = self.evaluated_index_input(state, receiver);

        if referent(&receiver) == &ExecutionCondition::Unknown {
            return false;
        }

        self.sequence_lengths.iter().any(|length| {
            let length = self.value(state, *length);

            let ExecutionCondition::Call(_, arguments) = &length else {
                return false;
            };

            let [observed] = arguments.as_ref() else {
                return false;
            };

            referent(&receiver) == referent(observed) && proves(operator, index.clone(), length)
        })
    }
}

fn referent(mut condition: &ExecutionCondition) -> &ExecutionCondition {
    while let ExecutionCondition::Borrowed(value) = condition {
        condition = value;
    }

    condition.owner_identity()
}
