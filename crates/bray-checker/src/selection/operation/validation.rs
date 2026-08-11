use bray_bound_tree::{
    BoundExpression, BoundStructuredExpressionKind, ConstructionTarget, ConversionTarget,
    ExpressionTypeResult, IndexTarget, OperatorTarget, SelectedConversion, SelectedOperation,
};
use bray_compiler_known::CompilerKnownOperationRole;
use bray_symbols::{
    CallableInstanceData, CallableSignature, GenericArgument, ImplementationRequirementKey,
    ImplementationSelection, ReceiverMode, TypeData, TypeId,
};

use crate::representation::named_type;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::super::{CompilerKnownOperationEvidence, ImplementationSelectionEvidence};
use super::role::compiler_known_operation_role;

pub(super) fn validate_operation_instances<C>(
    request: CheckerUnitView<'_, C>,
    operation: &SelectedOperation,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(target) = operation.operator_target() {
        match target {
            OperatorTarget::Trait {
                member,
                fulfillment,
                ..
            } => {
                validate_trait_callable_instance(request, member)?;
                validate_trait_callable_fulfillment(request, fulfillment)?;
            }
            OperatorTarget::TraitConstraint { member, .. } => {
                validate_trait_callable_instance(request, member)?;
            }
            OperatorTarget::BuiltIn(_) => {}
        }

        return Ok(());
    }

    match operation {
        SelectedOperation::Index {
            target:
                IndexTarget::Custom {
                    member,
                    fulfillment,
                    ..
                },
            ..
        } => {
            validate_trait_callable_instance(request, *member)?;
            validate_trait_callable_fulfillment(request, *fulfillment)?;
        }
        SelectedOperation::Index {
            target: IndexTarget::TraitConstraint { member, .. },
            ..
        } => validate_trait_callable_instance(request, *member)?,
        SelectedOperation::Construction(construction) => {
            if let ConstructionTarget::TypeForm { callable, .. } = construction.target() {
                validate_callable_instance(request, callable)?;
            }
        }
        SelectedOperation::Conversion(conversion) => {
            validate_conversion_instances(request, conversion)?;
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::CompoundAssignment(_)
        | SelectedOperation::Index { .. }
        | SelectedOperation::Implementation(_) => {}
    }

    Ok(())
}

fn validate_conversion_instances<C>(
    request: CheckerUnitView<'_, C>,
    conversion: &SelectedConversion,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        match conversion.target() {
            ConversionTarget::Trait {
                member,
                fulfillment,
                ..
            } => {
                validate_trait_callable_instance(request, *member)?;
                validate_trait_callable_fulfillment(request, *fulfillment)?;
            }
            ConversionTarget::TraitConstraint { member, .. } => {
                validate_trait_callable_instance(request, *member)?;
            }
            ConversionTarget::Composite(children) => pending.extend(children.iter()),
            ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
        }
    }

    Ok(())
}

fn validate_callable_instance<C>(
    request: CheckerUnitView<'_, C>,
    callable: CallableInstanceData,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .semantic_values()
        .intern_callable_instance(callable)
        .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    Ok(())
}

fn validate_trait_callable_instance<C>(
    request: CheckerUnitView<'_, C>,
    callable: CallableInstanceData,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if callable.definition().symbol().kind() != bray_symbols::SymbolKind::TraitCallableMember {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    validate_callable_instance(request, callable)
}

fn validate_trait_callable_fulfillment<C>(
    request: CheckerUnitView<'_, C>,
    callable: CallableInstanceData,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if callable.definition().symbol().kind() != bray_symbols::SymbolKind::TraitCallableFulfillment {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    validate_callable_instance(request, callable)
}

pub(super) fn implementation_selections_match<C>(
    request: CheckerUnitView<'_, C>,
    operation: &SelectedOperation,
    evidence: &[ImplementationSelectionEvidence],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut evidence = evidence.iter().collect::<Vec<_>>();

    evidence.sort_unstable_by_key(|item| item.requirement());

    if evidence
        .windows(2)
        .any(|pair| pair[0].requirement() == pair[1].requirement())
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let mut required = operation.witnesses();

    required.sort_unstable();

    if required.len() != evidence.len() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    for (required, evidence) in required.iter().zip(evidence) {
        if required.requirement() != evidence.requirement() {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        }

        if evidence.selection() != &ImplementationSelection::Selected(required.witness()) {
            return Ok(false);
        }

        request
            .semantic_values()
            .implementation_instance_data(required.witness())
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;
    }

    Ok(true)
}

pub(super) fn compiler_known_operations_match<C>(
    request: CheckerUnitView<'_, C>,
    operation: &SelectedOperation,
    expression: &BoundExpression,
    actual_types: &[ExpressionTypeResult],
    evidence: &[CompilerKnownOperationEvidence],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut required = Vec::new();

    collect_compiler_known_operations(request, operation, expression, actual_types, &mut required)?;

    required.sort_unstable_by_key(|operation| operation.requirement());
    required.dedup();

    let mut evidence = evidence.iter().collect::<Vec<_>>();

    evidence.sort_unstable_by_key(|item| item.requirement());

    if evidence
        .windows(2)
        .any(|pair| pair[0].requirement() == pair[1].requirement())
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    if required.len() != evidence.len() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    for (required, evidence) in required.into_iter().zip(evidence) {
        if !trait_operation_matches(request, required, evidence)? {
            return Ok(false);
        }
    }

    Ok(true)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RequiredTraitOperation {
    Callable {
        role: CompilerKnownOperationRole,
        requirement: ImplementationRequirementKey,
        callable: CallableInstanceData,
        receiver: TypeId,
        parameter_types: Vec<TypeId>,
        callable_result: RequiredCallableResult,
        receiver_mode: ReceiverMode,
    },
    Conversion {
        role: CompilerKnownOperationRole,
        requirement: ImplementationRequirementKey,
        callable: CallableInstanceData,
        source: TypeId,
        target: TypeId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequiredCallableResult {
    Expression(TypeId),
    FixedContractType,
}

impl RequiredTraitOperation {
    const fn role(&self) -> CompilerKnownOperationRole {
        match self {
            Self::Callable { role, .. } | Self::Conversion { role, .. } => *role,
        }
    }

    const fn requirement(&self) -> ImplementationRequirementKey {
        match self {
            Self::Callable { requirement, .. } | Self::Conversion { requirement, .. } => {
                *requirement
            }
        }
    }
}

fn collect_compiler_known_operations<C>(
    request: CheckerUnitView<'_, C>,
    operation: &SelectedOperation,
    expression: &BoundExpression,
    actual_types: &[ExpressionTypeResult],
    required: &mut Vec<RequiredTraitOperation>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let (Some(target), Some(result_type)) =
        (operation.operator_target(), operation.operator_value_type())
    {
        if let OperatorTarget::Trait {
            operator,
            member,
            requirement,
            ..
        }
        | OperatorTarget::TraitConstraint {
            operator,
            member,
            requirement,
            ..
        } = target
        {
            let Some((receiver, parameter_types)) = actual_types.split_first() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            let role = compiler_known_operation_role(expression, operator)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            let callable_result = if role == CompilerKnownOperationRole::Comparison {
                RequiredCallableResult::FixedContractType
            } else {
                RequiredCallableResult::Expression(result_type)
            };

            required.push(RequiredTraitOperation::Callable {
                role,
                requirement,
                callable: member,
                receiver: receiver.ty(),
                parameter_types: parameter_types.iter().map(|result| result.ty()).collect(),
                callable_result,
                receiver_mode: ReceiverMode::Shared,
            });
        }

        return Ok(());
    }

    match operation {
        SelectedOperation::Index {
            target:
                IndexTarget::Custom {
                    borrow_kind,
                    member,
                    requirement,
                    ..
                }
                | IndexTarget::TraitConstraint {
                    borrow_kind,
                    member,
                    requirement,
                    ..
                },
            result_type,
        } => {
            let Some((receiver, parameter_types)) = actual_types.split_first() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            let role = match expression {
                BoundExpression::Structured(source) => match (source.kind(), borrow_kind) {
                    (BoundStructuredExpressionKind::ElementIndex, bray_symbols::BorrowKind::Shared) => {
                        CompilerKnownOperationRole::ElementIndex
                    }
                    (BoundStructuredExpressionKind::ElementIndex, bray_symbols::BorrowKind::Mutable) => {
                        CompilerKnownOperationRole::MutableElementIndex
                    }
                    (BoundStructuredExpressionKind::SliceIndex, bray_symbols::BorrowKind::Shared) => {
                        CompilerKnownOperationRole::SliceIndex
                    }
                    (BoundStructuredExpressionKind::SliceIndex, bray_symbols::BorrowKind::Mutable) => {
                        CompilerKnownOperationRole::MutableSliceIndex
                    }
                    _ => {
                        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
                    }
                },
                _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
            };

            let parameter_types = match role {
                CompilerKnownOperationRole::SliceIndex
                | CompilerKnownOperationRole::MutableSliceIndex => {
                    custom_slice_parameter_types(request, *requirement)?
                }
                _ => parameter_types.iter().map(|result| result.ty()).collect(),
            };

            let callable_result = request
                .semantic_values()
                .intern_type(TypeData::Borrow {
                    kind: *borrow_kind,
                    target: *result_type,
                })
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            required.push(RequiredTraitOperation::Callable {
                role,
                requirement: *requirement,
                callable: *member,
                receiver: receiver.ty(),
                parameter_types,
                callable_result: RequiredCallableResult::Expression(callable_result),
                receiver_mode: match borrow_kind {
                    bray_symbols::BorrowKind::Shared => ReceiverMode::Shared,
                    bray_symbols::BorrowKind::Mutable => ReceiverMode::Mutable,
                },
            });
        }
        SelectedOperation::Conversion(conversion) => {
            collect_conversion_operations(conversion, required);
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::CompoundAssignment(_)
        | SelectedOperation::Index { .. }
        | SelectedOperation::Construction(_)
        | SelectedOperation::Implementation(_) => {}
    }

    Ok(())
}

fn collect_conversion_operations(
    conversion: &SelectedConversion,
    required: &mut Vec<RequiredTraitOperation>,
) {
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        match conversion.target() {
            ConversionTarget::Trait {
                member,
                requirement,
                ..
            }
            | ConversionTarget::TraitConstraint {
                member,
                requirement,
                ..
            } => required.push(RequiredTraitOperation::Conversion {
                role: CompilerKnownOperationRole::PlainConversion,
                requirement: *requirement,
                callable: *member,
                source: conversion.source_type(),
                target: conversion.target_type(),
            }),
            ConversionTarget::Composite(children) => pending.extend(children.iter()),
            ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
        }
    }
}

fn trait_operation_matches<C>(
    request: CheckerUnitView<'_, C>,
    required: RequiredTraitOperation,
    evidence: &CompilerKnownOperationEvidence,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if evidence.role() != required.role()
        || evidence.requirement() != required.requirement()
        || evidence.callable()
            != match required {
                RequiredTraitOperation::Callable { callable, .. }
                | RequiredTraitOperation::Conversion { callable, .. } => callable,
            }
    {
        return Ok(false);
    }

    let Some(contract) = request
        .available_compiler_known_symbols()
        .operation_contract(required.role())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let application = request
        .semantic_values()
        .trait_application_data(required.requirement().trait_application())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let callable_symbol = evidence.callable().definition().symbol();

    if application.definition() != contract.trait_definition()
        || Some(callable_symbol) != contract.callable().map(Into::into)
    {
        return Ok(false);
    }

    Ok(match required {
        RequiredTraitOperation::Callable {
            receiver,
            parameter_types,
            callable_result,
            receiver_mode,
            ..
        } => {
            let callable_result = match callable_result {
                RequiredCallableResult::Expression(result) => result,
                RequiredCallableResult::FixedContractType => {
                    let Some(definition) = contract.fixed_callable_result_type() else {
                        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
                    };

                    named_type(request, definition)?
                }
            };

            signature_matches(
                evidence.signature(),
                receiver,
                &parameter_types,
                callable_result,
                receiver_mode,
            )
        }
        RequiredTraitOperation::Conversion { source, target, .. } => signature_matches(
            evidence.signature(),
            source,
            &[],
            target,
            ReceiverMode::Consuming,
        ),
    })
}

fn signature_matches(
    signature: &CallableSignature,
    receiver_type: TypeId,
    parameter_types: &[TypeId],
    result: TypeId,
    receiver_mode: ReceiverMode,
) -> bool {
    signature
        .receiver()
        .is_some_and(|receiver| receiver.ty() == receiver_type && receiver.mode() == receiver_mode)
        && signature.result() == result
        && signature
            .parameters()
            .iter()
            .map(|parameter| parameter.ty())
            .eq(parameter_types.iter().copied())
}

fn custom_slice_parameter_types<C>(
    request: CheckerUnitView<'_, C>,
    requirement: ImplementationRequirementKey,
) -> Result<Vec<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let application = request
        .semantic_values()
        .trait_application_data(requirement.trait_application())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .generic_substitution_data(application.substitution())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let [binding] = substitution.bindings() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let GenericArgument::Type(bound) = binding.argument() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let nullable = request
        .semantic_values()
        .intern_type(TypeData::Nullable(bound))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(vec![nullable, nullable])
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundUnit, BoundUnitId, ConversionTarget, ExpressionTypeResult, ExpressionTypeStatus,
        SelectedConversion, SelectedOperation,
    };
    use bray_compiler_known::CompilerKnownOperationRole;
    use bray_symbols::{
        CallableSignature, ImplementationRequirementKey, ImplementationSelection, ReceiverMode,
        ReceiverParameterSignature, ReceiverParameterSymbolId, SymbolId,
        TraitCallableFulfillmentSymbolId, TraitCallableMemberSymbolId, TraitSymbolId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, callable_instance, compiler_known_symbol,
        expression_unit, integer_literal_expression, push_expression, semantic_values,
        trait_callable_instance, tuple_type,
    };
    use crate::{CheckerUnitView, CompilerKnownOperationEvidence, ImplementationSelectionEvidence};

    use super::{compiler_known_operations_match, implementation_selections_match};

    #[test]
    fn nested_trait_conversions_require_the_exact_contract_and_witness() {
        let fixture = trait_conversion_fixture(BoundUnitId::new(87));
        let context = TestCheckerContext::new(false);
        let entry = callable_entry(fixture.unit.key());

        let request = match CheckerUnitView::new(&fixture.unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("trait conversion request must validate: {error:?}"),
        };

        let Some(source_expression) = fixture.unit.view().expression(fixture.source_expression)
        else {
            panic!("test source expression must be committed");
        };

        assert_eq!(
            implementation_selections_match(
                request,
                &fixture.operation,
                std::slice::from_ref(&fixture.implementation)
            ),
            Ok(true)
        );

        assert_eq!(
            compiler_known_operations_match(
                request,
                &fixture.operation,
                source_expression,
                &[ExpressionTypeResult::new(
                    fixture.source,
                    ExpressionTypeStatus::Valid
                )],
                std::slice::from_ref(&fixture.contract)
            ),
            Ok(true)
        );

        let wrong_witness = ImplementationSelectionEvidence::new(
            fixture.requirement,
            ImplementationSelection::Selected(fixture.other_witness),
        );

        assert_eq!(
            implementation_selections_match(request, &fixture.operation, &[wrong_witness]),
            Ok(false)
        );

        let wrong_signature = CallableSignature::new(
            fixture.source_element,
            fixture.contract.signature().receiver().map(|receiver| {
                ReceiverParameterSignature::new(
                    receiver.parameter(),
                    receiver.ty(),
                    ReceiverMode::Shared,
                )
            }),
            [],
            fixture.target_element,
        );

        let wrong_contract = CompilerKnownOperationEvidence::new(
            CompilerKnownOperationRole::PlainConversion,
            fixture.requirement,
            fixture.contract.callable(),
            wrong_signature,
        );

        assert_eq!(
            compiler_known_operations_match(
                request,
                &fixture.operation,
                source_expression,
                &[ExpressionTypeResult::new(
                    fixture.source,
                    ExpressionTypeStatus::Valid
                )],
                &[wrong_contract]
            ),
            Ok(false)
        );

        let wrong_role = CompilerKnownOperationEvidence::new(
            CompilerKnownOperationRole::Equality,
            fixture.requirement,
            fixture.contract.callable(),
            fixture.contract.signature().clone(),
        );

        assert_eq!(
            compiler_known_operations_match(
                request,
                &fixture.operation,
                source_expression,
                &[ExpressionTypeResult::new(
                    fixture.source,
                    ExpressionTypeStatus::Valid
                )],
                &[wrong_role]
            ),
            Ok(false)
        );
    }

    struct TraitConversionFixture {
        unit: BoundUnit,
        operation: SelectedOperation,
        implementation: ImplementationSelectionEvidence,
        contract: CompilerKnownOperationEvidence,
        source_expression: bray_bound_tree::BoundExpressionId,
        requirement: ImplementationRequirementKey,
        other_witness: bray_symbols::ImplementationInstanceId,
        source: bray_symbols::TypeId,
        source_element: bray_symbols::TypeId,
        target_element: bray_symbols::TypeId,
    }

    fn trait_conversion_fixture(unit: BoundUnitId) -> TraitConversionFixture {
        let source_element = tuple_type([]);
        let target_element = tuple_type([source_element]);

        let source = tuple_type([source_element]);
        let target = tuple_type([target_element]);

        let trait_definition = compiler_known_symbol::<TraitSymbolId>("ConvertTo");

        let callable_definition =
            compiler_known_symbol::<TraitCallableMemberSymbolId>("ConvertToCall");

        let requirement = bray_symbols::testing::implementation_requirement(
            semantic_values(),
            trait_definition,
            source_element,
            target_element,
        );

        let callable = trait_callable_instance(callable_definition);

        let fulfillment = callable_instance(
            TraitCallableFulfillmentSymbolId::from_symbol_id(SymbolId::new(32)).into(),
        );

        let witness = bray_symbols::testing::implementation_instance(semantic_values(), 30);
        let other_witness = bray_symbols::testing::implementation_instance(semantic_values(), 31);

        let child = SelectedConversion::new(
            source_element,
            target_element,
            ConversionTarget::Trait {
                member: callable,
                fulfillment,
                requirement,
                witness,
            },
        );

        let operation = SelectedOperation::Conversion(SelectedConversion::new(
            source,
            target,
            ConversionTarget::Composite([child].into()),
        ));

        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(40));

        let signature = CallableSignature::new(
            source_element,
            Some(ReceiverParameterSignature::new(
                receiver,
                source_element,
                ReceiverMode::Consuming,
            )),
            [],
            target_element,
        );

        let contract = CompilerKnownOperationEvidence::new(
            CompilerKnownOperationRole::PlainConversion,
            requirement,
            callable,
            signature,
        );

        let implementation = ImplementationSelectionEvidence::new(
            requirement,
            ImplementationSelection::Selected(witness),
        );

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let expression =
                push_expression(tree, integer_literal_expression(origin, Some(source)));

            vec![expression]
        });

        TraitConversionFixture {
            unit,
            operation,
            implementation,
            contract,
            source_expression: expressions[0],
            requirement,
            other_witness,
            source,
            source_element,
            target_element,
        }
    }
}
