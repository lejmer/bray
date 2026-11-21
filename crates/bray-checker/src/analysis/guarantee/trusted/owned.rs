use crate::execution_guarantees::ExecutionAssumptions;

use super::super::{flow::ExecutionFlowDomain, state::ExecutionState};
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

/// The output shape under which a checked identity transfer is available.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis) enum OwnedReturnCondition {
    Unconditional,
    ActivePayload,
}

/// Value, writes and ownership observed together at one reachable normal exit.
#[derive(Eq, PartialEq)]
pub(in crate::analysis) struct NormalReturn {
    pub(in crate::analysis) captured: bray_symbols::CapturedValueOutcome,
    pub(in crate::analysis) carriers: std::collections::BTreeSet<super::carriers::ReturnedCarrier>,
    pub(in crate::analysis) transfers: DependencyOwnedResult<StorageProjection, (bray_symbols::ConstantTermId, OwnedReturnCondition)>,
}

impl super::check::TrustedChecks {
    pub(super) fn normal_return<C: CheckerRequestContext + ?Sized>(
        &self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
        writes: &std::collections::BTreeSet<bray_bound_tree::StoragePath>,
    ) -> Option<NormalReturn> {
        let inputs = domain.request.trusted_contracts()?;

        let result = if inputs.infer_result_witnesses {
        let result_type = domain.semantics.types().callable_result_type().unwrap_or_else(|| {
            let crate::CheckerUnitRoot::Expression(root) = domain.request.root() else {
                unreachable!("a non-callable value producer retains its expression root");
            };

            domain.semantics.types().expression(root)
                .expect("checked default or constant producer retains its result type").ty()
        });

        result.clone().coerce(domain.request.semantic_values(), result_type, result_type)
        } else { result.clone() };

        let result = &result;
        let return_substitution = super::super::flow::return_substitution(result);
        let guards = super::captured::exit_guards(domain, state, result, &self.owned_entry_conditions, &return_substitution);

        Some(NormalReturn {
            carriers: self.exit_carriers(domain.request, state, result, &return_substitution),
            captured: super::captured::captured_outcome(domain, state, result, writes, &guards),
            transfers: if inputs.infer_result_witnesses { state.owned_result(result)
            .try_map(
                &mut |transfer| {
                    let source = transfer.input();

                    assert!(matches!(source.kind(), crate::ExecutionConditionKind::Input(_)),
                        "owned input lineage retains its formal place");

                    let active = activity(result, transfer.output(), &state.assumptions, &domain.proofs);

                    if active == Some(false) { return Ok(DependencyOwnedResult::default()); }

                    let shape = if active.is_some() { OwnedReturnCondition::Unconditional } else { OwnedReturnCondition::ActivePayload };
                    let input = super::carriers::portable(domain.request, &source);

                    Ok::<_, std::convert::Infallible>(
                        input
                            .map(|input| {
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    (input, shape),
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

                    Ok(super::carriers::portable(domain.request, &mapped).map(|condition| (condition, OwnedReturnCondition::Unconditional)))
                },
            )
            .expect("formal ownership publication is infallible") } else { Default::default() },
        })
    }
}

/// An inactive alternative owns no payload and imposes no input-identity requirement.
fn activity(
    result: &ExecutionCondition,
    path: &[StorageProjection],
    assumptions: &ExecutionAssumptions,
    cache: &crate::execution_guarantees::ExecutionProofCache,
) -> Option<bool> {
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
            return Some(false);
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
        Some(false)
    } else {
        active
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn outcome_reachable(
        &self, assumptions: &ExecutionAssumptions, entry: &crate::ExecutionCallEvidence, result: &ExecutionCondition,
    ) -> bool {
        entry.outputs.0.is_empty() || entry.outputs.reachable_rows().any(|row| {
            let guard = row.guard.bind(entry, result);

            self.admits_output(assumptions, entry, result, &guard, &row.ownership)
        })
    }
    pub(in crate::analysis::guarantee) fn admits_output(&self, assumptions: &ExecutionAssumptions,
        entry: &crate::ExecutionCallEvidence, result: &ExecutionCondition, guard: &ExecutionCondition, ownership: &DependencyOwnedResult,
    ) -> bool {
        let mut selected = assumptions.joined_with(&entry.assumptions);

        if !self.proofs.ordinary_proof(&selected).can_assume(guard, true) { return false; }
        guard.clone().assume(true, selected.to_mut());

        feasible_layout(ownership, &selected, &|term| entry.bind_term(self.request.semantic_values(), term, result))
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

    #[test]
    fn inactive_alternatives_keep_common_identity_without_weakening_present_masks() {
        use bray_symbols::CapturedValueOutcome;

        let store = crate::test_support::semantic_values();

        let argument = |ordinal| store.intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(ordinal)))
            .expect("formal exit input");

        let present = argument(1);
        let absent = argument(2);
        let transferred = DependencyOwnedTransfer::new(argument(0), []).with_preservation(DependencyValuePreservation::all());

        let rows: [CapturedValueOutcome; 2] = [
            CapturedValueOutcome::new(Some(present), []).with_ownership(DependencyOwnedResult::Transfer(transferred.clone())),
            CapturedValueOutcome::new(Some(absent), []),
        ];

        assert_eq!(CapturedValueOutcome::ownership_layout(&rows), DependencyOwnedResult::choice([
            DependencyOwnedResult::case(present, DependencyOwnedResult::Transfer(transferred)),
            DependencyOwnedResult::case(absent, DependencyOwnedResult::default()),
        ]));

        assert!(rows[1].ownership.is_empty());
    }
}
