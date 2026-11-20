use crate::execution_guarantees::ExecutionAssumptions;
use std::collections::BTreeMap;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, StorageProjection};
use bray_symbols::{
    DependencyReferenceKind, DependencySubject,
};

use super::super::flow::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

/// The produced component and its admitted ownership lineage for one possible transfer.
struct ReturnedValue {
    path: Vec<StorageProjection>,
    value: ExecutionCondition,
    source: ExecutionCondition,
    extracted: bool,
}

type ReturnedValues = BTreeMap<bray_symbols::DependencyOwnedTransfer, ReturnedValue>;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn reference_value_source(
        &self,
        expression: BoundExpressionId,
    ) -> Option<BoundExpressionId> {
        use bray_bound_tree::{
            BoundExpression, BoundStructuredExpressionKind, ConversionTarget, SelectedOperation,
            SemanticSelection,
        };

        match self.request.view().expression(expression)? {
            BoundExpression::Structured(operation)
                if matches!(
                    operation.kind(),
                    BoundStructuredExpressionKind::Condition
                        | BoundStructuredExpressionKind::TrustBoundary
                ) =>
            {
                operation.operands().first().copied()
            }
            BoundExpression::Conversion(conversion) => {
                let SemanticSelection::Operation(SelectedOperation::Conversion(selected)) =
                    self.semantics.selections().expression(expression)?
                else {
                    return None;
                };

                matches!(selected.target(), ConversionTarget::Identity)
                    .then_some(conversion.operand())
            }
            // Children establish evaluation and lifetime dependencies. They do not
            // establish that a constructed value or call result reaches their storage.
            _ => None,
        }
    }

    fn feasible_owned_transfers<'layout>(
        &self,
        entry: &crate::ExecutionCallEvidence,
        assumptions: &ExecutionAssumptions,
        result: &ExecutionCondition,
        outcome_guard: Option<bray_symbols::ConstantTermId>,
        ownership: &'layout bray_symbols::DependencyOwnedResult,
    ) -> Vec<(&'layout bray_symbols::DependencyOwnedTransfer, ExecutionCondition)> {

        let assumptions = assumptions.joined_with(&entry.assumptions);
        let bind = |term| entry.bind_term(self.request.semantic_values(), term, result);

        ownership.guarded_transfers().filter_map(|(transfer, cases)| {
            let guard = outcome_guard.into_iter().chain(cases.into_iter().copied()).chain(transfer.condition()).map(bind)
                .fold(ExecutionCondition::boolean(true), |guard, condition|
                    ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![guard, condition]));

            (guard.prove_ordinary(&assumptions, Some(&self.proofs)) != Some(false)).then_some((transfer, guard))
        }).collect()
    }

    pub(in crate::analysis::guarantee) fn extracted_input_values(
        &self,
        storage: &bray_bound_tree::BoundCallStorage,
        rows: &[bray_symbols::CapturedValueOutcome],
        entry: &crate::ExecutionCallEvidence,
    ) -> BTreeMap<bray_symbols::ConstantTermId, ExecutionCondition> {
        let result = ExecutionCondition::unknown();

        rows.iter().flat_map(|outcome| {
            self.feasible_owned_transfers(entry, &entry.assumptions, &result, outcome.guard, &outcome.ownership)
        })
            .map(|(transfer, _)| transfer)
            .filter(|transfer| !storage.returned_value(transfer).is_borrowed())
            .filter_map(|transfer| {
                let input = transfer.input();
                let source = entry.bind_term(self.request.semantic_values(), input, &result);

                if !source.is_reference() { return None; }

                let value = source.argument_value(false);

                value.ownership_identity().is_some_and(|(role, _)|
                    role == crate::execution_guarantees::ExecutionObservationKind::Owned).then_some((input, value))
            }).collect()
    }

    fn transferred_values(
        &self,
        state: &super::super::state::ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        result: &ExecutionCondition,
        ownership: &bray_symbols::DependencyOwnedResult,
        storage: Option<&bray_bound_tree::BoundCallStorage>,
    ) -> ReturnedValues {
        let mut fields = BTreeMap::new();

        for (transfer, _) in self.feasible_owned_transfers(entry, &state.assumptions, result, None, ownership) {
            if fields.contains_key(transfer) { continue; }

            let path = crate::storage::owned_storage_projections(self.dependencies, transfer.output());
            let actual = path.iter().copied().fold(result.clone(), ExecutionCondition::project);
            let borrowed = storage.expect("selected owned transfer retains its checked storage contract")
                .returned_value(transfer).is_borrowed();

            let extraction = entry.extracted_values.get(&transfer.input()).filter(|_| !borrowed);

            let is_extraction = extraction.is_some();

            let source = extraction.cloned()
                .unwrap_or_else(|| entry.bind_term(self.request.semantic_values(), transfer.input(), result));

            if source.is_unknown() { continue; }

            let actual = if borrowed {
                actual.borrowed()
            } else if is_extraction && actual.is_reference() {
                actual.argument_value(false)
            } else { actual };

            let value = if actual.is_reference() || source.is_reference() || source.owner_identity() == actual.owner_identity() {
                actual
            } else {
                ExecutionCondition::owned_value(source.owner_identity().clone(), actual)
            };

            fields.insert(transfer.clone(), ReturnedValue { path, value, source, extracted: is_extraction });
        }

        fields
    }

    /// Qualify live input evidence by the result layouts that actually retain it.
    fn guard_returned_inputs(
        &self,
        state: &mut super::super::state::ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        returned: &ReturnedValues,
        result: &ExecutionCondition,
        ownership: &bray_symbols::DependencyOwnedResult,
    ) {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let mut guards = BTreeMap::new();

        for (transfer, guard) in self.feasible_owned_transfers(entry, &state.assumptions, result, None, ownership) {
            for value in returned.get(transfer).into_iter()
                .flat_map(|returned| state.owned_values(&returned.value))
            {
                let owner = value.owner_identity().clone();

                guards
                    .entry(owner)
                    .and_modify(|previous: &mut ExecutionCondition| {
                        *previous = ExecutionCondition::operation(
                            BoundOperator::LogicalOr,
                            vec![previous.clone(), guard.clone()],
                        );
                    })
                    .or_insert_with(|| guard.clone());
            }
        }

        let mut retired = BTreeSet::new();
        let mut published = Vec::new();
        let proof = state.proofs.proof(&state.trusted_assumptions, &state.assumptions);
        let selected = state.assumptions.joined_with(&entry.assumptions);

        for (owner, guard) in guards {
            if guard.prove_ordinary(&selected, Some(&state.proofs))
                == Some(true)
            {
                continue;
            }

            let Some(witness) = state.witnesses.get_mut(&owner) else {
                continue;
            };

            let mut conditions =
                BTreeMap::<_, crate::execution_guarantees::ExecutionGuarantee>::new();

            for (condition, guarantee) in &witness.conditions {
                let qualified = ExecutionCondition::operation(
                    BoundOperator::LogicalOr,
                    vec![
                        ExecutionCondition::operation(
                            BoundOperator::LogicalNot,
                            vec![guard.clone()],
                        ),
                        condition.clone(),
                    ],
                );

                if witness.available
                    && proof.prove_trusted(condition)
                        == Some(true)
                {
                    published.push(qualified.clone());
                }

                condition.clone().assume(true, &mut retired);
                conditions.entry(qualified).or_default().extend(guarantee);
            }

            if conditions != witness.conditions {
                std::sync::Arc::make_mut(witness).conditions = conditions;
            }
        }

        state.exclude_live_witnesses(&mut retired);

        state
            .trusted_assumptions
            .retain(|fact| !retired.contains(fact));

        for condition in published {
            condition.assume(true, &mut state.trusted_assumptions);
        }
    }

    /// Owners outside the completing frame keep copied observations alive.
    pub(super) fn surviving_call_values(
        &self,
        state: &super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        entry: &crate::ExecutionCallEvidence,
    ) -> std::collections::BTreeSet<ExecutionCondition> {
        if entry.owned_inputs.is_empty() && entry.extracted_values.is_empty() {
            return Default::default();
        }

        let result_place = self.invocation_result_place(invocation);

        let consumed = self.storage.value_retirements(
            invocation, bray_bound_tree::StorageValueConsumptionPhase::Admission,
        ).filter_map(|path| ExecutionPlace::storage_path(self.storage, path))
            .collect::<Vec<_>>();

        state
            .current
            .live_entries()
            .filter(|(place, _)| {
                if result_place.as_ref().is_some_and(|result| result.contains(place))
                    || consumed.iter().any(|owner| owner.contains(place))
                {
                    return false;
                }

                // Staged arguments belong to this callee after admission. Including
                // them would retain authority even when the call consumes its owner.
                !self.is_invocation_input_place(invocation, place)
            })
            .map(|(_, value)| value)
            .flat_map(|value| state.witness_values(&value))
            .collect()
    }

    fn is_invocation_input_place(&self, invocation: BoundExecutionSite, place: &ExecutionPlace) -> bool {
        let crate::execution_guarantees::ExecutionInput::Storage(identity) = place.root else {
            return false;
        };

        matches!(self.storage.identity(identity), Some(
            bray_bound_tree::StorageIdentity::InvocationInput { expression, ordinal }
        ) if invocation == expression.into()
            || invocation == (BoundExecutionSite::CallInput { expression, ordinal }))
    }

    pub(in crate::analysis::guarantee) fn retire_failed_call(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
    ) {
        let Some(entry) = state.entries.get(&invocation) else {
            return;
        };

        // Retire private input evidence when execution produces no normal result.
        // Retained caller values remain distinct from the callee's owned inputs.
        let consumed = entry.retiring_witness_values().collect::<Vec<_>>();
        let retained = self.surviving_call_values(state, invocation, entry);

        let private = state.current.iter()
            .filter(|(place, _)| self.is_invocation_input_place(invocation, place))
            .map(|(place, _)| place.clone()).collect::<Vec<_>>();

        for place in private {
            state.current.retire(&place);
        }

        if let Some(place) = self.invocation_result_place(invocation) {
            state.current.retire(&place);
        }

        state.entries.remove(&invocation);
        state.retire_trusted_values(consumed, retained);
    }

    fn retire_owned_alternative(
        &self,
        state: &mut super::super::state::ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        result: &ExecutionCondition,
        outside: &std::collections::BTreeSet<ExecutionCondition>,
    ) {
        let consumed = entry.retiring_witness_values().collect::<Vec<_>>();

        if consumed.is_empty() {
            return;
        }

        let retained = state
            .witness_values(result)
            .chain(outside.iter().cloned())
            .collect::<crate::execution_guarantees::RetainedSnapshots>();

        state.retire_trusted_values(consumed, retained);
    }

    pub(in crate::analysis::guarantee) fn preserved_borrowed_inputs<'s>(
        &'s self,
        references: &'s [bray_bound_tree::BoundCallReference],
        arguments: &'s BTreeMap<ExecutionPlace, ExecutionCondition>,
        ordinal: impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal> + 's,
    ) -> impl Iterator<Item = ExecutionCondition> + 's {
        references.iter()
            .filter_map(move |reference| {
                let relation = reference.relation();

                if !reference.preserves_value()
                    || relation.kind() != DependencyReferenceKind::Borrow
                    || relation.entry() != Some(relation.exit())
                    || !relation.is_exact()
                    || relation.dereferences() != 0
                    || !relation.target_projections().is_empty()
                {
                    return None;
                }

                let subject = relation.exit();

                let ordinal = ordinal(subject.subject_root())?;

                // A checked preserved reference names caller-owned storage. Consuming
                // its containing value retires the descriptor, not the referent.
                project_place(self.request.semantic_values(), self.dependencies, ExecutionPlace::argument(ordinal), subject)?.value_in(arguments)
            })
    }

    fn complete_owned_result(
        &self,
        invocation: BoundExecutionSite,
        state: &mut super::super::state::ExecutionState,
        mut result: ExecutionCondition,
        entry: &crate::ExecutionCallEvidence,
        returned: &ReturnedValues,
        ownership: &bray_symbols::DependencyOwnedResult,
        outside: &std::collections::BTreeSet<ExecutionCondition>,
    ) -> Option<ExecutionCondition> {
        use bray_symbols::DependencyOwnedResult;

        match ownership {
            DependencyOwnedResult::Case {
                condition,
                result: layout,
            } => {
                let condition = entry.bind_term(self.request.semantic_values(), *condition, &result);

                let selected = state.assumptions.joined_with(&entry.assumptions);

                if condition.prove_ordinary(&selected, Some(&state.proofs)) == Some(false) {
                    return None;
                }

                condition.assume(true, &mut state.assumptions);

                self.complete_owned_result(invocation, state, result, entry, returned, layout, outside)
            }
            DependencyOwnedResult::Transfer(transfer) => {
                let Some(returned) = returned.get(transfer) else { return Some(result); };

                if !returned.source.is_reference() && !returned.value.is_reference() {
                    let preservation = crate::storage::owned_storage_preservation(self.dependencies, transfer.preservation());

                    state.preserve_owned_result(&returned.source, &returned.value, returned.extracted,
                        |path| preservation.subtree(path).into_owned());
                }

                self.refine_witnesses(state);

                Some(result.with_storage_component(&returned.path, returned.value.clone()))
            }
            DependencyOwnedResult::All(values) => {
                // Install containing owners first so explicit surviving child
                // transfers refine their data rather than being overwritten.
                let mut ordered = values.iter().enumerate().collect::<Vec<_>>();

                ordered.sort_by_key(|(_, value)| {
                    value.transfers().map(|transfer| transfer.output().len()).min().unwrap_or(usize::MAX)
                });

                for (index, value) in ordered {
                    let mut retained = outside.clone();

                    retained.extend(
                        values
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .flat_map(|(_, sibling)| sibling.transfers())
                            .filter_map(|transfer| returned.get(transfer))
                            .flat_map(|returned| state.witness_values(&returned.value)),
                    );

                    result = self.complete_owned_result(
                        invocation, state, result, entry, returned, value, &retained,
                    )?;
                }

                Some(result)
            }
            DependencyOwnedResult::Choice(values) => {
                let baseline = state.clone();
                let root = result;
                let mut joined = None;

                for value in values.iter() {
                    let mut branch = baseline.clone();

                    let Some(value) = self.complete_owned_result(
                        invocation,
                        &mut branch,
                        root.clone(),
                        entry,
                        returned,
                        value,
                        outside,
                    ) else {
                        continue;
                    };

                    self.retire_owned_alternative(
                        &mut branch,
                        entry,
                        &value,
                        outside,
                    );

                    match &mut joined {
                        None => joined = Some((branch, value)),
                        Some((previous, previous_value)) => {
                            *previous_value = super::super::flow::merge_returned(previous, &branch, previous_value, &value, &root, match (root.owner_identity()).kind() { crate::ExecutionConditionKind::Expression(expression) => *expression, _ => invocation.expression().expect("normal result alternative retains its invocation expression") });
                        }
                    }
                }

                let (completed, result) = joined?;

                *state = completed;

                Some(result)
            }
        }
    }

    pub(in crate::analysis::guarantee) fn complete_transferred_ownership(
        &self, state: &mut super::super::state::ExecutionState, invocation: BoundExecutionSite,
        storage: Option<&bray_bound_tree::BoundCallStorage>, entry: &crate::ExecutionCallEvidence,
        result: ExecutionCondition,
        (ownership, preserved): (&bray_symbols::DependencyOwnedResult, impl Iterator<Item = ExecutionCondition>),
    ) -> ExecutionCondition {
        let returned = self.transferred_values(state, entry, &result, ownership, storage);

        if !entry.witness_inputs.is_empty() {
            let retained = state.witness_snapshots(returned.values()
                .filter(|returned| !returned.value.is_reference() && !returned.source.is_reference())
                .map(|returned| returned.value.clone()));

            state.resume_owned_witnesses(entry, &retained);
        }

        self.guard_returned_inputs(state, entry, &returned, &result, ownership);

        let mut outside = self.surviving_call_values(state, invocation, entry);

        outside.extend(preserved);

        let result = self.complete_owned_result(invocation, state, result, entry, &returned, ownership, &outside)
            .expect("completed checked call has a feasible ownership layout");

        self.retire_owned_alternative(state, entry, &result, &outside);

        result
    }
}

pub(in crate::analysis::guarantee) fn project_place(
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    place: ExecutionPlace,
    subject: &DependencySubject,
) -> Option<ExecutionPlace> {
    place.project(&crate::storage::storage_projections(values, dependencies, subject.projections())?)
}
