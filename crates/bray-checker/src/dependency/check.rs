use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, CheckedDependencyContracts, CheckedSemanticSelections,
    DependencyContractInstantiationError, SemanticSelection, StorageFlowFacts, StoragePlan,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CallableSignatureFact;

use super::call::selected_call_contract;
use super::operation::{operation_access_requirements, operation_requirements};
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerSemanticFactProvider,
    CheckerUnitView,
};

pub(crate) fn check_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    flow: &StorageFlowFacts,
) -> CheckerOutcome<CheckedDependencyContracts>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if selections.unit() != request.unit().unit()
        || selections.kind() != request.unit().key().kind()
        || storage.unit() != request.unit().unit()
        || storage.kind() != request.unit().key().kind()
        || flow.unit() != request.unit().unit()
        || flow.kind() != request.unit().key().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        );
    }

    let mut expression_requirements = BTreeMap::new();
    let mut access_requirements = BTreeMap::new();
    let mut is_recovered = flow.is_recovered();

    for operation in flow.operations() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        if operation.status() != bray_bound_tree::StorageOperationStatus::Valid {
            is_recovered = true;

            continue;
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
        let SemanticSelection::Call(call) = entry.selection() else {
            continue;
        };

        match selected_call_contract(request, storage, entry.expression(), call) {
            Ok(contract) => expression_requirements
                .entry(entry.expression())
                .or_insert_with(Vec::new)
                .extend(contract.requirements().iter().cloned()),
            Err(DependencyContractInstantiationError::Resolution(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )) => is_recovered = true,
            Err(DependencyContractInstantiationError::Resolution(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            Err(DependencyContractInstantiationError::ForeignUnit) => {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidStorageFlowFacts,
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
    }

    let expressions = request.unit().tree().expressions().map(|(expression, _)| {
        let requirements = expression_requirements
            .get(&expression)
            .cloned()
            .unwrap_or_default();

        (expression, BoundDependencyContract::new(requirements))
    });

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
        accesses,
        borrows,
        is_recovered,
    ) {
        Ok(contracts) => contracts,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlowFacts,
            );
        }
    };

    CheckerOutcome::Complete(DiagnosticResult::new(contracts, DiagnosticBag::new()))
}
