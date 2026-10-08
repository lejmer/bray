use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, CheckedDependencyContracts, CheckedSemanticSelections,
    DependencyContractInstantiationError, SemanticSelection, StorageFlow, StoragePlan,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CallableSignatureQuery;

use super::call::{
    selected_call_contracts, selected_iteration_contract, selected_scoped_contracts,
};
use super::operation::{operation_access_requirements, operation_requirements};
use crate::unit::assert_unit_inputs;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerStorageFlowFailure, CheckerUnitView,
};

pub(crate) fn check_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    patterns: &bray_bound_tree::CheckedPatterns,
    storage: &StoragePlan,
    flow: &StorageFlow,
) -> CheckerOutcome<CheckedDependencyContracts, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    crate::unit::assert_unit_inputs(request, [("patterns", (patterns.unit(), patterns.kind()))]);

    assert_unit_inputs(
        request,
        [
            (
                "semantic selections",
                (selections.unit(), selections.kind()),
            ),
            ("storage plan", (storage.unit(), storage.kind())),
            ("storage flow", (flow.unit(), flow.kind())),
        ],
    );

    let mut diagnostics = DiagnosticBag::new();
    let mut expression_requirements = BTreeMap::new();
    let mut deferred_expression_requirements = BTreeMap::new();
    let mut access_requirements = BTreeMap::new();
    let mut is_recovered = flow.is_recovered();

    for operation in flow.operations() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        match operation.status() {
            bray_bound_tree::StorageOperationStatus::Unreachable => continue,
            bray_bound_tree::StorageOperationStatus::Valid => {}
            _ => {
                is_recovered = true;

                continue;
            }
        }

        let requirements = operation_requirements(storage, operation);

        expression_requirements
            .entry(bray_bound_tree::BoundExecutionSite::from(
                operation.expression(),
            ))
            .or_insert_with(Vec::new)
            .extend(requirements.iter().cloned());

        access_requirements
            .entry(operation.access())
            .or_insert_with(Vec::new)
            .extend(requirements);
    }

    let value_inputs = crate::dependency::ValueInputs::new(request.unit(), selections, patterns);

    for entry in selections.entries() {
        let contract = match entry.selection() {
            SemanticSelection::Call(call) => {
                match selected_call_contracts(
                    request,
                    storage,
                    entry.expression(),
                    call,
                    &value_inputs,
                ) {
                    Ok(contracts) => {
                        if let Some(root) = contracts.escaping_evaluation_inputs.first().copied() {
                            let diagnostic = super::defaults::escaping_evaluation_diagnostic(
                                request,
                                entry.expression(),
                                call,
                                root,
                                crate::diagnostic::diagnostic_id(diagnostics.len()),
                            );

                            match diagnostic {
                                Ok(diagnostic) => diagnostics.add(diagnostic),
                                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                            }

                            is_recovered = true;
                        }

                        if let Some(deferred) = contracts.deferred() {
                            deferred_expression_requirements
                                .entry(bray_bound_tree::BoundExecutionSite::from(
                                    entry.expression(),
                                ))
                                .or_insert_with(Vec::new)
                                .extend(deferred.requirements().iter().cloned());
                        }

                        Ok(contracts.invocation().clone())
                    }
                    Err(error) => Err(error),
                }
            }
            SemanticSelection::Iteration(iteration) => {
                selected_iteration_contract(request, storage, iteration)
            }
            SemanticSelection::ScopedUse(scoped) => {
                for occurrence in [
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(entry.expression()),
                    bray_bound_tree::BoundExecutionSite::ScopedExit(entry.expression()),
                ] {
                    match selected_scoped_contracts(request, storage, scoped, occurrence) {
                        Ok(contracts) => {
                            expression_requirements
                                .entry(occurrence)
                                .or_insert_with(Vec::new)
                                .extend(contracts.invocation().requirements().iter().cloned());

                            if let Some(deferred) = contracts.deferred() {
                                deferred_expression_requirements
                                    .entry(occurrence)
                                    .or_insert_with(Vec::new)
                                    .extend(deferred.requirements().iter().cloned());
                            }
                        }
                        Err(error) => return failed_contract(error, entry.expression()),
                    }
                }

                continue;
            }
            SemanticSelection::Reference(_)
            | SemanticSelection::CallableReference(_)
            | SemanticSelection::StaticReference(_)
            | SemanticSelection::Predicate(_)
            | SemanticSelection::Operation(_)
            | SemanticSelection::Propagation(_) => continue,
        };

        match contract {
            Ok(contract) => expression_requirements
                .entry(bray_bound_tree::BoundExecutionSite::from(
                    entry.expression(),
                ))
                .or_insert_with(Vec::new)
                .extend(contract.requirements().iter().cloned()),
            Err(DependencyContractInstantiationError::Resolution(
                CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ),
            )) => is_recovered = true,
            Err(error) => return failed_contract(error, entry.expression()),
        }
    }

    inherit_child_requirements(request, &mut expression_requirements);
    inherit_child_requirements(request, &mut deferred_expression_requirements);

    for (expression, _) in request.unit().tree().expressions() {
        expression_requirements
            .entry(expression.into())
            .or_default();
    }

    let expressions = expression_requirements
        .into_iter()
        .map(|(occurrence, requirements)| (occurrence, BoundDependencyContract::new(requirements)));

    let deferred_expressions = deferred_expression_requirements
        .into_iter()
        .map(|(occurrence, requirements)| (occurrence, BoundDependencyContract::new(requirements)));

    let borrows = storage
        .borrow_capability_entries()
        .map(|(borrow, capability)| {
            let mut requirements = access_requirements
                .get(&capability.access())
                .cloned()
                .unwrap_or_else(|| operation_access_requirements(storage, capability.access()));

            requirements.push(BoundDependencyRequirement::direct(
                BoundDependencySubject::BorrowCapability(borrow),
                BoundDependencyRequirementKind::BorrowCapabilityActive(capability.kind()),
            ));

            (borrow, BoundDependencyContract::new(requirements))
        })
        .collect::<Vec<_>>();

    let accesses = storage.access_entries().map(|(access, _)| {
        let requirements = access_requirements.remove(&access).unwrap_or_default();

        (access, BoundDependencyContract::new(requirements))
    });

    let contracts = match CheckedDependencyContracts::try_new(
        request.unit(),
        storage,
        expressions,
        deferred_expressions,
        accesses,
        borrows,
        is_recovered,
    ) {
        Ok(contracts) => contracts,
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::DependencyContractsConstruction(error),
            ));
        }
    };

    CheckerOutcome::Complete(DiagnosticResult::new(contracts, diagnostics))
}

fn inherit_child_requirements<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression_requirements: &mut BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        Vec<BoundDependencyRequirement>,
    >,
) {
    for (expression, node) in request.unit().tree().expressions() {
        let child_requirements = node
            .child_expressions()
            .flat_map(|child| {
                expression_requirements
                    .get(&child.into())
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect::<Vec<_>>();

        expression_requirements
            .entry(expression.into())
            .or_default()
            .extend(child_requirements);
    }
}

fn failed_contract<E, T>(
    error: DependencyContractInstantiationError<CheckerQueryError<E>>,
    expression: bray_bound_tree::BoundExpressionId,
) -> CheckerOutcome<T, E> {
    match error {
        DependencyContractInstantiationError::Resolution(CheckerQueryError::Cancelled) => {
            CheckerOutcome::Cancelled
        }
        DependencyContractInstantiationError::Resolution(CheckerQueryError::Infrastructure(
            error,
        )) => CheckerOutcome::InfrastructureFailure(error),
        DependencyContractInstantiationError::Resolution(CheckerQueryError::Upstream(error)) => {
            CheckerOutcome::UpstreamFailure(error)
        }
        DependencyContractInstantiationError::UnresolvedWitness => {
            // rust-style: allow(context-erasing-failure-conversion, reason = "unresolved witness error has no payload and its exact expression is retained")
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::UnresolvedDependencyWitness { expression },
            ))
        }
        DependencyContractInstantiationError::ForeignUnit => {
            // rust-style: allow(context-erasing-failure-conversion, reason = "foreign unit error has no payload and its exact cause is retained")
            CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::ForeignDependencyContract,
            ))
        }
    }
}
