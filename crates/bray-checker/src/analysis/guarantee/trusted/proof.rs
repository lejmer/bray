use crate::execution_guarantees::ExecutionAssumptions;
use std::collections::BTreeSet;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn empty_memory_copy(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> bool {
        use bray_bound_tree::{
            AnyBoundNodeId, BoundExecutionSite, SelectedArgument, SemanticSelection,
        };

        use bray_compiler_known::ImplementationHook;

        let BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)) = invocation else {
            return false;
        };

        let Some(SemanticSelection::Call(call)) =
            self.semantics.selections().expression(expression)
        else {
            return false;
        };

        if !matches!(
            call.implementation_hook(),
            Some(ImplementationHook::MemoryCopy | ImplementationHook::MemoryCopyOverlapping)
        ) {
            return false;
        }

        let Some(count) = call.arguments().iter().find_map(|argument| match argument {
            SelectedArgument::Explicit {
                expression,
                ordinal: 2,
                ..
            } => Some(*expression),
            _ => None,
        }) else {
            return false;
        };

        let Some(entry) = state.entries.get(&invocation) else {
            return false;
        };

        let equalities = self.proofs.equalities(&entry.assumptions, None);
        let count = self.value(state, count).with_equalities(&equalities);

        matches!(count, ExecutionCondition::Literal(value)
            if matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(integer) if integer.to_u64() == Some(0)))
    }

    pub(in crate::analysis::guarantee) fn allocation_subjects(
        &self,
        conditions: &[ExecutionCondition],
    ) -> Vec<(ExecutionCondition, crate::ExecutionPlace)> {
        let mut pending = conditions.iter().collect::<Vec<_>>();
        let mut subjects = Vec::new();

        while let Some(condition) = pending.pop() {
            match condition {
                ExecutionCondition::Trusted(condition) => pending.push(condition),
                ExecutionCondition::Operation(_, operands) => pending.extend(operands.iter()),
                ExecutionCondition::Predicate(predicate, _, arguments)
                    if Some(*predicate) == self.owned_allocation =>
                {
                    for input in arguments[0].inputs() {
                        if !input.projections.is_empty() {
                            let mut input = input.clone();

                            input.projections = std::sync::Arc::new([]);
                            subjects.push((condition.clone(), input));
                        }
                    }
                }
                _ => {}
            }
        }

        subjects
    }

    pub(in crate::analysis::guarantee) fn call_trusted_assumptions(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        arguments: &std::collections::BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        transferred: &BTreeSet<crate::ExecutionPlace>,
    ) -> ExecutionAssumptions {
        if state.allocation_owners.is_empty() {
            return state.trusted_assumptions.clone();
        }

        let subjects = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
            .map(|contract| self.allocation_subjects(&contract.requirements))
            .unwrap_or_default();

        state.call_trusted_assumptions(subjects.into_iter().filter_map(|(_, subject)| {
            transferred
                .contains(&subject)
                .then(|| subject.value_in(arguments))
                .flatten()
                .filter(|value| *value != ExecutionCondition::Unknown)
        }))
    }

    pub(in crate::analysis::guarantee) fn prove_trusted_requirements(
        &self,
        entry: &crate::ExecutionCallEvidence,
        requirements: &[ExecutionCondition],
    ) -> Result<(), crate::UnprovenExecutionCondition> {
        if requirements.is_empty() {
            return Ok(());
        }

        let equalities = self.proofs.equalities(&entry.assumptions, None);

        let normalize = |conditions: &BTreeSet<(ExecutionCondition, bool)>| {
            entry
                .proof_facts(conditions)
                .map(|(condition, value)| {
                    (
                        condition.with_equalities(&equalities).with_reference_origins(
                            self.request.semantic_values(), self.dependencies, &entry.assumptions)
                            .pointer_observations(&self.pointer_predicates),
                        value,
                    )
                })
                .collect::<BTreeSet<_>>()
        };

        let ordinary = normalize(&entry.assumptions);

        let known = crate::execution_guarantees::trusted_condition_closure(
            normalize(&entry.trusted_assumptions),
            &ordinary,
        );

        let mut ranges = None;

        let instantiated = requirements
            .iter()
            .map(|requirement| {
                let condition = requirement
                    .substitute(
                        &|place| {
                            place
                                .value_in(&entry.arguments)
                                .unwrap_or(ExecutionCondition::Unknown)
                        },
                        &ExecutionCondition::Unknown,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    )
                    .with_equalities(&equalities);

                condition.with_reference_origins(self.request.semantic_values(), self.dependencies, &entry.assumptions)
                    .pointer_observations(&self.pointer_predicates)
            })
            .collect::<Vec<_>>();

        let proof = crate::execution_guarantees::ExecutionProof::new(&known, &ordinary);

        let mut prove_condition = |condition: &ExecutionCondition| {
            if proof.prove_mixed(condition, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
            {
                return true;
            }

            // Unsigned representations establish their zero lower bound even when
            // caller evidence does not otherwise constrain the current value.
            if let ExecutionCondition::Operation(
                bray_bound_tree::BoundOperator::LessEqual,
                operands,
            ) = condition
                && let [ExecutionCondition::Literal(lower), value] = operands.as_ref()
                && matches!(lower.kind(), bray_symbols::ConstantValueKind::Integer(integer) if integer.to_u64() == Some(0))
                && self.unsigned_value(value)
            {
                return true;
            }

            let ExecutionCondition::Trusted(condition) = condition else {
                return false;
            };

            let ExecutionCondition::Predicate(predicate, substitution, arguments) =
                condition.as_ref()
            else {
                return false;
            };

            let [pointer, count] = arguments.as_ref() else {
                return false;
            };

            if !self.pointer_predicates.contains_key(predicate) {
                return false;
            }

            let (ranges, lower_bounds) = ranges.get_or_insert_with(|| {
                let mut ranges = std::collections::BTreeMap::<_, Vec<_>>::new();

                for (condition, value) in known.iter().take(ExecutionCondition::WORK_LIMIT) {
                    if !value {
                        continue;
                    }

                    let ExecutionCondition::Trusted(condition) = condition else {
                        continue;
                    };

                    let ExecutionCondition::Predicate(predicate, substitution, arguments) =
                        condition.as_ref()
                    else {
                        continue;
                    };

                    if let [pointer, count] = arguments.as_ref() {
                        ranges
                            .entry((predicate, substitution, pointer))
                            .or_default()
                            .push(count);
                    }
                }

                (ranges, constant_extent_lower_bounds(&ordinary))
            });

            ranges
                .get(&(predicate, substitution, pointer))
                .is_some_and(|extents| {
                    extents.iter().any(|extent| {
                        range_count_follows(count, extent, &ordinary, lower_bounds, &|value| {
                            self.unsigned_value(value)
                        })
                    })
                })
        };

        for (required, actual) in requirements.iter().zip(instantiated) {
            if let Err(failed) = prove_requirement(&actual, &known, &mut prove_condition, &mut {
                ExecutionCondition::WORK_LIMIT
            }) {
                return Err(crate::UnprovenExecutionCondition::new(required.clone(), failed.clone()));
            }
        }

        Ok(())
    }

}

// Share one lazy range index across every Boolean clause in the caller's contract.
fn prove_requirement<'a>(
    condition: &'a ExecutionCondition,
    known: &BTreeSet<(ExecutionCondition, bool)>,
    prove_condition: &mut impl FnMut(&ExecutionCondition) -> bool,
    budget: &mut usize,
) -> Result<(), &'a ExecutionCondition> {
    let Some(remaining) = budget.checked_sub(1) else {
        return Err(condition);
    };

    *budget = remaining;

    // Preserve conditional evidence as a whole without proving each subtree again.
    if known.contains(&(condition.clone(), true)) {
        return Ok(());
    }

    match condition {
        ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalAnd, operands) => {
            for operand in operands.iter() {
                prove_requirement(operand, known, prove_condition, budget)?;
            }

            Ok(())
        }
        ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, operands) => {
            if prove_condition(condition)
                || operands.iter().any(|operand| prove_requirement(operand, known, prove_condition, budget).is_ok()) {
                Ok(())
            } else { Err(condition) }
        }
        condition => prove_condition(condition).then_some(()).ok_or(condition),
    }
}

#[cfg(test)]
mod tests {
    use super::prove_requirement;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::BoundOperator;
    use bray_symbols::SymbolOrdinal;
    use std::collections::BTreeSet;

    #[test]
    fn disjunction_requirements_use_whole_clause_proof_before_selecting_alternatives() {
        let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let promised = ExecutionCondition::Trusted(std::sync::Arc::new(input(2)));
        let extra = ExecutionCondition::Trusted(std::sync::Arc::new(input(1)));
        let established = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(0), promised]);
        let required = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![established.clone(), extra]);
        let known = BTreeSet::from([(established, true)]);

        let mut prove = |condition: &ExecutionCondition| condition.prove_mixed(&known, &BTreeSet::new(),
            &mut { ExecutionCondition::WORK_LIMIT }) == Some(true);

        assert_eq!(prove_requirement(&required, &known, &mut prove,
            &mut { ExecutionCondition::WORK_LIMIT }), Ok(()));
    }

    #[test]
    fn failed_conjunction_reports_the_unproven_leaf() {
        let first = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let second = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let known = BTreeSet::from([(first.clone(), true)]);

        let condition = ExecutionCondition::operation(BoundOperator::LogicalAnd,
            vec![first, second.clone()]);

        assert_eq!(prove_requirement(&condition, &known, &mut |_| false,
            &mut { ExecutionCondition::WORK_LIMIT }), Err(&second));
    }

    #[test]
    fn failed_disjunction_retains_all_alternatives() {
        let first = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let second = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let condition = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![first, second]);

        assert_eq!(prove_requirement(&condition, &BTreeSet::new(), &mut |_| false,
            &mut { ExecutionCondition::WORK_LIMIT }), Err(&condition));
    }
}

fn range_count_follows(
    count: &ExecutionCondition,
    extent: &ExecutionCondition,
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
    lower_bounds: &std::collections::BTreeMap<&ExecutionCondition, u64>,
    unsigned: &impl Fn(&ExecutionCondition) -> bool,
) -> bool {
    use bray_bound_tree::BoundOperator;

    let bound = ExecutionCondition::operation(
        BoundOperator::LessEqual,
        vec![count.clone(), extent.clone()],
    );

    if bound.prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
        || super::super::integer::unsigned_arithmetic_is_bounded(&bound, assumptions, unsigned)
    {
        return true;
    }

    for (forward, reverse, holds) in [
        (BoundOperator::Less, BoundOperator::Greater, true),
        (BoundOperator::LessEqual, BoundOperator::GreaterEqual, true),
        (BoundOperator::Greater, BoundOperator::Less, false),
        (BoundOperator::GreaterEqual, BoundOperator::LessEqual, false),
    ] {
        if assumptions.contains(&(
            ExecutionCondition::operation(forward, vec![count.clone(), extent.clone()]),
            holds,
        )) || assumptions.contains(&(
            ExecutionCondition::operation(reverse, vec![extent.clone(), count.clone()]),
            holds,
        )) {
            return true;
        }
    }

    if let ExecutionCondition::Operation(BoundOperator::Add, operands) = count
        && let [
            ExecutionCondition::Literal(left),
            ExecutionCondition::Literal(right),
        ] = operands.as_ref()
        && let Ok(value) = crate::constant::fold_binary(
            BoundOperator::Add,
            left.kind(),
            right.kind(),
            crate::ConstantEvaluationLimits::default().integer_bits(),
        )
    {
        let count = ExecutionCondition::Literal(std::sync::Arc::new(
            bray_symbols::ConstantValueData::new(left.ty(), value),
        ));

        return range_count_follows(&count, extent, assumptions, lower_bounds, unsigned);
    }

    if let ExecutionCondition::Operation(BoundOperator::Add, operands) = count
        && let [index, ExecutionCondition::Literal(step)] = operands.as_ref()
        && matches!(step.kind(), bray_symbols::ConstantValueKind::Integer(integer) if integer.to_u64() == Some(1))
    {
        return assumptions.contains(&(
            ExecutionCondition::operation(BoundOperator::Less, vec![index.clone(), extent.clone()]),
            true,
        )) || assumptions.contains(&(
            ExecutionCondition::operation(
                BoundOperator::GreaterEqual,
                vec![index.clone(), extent.clone()],
            ),
            false,
        ));
    }

    let ExecutionCondition::Literal(value) = count else {
        return false;
    };

    let bray_symbols::ConstantValueKind::Integer(integer) = value.kind() else {
        return false;
    };

    let Some(count) = integer.to_u64() else {
        return false;
    };

    if lower_bounds
        .get(extent)
        .is_some_and(|lower| count <= *lower)
    {
        return true;
    }

    if count == 1
        && let ExecutionCondition::Operation(BoundOperator::Subtract, operands) = extent
        && let [end, start] = operands.as_ref()
    {
        return ExecutionCondition::operation(
            BoundOperator::Less,
            vec![start.clone(), end.clone()],
        )
        .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            == Some(true)
            || assumptions.contains(&(
                ExecutionCondition::operation(
                    BoundOperator::GreaterEqual,
                    vec![start.clone(), end.clone()],
                ),
                false,
            ));
    }

    false
}

// Keep the strongest constant bound per extent rather than rescanning conditions for each requirement.
fn constant_extent_lower_bounds(
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
) -> std::collections::BTreeMap<&ExecutionCondition, u64> {
    use bray_bound_tree::BoundOperator;

    let mut bounds = std::collections::BTreeMap::<_, u64>::new();

    for (condition, holds) in assumptions {
        let ExecutionCondition::Operation(operator, operands) = condition else {
            continue;
        };

        let [left, right] = operands.as_ref() else {
            continue;
        };

        let (lower, extent, strict) = match (*operator, *holds) {
            (BoundOperator::Less, true) | (BoundOperator::GreaterEqual, false) => {
                (left, right, true)
            }
            (BoundOperator::LessEqual, true) | (BoundOperator::Greater, false) => {
                (left, right, false)
            }
            (BoundOperator::Greater, true) | (BoundOperator::LessEqual, false) => {
                (right, left, true)
            }
            (BoundOperator::GreaterEqual, true) | (BoundOperator::Less, false) => {
                (right, left, false)
            }
            _ => continue,
        };

        let ExecutionCondition::Literal(value) = lower else {
            continue;
        };

        let bray_symbols::ConstantValueKind::Integer(integer) = value.kind() else {
            continue;
        };

        let Some(lower) = integer
            .to_u64()
            .and_then(|lower| lower.checked_add(u64::from(strict)))
        else {
            continue;
        };

        bounds
            .entry(extent)
            .and_modify(|prior| *prior = (*prior).max(lower))
            .or_insert(lower);
    }

    bounds
}
