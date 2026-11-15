use crate::execution_guarantees::ExecutionAssumptions;
use std::collections::BTreeMap;

use super::super::{flow::ExecutionFlowDomain, state::ExecutionState};
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

/// The output shape under which a checked identity transfer is available.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) enum OwnedReturnCondition {
    Unconditional,
    ActivePayload,
    PresentValue,
}

/// Only ordinary result-shape facts are needed to compare normal ownership exits.
#[derive(Eq, PartialEq)]
pub(super) struct OwnedReturn {
    value: ExecutionCondition,
    assumptions: ExecutionAssumptions,
    transfers: DependencyOwnedResult<StorageProjection>,
}

impl OwnedReturn {
    pub(super) fn excludes(&self, path: &[StorageProjection], cache: &crate::execution_guarantees::ExecutionProofCache) -> bool {
        activity(&self.value, path, &self.assumptions, cache).0 == Some(false)
    }
}

impl super::check::TrustedChecks {
    pub(super) fn collect_owned_return<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
    ) {
        if !domain
            .request
            .trusted_contracts()
            .is_some_and(|inputs| inputs.infer_result_witnesses)
        {
            return;
        }

        let result_type = domain.semantics.types().callable_result_type().unwrap_or_else(|| {
            let crate::CheckerUnitRoot::Expression(root) = domain.request.root() else {
                unreachable!("a non-callable value producer retains its expression root");
            };

            domain.semantics.types().expression(root)
                .expect("checked default or constant producer retains its result type").ty()
        });

        let result = result.clone().coerce(domain.request.semantic_values(), result_type, result_type);
        let result = &result;
        let return_substitution = super::super::flow::return_substitution(result);

        let mut returned = OwnedReturn {
            value: result.clone(),
            assumptions: state.proofs.ordinary_proof(&state.assumptions)
                .ordinary_facts()
                .filter(|(condition, holds)| {
                    condition.nullable_absence(*holds).is_some()
                        || condition.nullable_absence(!*holds).is_some()
                        || matches!(condition.kind(), crate::ExecutionConditionKind::VariantTest(_, _))
                })
                .cloned()
                .collect(),
            transfers: super::super::flow::owned_alternatives(
                (state, result),
                (state, result),
                &ExecutionCondition::result(),
            )
            .try_map(
                &mut |transfer| {
                    let source = transfer.input();

                    assert!(matches!(source.kind(), crate::ExecutionConditionKind::Input(_)),
                        "owned input lineage retains its formal place");

                    let input = super::carriers::portable(domain.request, &source);

                    Ok::<_, std::convert::Infallible>(
                        input
                            .map(|input| {
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    input,
                                    transfer.output().iter().copied(),
                                ).with_preservation(transfer.preservation().clone()))
                            })
                            .unwrap_or_default(),
                    )
                },
                &mut |condition| {
                    if *condition == ExecutionCondition::boolean(true) {
                        return Ok(None);
                    }

                    let mapped = return_substitution(condition);

                    Ok(super::carriers::portable(domain.request, &mapped))
                },
            )
            .expect("formal ownership publication is infallible"),
        };

        for (condition, holds) in state.assumptions.difference(&self.owned_entry_conditions) {
            let formal = matches!(condition.kind(), crate::ExecutionConditionKind::Input(place) if place.is_callable_input());

            let condition = if formal { condition.clone() } else {
                if condition.contains_trusted() { continue; }

                let mapped = condition.boolean_consequence(*holds, &mut |atom| {
                    let mapped = return_substitution(atom);

                    super::carriers::portable(domain.request, &mapped)?;

                    // Mapping each atom keeps one clause's unrelated local terms
                    // from consuming the current-snapshot rewrite budget.
                    let actual = mapped.substitute(&|place| ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())), result,
                        &mut { ExecutionCondition::WORK_LIMIT });

                    (actual == *atom).then_some(mapped)
                }, &mut { ExecutionCondition::WORK_LIMIT });

                if !mapped.observes_completion_result() { continue; }

                mapped
            };

            let condition = if !formal || *holds {
                condition
            } else {
                ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LogicalNot,
                    vec![condition],
                )
            };

            let condition = match super::carriers::portable(domain.request, &condition) {
                Some(condition) => condition,
                None if formal => unreachable!("formal Boolean exit condition is portable"),
                None => continue,
            };

            returned.transfers = DependencyOwnedResult::case(condition, returned.transfers);
        }

        if !self.owned_returns.contains(&returned) {
            self.owned_returns.push(returned);
        }
    }

    pub(in crate::analysis) fn owned_result<C: CheckerRequestContext + ?Sized>(
        &self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> DependencyOwnedResult<
        StorageProjection,
        (bray_symbols::ConstantTermId, OwnedReturnCondition),
    > {
        let mut candidates: Vec<DependencyOwnedTransfer<StorageProjection>> = Vec::new();

        for transfer in self.owned_returns.iter().flat_map(|returned| returned.transfers.transfers()) {
            if let Some(previous) = candidates.iter_mut().find(|previous| previous.same_identity(transfer)) {
                *previous = previous.clone().with_preservation(
                    previous.preservation().intersection(transfer.preservation()),
                );
            } else {
                candidates.push(transfer.clone());
            }
        }

        let mut shared = BTreeMap::new();
        let mut shapes = BTreeMap::new();

        for transfer in candidates {
            let mut conditional = false;
            let mut presence = false;

            let mut valid = true;

            for returned in &self.owned_returns {
                let (active, leaf_absent) =
                    activity(&returned.value, transfer.output(), &returned.assumptions, &domain.proofs);

                let contains = returned
                    .transfers
                    .transfers()
                    .any(|candidate| candidate.same_identity(&transfer));

                if active == Some(false) {
                    conditional = true;
                    presence |= leaf_absent;
                } else {
                    conditional |= contains && active.is_none();

                    valid &= every_active_layout_transfers(
                        domain,
                        &returned.transfers,
                        &transfer,
                        &Default::default(),
                    );
                }
            }

            let shape = if presence {
                OwnedReturnCondition::PresentValue
            } else if conditional {
                OwnedReturnCondition::ActivePayload
            } else {
                OwnedReturnCondition::Unconditional
            };

            shapes.insert(transfer.clone(), shape);

            if valid {
                shared.insert(transfer, shape);
            }
        }

        // Common conditional moves also describe inactive exits. Other exits retain
        // their complete alternatives, including layouts that transfer no input.
        let common = shared.keys().collect::<Vec<_>>();

        DependencyOwnedResult::choice(self.owned_returns.iter().map(|returned| {
            let factored = lift_shared_cases(&returned.transfers, &|transfer| {
                shared.keys().any(|candidate| candidate.same_identity(transfer))
            });
            let factored = complete_shared_alternatives(&factored, &common, &[]);

            fill_shared_alternative(factored, &common)
                .try_map(
                    &mut |transfer| {
                        let (_, shape) = shapes.iter()
                            .find(|(candidate, _)| candidate.same_identity(transfer))
                            .expect("normal ownership layout retains its inferred shape");

                        let shape = *shape;

                        Ok::<_, std::convert::Infallible>(DependencyOwnedResult::Transfer(
                            DependencyOwnedTransfer::new(
                                (transfer.input(), shape),
                                transfer.output().iter().copied(),
                            ).with_preservation(transfer.preservation().clone()),
                        ))
                    },
                    &mut |condition| Ok(Some((*condition, OwnedReturnCondition::Unconditional))),
                )
                .expect("ownership mapping is infallible")
        }))
    }
}

// Keep a common identity outside branch guards, while each complete alternative
// retains its own contents mask and the guards that make that alternative feasible.
fn lift_shared_cases(
    layout: &DependencyOwnedResult<StorageProjection>,
    shared: &impl Fn(&DependencyOwnedTransfer<StorageProjection>) -> bool,
) -> DependencyOwnedResult<StorageProjection> {
    match layout {
        DependencyOwnedResult::Transfer(_) => layout.clone(),
        DependencyOwnedResult::All(values) => DependencyOwnedResult::all(values.iter().map(|value| lift_shared_cases(value, shared))),
        DependencyOwnedResult::Choice(values) => DependencyOwnedResult::choice(values.iter().map(|value| lift_shared_cases(value, shared))),
        DependencyOwnedResult::Case { condition, result } => {
            factor_guard(*condition, lift_shared_cases(result, shared), shared)
        }
    }
}

fn factor_guard(
    condition: bray_symbols::ConstantTermId,
    layout: DependencyOwnedResult<StorageProjection>,
    shared: &impl Fn(&DependencyOwnedTransfer<StorageProjection>) -> bool,
) -> DependencyOwnedResult<StorageProjection> {
    match layout {
        DependencyOwnedResult::Transfer(ref transfer) if shared(transfer) => DependencyOwnedResult::all([
            layout, DependencyOwnedResult::case(condition, DependencyOwnedResult::default()),
        ]),
        DependencyOwnedResult::All(values) if !values.is_empty() => DependencyOwnedResult::all(values.iter()
            .map(|value| factor_guard(condition, value.clone(), shared))),
        DependencyOwnedResult::Choice(values) => DependencyOwnedResult::choice(values.iter()
            .map(|value| factor_guard(condition, value.clone(), shared))),
        layout => DependencyOwnedResult::case(condition, layout),
    }
}

// Complete inactive alternatives before the caller retires unreturned inputs.
// Simultaneous siblings can already provide an identity with a stronger mask.
fn complete_shared_alternatives(
    layout: &DependencyOwnedResult<StorageProjection>,
    shared: &[&DependencyOwnedTransfer<StorageProjection>],
    covered: &[&DependencyOwnedTransfer<StorageProjection>],
) -> DependencyOwnedResult<StorageProjection> {
    match layout {
        DependencyOwnedResult::Transfer(_) => layout.clone(),
        DependencyOwnedResult::Case { condition, result } => DependencyOwnedResult::case(
            *condition, complete_shared_alternatives(result, shared, covered),
        ),
        DependencyOwnedResult::All(values) => {
            let guaranteed = values.iter().map(DependencyOwnedResult::guaranteed_transfers).collect::<Vec<_>>();

            DependencyOwnedResult::all(values.iter().enumerate().map(|(index, value)| {
                let covered = covered.iter().copied().chain(guaranteed.iter().enumerate()
                    .filter(|(other, _)| *other != index).flat_map(|(_, transfers)| transfers)).collect::<Vec<_>>();

                complete_shared_alternatives(value, shared, &covered)
            }))
        }
        DependencyOwnedResult::Choice(values) => {
            let required = shared.iter().copied().filter(|transfer| {
                !covered.iter().any(|other| other.same_identity(transfer))
                    && layout.transfers().any(|actual| actual.same_identity(transfer))
            }).collect::<Vec<_>>();

            DependencyOwnedResult::choice(values.iter().map(|value| {
                fill_shared_alternative(complete_shared_alternatives(value, shared, covered), &required)
            }))
        }
    }
}

fn fill_shared_alternative(
    layout: DependencyOwnedResult<StorageProjection>,
    shared: &[&DependencyOwnedTransfer<StorageProjection>],
) -> DependencyOwnedResult<StorageProjection> {
    let guaranteed = layout.guaranteed_transfers();
    let missing = shared.iter().filter(|transfer| !guaranteed.iter().any(|actual| actual.same_identity(transfer)))
        .map(|transfer| DependencyOwnedResult::Transfer((*transfer).clone()));

    DependencyOwnedResult::all(std::iter::once(layout).chain(missing))
}

/// An inactive alternative owns no payload and imposes no input-identity requirement.
pub(super) fn activity(
    result: &ExecutionCondition,
    path: &[StorageProjection],
    assumptions: &ExecutionAssumptions,
    cache: &crate::execution_guarantees::ExecutionProofCache,
) -> (Option<bool>, bool) {
    let proof = cache.ordinary_proof(assumptions);
    let mut value = result.clone();
    let mut active = Some(true);

    for projection in path {
        let known = match projection {
            StorageProjection::NullableValue => {
                if let Some(present) = value.known_presence() {
                    Some(present)
                } else {
                    proof.ordinary_facts().find_map(|(condition, holds)| {
                        condition
                            .nullable_absence(*holds)
                            .map(|(subject, _)| (subject, false))
                            .or_else(|| {
                                condition
                                    .nullable_absence(!*holds)
                                    .map(|(subject, _)| (subject, true))
                            })
                            .and_then(|(subject, present)| {
                                (subject == &value || subject == value.owner_identity())
                                    .then_some(present)
                            })
                    })
                }
            }
            StorageProjection::ActiveUnionPayloadField { variant, .. } => value
                .clone()
                .matches_variant(*variant)
                .prove_ordinary(assumptions, Some(cache)),
            _ => Some(true),
        };

        if known == Some(false) {
            return (Some(false), false);
        }

        if known.is_none() {
            active = None;
        }

        value = value.project(*projection);
    }

    if value.known_presence() == Some(false)
        || proof.ordinary_facts().any(|(condition, holds)| {
            condition
                .nullable_absence(*holds)
                .is_some_and(|(subject, _)| subject == &value || subject == value.owner_identity())
        })
    {
        (Some(false), true)
    } else {
        (active, false)
    }
}

fn every_active_layout_transfers<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    result: &DependencyOwnedResult<StorageProjection>,
    transfer: &DependencyOwnedTransfer<StorageProjection>,
    assumptions: &ExecutionAssumptions,
) -> bool {
    if activity(&ExecutionCondition::result(), transfer.output(), assumptions, &domain.proofs).0 == Some(false) {
        return true;
    }

    match result {
        DependencyOwnedResult::Transfer(candidate) => candidate.same_identity(transfer),
        DependencyOwnedResult::All(values) => values
            .iter()
            .any(|value| every_active_layout_transfers(domain, value, transfer, assumptions)),
        DependencyOwnedResult::Choice(values) => values
            .iter()
            .all(|value| every_active_layout_transfers(domain, value, transfer, assumptions)),
        DependencyOwnedResult::Case { condition, result } => {
            let inputs = domain
                .request
                .trusted_contracts()
                .expect("owned normal return retains its formal inputs");

            let condition = crate::execution_condition_from_type_term(
                domain.request.semantic_values(),
                *condition,
                inputs.inputs.len(),
            )
            .expect("checked normal-result shape decodes in the same semantic store");

            let mut assumptions = assumptions.clone();

            condition.assume(true, &mut assumptions);

            every_active_layout_transfers(domain, result, transfer, &assumptions)
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn owned_normal_completion(
        &self,
        assumptions: &ExecutionAssumptions,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &crate::ExecutionCallEvidence,
        result: &ExecutionCondition,
    ) -> bool {
        let Some(contract) = self.dependencies.calls().get(&invocation) else {
            return true;
        };

        let instantiate = |term| {
            crate::execution_condition_from_type_term(
                self.request.semantic_values(),
                term,
                contract.input_count(),
            )
            .expect("checked ownership condition decodes in the same semantic store")
            .substitute(
                &|place| {
                    place
                        .value_in(&entry.arguments)
                        .unwrap_or_else(ExecutionCondition::unknown)
                },
                result,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
        };

        let mut assumptions = assumptions.clone();

        assumptions.extend(entry.assumptions.iter().cloned());

        feasible_layout(contract.owned_result(), &assumptions, &instantiate)
    }
}

fn feasible_layout(
    result: &DependencyOwnedResult,
    assumptions: &ExecutionAssumptions,
    instantiate: &impl Fn(bray_symbols::ConstantTermId) -> ExecutionCondition,
) -> bool {
    match result {
        DependencyOwnedResult::Transfer(_) => true,
        DependencyOwnedResult::All(values) => values
            .iter()
            .all(|value| feasible_layout(value, assumptions, instantiate)),
        DependencyOwnedResult::Choice(values) => values
            .iter()
            .any(|value| feasible_layout(value, assumptions, instantiate)),
        DependencyOwnedResult::Case { condition, result } => {
            let condition = instantiate(*condition);

            if condition.prove_ordinary(assumptions, None) == Some(false) {
                return false;
            }

            let mut assumptions = assumptions.clone();

            condition.assume(true, &mut assumptions);

            feasible_layout(result, &assumptions, instantiate)
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{ConstantTermData, DependencyOwnedResult, DependencyOwnedTransfer, DependencyValuePreservation, SymbolOrdinal};

    use super::{complete_shared_alternatives, fill_shared_alternative, lift_shared_cases};

    #[test]
    fn inactive_alternatives_keep_common_identity_without_weakening_present_masks() {
        let input = crate::test_support::semantic_values()
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .expect("formal ownership input");
        let guard = crate::test_support::semantic_values()
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .expect("ordinary result guard");
        let complete = DependencyOwnedTransfer::new(input, [])
            .with_preservation(DependencyValuePreservation::all());
        let changed = complete.clone().with_preservation(DependencyValuePreservation::none());
        let guarded = DependencyOwnedResult::case(guard, DependencyOwnedResult::Transfer(complete.clone()));
        let empty = DependencyOwnedResult::case(guard, DependencyOwnedResult::default());
        let layout = DependencyOwnedResult::choice([guarded, empty.clone()]);
        let lifted = lift_shared_cases(&layout, &|transfer| transfer.same_identity(&complete));
        let common = [&changed];
        let actual = fill_shared_alternative(complete_shared_alternatives(&lifted, &common, &[]), &common);

        assert_eq!(actual, DependencyOwnedResult::choice([
            DependencyOwnedResult::all([DependencyOwnedResult::Transfer(complete.clone()), empty.clone()]),
            DependencyOwnedResult::all([DependencyOwnedResult::Transfer(changed.clone()), empty]),
        ]));

        let simultaneous = DependencyOwnedResult::all([
            DependencyOwnedResult::Transfer(complete),
            DependencyOwnedResult::choice([
                DependencyOwnedResult::Transfer(changed), DependencyOwnedResult::default(),
            ]),
        ]);
        let common = simultaneous.transfers().collect::<Vec<_>>();

        assert_eq!(complete_shared_alternatives(&simultaneous, &common, &[]), simultaneous,
            "an enclosing simultaneous owner must not receive a weaker fallback in an inactive child");
    }
}
