use std::collections::BTreeMap;

use bray_bound_tree::BoundReferenceTarget;

use super::ExecutionCondition;

pub(super) fn comparison_is_implied(
    operator: bray_bound_tree::BoundOperator,
    operands: &[ExecutionCondition],
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
) -> Option<bool> {
    use bray_bound_tree::BoundOperator;

    let [left, right] = operands else {
        return None;
    };

    let (strict, reversed, allows_equal) = match operator {
        BoundOperator::Less => (BoundOperator::Less, BoundOperator::Greater, false),
        BoundOperator::LessEqual => (BoundOperator::Less, BoundOperator::Greater, true),
        BoundOperator::Greater => (BoundOperator::Greater, BoundOperator::Less, false),
        BoundOperator::GreaterEqual => (BoundOperator::Greater, BoundOperator::Less, true),
        _ => return None,
    };

    let known = |operator, left: &ExecutionCondition, right: &ExecutionCondition| {
        // Lookup keys retain the immutable operand terms without scanning assumptions.
        assumptions.contains(&(
            ExecutionCondition::operation(operator, vec![left.clone(), right.clone()]),
            true,
        ))
    };

    if known(strict, left, right)
        || known(reversed, right, left)
        || allows_equal
            && (known(BoundOperator::Equal, left, right)
                || known(BoundOperator::Equal, right, left))
    {
        return Some(true);
    }

    // Only positive ordering evidence is used. A false comparison can include NaN.
    if known(reversed, left, right) || known(strict, right, left) {
        return Some(false);
    }

    if !allows_equal {
        let weak = if reversed == BoundOperator::Less {
            BoundOperator::LessEqual
        } else {
            BoundOperator::GreaterEqual
        };

        let reversed_weak = if strict == BoundOperator::Less {
            BoundOperator::LessEqual
        } else {
            BoundOperator::GreaterEqual
        };

        if known(weak, left, right)
            || known(reversed_weak, right, left)
            || known(BoundOperator::Equal, left, right)
            || known(BoundOperator::Equal, right, left)
        {
            return Some(false);
        }
    }

    None
}

/// Checks whether entry assumptions establish a condition after matching callable inputs.
/// Unmapped inputs and exhausted reasoning remain unknown and cannot establish a guarantee.
pub fn execution_condition_is_implied(
    condition: &ExecutionCondition,
    assumptions: &[ExecutionCondition],
    inputs: &BTreeMap<BoundReferenceTarget, BoundReferenceTarget>,
) -> bool {
    let condition = remap_execution_condition_inputs(condition, inputs);
    let mut known = Default::default();

    for assumption in assumptions {
        // The proof set retains the immutable terms for the duration of this implication.
        assumption.clone().assume(true, &mut known);
    }

    condition.prove(&known, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
}

/// Matches callable input identities while preserving field paths and completion results.
/// Unmapped inputs cannot supply logical evidence.
pub fn remap_execution_condition_inputs(
    condition: &ExecutionCondition,
    inputs: &BTreeMap<BoundReferenceTarget, BoundReferenceTarget>,
) -> ExecutionCondition {
    condition.substitute(
        &|place| {
            let Some(root) = place
                .reference()
                .and_then(|reference| inputs.get(&reference))
            else {
                return ExecutionCondition::Unknown;
            };

            let mut mapped = super::ExecutionPlace::from(*root);

            for field in place.projections.iter() {
                mapped = mapped.component(*field);
            }

            ExecutionCondition::Input(mapped)
        },
        &ExecutionCondition::Result,
        &mut { ExecutionCondition::WORK_LIMIT },
    )
}

/// Substitutes the exact generic identities carried by normalized predicate applications.
pub fn map_execution_condition_substitutions<E>(
    condition: &ExecutionCondition,
    map: &impl Fn(bray_symbols::GenericSubstitutionId) -> Result<bray_symbols::GenericSubstitutionId, E>,
) -> Result<ExecutionCondition, E> {
    Ok(match condition {
        ExecutionCondition::Trusted(value) => ExecutionCondition::Trusted(std::sync::Arc::new(
            map_execution_condition_substitutions(value, map)?,
        )),
        ExecutionCondition::Borrowed(value) => {
            map_execution_condition_substitutions(value, map)?.borrowed()
        }
        ExecutionCondition::Entry {
            condition,
            captured,
        } => ExecutionCondition::Entry {
            condition: std::sync::Arc::new(map_execution_condition_substitutions(condition, map)?),
            captured: *captured,
        },
        ExecutionCondition::Predicate(predicate, substitution, operands) => {
            ExecutionCondition::predicate(
                *predicate,
                map(*substitution)?,
                operands
                    .iter()
                    .map(|operand| map_execution_condition_substitutions(operand, map))
                    .collect::<Result<_, _>>()?,
            )
        }
        ExecutionCondition::Operation(operator, operands) => ExecutionCondition::operation(
            *operator,
            operands
                .iter()
                .map(|operand| map_execution_condition_substitutions(operand, map))
                .collect::<Result<_, _>>()?,
        ),
        ExecutionCondition::Call(callable, operands) => ExecutionCondition::call(
            bray_symbols::CallableInstanceData::new(
                callable.definition(),
                map(callable.substitution())?,
            ),
            operands
                .iter()
                .map(|operand| map_execution_condition_substitutions(operand, map))
                .collect::<Result<_, _>>()?,
        ),
        ExecutionCondition::Projection(field, value) => {
            map_execution_condition_substitutions(value, map)?.component(*field)
        }
        ExecutionCondition::Constructed(expression, fields) => ExecutionCondition::Constructed(
            *expression,
            fields
                .iter()
                .map(|(field, value)| {
                    Ok((*field, map_execution_condition_substitutions(value, map)?))
                })
                .collect::<Result<Vec<_>, E>>()?
                .into(),
        ),
        _ => condition.clone(),
    })
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use super::{BoundReferenceTarget, ExecutionCondition, execution_condition_is_implied};

    #[test]
    fn mixed_boolean_proof_visits_each_clause_with_one_shared_budget() {
        let mut condition = ExecutionCondition::Boolean(true);
        let mut ordinary = Default::default();

        for index in 1..33 {
            let input = ExecutionCondition::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            );

            input.clone().assume(true, &mut ordinary);

            condition =
                ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![condition, input]);
        }

        let mut budget = 129;

        assert_eq!(
            condition.prove_mixed(&Default::default(), &ordinary, &mut budget),
            Some(true)
        );

        assert_eq!(budget, 0, "each clause consumes four shared proof steps");

        assert_eq!(
            condition.prove_mixed(&Default::default(), &ordinary, &mut 128),
            None
        );
    }

    #[test]
    fn comparison_implication_uses_positive_ordering_without_assuming_total_order() {
        let left = ExecutionCondition::Input(
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(1)).into(),
            )
            .into(),
        );

        let right = ExecutionCondition::Input(
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(2)).into(),
            )
            .into(),
        );

        let comparison =
            |operator| ExecutionCondition::operation(operator, vec![left.clone(), right.clone()]);

        let mut known = Default::default();

        comparison(BoundOperator::Greater).assume(true, &mut known);

        assert_eq!(
            comparison(BoundOperator::GreaterEqual).prove(&known, &mut 128),
            Some(true)
        );

        assert_eq!(
            comparison(BoundOperator::LessEqual).prove(&known, &mut 128),
            Some(false)
        );

        assert_eq!(
            super::comparison_is_implied(
                BoundOperator::LessEqual,
                &[right.clone(), left.clone()],
                &known
            ),
            Some(true)
        );

        for (provided, rejected) in [
            (BoundOperator::LessEqual, BoundOperator::Greater),
            (BoundOperator::GreaterEqual, BoundOperator::Less),
            (BoundOperator::Equal, BoundOperator::Greater),
            (BoundOperator::Equal, BoundOperator::Less),
        ] {
            let known = std::collections::BTreeSet::from([(comparison(provided), true)]);

            assert_eq!(comparison(rejected).prove(&known, &mut 128), Some(false));
        }

        let mut unordered = Default::default();

        comparison(BoundOperator::Greater).assume(false, &mut unordered);

        assert_eq!(
            comparison(BoundOperator::LessEqual).prove(&unordered, &mut 128),
            None
        );

        assert_eq!(
            comparison(BoundOperator::GreaterEqual).prove(&unordered, &mut 128),
            None
        );
    }

    #[test]
    fn implication_maps_inputs_and_preserves_unknowns() {
        let input = |index| {
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
            )
        };

        let source = ExecutionCondition::Input(input(1).into());
        let target = ExecutionCondition::Input(input(2).into());
        let mapping = [(input(1), input(2))].into();

        assert!(execution_condition_is_implied(&source, &[target], &mapping));
        assert!(!execution_condition_is_implied(&source, &[], &mapping));

        assert!(!execution_condition_is_implied(
            &ExecutionCondition::Unknown,
            &[ExecutionCondition::Unknown],
            &mapping,
        ));

        assert!(!execution_condition_is_implied(
            &source,
            &[source.clone()],
            &Default::default(),
        ));
    }

    #[test]
    fn conjunction_can_establish_a_weaker_entry_domain() {
        let input = BoundReferenceTarget::Surface(
            CallableParameterSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        );

        let condition = ExecutionCondition::Input(input.into());

        let conjunction = ExecutionCondition::operation(
            BoundOperator::LogicalAnd,
            vec![condition.clone(), ExecutionCondition::Boolean(true)],
        );

        assert!(execution_condition_is_implied(
            &condition,
            &[conjunction],
            &[(input, input)].into(),
        ));
    }

    #[test]
    fn equality_complements_do_not_assume_ordered_operands() {
        let input = |index| {
            ExecutionCondition::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            )
        };

        for (operator, complement) in [
            (BoundOperator::Equal, BoundOperator::NotEqual),
            (BoundOperator::NotEqual, BoundOperator::Equal),
        ] {
            let condition = ExecutionCondition::operation(operator, vec![input(1), input(2)]);
            let complement = ExecutionCondition::operation(complement, vec![input(1), input(2)]);
            let mut assumptions = Default::default();

            condition.clone().assume(false, &mut assumptions);

            assert_eq!(
                complement.prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
                Some(true)
            );

            assert_eq!(
                condition.prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
                Some(false)
            );
        }

        // Runtime operands may be unordered floating-point values.
        let mut assumptions = Default::default();

        ExecutionCondition::operation(BoundOperator::GreaterEqual, vec![input(1), input(2)])
            .assume(false, &mut assumptions);

        assert_eq!(
            ExecutionCondition::operation(BoundOperator::Less, vec![input(1), input(2)])
                .prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }

    #[test]
    fn failed_inequality_retains_scalar_equality_for_contract_matching() {
        let input = |index| {
            ExecutionCondition::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            )
        };

        let mut assumptions = Default::default();

        ExecutionCondition::operation(BoundOperator::NotEqual, vec![input(1), input(2)])
            .assume(false, &mut assumptions);

        let equalities = ExecutionCondition::equalities(&assumptions, None);

        assert_eq!(input(2).with_equalities(&equalities), input(1));
    }
}
