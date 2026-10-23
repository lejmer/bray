use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, CheckedDependencyContracts, CheckedSemanticSelections,
    DependencyContractInstantiationError, SemanticSelection, StoragePlan,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::CallableSignatureQuery;

use super::call::{selected_call_contracts, selected_implicit_contracts};
use super::operation::{operation_access_requirements, operation_requirements};
use crate::unit::assert_unit_inputs;
use crate::{
    CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerUnitView,
};

// rust-style: allow(function-too-large, reason = "dependency collection publishes source access, invocation, and lifetime requirements as one coherent contract graph")
pub(crate) fn check_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    selections: &CheckedSemanticSelections,
    value_inputs: &crate::dependency::ValueInputs,
    storage: &StoragePlan,
) -> CheckerOutcome<CheckedDependencyContracts, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    assert_unit_inputs(
        request,
        [
            (
                "prepared analysis",
                (prepared.graph.unit(), prepared.kind()),
            ),
            (
                "semantic selections",
                (selections.unit(), selections.kind()),
            ),
            ("storage plan", (storage.unit(), storage.kind())),
        ],
    );

    let mut diagnostics = DiagnosticBag::new();
    let mut expression_requirements = BTreeMap::new();
    let mut calls = BTreeMap::new();
    let mut deferred_expression_requirements = BTreeMap::new();
    let mut access_requirements = BTreeMap::new();
    let mut is_recovered = storage.is_recovered();

    for operation in storage.access_plans().iter().copied() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        if storage
            .access(operation.access())
            .expect("planned operation retains its access")
            .is_recovered()
        {
            continue;
        }

        let requirements = operation_requirements(storage, operation);

        let occurrence = match operation.occurrence() {
            scoped @ (bray_bound_tree::BoundExecutionSite::Iterate(_)
            | bray_bound_tree::BoundExecutionSite::Next(_)
            | bray_bound_tree::BoundExecutionSite::CallInput { .. }
            | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
            | bray_bound_tree::BoundExecutionSite::ScopedExit(_)) => scoped,
            bray_bound_tree::BoundExecutionSite::Node(_) => operation.expression().into(),
            bray_bound_tree::BoundExecutionSite::Cleanup(_) => {
                panic!("cleanup suspension is not a source access operation")
            }
        };

        expression_requirements
            .entry(occurrence)
            .or_insert_with(Vec::new)
            .extend(requirements.iter().cloned());

        access_requirements
            .entry(operation.access())
            .or_insert_with(Vec::new)
            .extend(requirements);
    }

    for entry in selections.entries() {
        let contract = match entry.selection() {
            SemanticSelection::Call(_) | SemanticSelection::Operation(_) => {
                let selected = match entry.selection() {
                    SemanticSelection::Call(_) => selected_call_contracts(
                        request,
                        storage,
                        entry.expression(),
                        entry.selection(),
                        value_inputs,
                    )
                    .map(Some),
                    SemanticSelection::Operation(operation) => {
                        super::call::selected_operation_contracts(
                            request,
                            storage,
                            entry.expression(),
                            operation,
                            value_inputs,
                        )
                    }
                    _ => {
                        unreachable!("invocation contract dispatch retains its selected operation")
                    }
                };

                match selected {
                    Ok(Some((contracts, escaping))) => {
                        if entry.selection().evaluates_defaults() {
                            let inputs = match super::call::selected_input_contracts(
                                request,
                                storage,
                                entry.expression(),
                                entry.selection(),
                                value_inputs,
                            ) {
                                Ok(inputs) => inputs,
                                Err(error) => return failed_contract(error, entry.expression()),
                            };

                            for (site, input) in inputs {
                                expression_requirements
                                    .entry(site)
                                    .or_insert_with(Vec::new)
                                    .extend(input.invocation().requirements().iter().cloned());

                                calls.insert(site, input);
                            }
                        }

                        if let Some(root) = escaping.first().copied() {
                            diagnostics.add(super::defaults::escaping_evaluation_diagnostic(
                                request,
                                entry.expression(),
                                entry.selection(),
                                root,
                                crate::diagnostic::diagnostic_id(diagnostics.len()),
                            ));

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

                        // Bound contracts share immutable requirement arrays across publications.
                        let invocation = contracts.invocation().clone();

                        calls.insert(entry.expression().into(), contracts);

                        Ok(invocation)
                    }
                    Ok(None) => continue,
                    Err(error) => Err(error),
                }
            }
            SemanticSelection::Iteration(_) | SemanticSelection::ScopedUse(_) => {
                let sites = match entry.selection() {
                    SemanticSelection::Iteration(_) => [
                        bray_bound_tree::BoundExecutionSite::Iterate(entry.expression()),
                        bray_bound_tree::BoundExecutionSite::Next(entry.expression()),
                    ],
                    SemanticSelection::ScopedUse(_) => [
                        bray_bound_tree::BoundExecutionSite::ScopedEnter(entry.expression()),
                        bray_bound_tree::BoundExecutionSite::ScopedExit(entry.expression()),
                    ],
                    _ => unreachable!("implicit invocation selection was matched above"),
                };

                for occurrence in sites {
                    match selected_implicit_contracts(
                        request,
                        storage,
                        entry.selection(),
                        occurrence,
                        value_inputs,
                    ) {
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

                            calls.insert(occurrence, contracts);
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
            | SemanticSelection::Propagation(_) => continue,
        };

        match contract {
            Ok(contract) => expression_requirements
                .entry(bray_bound_tree::BoundExecutionSite::from(
                    entry.expression(),
                ))
                .or_insert_with(Vec::new)
                .extend(contract.requirements().iter().cloned()),
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

    let borrows = borrow_requirements(storage, &access_requirements);

    let accesses = storage.access_entries().map(|(access, _)| {
        let requirements = access_requirements.remove(&access).unwrap_or_default();

        (access, BoundDependencyContract::new(requirements))
    });

    let lifecycle_dependencies = match prepared.cleanup.check_dependencies(request) {
        Ok(references) => references,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    };

    let contracts = CheckedDependencyContracts::try_new(
        request.unit(),
        storage,
        expressions,
        deferred_expressions,
        accesses,
        borrows,
        is_recovered,
    )
    .and_then(|contracts| contracts.try_with_calls(calls))
    .expect(
        "checked source access plans and selected invocations define valid dependency contracts",
    )
    .with_lifecycle_dependencies(lifecycle_dependencies);

    let prepared = prepare_static_carriers(request, contracts).and_then(|contracts| {
        let variants = super::projection::prepare_variants(
            request,
            &contracts,
            storage,
            value_inputs,
            selections,
        )?;

        Ok(contracts.with_projection_variants(variants))
    });

    match prepared {
        Ok(contracts) => CheckerOutcome::Complete(DiagnosticResult::new(contracts, diagnostics)),
        Err(CheckerQueryError::Cancelled) => CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => CheckerOutcome::UpstreamFailure(error),
    }
}

fn borrow_requirements(
    storage: &StoragePlan,
    access_requirements: &BTreeMap<
        bray_bound_tree::StorageAccessId,
        Vec<BoundDependencyRequirement>,
    >,
) -> Vec<(bray_bound_tree::BorrowCapabilityId, BoundDependencyContract)> {
    storage
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
        .collect::<Vec<_>>()
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
        DependencyContractInstantiationError::Resolution(CheckerQueryError::Upstream(error)) => {
            CheckerOutcome::UpstreamFailure(error)
        }
        DependencyContractInstantiationError::UnresolvedWitness => {
            panic!(
                "A call reached concrete dependency checking without its selected witness. in failed_contract, expression: {:?}",
                expression
            )
        }
        DependencyContractInstantiationError::ForeignUnit => {
            panic!(
                "A selected call or iteration produced a dependency contract for another source body. in failed_contract"
            )
        }
    }
}

fn prepare_static_carriers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    contracts: CheckedDependencyContracts,
) -> Result<CheckedDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let mut static_carriers = BTreeMap::new();

    for selection in request
        .trusted_contracts()
        .into_iter()
        .flat_map(|inputs| &inputs.statics)
    {
        let template = request.static_result_dependencies(selection.template().declaration())?;

        let values = request.semantic_values();

        let template = values
            .substitute_dependency_contract(template, selection.substitution())
            .expect("selected static substitution belongs to the same semantic store");

        let template = values.dependency_contract_template_data(template);

        static_carriers.insert(selection.clone(), template.carriers().into());
    }

    Ok(contracts.with_static_carriers(static_carriers))
}
