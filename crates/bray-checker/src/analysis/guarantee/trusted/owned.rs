use std::collections::{BTreeMap, BTreeSet};

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
    assumptions: BTreeSet<(ExecutionCondition, bool)>,
    transfers: DependencyOwnedResult<StorageProjection>,
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

        let mut returned = OwnedReturn {
            value: result.clone(),
            assumptions: state
                .assumptions
                .iter()
                .filter(|(condition, holds)| {
                    condition.nullable_absence(*holds).is_some()
                        || condition.nullable_absence(!*holds).is_some()
                        || matches!(condition, ExecutionCondition::VariantTest(_, _))
                })
                .cloned()
                .collect(),
            transfers: super::super::flow::owned_alternatives(
                (state, result),
                (state, result),
                &ExecutionCondition::Result,
            )
            .try_map(
                &mut |transfer| {
                    let ExecutionCondition::Input(place) = transfer.input() else {
                        unreachable!("owned input lineage retains its formal place");
                    };

                    let input = (!domain
                        .borrowed_inputs
                        .iter()
                        .any(|borrow| borrow.contains(&place)))
                    .then(|| {
                        super::carriers::portable(domain.request, &ExecutionCondition::Input(place))
                    })
                    .flatten();

                    Ok::<_, std::convert::Infallible>(
                        input
                            .map(|input| {
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    input,
                                    transfer.output().iter().copied(),
                                ))
                            })
                            .unwrap_or_default(),
                    )
                },
                &mut |condition| {
                    if condition.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                        == Some(true)
                    {
                        return Ok(None);
                    }

                    let mapped = super::super::flow::returned_condition(condition, result);

                    Ok(super::carriers::portable(domain.request, &mapped))
                },
            )
            .expect("formal ownership publication is infallible"),
        };

        for (condition, holds) in state.assumptions.difference(&self.owned_entry_conditions) {
            if !matches!(condition, ExecutionCondition::Input(place) if place.is_callable_input()) {
                continue;
            }

            let condition = if *holds {
                condition.clone()
            } else {
                ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LogicalNot,
                    vec![condition.clone()],
                )
            };

            let condition = super::carriers::portable(domain.request, &condition)
                .expect("formal Boolean exit condition is portable");

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
        let candidates = self
            .owned_returns
            .iter()
            .flat_map(|returned| returned.transfers.transfers())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut shared = BTreeMap::new();
        let mut shapes = BTreeMap::new();

        for transfer in candidates {
            let mut conditional = false;
            let mut presence = false;

            let mut valid = true;

            for returned in &self.owned_returns {
                let (active, leaf_absent) =
                    activity(&returned.value, transfer.output(), &returned.assumptions);

                let contains = returned
                    .transfers
                    .transfers()
                    .any(|candidate| candidate == &transfer);

                if active == Some(false) {
                    conditional = true;
                    presence |= leaf_absent;
                } else {
                    conditional |= contains && active.is_none();

                    valid &= every_active_layout_transfers(
                        domain,
                        &returned.transfers,
                        &transfer,
                        &BTreeSet::new(),
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
        DependencyOwnedResult::choice(self.owned_returns.iter().map(|returned| {
            let local = returned
                .transfers
                .try_map(
                    &mut |transfer| {
                        if shared.contains_key(transfer) {
                            return Ok(DependencyOwnedResult::default());
                        }

                        let shape = shapes[transfer];

                        Ok::<_, std::convert::Infallible>(DependencyOwnedResult::Transfer(
                            DependencyOwnedTransfer::new(
                                (transfer.input(), shape),
                                transfer.output().iter().copied(),
                            ),
                        ))
                    },
                    &mut |condition| Ok(Some((*condition, OwnedReturnCondition::Unconditional))),
                )
                .expect("ownership mapping is infallible");

            DependencyOwnedResult::all(std::iter::once(local).chain(shared.iter().map(
                |(transfer, shape)| {
                    DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                        (transfer.input(), *shape),
                        transfer.output().iter().copied(),
                    ))
                },
            )))
        }))
    }
}

/// An inactive alternative owns no payload and imposes no input-identity requirement.
fn activity(
    result: &ExecutionCondition,
    path: &[StorageProjection],
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
) -> (Option<bool>, bool) {
    let mut value = result.clone();
    let mut active = Some(true);

    for projection in path {
        let known = match projection {
            StorageProjection::NullableValue => {
                if matches!(value, ExecutionCondition::Present(_)) {
                    Some(true)
                } else if matches!(&value, ExecutionCondition::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
                {
                    Some(false)
                } else {
                    assumptions.iter().find_map(|(condition, holds)| {
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
                .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
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

    if matches!(&value, ExecutionCondition::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
        || assumptions.iter().any(|(condition, holds)| {
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
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
) -> bool {
    if activity(&ExecutionCondition::Result, transfer.output(), assumptions).0 == Some(false) {
        return true;
    }

    match result {
        DependencyOwnedResult::Transfer(candidate) => candidate == transfer,
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
        assumptions: &BTreeSet<(ExecutionCondition, bool)>,
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
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                result,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
        };

        feasible_layout(contract.owned_result(), assumptions, &instantiate)
    }
}

fn feasible_layout(
    result: &DependencyOwnedResult,
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
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

            if condition.prove_trusted(&BTreeSet::new(), assumptions) == Some(false) {
                return false;
            }

            let mut assumptions = assumptions.clone();

            condition.assume(true, &mut assumptions);

            feasible_layout(result, &assumptions, instantiate)
        }
    }
}
