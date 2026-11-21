use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundCallReference, BoundExecutionSite, BoundOperator};

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::observations::GuaranteeSource;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionConditionKind, ExecutionPlace, TrustedCallContract};

#[derive(Default)]
pub(super) struct CompletionGuarantees<'contract> {
    conditions: Vec<ExecutionCondition>,
    subjects: Vec<(ExecutionCondition, &'contract ExecutionCondition)>,
    addresses: Vec<(ExecutionCondition, ExecutionCondition, bool)>,
}

impl<'contract> CompletionGuarantees<'contract> {
    pub(super) fn prepare(
        contract: &'contract TrustedCallContract,
        entry: &ExecutionCallEvidence,
    ) -> (Self, Self) {
        let mut row = Self::default();
        let mut published = Self::default();
        let publication = |condition: &ExecutionCondition| {
            condition.contains_trusted()
                && matches!(condition.kind(), ExecutionConditionKind::Operation(BoundOperator::LogicalOr, operands)
                    if operands.iter().any(|operand| {
                        !operand.contains_trusted() && operand.observes_completion_result()
                    }))
                && !contract.address_subjects.iter().any(|(clause, _, _)| clause == condition)
        };

        for condition in contract.guarantees.iter().flat_map(ExecutionCondition::conjuncts) {
            let captured = entry.capture_condition(condition);

            if publication(condition) {
                published.conditions.push(captured);
            } else {
                row.conditions.push(captured);
            }
        }

        for (condition, subject) in &contract.witness_subjects {
            let captured = entry.capture_condition(condition);

            if !captured.contains_trusted() {
                continue;
            }

            if publication(condition) {
                published.subjects.push((captured, subject));
            } else {
                row.subjects.push((captured, subject));
            }
        }

        row.addresses.extend(contract.address_subjects.iter().map(|(condition, subject, owning)| {
            (entry.capture_condition(condition), subject.clone(), *owning)
        }));

        (row, published)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.conditions.is_empty() && self.subjects.is_empty() && self.addresses.is_empty()
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn publish_call_guarantees(
        &self,
        state: &mut ExecutionState,
        (invocation, references): (BoundExecutionSite, &[BoundCallReference]),
        (contract, entry): (&TrustedCallContract, &ExecutionCallEvidence),
        guarantees: &CompletionGuarantees<'_>,
        result: &ExecutionCondition,
        (arguments, owners): (&BTreeMap<ExecutionPlace, ExecutionCondition>, &BTreeMap<BoundExecutionSite, GuaranteeSource>),
        supported: &BTreeSet<ExecutionCondition>,
        instantiate: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) {
        let trace = self.trace_authority_site(bray_bound_tree::StorageMutation::Operation(invocation))
            && !guarantees.addresses.is_empty();

        if trace {
            eprintln!("address admission {invocation:?}: entry={:?} supported={supported:?} subjects={:?}", entry.addresses, guarantees.addresses);
        }

        let carry_result = |state: &mut ExecutionState,
                            template: &ExecutionCondition,
                            guarantee: &ExecutionCondition,
                            carrier: &ExecutionCondition,
                            owning: bool| {
            let sources = self.result_guarantee_owners(
                (invocation, references),
                &entry.arguments,
                template,
                owning,
                owners,
                &guarantees.addresses,
                instantiate,
            );

            if trace {
                eprintln!("address guarantee {invocation:?}: guarantee={guarantee:?} owner={carrier:?} support={sources:?}");
            }

            self.carry_result_guarantee(
                state,
                carrier,
                guarantee,
                !contract.transferred_inputs.is_empty(),
                owning,
                sources,
            );
        };

        for (template, subject) in &guarantees.subjects {
            let guarantee = instantiate(template);
            let condition = instantiate(subject);

            if subject.observes_completion_result() && supported.iter().any(|proof| proof.implies(&guarantee)) {
                continue;
            }

            // A guarantee discharged solely by its ordinary guard carries no trusted authority.
            if condition == ExecutionCondition::unknown()
                || guarantee.prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(true)
            {
                continue;
            }

            if subject.observes_completion_result() {
                let carrier = self.borrowed_result_carrier(result, subject).unwrap_or(condition);

                carry_result(state, template, &guarantee, &carrier, true);
                continue;
            }

            let carriers = self.input_witness_owners(
                state, template, subject, &condition, arguments, &guarantees.addresses,
            );

            if carriers.is_empty() { continue; }

            let addresses = self.input_guarantee_addresses(
                (invocation, references),
                &entry.arguments,
                template,
                owners,
                &guarantees.addresses,
                instantiate,
            );

            for carrier in carriers {
                state
                    .witnesses
                    .entry(carrier.owner_identity().clone())
                    .or_default()
                    .carry(&guarantee, true, &addresses);
            }
        }

        self.carry_input_access_guarantees(
            (invocation, references),
            state,
            &entry.arguments,
            &guarantees.addresses,
            owners,
            instantiate,
        );

        for template in guarantees.conditions
            .iter()
            .filter(|guarantee| guarantee.contains_trusted() && guarantee.observes_completion_result())
        {
            if guarantees.subjects.iter().any(|(guarantee, subject)| {
                guarantee == template && subject.observes_completion_result()
            }) {
                continue;
            }

            let guarantee = instantiate(template);

            if !supported.iter().any(|proof| proof.implies(&guarantee))
                && guarantee.prove_ordinary(&state.assumptions, Some(&state.proofs)) != Some(true)
            {
                carry_result(state, template, &guarantee, result, false);
            }
        }

        if self.trace_authority_site(bray_bound_tree::StorageMutation::Operation(invocation)) {
            let facts = guarantees.conditions.iter().map(|condition|
                format!("{:?}", instantiate(condition).kind()).chars().take(570).collect::<String>()).collect::<Vec<_>>();

            eprintln!("admitted completion {invocation:?}: facts={facts:?}");
        }

        assume_guarantees(state, guarantees.conditions.iter().map(instantiate));
    }

    pub(super) fn carry_result_guarantee(
        &self,
        state: &mut ExecutionState,
        carrier: &ExecutionCondition,
        guarantee: &ExecutionCondition,
        transferred: bool,
        owning: bool,
        sources: (
            BTreeSet<crate::ExecutionPlace>,
            crate::execution_guarantees::AddressDependencies,
        ),
    ) {
        let (dependencies, addresses) = sources;

        let witness = state
            .witnesses
            .entry(carrier.owner_identity().clone())
            .or_default();

        if !dependencies.is_subset(&witness.dependencies) {
            std::sync::Arc::make_mut(witness)
                .dependencies
                .extend(dependencies);
        }

        witness.carry(guarantee, owning, &addresses);

        // Copyable raw-address facts do not own allocation authority. Only a complete
        // owning carrier restricts which transferred values may supply that authority.
        if !owning {
            return;
        }

        let mut pending = vec![guarantee];

        while let Some(predicate) = pending.pop() {
            match predicate.kind() {
                crate::ExecutionConditionKind::Trusted(predicate) => pending.push(predicate),
                crate::ExecutionConditionKind::Operation(_, operands) => pending.extend(operands.iter()),
                crate::ExecutionConditionKind::Predicate(symbol, _, _)
                    if Some(*symbol) == self.owned_allocation
                        && (predicate.observes(carrier) || transferred) =>
                {
                    state
                        .allocation_owners
                        .insert(predicate.clone(), carrier.owner_identity().clone());
                }
                _ => {}
            }
        }
    }
}

pub(super) fn assume_guarantees(
    state: &mut ExecutionState,
    guarantees: impl Iterator<Item = ExecutionCondition>,
) {
    for guarantee in guarantees {
        guarantee.assume_proof(true, false, &mut |condition, holds| {
            if condition.contains_trusted() {
                // An ordinary exit guard has no witness support to revoke.
                condition.assume(holds, &mut state.trusted_assumptions);
            } else {
                state.assumptions.insert((condition, holds));
            }
        });
    }
}
