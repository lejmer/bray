use bray_bound_tree::{
    BoundCallableTarget, BoundDependencyContract, BoundExpressionId,
    DependencyContractInstantiationError, SelectedCall, StoragePlan,
};
use bray_symbols::{
    CallableDependencyContracts, CallableSignatureFact, SymbolFactRequest, TypeData,
    TypeExpressionTemplate,
};

use super::instantiation::CallInstantiationContext;
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticFactProvider, CheckerUnitView,
};

pub(in crate::dependency) fn selected_call_contract<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    expression: BoundExpressionId,
    call: &SelectedCall,
) -> Result<BoundDependencyContract, DependencyContractInstantiationError<CheckerInfrastructureError>>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    let contracts = callable_dependency_contracts(request, call.target())
        .map_err(DependencyContractInstantiationError::Resolution)?;

    let mut context = CallInstantiationContext::new(request, storage, expression, call);

    let invocation = request
        .semantic_values()
        .dependency_contract_template_data(contracts.invocation())
        .map_err(|_| {
            DependencyContractInstantiationError::Resolution(
                CheckerInfrastructureError::SemanticValueUnavailable,
            )
        })?;

    let mut requirements = BoundDependencyContract::try_instantiate(&invocation, &mut context)?
        .requirements()
        .to_vec();

    if let Some(deferred) = contracts.deferred_execution() {
        let deferred = request
            .semantic_values()
            .dependency_contract_template_data(deferred)
            .map_err(|_| {
                DependencyContractInstantiationError::Resolution(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        requirements.extend(
            BoundDependencyContract::try_instantiate(&deferred, &mut context)?
                .requirements()
                .iter()
                .cloned(),
        );
    }

    Ok(BoundDependencyContract::new(requirements))
}

fn callable_dependency_contracts<C>(
    request: CheckerUnitView<'_, C>,
    target: BoundCallableTarget,
) -> Result<CallableDependencyContracts, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    match target {
        BoundCallableTarget::Declaration(instance) => {
            let signature = request
                .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                    instance.definition().callable_symbol(),
                ))
                .map_err(|error| match error {
                    crate::CheckerFactError::Cancelled => {
                        CheckerInfrastructureError::InvalidSemanticSelectionInput
                    }
                    crate::CheckerFactError::Infrastructure(error) => error,
                })?;

            let dependencies =
                callable_type_dependencies(request, signature.value().callable_type())?;

            let invocation = request
                .semantic_values()
                .substitute_dependency_contract(dependencies.invocation(), instance.substitution())
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let deferred = dependencies
                .deferred_execution()
                .map(|contract| {
                    request
                        .semantic_values()
                        .substitute_dependency_contract(contract, instance.substitution())
                })
                .transpose()
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            Ok(match deferred {
                Some(deferred) => CallableDependencyContracts::asynchronous(invocation, deferred),
                None => CallableDependencyContracts::synchronous(invocation),
            })
        }
        BoundCallableTarget::Indirect(ty) => {
            let data = request
                .semantic_values()
                .type_data(ty)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            Ok(callable.dependency_contracts())
        }
        BoundCallableTarget::Anonymous(_) => request
            .semantic_values()
            .empty_dependency_contract_template()
            .map(CallableDependencyContracts::synchronous)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable),
    }
}

fn callable_type_dependencies<C>(
    request: CheckerUnitView<'_, C>,
    callable: &TypeExpressionTemplate,
) -> Result<CallableDependencyContracts, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    match callable {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.dependencies()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = request
                .semantic_values()
                .type_data(*ty)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            Ok(callable.dependency_contracts())
        }
        TypeExpressionTemplate::Named { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. }
        | TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::TraitView(_)
        | TypeExpressionTemplate::OwnedIndirection { .. } => {
            Err(CheckerInfrastructureError::InvalidSemanticSelectionInput)
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundCallResult, BoundCallableTarget, BoundDependencyGuard, BoundDependencyRequirement,
        BoundDependencyRequirementKind, BoundDependencySubject, BoundErrorExpression,
        BoundExpression, BoundResolvedCall, BoundUnitId, SelectedArgument, SelectedCall,
        StorageAccess, StorageAccessRoot, StorageIdentity, StoragePlanBuilder, StorageProjection,
    };
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDependencyContracts, CallableTrust,
        CallableTypeData, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
        SymbolOrdinal, TypeData,
    };

    use super::selected_call_contract;
    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, error_type, expression_unit, push_expression,
        semantic_values,
    };

    #[test]
    fn selected_calls_instantiate_direct_and_guarded_dependency_templates() {
        let unit_id = BoundUnitId::new(29);
        let (unit, expressions) = expression_unit(unit_id, |tree, origin| {
            vec![
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                ),
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                ),
            ]
        });
        let [argument, call_expression] = expressions.as_slice() else {
            panic!("test unit must contain an argument and call expression");
        };

        let mut storage = StoragePlanBuilder::new(unit_id, unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(*argument))
            .unwrap_or_else(|error| panic!("test argument storage must build: {error:?}"));

        let argument_access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                error_type(),
                unit.key().source(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test argument access must build: {error:?}"));

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

        let callable_type = semantic_values()
            .intern_type(TypeData::Callable(CallableTypeData::new(
                [],
                error_type(),
                CallableConstness::Runtime,
                CallableTrust::Safe,
                CallableAbi::Bray,
                CallableDependencyContracts::synchronous(template),
            )))
            .unwrap_or_else(|error| panic!("test callable type must intern: {error:?}"));

        let call = SelectedCall::new(
            BoundResolvedCall::new(
                BoundCallableTarget::Indirect(callable_type),
                [],
                BoundCallResult::Immediate(error_type()),
            ),
            CallableAbi::Bray,
            [SelectedArgument::Explicit {
                expression: *argument,
                parameter: None,
                ordinal: 0,
            }],
            [],
        );

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let contract = selected_call_contract(request, &storage, *call_expression, &call)
            .unwrap_or_else(|error| panic!("selected call contract must instantiate: {error:?}"));

        assert!(contract.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::StorageAccess(access),
                    kind: BoundDependencyRequirementKind::StorageAlive,
                } if *access == argument_access
            )
        }));

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
}
