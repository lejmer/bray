use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundCallReference, BoundExecutionSite};

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::observations::GuaranteeSource;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace, TrustedCallContract};

#[derive(Default)]
pub(super) struct CompletionGuarantees<'contract> {
    conditions: Vec<ExecutionCondition>,
    subjects: Vec<(ExecutionCondition, &'contract ExecutionCondition)>,
    pub(super) addresses: Vec<(ExecutionCondition, ExecutionCondition, bool)>,
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
                && condition.observes_completion_result()
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

            if std::env::var_os("BRAY_BRA694_TRACE").is_some()
                && ["SymbolId(5264)", "SymbolId(2031)", "SymbolId(5713)", "SymbolId(2073)"].iter()
                    .any(|predicate| format!("{:?}", guarantee.kind()).contains(predicate))
            {
                eprintln!("BRA694 GUARANTEE SUBJECT invocation={invocation:?} guarantee={:?} subject={:?} reached={:?}",
                    guarantee.kind(), subject.kind(), condition.kind());
            }

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

            if std::env::var_os("BRAY_BRA694_TRACE").is_some()
                && ["SymbolId(5264)", "SymbolId(2031)", "SymbolId(5713)", "SymbolId(2073)"].iter()
                    .any(|predicate| format!("{:?}", guarantee.kind()).contains(predicate))
            {
                eprintln!("BRA694 GUARANTEE INPUT invocation={invocation:?} carriers={} witnesses={} carrier={:?}",
                    carriers.len(), state.witnesses.len(), carriers.first().map(ExecutionCondition::kind));
            }

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

        for condition in &guarantees.conditions {
            state.assume_contract_condition(instantiate(condition));
        }
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
