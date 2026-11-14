use std::collections::BTreeMap;

use bray_bound_tree::BoundReferenceTarget;

use super::ExecutionCondition;

pub(super) fn comparison_is_implied(
    operator: bray_bound_tree::BoundOperator,
    operands: &[ExecutionCondition],
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    budget: &mut usize,
    memo: Option<&super::condition::ProofMemo>,
) -> Option<bool> {
    use bray_bound_tree::BoundOperator;

    if !matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual
        | BoundOperator::Less | BoundOperator::LessEqual | BoundOperator::Greater | BoundOperator::GreaterEqual) {
        return None;
    }

    let [left, right] = operands else {
        return None;
    };

    let local = std::cell::OnceCell::new();

    let ordered = || memo.map_or_else(
        || local.get_or_init(|| super::ordered_facts::OrderedFacts::new(assumptions)),
        |memo| memo.ordered(assumptions),
    );

    if left == right && ordered().positively_ordered(left, budget) {
        return match operator {
            BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual => {
                Some(true)
            }
            BoundOperator::NotEqual | BoundOperator::Less | BoundOperator::Greater => Some(false),
            _ => None,
        };
    }

    let known = |operator, left: &ExecutionCondition, right: &ExecutionCondition| {
        // Lookup keys retain the immutable operand terms without scanning assumptions.
        assumptions.contains(&(
            ExecutionCondition::operation(operator, vec![left.clone(), right.clone()]),
            true,
        ))
    };

    if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual) {
        let strictly_ordered = [BoundOperator::Less, BoundOperator::Greater]
            .into_iter()
            .any(|operator| known(operator, left, right) || known(operator, right, left));

        return (strictly_ordered
            || ordered().path(left, right, true, budget)
            || ordered().path(right, left, true, budget))
        .then_some(operator == BoundOperator::NotEqual);
    }

    let (strict, reversed, allows_equal) = match operator {
        BoundOperator::Less => (BoundOperator::Less, BoundOperator::Greater, false),
        BoundOperator::LessEqual => (BoundOperator::Less, BoundOperator::Greater, true),
        BoundOperator::Greater => (BoundOperator::Greater, BoundOperator::Less, false),
        BoundOperator::GreaterEqual => (BoundOperator::Greater, BoundOperator::Less, true),
        _ => return None,
    };

    let weak = if strict == BoundOperator::Less {
        BoundOperator::LessEqual
    } else {
        BoundOperator::GreaterEqual
    };

    let reversed_weak = if reversed == BoundOperator::Less {
        BoundOperator::LessEqual
    } else {
        BoundOperator::GreaterEqual
    };

    if known(strict, left, right) || known(reversed, right, left) {
        return Some(true);
    }

    let unequal = known(BoundOperator::NotEqual, left, right)
        || known(BoundOperator::NotEqual, right, left)
        || assumptions.contains(&(
            ExecutionCondition::operation(BoundOperator::Equal, vec![left.clone(), right.clone()]),
            false,
        ))
        || assumptions.contains(&(
            ExecutionCondition::operation(BoundOperator::Equal, vec![right.clone(), left.clone()]),
            false,
        ));

    if unequal && (known(weak, left, right) || known(reversed_weak, right, left))
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

    let (lower, upper) = if strict == BoundOperator::Less {
        (left, right)
    } else {
        (right, left)
    };

    if ordered().path(lower, upper, !allows_equal && !unequal, budget) {
        return Some(true);
    }

    ordered().path(upper, lower, allows_equal, budget).then_some(false)
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
                return ExecutionCondition::unknown();
            };

            let mut mapped = super::ExecutionPlace::from(*root);

            for field in place.projections.iter() {
                mapped = mapped.component(*field);
            }

            ExecutionCondition::new(crate::ExecutionConditionKind::Input(mapped))
        },
        &ExecutionCondition::result(),
        &mut { ExecutionCondition::WORK_LIMIT },
    )
}

/// Maps the checked references, typed literals and inputs in a normalized condition.
pub fn map_execution_condition_references<E>(
    condition: &ExecutionCondition,
    map: &impl Fn(bray_symbols::GenericSubstitutionId) -> Result<bray_symbols::GenericSubstitutionId, E>,
    input: &impl Fn(&super::ExecutionPlace) -> Result<ExecutionCondition, E>,
    literal: &impl Fn(
        &std::sync::Arc<bray_symbols::ConstantValueData>,
    ) -> Result<std::sync::Arc<bray_symbols::ConstantValueData>, E>,
    call: &mut impl FnMut(
        bray_symbols::CallableInstanceData,
        Option<bray_symbols::ImplementationInstanceData>,
        Vec<bray_symbols::ImplementationInstanceData>,
        Vec<ExecutionCondition>,
        Option<bray_symbols::TypeId>,
    ) -> Result<ExecutionCondition, E>,
) -> Result<ExecutionCondition, E> {
    Ok(match condition.kind() {
        crate::ExecutionConditionKind::Discriminant(value) => map_execution_condition_references(value, map, input, literal, call)?.discriminant(),
        crate::ExecutionConditionKind::Descriptor(kind, value) => map_execution_condition_references(value, map, input, literal, call)?.descriptor(*kind),
        crate::ExecutionConditionKind::ValueAlias(left, right) => ExecutionCondition::value_alias(
            map_execution_condition_references(left, map, input, literal, call)?,
            map_execution_condition_references(right, map, input, literal, call)?,
        ),
        crate::ExecutionConditionKind::Input(place) => input(place)?,
        crate::ExecutionConditionKind::Literal(value) => ExecutionCondition::new(crate::ExecutionConditionKind::Literal(literal(value)?)),
        crate::ExecutionConditionKind::Trusted(value) => ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(
            map_execution_condition_references(value, map, input, literal, call)?,
        ))),
        crate::ExecutionConditionKind::VariantTest(variant, value) => {
            map_execution_condition_references(value, map, input, literal, call)?
                .matches_variant(*variant)
        }
        crate::ExecutionConditionKind::Tagged(variant, value) => ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
            *variant,
            std::sync::Arc::new(map_execution_condition_references(
                value, map, input, literal, call,
            )?),
        )),
        crate::ExecutionConditionKind::Present(value) => {
            ExecutionCondition::new(crate::ExecutionConditionKind::Present(std::sync::Arc::new(map_execution_condition_references(
                value, map, input, literal, call,
            )?)))
        }
        crate::ExecutionConditionKind::Borrowed(value) => {
            map_execution_condition_references(value, map, input, literal, call)?.borrowed()
        }
        crate::ExecutionConditionKind::Entry {
            condition,
            captured,
        } => ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
            condition: std::sync::Arc::new(map_execution_condition_references(
                condition, map, input, literal, call,
            )?),
            captured: *captured,
        }),
        crate::ExecutionConditionKind::Predicate(predicate, substitution, operands) => {
            ExecutionCondition::predicate(
                *predicate,
                map(*substitution)?,
                operands
                    .iter()
                    .map(|operand| {
                        map_execution_condition_references(operand, map, input, literal, call)
                    })
                    .collect::<Result<_, _>>()?,
            )
        }
        crate::ExecutionConditionKind::Operation(operator, operands) => ExecutionCondition::operation(
            *operator,
            operands
                .iter()
                .map(|operand| {
                    map_execution_condition_references(operand, map, input, literal, call)
                })
                .collect::<Result<_, _>>()?,
        ),
        crate::ExecutionConditionKind::Call(callable, implementation, witnesses, operands, reference_type) => {
            let callable = bray_symbols::CallableInstanceData::new(
                callable.definition(),
                map(callable.substitution())?,
            );

            let implementation = implementation
                .map(|witness| {
                    Ok::<_, E>(bray_symbols::ImplementationInstanceData::new(
                        witness.definition(),
                        map(witness.substitution())?,
                    ))
                })
                .transpose()?;

            let witnesses = witnesses
                .iter()
                .map(|witness| {
                    Ok(bray_symbols::ImplementationInstanceData::new(
                        witness.definition(),
                        map(witness.substitution())?,
                    ))
                })
                .collect::<Result<Vec<_>, E>>()?;

            let operands = operands
                .iter()
                .map(|operand| {
                    map_execution_condition_references(operand, map, input, literal, call)
                })
                .collect::<Result<Vec<_>, E>>()?;

            call(callable, implementation, witnesses, operands, *reference_type)?
        }
        crate::ExecutionConditionKind::CapturedInput {
            owner,
            invocation,
            ordinal,
        } => ExecutionCondition::new(crate::ExecutionConditionKind::CapturedInput {
            owner: std::sync::Arc::new(map_execution_condition_references(
                owner, map, input, literal, call,
            )?),
            invocation: *invocation,
            ordinal: *ordinal,
        }),
        crate::ExecutionConditionKind::Projection(field, value) => {
            map_execution_condition_references(value, map, input, literal, call)?.component(*field)
        }
        crate::ExecutionConditionKind::Observation { owner, contents, kind } => ExecutionCondition::observation(
            map_execution_condition_references(owner, map, input, literal, call)?,
            map_execution_condition_references(contents, map, input, literal, call)?,
            *kind,
        ),
        crate::ExecutionConditionKind::Constructed(base, fields) => ExecutionCondition::constructed(map_execution_condition_references(
                base, map, input, literal, call,
            )?,
            fields
                .iter()
                .map(|(field, value)| {
                    Ok((
                        *field,
                        map_execution_condition_references(value, map, input, literal, call)?,
                    ))
                })
                .collect::<Result<Vec<_>, E>>()?
                .into(),
        ),
        _ => condition.clone(),
    })
}

/// Applies an already checked generic application to substitutions and generic constant inputs.
pub fn substitute_execution_condition(
    values: &bray_symbols::SemanticValueStore,
    condition: &ExecutionCondition,
    substitution: bray_symbols::GenericSubstitutionId,
) -> Result<ExecutionCondition, bray_symbols::SemanticValueStoreError> {
    map_execution_condition_references(
        condition,
        &|nested| values.substitute_generic_substitution(nested, substitution),
        &|place| {
            if let Some(term) = place.constant_term() {
                let term = values.substitute_constant_term(term, substitution)?;
                let mut value = crate::execution_condition_from_type_term(values, term, 0)?;

                for projection in &*place.projections {
                    value = value.component(*projection);
                }

                return Ok(value);
            }

            let Some(bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::AnySymbolId::GenericConstParameter(parameter),
            )) = place.reference()
            else {
                // The instantiated condition retains the same immutable runtime input path.
                return Ok(ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())));
            };

            let term = values
                .intern_constant_term(bray_symbols::ConstantTermData::Parameter(parameter))?;

            let term = values.substitute_constant_term(term, substitution)?;
            let mut value = crate::execution_condition_from_type_term(values, term, 0)?;

            for projection in &*place.projections {
                value = value.component(*projection);
            }

            Ok(value)
        },
        &|value| {
            // Aggregate payloads are shared. Reuse the store's recursive value substitution.
            let value = values.intern_constant_value((**value).clone())?;
            let value = values.substitute_constant_value(value, substitution)?;

            Ok(values.constant_value_data(value))
        },
        &mut |callable, implementation, witnesses, operands, reference_type| {
            Ok(ExecutionCondition::call(
                callable,
                implementation,
                witnesses,
                operands,
                reference_type.map(|ty| values.substitute_type(ty, substitution)).transpose()?,
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use super::{BoundReferenceTarget, ExecutionCondition, execution_condition_is_implied};

    #[test]
    fn mixed_boolean_proof_visits_each_clause_with_one_shared_budget() {
        let mut condition = ExecutionCondition::boolean(true);
        let mut ordinary = Default::default();

        for index in 1..33 {
            let input = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            ));

            input.clone().assume(true, &mut ordinary);

            condition =
                ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![condition, input]);
        }

        // Constant identity normalization leaves 32 leaves and 31 conjunctions.
        let mut budget = 63;

        assert_eq!(
            condition.prove_mixed(&Default::default(), &ordinary, &mut budget),
            Some(true)
        );

        assert_eq!(
            budget, 0,
            "each normalized Boolean node consumes one shared proof step"
        );

        assert_eq!(
            condition.prove_mixed(&Default::default(), &ordinary, &mut 62),
            None
        );

        let empty = Default::default();
        let proof = crate::execution_guarantees::ExecutionProof::new(&empty, &ordinary);

        for _ in 0..2 {
            let mut sufficient = 63;
            let mut insufficient = 62;

            assert_eq!(proof.prove_mixed(&condition, &mut sufficient), Some(true));
            assert_eq!(sufficient, 0);
            assert_eq!(proof.prove_mixed(&condition, &mut insufficient), None);
            assert_eq!(insufficient, 0, "cached proof must replay its exact work budget");
        }
    }

    #[test]
    fn comparison_implication_uses_positive_ordering_without_assuming_total_order() {
        let left = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(1)).into(),
            )
            .into(),
        ));

        let right = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(2)).into(),
            )
            .into(),
        ));

        let comparison =
            |operator| ExecutionCondition::operation(operator, vec![left.clone(), right.clone()]);

        let mut known = Default::default();

        comparison(BoundOperator::Greater).assume(true, &mut known);

        let mut budget = 128;

        assert_eq!(super::comparison_is_implied(BoundOperator::LogicalOr,
            &[left.clone(), left.clone()], &known, &mut budget, None), None);

        assert_eq!(budget, 128, "Boolean nodes must not spend comparison proof work");

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
                &known,
                &mut 128,
                None,
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

        for (operator, valid) in [
            (BoundOperator::Equal, true),
            (BoundOperator::LessEqual, true),
            (BoundOperator::GreaterEqual, true),
            (BoundOperator::NotEqual, false),
            (BoundOperator::Less, false),
            (BoundOperator::Greater, false),
        ] {
            let reflexive =
                ExecutionCondition::operation(operator, vec![left.clone(), left.clone()]);

            assert_eq!(reflexive.prove(&known, &mut 128), Some(valid));

            let unconditional =
                matches!(operator, BoundOperator::Less | BoundOperator::Greater).then_some(false);

            assert_eq!(
                reflexive.prove(&Default::default(), &mut 128),
                unconditional
            );

            let false_order =
                std::collections::BTreeSet::from([(comparison(BoundOperator::Greater), false)]);

            assert_eq!(reflexive.prove(&false_order, &mut 128), unconditional);
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
    fn positive_ordering_composes_strict_and_weak_edges_with_a_shared_budget() {
        let input = |index| {
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            ))
        };

        let relation = |operator, left, right| {
            ExecutionCondition::operation(operator, vec![input(left), input(right)])
        };

        for (first, second, strict) in [
            (BoundOperator::Less, BoundOperator::LessEqual, true),
            (BoundOperator::LessEqual, BoundOperator::Less, true),
            (BoundOperator::LessEqual, BoundOperator::LessEqual, false),
            (BoundOperator::Equal, BoundOperator::Less, true),
        ] {
            let known = std::collections::BTreeSet::from([
                (relation(first, 1, 2), true),
                (relation(second, 2, 3), true),
            ]);

            assert_eq!(
                relation(BoundOperator::LessEqual, 1, 3).prove(&known, &mut 128),
                Some(true)
            );

            assert_eq!(
                relation(BoundOperator::Greater, 1, 3).prove(&known, &mut 128),
                Some(false)
            );

            assert_eq!(
                relation(BoundOperator::Less, 1, 3).prove(&known, &mut 128),
                strict.then_some(true)
            );

            assert_eq!(
                relation(BoundOperator::LessEqual, 1, 3).prove(&known, &mut 1),
                None
            );
        }

        let unordered = std::collections::BTreeSet::from([
            (relation(BoundOperator::Greater, 1, 2), false),
            (relation(BoundOperator::Less, 2, 3), true),
        ]);

        assert_eq!(
            relation(BoundOperator::LessEqual, 1, 3).prove(&unordered, &mut 128),
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

        let source = ExecutionCondition::new(crate::ExecutionConditionKind::Input(input(1).into()));
        let target = ExecutionCondition::new(crate::ExecutionConditionKind::Input(input(2).into()));
        let mapping = [(input(1), input(2))].into();

        assert!(execution_condition_is_implied(&source, &[target], &mapping));
        assert!(!execution_condition_is_implied(&source, &[], &mapping));

        assert!(!execution_condition_is_implied(
            &ExecutionCondition::unknown(),
            &[ExecutionCondition::unknown()],
            &mapping,
        ));

        assert!(!execution_condition_is_implied(
            &source,
            std::slice::from_ref(&source),
            &Default::default(),
        ));
    }

    #[test]
    fn conjunction_can_establish_a_weaker_entry_domain() {
        let input = BoundReferenceTarget::Surface(
            CallableParameterSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        );

        let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Input(input.into()));

        let conjunction = ExecutionCondition::operation(
            BoundOperator::LogicalAnd,
            vec![condition.clone(), ExecutionCondition::boolean(true)],
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
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            ))
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
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                BoundReferenceTarget::Surface(
                    CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
                )
                .into(),
            ))
        };

        let mut assumptions = Default::default();

        ExecutionCondition::operation(BoundOperator::NotEqual, vec![input(1), input(2)])
            .assume(false, &mut assumptions);

        let equalities = ExecutionCondition::equalities(&assumptions, None);

        assert_eq!(input(2).with_equalities(&equalities), input(1));
    }
}
