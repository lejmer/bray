use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, CheckedDependencyContracts, CheckedSemanticSelections,
    DependencyContractInstantiationError, SemanticSelection, StorageFlow, StoragePlan,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CallableSignatureQuery;

use super::call::{selected_call_contracts, selected_iteration_contract};
use super::operation::{operation_access_requirements, operation_requirements};
use crate::unit::storage_flow_input_failure;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerStorageFlowFailure, CheckerUnitView, StorageFlowInputKind,
};

pub(crate) fn check_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    flow: &StorageFlow,
) -> CheckerOutcome<CheckedDependencyContracts, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if let Some(error) = storage_flow_input_failure(
        request,
        [
            (
                StorageFlowInputKind::SemanticSelections,
                (selections.unit(), selections.kind()),
            ),
            (
                StorageFlowInputKind::StoragePlan,
                (storage.unit(), storage.kind()),
            ),
            (
                StorageFlowInputKind::StorageFlow,
                (flow.unit(), flow.kind()),
            ),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

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
            .entry(operation.expression())
            .or_insert_with(Vec::new)
            .extend(requirements.iter().cloned());

        access_requirements
            .entry(operation.access())
            .or_insert_with(Vec::new)
            .extend(requirements);
    }

    for entry in selections.entries() {
        let contract = match entry.selection() {
            SemanticSelection::Call(call) => {
                match selected_call_contracts(request, storage, entry.expression(), call) {
                    Ok(contracts) => {
                        if let Some(deferred) = contracts.deferred() {
                            deferred_expression_requirements
                                .entry(entry.expression())
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
            SemanticSelection::Reference(_)
            | SemanticSelection::CallableReference(_)
            | SemanticSelection::StaticReference(_)
            | SemanticSelection::Predicate(_)
            | SemanticSelection::Operation(_)
            | SemanticSelection::Propagation(_) => continue,
        };

        match contract {
            Ok(contract) => expression_requirements
                .entry(entry.expression())
                .or_insert_with(Vec::new)
                .extend(contract.requirements().iter().cloned()),
            Err(DependencyContractInstantiationError::Resolution(
                CheckerQueryError::Infrastructure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ),
            )) => is_recovered = true,
            Err(DependencyContractInstantiationError::Resolution(CheckerQueryError::Cancelled)) => {
                return CheckerOutcome::Cancelled;
            }
            Err(DependencyContractInstantiationError::Resolution(
                CheckerQueryError::Infrastructure(error),
            )) => return CheckerOutcome::InfrastructureFailure(error),
            Err(DependencyContractInstantiationError::Resolution(CheckerQueryError::Upstream(
                error,
            ))) => return CheckerOutcome::UpstreamFailure(error),
            Err(DependencyContractInstantiationError::ForeignUnit) => {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::StorageFlow(
                        CheckerStorageFlowFailure::ForeignDependencyContract,
                    ),
                );
            }
        }
    }

    for (expression, node) in request.unit().tree().expressions() {
        let child_requirements = node
            .child_expressions()
            .flat_map(|child| {
                expression_requirements
                    .get(&child)
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect::<Vec<_>>();

        expression_requirements
            .entry(expression)
            .or_default()
            .extend(child_requirements);

        let child_deferred_requirements = node
            .child_expressions()
            .flat_map(|child| {
                deferred_expression_requirements
                    .get(&child)
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect::<Vec<_>>();

        deferred_expression_requirements
            .entry(expression)
            .or_default()
            .extend(child_deferred_requirements);
    }

    let expressions = request.unit().tree().expressions().map(|(expression, _)| {
        let requirements = expression_requirements
            .get(&expression)
            .cloned()
            .unwrap_or_default();

        (expression, BoundDependencyContract::new(requirements))
    });

    let deferred_expressions = deferred_expression_requirements
        .into_iter()
        .map(|(expression, requirements)| (expression, BoundDependencyContract::new(requirements)));

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
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::StorageFlow(
                    CheckerStorageFlowFailure::DependencyContractsConstruction(error),
                ),
            );
        }
    };

    CheckerOutcome::Complete(DiagnosticResult::new(contracts, DiagnosticBag::new()))
}
