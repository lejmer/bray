use bray_bound_tree::{
    BoundCallableTarget, BoundDependencyContract, BoundExpressionId,
    DependencyContractInstantiationError, SelectedCall, SelectedIterationSource, StorageIdentity,
    StoragePlan,
};
use bray_symbols::{
    BorrowKind, CallableDependencyContracts, CallableInstanceData, CallableSignatureQuery,
    DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
    DependencySubject, DependencySubjectRoot, SymbolQueryRequest, TypeData, TypeExpressionTemplate,
};

use super::instantiation::{CallInstantiationContext, expression_access, identity_access};
use crate::dependency::implementation::implementation_dependency_source;
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

pub(crate) struct InstantiatedCallContracts {
    invocation: BoundDependencyContract,
    result: BoundDependencyContract,
    deferred: Option<BoundDependencyContract>,
    pub(crate) escaping_evaluation_inputs: Vec<DependencySubjectRoot>,
}

impl InstantiatedCallContracts {
    pub(crate) const fn result(&self) -> &BoundDependencyContract {
        &self.result
    }

    pub(crate) const fn invocation(&self) -> &BoundDependencyContract {
        &self.invocation
    }

    pub(crate) const fn deferred(&self) -> Option<&BoundDependencyContract> {
        self.deferred.as_ref()
    }
}

pub(crate) fn selected_call_contracts<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    expression: BoundExpressionId,
    call: &SelectedCall,
    values: &crate::dependency::ValueInputs,
) -> Result<
    InstantiatedCallContracts,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let contracts = callable_dependency_contracts(request, call.target())
        .map_err(DependencyContractInstantiationError::Resolution)?;

    let contracts =
        callable_dependencies_for_implementation(request, contracts, call.implementation_hook())
            .map_err(DependencyContractInstantiationError::Resolution)?;

    let mut context = CallInstantiationContext::new(request, storage, expression, call);

    let mut instantiated = instantiate_callable_contracts(request, contracts, &mut context)?;

    let template = crate::dependency::call_result_template(request, call, |callable| {
        request.context().callable_result_dependencies(callable)
    })
    .map_err(DependencyContractInstantiationError::Resolution)?;

    let mut result_context = CallInstantiationContext::new(request, storage, expression, call);
    result_context.set_result_values(values);

    let concrete = DependencyContractTemplateData::new(
        template
            .template
            .requirements()
            .iter()
            .filter(|requirement| {
                !matches!(
                    requirement,
                    DependencyRequirement::ResultCall { .. }
                        | DependencyRequirement::FixedPoint { .. }
                        | DependencyRequirement::Variable { .. }
                )
            })
            .cloned(),
    );

    instantiated.result = BoundDependencyContract::try_instantiate(&concrete, &mut result_context)
        .map_err(|error| error.map_resolution(CheckerQueryError::Infrastructure))?;

    instantiated.escaping_evaluation_inputs = template.escaping_evaluation_inputs;

    if crate::dependency::opaque_result(call)
        && let Some(bray_bound_tree::BoundExpression::Call(bound)) =
            request.view().expression(expression)
        && let Some(access) = expression_access(storage, bound.callee())
    {
        instantiated.result =
            BoundDependencyContract::new(instantiated.result.requirements().iter().cloned().chain(
                [bray_bound_tree::BoundDependencyRequirement::Direct {
                    subject: bray_bound_tree::BoundDependencySubject::StorageAccess(access),
                    kind: bray_bound_tree::BoundDependencyRequirementKind::ValueDependencies,
                }],
            ));
    }

    Ok(instantiated)
}

fn callable_dependencies_for_implementation<C>(
    request: CheckerUnitView<'_, C>,
    contracts: CallableDependencyContracts,
    implementation: Option<bray_compiler_known::ImplementationHook>,
) -> Result<CallableDependencyContracts, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(implementation) = implementation else {
        return Ok(contracts);
    };

    let Some((parameter, authority)) = implementation_dependency_source(implementation) else {
        return Ok(contracts);
    };

    let existing = request
        .semantic_values()
        .dependency_contract_template_data(contracts.invocation())
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let source = || DependencySubject::root(DependencySubjectRoot::Parameter(parameter));

    let mut dependencies = vec![DependencyRequirement::direct(
        source(),
        DependencyRequirementKind::StorageAlive,
    )];

    if let Some(kind) = authority {
        dependencies.push(DependencyRequirement::direct(
            source(),
            DependencyRequirementKind::StorageInitialized,
        ));

        dependencies.push(DependencyRequirement::direct(
            source(),
            DependencyRequirementKind::BorrowCapabilityActive(kind),
        ));

        if kind == BorrowKind::Mutable {
            dependencies.push(DependencyRequirement::direct(
                source(),
                DependencyRequirementKind::ExclusiveMutationAuthority,
            ));
        }
    }

    let invocation = request
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new(
            existing.requirements().iter().cloned().chain(dependencies),
        ))
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(match contracts.deferred_execution() {
        Some(deferred) => CallableDependencyContracts::asynchronous(invocation, deferred),
        None => CallableDependencyContracts::synchronous(invocation),
    })
}

pub(in crate::dependency) fn selected_iteration_contract<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    selection: &SelectedIterationSource,
) -> Result<
    BoundDependencyContract,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let source = expression_access(storage, selection.source()).ok_or(
        DependencyContractInstantiationError::Resolution(
            CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
        ),
    )?;

    let cursor = identity_access(
        storage,
        StorageIdentity::IterationCursor(selection.expression()),
    )
    .ok_or(DependencyContractInstantiationError::Resolution(
        CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
    ))?;

    let element = identity_access(
        storage,
        StorageIdentity::IterationElement(selection.expression()),
    )
    .ok_or(DependencyContractInstantiationError::Resolution(
        CheckerInfrastructureError::InvalidSemanticSelectionInput.into(),
    ))?;

    let iterate =
        instantiate_hidden_iteration_call(request, storage, selection.iterate(), source, cursor)?;

    let next =
        instantiate_hidden_iteration_call(request, storage, selection.next(), cursor, element)?;

    Ok(BoundDependencyContract::new(
        iterate
            .requirements()
            .iter()
            .chain(next.requirements())
            .cloned(),
    ))
}

fn instantiate_hidden_iteration_call<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    callable: CallableInstanceData,
    receiver: bray_bound_tree::StorageAccessId,
    result: bray_bound_tree::StorageAccessId,
) -> Result<
    BoundDependencyContract,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let contracts =
        callable_dependency_contracts(request, BoundCallableTarget::Declaration(callable))
            .map_err(DependencyContractInstantiationError::Resolution)?;

    let mut context = CallInstantiationContext::hidden(request, storage, receiver, result);

    instantiate_callable_contracts(request, contracts, &mut context)
        .map(|contracts| contracts.invocation)
}

fn instantiate_callable_contracts<C>(
    request: CheckerUnitView<'_, C>,
    contracts: CallableDependencyContracts,
    context: &mut CallInstantiationContext<'_, C>,
) -> Result<
    InstantiatedCallContracts,
    DependencyContractInstantiationError<CheckerQueryError<C::UpstreamError>>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let invocation = request
        .semantic_values()
        .dependency_contract_template_data(contracts.invocation())
        .map_err(|error| {
            DependencyContractInstantiationError::Resolution(
                CheckerInfrastructureError::SemanticValueStore(error).into(),
            )
        })?;

    let invocation = BoundDependencyContract::try_instantiate(&invocation, context)
        .map_err(|error| error.map_resolution(CheckerQueryError::Infrastructure))?;

    let deferred = contracts
        .deferred_execution()
        .map(|deferred| {
            context.begin_deferred_execution();

            let deferred = request
                .semantic_values()
                .dependency_contract_template_data(deferred)
                .map_err(|error| {
                    DependencyContractInstantiationError::Resolution(
                        CheckerInfrastructureError::SemanticValueStore(error).into(),
                    )
                })?;

            BoundDependencyContract::try_instantiate(&deferred, context)
                .map_err(|error| error.map_resolution(CheckerQueryError::Infrastructure))
        })
        .transpose()?;

    Ok(InstantiatedCallContracts {
        escaping_evaluation_inputs: Vec::new(),
        invocation,
        result: BoundDependencyContract::new([]),
        deferred,
    })
}

fn callable_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    target: BoundCallableTarget,
) -> Result<CallableDependencyContracts, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    match target {
        BoundCallableTarget::Declaration(instance) => {
            let signature =
                request.resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                    instance.definition().callable_symbol(),
                ))?;

            let dependencies =
                callable_type_dependencies(request, signature.value().callable_type())?;

            let invocation = request
                .semantic_values()
                .substitute_dependency_contract(dependencies.invocation(), instance.substitution())
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let deferred = dependencies
                .deferred_execution()
                .map(|contract| {
                    request
                        .semantic_values()
                        .substitute_dependency_contract(contract, instance.substitution())
                })
                .transpose()
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            Ok(match deferred {
                Some(deferred) => CallableDependencyContracts::asynchronous(invocation, deferred),
                None => CallableDependencyContracts::synchronous(invocation),
            })
        }
        BoundCallableTarget::Indirect(ty) => {
            let data = request
                .semantic_values()
                .type_data(ty)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            };

            Ok(callable.dependency_contracts())
        }
        BoundCallableTarget::Predicate(_) | BoundCallableTarget::Anonymous(_) => request
            .semantic_values()
            .empty_dependency_contract_template()
            .map(CallableDependencyContracts::synchronous)
            .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into()),
    }
}

fn callable_type_dependencies<C>(
    request: CheckerUnitView<'_, C>,
    callable: &TypeExpressionTemplate,
) -> Result<CallableDependencyContracts, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    match callable {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.dependencies()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = request
                .semantic_values()
                .type_data(*ty)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            };

            Ok(callable.dependency_contracts())
        }
        TypeExpressionTemplate::CallableContract { target, .. } => {
            callable_type_dependencies(request, target)
        }
        TypeExpressionTemplate::Named { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. }
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::FlexibleArray(_)
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::TraitView(_)
        | TypeExpressionTemplate::OwnedIndirection { .. } => {
            Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundCallResult, BoundCallableTarget, BoundDependencyContract,
        BoundDependencyGuard, BoundDependencyRequirement, BoundDependencyRequirementKind,
        BoundDependencySubject, BoundErrorExpression, BoundExpression, BoundFutureConstruction,
        BoundResolvedCall, BoundUnitId, PlannedBorrowCapability, SelectedArgument, SelectedCall,
        StorageAccess, StorageAccessId, StorageAccessRoot, StorageIdentity, StorageIdentityId,
        StoragePlanBuilder, StorageProjection,
    };
    use bray_compiler_known::ImplementationHook;
    use bray_symbols::{
        BorrowKind, CallableAbi, CallableConstness, CallableDependencyContracts, CallableTrust,
        CallableTypeData, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
        SymbolOrdinal, TypeData, TypeId,
    };

    use super::{
        InstantiatedCallContracts, callable_dependencies_for_implementation,
        instantiate_callable_contracts, selected_call_contracts,
    };
    use crate::CheckerUnitView;
    use crate::dependency::call::instantiation::CallInstantiationContext;
    use crate::test_support::{
        TestCheckerContext, callable_entry, empty_callable_phase_behaviors, error_type,
        expression_unit, push_expression, semantic_values,
    };

    #[test]
    fn selected_calls_instantiate_direct_and_guarded_dependency_templates() {
        let unit_id = BoundUnitId::new(29);

        let (unit, argument, call_expression) = expression_pair(unit_id);

        let mut storage = StoragePlanBuilder::new(unit_id, unit.key().kind());

        let (identity, argument_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::Temporary(argument),
            unit.key().source(),
        );

        let nullable_value_access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [StorageProjection::NullableValue],
                error_type(),
                unit.key().source(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test nullable access must build: {error:?}"));

        let storage = storage.finish();

        let parameter =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));

        let nullable_value = DependencySubject::new(
            parameter.subject_root(),
            [DependencyProjection::NullableValue],
        );

        let template = semantic_values()
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::direct(
                    parameter.clone(),
                    DependencyRequirementKind::StorageAlive,
                ),
                DependencyRequirement::guarded(
                    DependencyGuard::NullablePresent(parameter),
                    [DependencyRequirement::direct(
                        nullable_value,
                        DependencyRequirementKind::StorageInitialized,
                    )],
                ),
            ]))
            .unwrap_or_else(|error| panic!("test dependency template must intern: {error:?}"));

        let callable_type =
            indirect_callable_type(CallableDependencyContracts::synchronous(template));

        let call = selected_indirect_call(
            callable_type,
            argument,
            BoundCallResult::Immediate(error_type()),
            error_type(),
        );

        let contracts = selected_contracts(&unit, &storage, call_expression, &call);
        let contract = contracts.invocation();

        assert_storage_requirement(
            contract,
            argument_access,
            BoundDependencyRequirementKind::StorageAlive,
        );

        assert!(contract.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                BoundDependencyRequirement::Guarded(guarded)
                    if guarded.guard()
                        == BoundDependencyGuard::NullablePresent(argument_access)
                        && guarded.requirements().iter().any(|nested| {
                            matches!(
                                nested,
                                BoundDependencyRequirement::Direct {
                                    subject: BoundDependencySubject::StorageAccess(access),
                                    kind: BoundDependencyRequirementKind::StorageInitialized,
                                } if *access == nullable_value_access
                            )
                        })
            )
        }));
    }

    #[test]
    fn borrowed_text_results_retain_their_source_dependency() {
        assert_result_source_dependency(ImplementationHook::StringUtf8);
    }

    #[test]
    fn trusted_raw_borrows_retain_owner_and_capability_dependencies() {
        assert_result_source_dependency(ImplementationHook::BorrowFrom);
        assert_result_source_dependency(ImplementationHook::BorrowMutFrom);
    }

    #[test]
    fn callable_address_conversions_retain_their_source_dependency() {
        assert_result_source_dependency(ImplementationHook::CallableFromPointer);
        assert_result_source_dependency(ImplementationHook::PointerFromCallable);
    }

    #[test]
    fn native_thread_start_retains_its_explicit_state() {
        let unit_id = BoundUnitId::new(33);

        let (unit, _, _) = expression_pair(unit_id);

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let empty = request
            .semantic_values()
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must intern: {error:?}"));

        let contracts = callable_dependencies_for_implementation(
            request,
            CallableDependencyContracts::synchronous(empty),
            Some(ImplementationHook::NativeThreadStart),
        )
        .unwrap_or_else(|error| panic!("native thread dependency must build: {error:?}"));

        let contract = request
            .semantic_values()
            .dependency_contract_template_data(contracts.invocation())
            .unwrap_or_else(|error| panic!("native thread dependency must be readable: {error:?}"));

        assert_eq!(
            contract.requirements(),
            &[DependencyRequirement::direct(
                DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(1))),
                DependencyRequirementKind::StorageAlive,
            )]
        );
    }

    #[test]
    fn deferred_argument_dependencies_follow_transferred_storage() {
        let unit_id = BoundUnitId::new(32);

        let (unit, argument, call_expression) = expression_pair(unit_id);

        let mut storage = StoragePlanBuilder::new(unit_id, unit.key().kind());

        let (_, argument_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::Temporary(argument),
            unit.key().source(),
        );

        let (_, future_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::Temporary(call_expression),
            unit.key().source(),
        );

        let storage = storage.finish();
        let values = semantic_values();

        let empty = values
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must intern: {error:?}"));

        let deferred = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(
                        0,
                    ))),
                    DependencyRequirementKind::StorageInitialized,
                ),
            ]))
            .unwrap_or_else(|error| panic!("deferred dependency template must intern: {error:?}"));

        let borrowed_type = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: error_type(),
            })
            .unwrap_or_else(|error| panic!("borrowed test type must intern: {error:?}"));

        for (target_type, expected_access) in [
            (error_type(), future_access),
            (borrowed_type, argument_access),
        ] {
            let callable_type =
                indirect_callable_type(CallableDependencyContracts::asynchronous(empty, deferred));

            let call = selected_indirect_call(
                callable_type,
                argument,
                BoundCallResult::LazyFuture(BoundFutureConstruction::new(
                    error_type(),
                    error_type(),
                )),
                target_type,
            );

            let contracts = selected_contracts(&unit, &storage, call_expression, &call);

            let contract = contracts
                .deferred()
                .unwrap_or_else(|| panic!("asynchronous call must retain a deferred contract"));

            assert_eq!(
                contract.requirements(),
                &[BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(expected_access),
                    kind: BoundDependencyRequirementKind::StorageInitialized,
                }]
            );
        }
    }

    fn assert_result_source_dependency(implementation: ImplementationHook) {
        let unit_id = BoundUnitId::new(31);

        let (unit, argument, call_expression) = expression_pair(unit_id);

        let mut storage = StoragePlanBuilder::new(unit_id, unit.key().kind());

        let (argument_identity, argument_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::Temporary(argument),
            unit.key().source(),
        );

        let capability_kind = match implementation {
            ImplementationHook::BorrowFrom => Some(BorrowKind::Shared),
            ImplementationHook::BorrowMutFrom => Some(BorrowKind::Mutable),
            _ => None,
        };

        let capability = capability_kind.map(|kind| {
            let equivalent_access = storage
                .push_access(StorageAccess::new(
                    StorageAccessRoot::Storage(argument_identity),
                    [],
                    error_type(),
                    unit.key().source(),
                    false,
                ))
                .unwrap_or_else(|error| panic!("equivalent owner access must build: {error:?}"));

            let id = storage
                .push_borrow_capability(PlannedBorrowCapability::new(
                    BorrowCapabilityOrigin::Expression(argument),
                    kind,
                    equivalent_access,
                    None,
                    unit.key().source(),
                    false,
                ))
                .unwrap_or_else(|error| panic!("owner borrow capability must build: {error:?}"));

            (id, kind)
        });

        let storage = storage.finish();

        let empty = semantic_values()
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must intern: {error:?}"));

        let callable_type = indirect_callable_type(CallableDependencyContracts::synchronous(empty));

        let call = selected_indirect_call(
            callable_type,
            argument,
            BoundCallResult::Immediate(error_type()),
            error_type(),
        )
        .with_implementation_hook(Some(implementation));

        let contracts = selected_contracts(&unit, &storage, call_expression, &call);
        let contract = contracts.invocation();

        assert_storage_requirement(
            contract,
            argument_access,
            BoundDependencyRequirementKind::StorageAlive,
        );

        if let Some((capability, expected)) = capability {
            assert!(contract.requirements().iter().any(|requirement| {
                matches!(
                    requirement,
                    BoundDependencyRequirement::Direct {
                        subject: BoundDependencySubject::StorageAccess(access),
                        kind: BoundDependencyRequirementKind::StorageInitialized,
                    } if *access == argument_access
                )
            }));

            assert!(contract.requirements().iter().any(|requirement| {
                matches!(
                    requirement,
                    BoundDependencyRequirement::Direct {
                        subject: BoundDependencySubject::BorrowCapability(actual),
                        kind: BoundDependencyRequirementKind::BorrowCapabilityActive(kind),
                    } if *actual == capability && *kind == expected
                )
            }));
        }

        if implementation == ImplementationHook::BorrowMutFrom {
            assert!(contract.requirements().iter().any(|requirement| {
                matches!(
                    requirement,
                    BoundDependencyRequirement::Direct {
                        subject: BoundDependencySubject::StorageAccess(access),
                        kind: BoundDependencyRequirementKind::ExclusiveMutationAuthority,
                    } if *access == argument_access
                )
            }));
        }
    }

    #[test]
    fn hidden_iteration_calls_instantiate_receiver_and_result_dependencies() {
        let unit_id = BoundUnitId::new(30);

        let (unit, source, iteration) = expression_pair(unit_id);

        let mut storage = StoragePlanBuilder::new(unit_id, unit.key().kind());

        let (_, source_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::Temporary(source),
            unit.key().source(),
        );

        let (_, cursor_access) = push_direct_storage(
            &mut storage,
            StorageIdentity::IterationCursor(iteration),
            unit.key().source(),
        );

        let storage = storage.finish();

        let template = semantic_values()
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Receiver),
                    DependencyRequirementKind::StorageAlive,
                ),
                DependencyRequirement::direct(
                    DependencySubject::root(DependencySubjectRoot::Result),
                    DependencyRequirementKind::StorageInitialized,
                ),
            ]))
            .unwrap_or_else(|error| panic!("test dependency template must intern: {error:?}"));

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let mut context =
            CallInstantiationContext::hidden(request, &storage, source_access, cursor_access);

        let contracts = instantiate_callable_contracts(
            request,
            CallableDependencyContracts::synchronous(template),
            &mut context,
        )
        .unwrap_or_else(|error| panic!("hidden call contract must instantiate: {error:?}"));

        let contract = contracts.invocation();

        assert_storage_requirement(
            contract,
            source_access,
            BoundDependencyRequirementKind::StorageAlive,
        );

        assert!(contract.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(access),
                    kind: BoundDependencyRequirementKind::StorageInitialized,
                } if *access == cursor_access
            )
        }));
    }

    fn assert_storage_requirement(
        contract: &BoundDependencyContract,
        expected_access: StorageAccessId,
        expected_kind: BoundDependencyRequirementKind,
    ) {
        assert!(contract.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(access),
                    kind,
                } if *access == expected_access && *kind == expected_kind
            )
        }));
    }

    fn expression_pair(
        unit: BoundUnitId,
    ) -> (
        bray_bound_tree::BoundUnit,
        bray_bound_tree::BoundExpressionId,
        bray_bound_tree::BoundExpressionId,
    ) {
        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect()
        });

        let [first, second] = expressions.as_slice() else {
            panic!("test expression pair must contain two expressions");
        };

        (unit, *first, *second)
    }

    fn indirect_callable_type(contracts: CallableDependencyContracts) -> TypeId {
        semantic_values()
            .intern_type(TypeData::Callable(CallableTypeData::new(
                [],
                error_type(),
                CallableConstness::Runtime,
                CallableTrust::Safe,
                CallableAbi::Bray,
                contracts,
            )))
            .unwrap_or_else(|error| panic!("test callable type must intern: {error:?}"))
    }

    fn selected_indirect_call(
        callable_type: TypeId,
        argument: bray_bound_tree::BoundExpressionId,
        result: BoundCallResult,
        target_type: TypeId,
    ) -> SelectedCall {
        SelectedCall::new(
            BoundResolvedCall::new(BoundCallableTarget::Indirect(callable_type), [], result),
            CallableAbi::Bray,
            empty_callable_phase_behaviors(),
            None,
            [SelectedArgument::Explicit {
                expression: argument,
                parameter: None,
                ordinal: 0,
                conversion: bray_bound_tree::SelectedConversion::new(
                    error_type(),
                    target_type,
                    bray_bound_tree::ConversionTarget::Identity,
                ),
            }],
            [],
        )
    }

    fn selected_contracts(
        unit: &bray_bound_tree::BoundUnit,
        storage: &bray_bound_tree::StoragePlan,
        expression: bray_bound_tree::BoundExpressionId,
        call: &SelectedCall,
    ) -> InstantiatedCallContracts {
        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        selected_call_contracts(
            request,
            storage,
            expression,
            call,
            &crate::dependency::ValueInputs::default(),
        )
        .unwrap_or_else(|error| panic!("selected call contract must instantiate: {error:?}"))
    }

    fn push_direct_storage(
        storage: &mut StoragePlanBuilder,
        identity: StorageIdentity,
        source: bray_bound_tree::BoundSourceAnchor,
    ) -> (StorageIdentityId, StorageAccessId) {
        let identity = storage
            .push_identity(identity)
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                error_type(),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

        (identity, access)
    }
}
