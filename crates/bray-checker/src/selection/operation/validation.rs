use bray_bound_tree::{
    ConstructionTarget, ConversionTarget, ExpressionTypeResult, IndexTarget, OperatorTarget,
    SelectedConversion, SelectedOperation,
};
use bray_symbols::{
    CallableInstanceData, CallableSignature, ImplementationSelection, ImplementationSelectionKey,
    ReceiverMode, TypeId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::super::{ImplementationSelectionEvidence, TraitOperationEvidence};

pub(super) fn validate_operation_instances<C>(
    request: UnitCheckRequest<'_, C>,
    operation: &SelectedOperation,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    match operation {
        SelectedOperation::Operator {
            target: OperatorTarget::Trait { callable, .. },
            ..
        }
        | SelectedOperation::Index {
            target: IndexTarget::Custom { callable, .. },
            ..
        } => validate_trait_callable_instance(request, *callable)?,
        SelectedOperation::Construction(construction) => {
            if let ConstructionTarget::TypeForm(callable) = construction.target() {
                validate_callable_instance(request, callable)?;
            }
        }
        SelectedOperation::Conversion(conversion) => {
            validate_conversion_instances(request, conversion)?;
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::Index { .. }
        | SelectedOperation::Implementation(_) => {}
    }

    Ok(())
}

fn validate_conversion_instances<C>(
    request: UnitCheckRequest<'_, C>,
    conversion: &SelectedConversion,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    match conversion.target() {
        ConversionTarget::Trait { callable, .. } => {
            validate_trait_callable_instance(request, *callable)?;
        }
        ConversionTarget::Composite(children) => {
            for child in children.iter() {
                validate_conversion_instances(request, child)?;
            }
        }
        ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
    }

    Ok(())
}

fn validate_callable_instance<C>(
    request: UnitCheckRequest<'_, C>,
    callable: bray_symbols::CallableInstanceData,
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
    request: UnitCheckRequest<'_, C>,
    callable: bray_symbols::CallableInstanceData,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if callable.definition().symbol().kind() != bray_symbols::SymbolKind::TraitCallableMember {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    validate_callable_instance(request, callable)
}

pub(super) fn implementation_selections_match<C>(
    request: UnitCheckRequest<'_, C>,
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

pub(super) fn trait_operations_match<C>(
    request: UnitCheckRequest<'_, C>,
    operation: &SelectedOperation,
    actual_types: &[ExpressionTypeResult],
    evidence: &[TraitOperationEvidence],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut required = Vec::new();

    collect_trait_operations(operation, actual_types, &mut required)?;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequiredTraitOperation<'types> {
    Callable {
        requirement: ImplementationSelectionKey,
        callable: CallableInstanceData,
        receiver: TypeId,
        parameter_types: &'types [ExpressionTypeResult],
        result: TypeId,
        receiver_mode: ReceiverMode,
    },
    Conversion {
        requirement: ImplementationSelectionKey,
        callable: CallableInstanceData,
        source: TypeId,
        target: TypeId,
    },
}

impl RequiredTraitOperation<'_> {
    const fn requirement(self) -> ImplementationSelectionKey {
        match self {
            Self::Callable { requirement, .. } | Self::Conversion { requirement, .. } => {
                requirement
            }
        }
    }
}

fn collect_trait_operations<'types>(
    operation: &SelectedOperation,
    actual_types: &'types [ExpressionTypeResult],
    required: &mut Vec<RequiredTraitOperation<'types>>,
) -> Result<(), CheckerInfrastructureError> {
    match operation {
        SelectedOperation::Operator {
            target:
                OperatorTarget::Trait {
                    callable,
                    requirement,
                    ..
                },
            result_type,
        }
        | SelectedOperation::Index {
            target:
                IndexTarget::Custom {
                    callable,
                    requirement,
                    ..
                },
            result_type,
        } => {
            let Some((receiver, parameter_types)) = actual_types.split_first() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            required.push(RequiredTraitOperation::Callable {
                requirement: *requirement,
                callable: *callable,
                receiver: receiver.ty(),
                parameter_types,
                result: *result_type,
                receiver_mode: ReceiverMode::Shared,
            });
        }
        SelectedOperation::Conversion(conversion) => {
            collect_conversion_operations(conversion, required);
        }
        SelectedOperation::Member(_)
        | SelectedOperation::Operator { .. }
        | SelectedOperation::Index { .. }
        | SelectedOperation::Construction(_)
        | SelectedOperation::Implementation(_) => {}
    }

    Ok(())
}

fn collect_conversion_operations<'types>(
    conversion: &SelectedConversion,
    required: &mut Vec<RequiredTraitOperation<'types>>,
) {
    match conversion.target() {
        ConversionTarget::Trait {
            callable,
            requirement,
            ..
        } => required.push(RequiredTraitOperation::Conversion {
            requirement: *requirement,
            callable: *callable,
            source: conversion.source_type(),
            target: conversion.target_type(),
        }),
        ConversionTarget::Composite(children) => {
            for child in children.iter() {
                collect_conversion_operations(child, required);
            }
        }
        ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
    }
}

fn trait_operation_matches<C>(
    request: UnitCheckRequest<'_, C>,
    required: RequiredTraitOperation<'_>,
    evidence: &TraitOperationEvidence,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if evidence.requirement() != required.requirement()
        || evidence.callable()
            != match required {
                RequiredTraitOperation::Callable { callable, .. }
                | RequiredTraitOperation::Conversion { callable, .. } => callable,
            }
    {
        return Ok(false);
    }

    let application = request
        .semantic_values()
        .trait_application_data(required.requirement().trait_application())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let callable_symbol = evidence.callable().definition().symbol();

    if application.definition() != evidence.trait_definition()
        || !request
            .available_compiler_known_symbols()
            .contains(evidence.trait_definition().into())
        || !request
            .available_compiler_known_symbols()
            .contains(callable_symbol)
    {
        return Ok(false);
    }

    Ok(match required {
        RequiredTraitOperation::Callable {
            receiver,
            parameter_types,
            result,
            receiver_mode,
            ..
        } => signature_matches(
            evidence.signature(),
            receiver,
            parameter_types,
            result,
            receiver_mode,
        ),
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
    parameter_types: &[ExpressionTypeResult],
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
            .eq(parameter_types.iter().map(|result| result.ty()))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundUnit, BoundUnitId, ConversionTarget, ExpressionTypeResult, ExpressionTypeStatus,
        SelectedConversion, SelectedOperation,
    };
    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, CallableSignature, GenericArgument,
        GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
        GenericTypeParameterSymbolId, ImplementationInstanceData, ImplementationSelection,
        ImplementationSelectionKey, NamedTraitImplementationSymbolId, ReceiverMode,
        ReceiverParameterSignature, ReceiverParameterSymbolId, SymbolId, TraitApplicationData,
        TraitCallableMemberSymbolId, TraitSymbolId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression, semantic_values, tuple_type,
    };
    use crate::{ImplementationSelectionEvidence, TraitOperationEvidence, UnitCheckRequest};

    use super::{implementation_selections_match, trait_operations_match};

    #[test]
    fn nested_trait_conversions_require_the_exact_contract_and_witness() {
        let fixture = trait_conversion_fixture(BoundUnitId::new(87));
        let context = TestCheckerContext::new(false);
        let entry = callable_entry(fixture.unit.key());

        let request = match UnitCheckRequest::new(&fixture.unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("trait conversion request must validate: {error:?}"),
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
            trait_operations_match(
                request,
                &fixture.operation,
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

        let wrong_contract = TraitOperationEvidence::new(
            fixture.requirement,
            fixture.contract.trait_definition(),
            fixture.contract.callable(),
            wrong_signature,
        );

        assert_eq!(
            trait_operations_match(
                request,
                &fixture.operation,
                &[ExpressionTypeResult::new(
                    fixture.source,
                    ExpressionTypeStatus::Valid
                )],
                &[wrong_contract]
            ),
            Ok(false)
        );
    }

    struct TraitConversionFixture {
        unit: BoundUnit,
        operation: SelectedOperation,
        implementation: ImplementationSelectionEvidence,
        contract: TraitOperationEvidence,
        requirement: ImplementationSelectionKey,
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

        let trait_definition = compiler_known_symbol::<TraitSymbolId>("Storage");

        let callable_definition =
            compiler_known_symbol::<TraitCallableMemberSymbolId>("StorageLoad");

        let requirement =
            implementation_requirement(trait_definition, source_element, target_element);

        let callable = callable_instance(callable_definition);
        let witness = implementation_instance(30);
        let other_witness = implementation_instance(31);

        let child = SelectedConversion::new(
            source_element,
            target_element,
            ConversionTarget::Trait {
                callable,
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

        let contract =
            TraitOperationEvidence::new(requirement, trait_definition, callable, signature);

        let implementation = ImplementationSelectionEvidence::new(
            requirement,
            ImplementationSelection::Selected(witness),
        );

        let (unit, _) = expression_unit(unit, |tree, origin| {
            let expression =
                push_expression(tree, integer_literal_expression(origin, Some(source)));

            vec![expression]
        });

        TraitConversionFixture {
            unit,
            operation,
            implementation,
            contract,
            requirement,
            other_witness,
            source,
            source_element,
            target_element,
        }
    }

    fn compiler_known_symbol<I: bray_symbols::ExactSymbolId>(key: &str) -> I {
        let Some(key) = CompilerKnownDeclarationKey::try_new(key) else {
            panic!("compiler-known test key must be valid");
        };

        let Some(symbol) =
            crate::test_support::available_compiler_known_symbols().declaration_symbol::<I>(&key)
        else {
            panic!("compiler-known test symbol must be available");
        };

        symbol
    }

    fn implementation_requirement(
        trait_definition: TraitSymbolId,
        source: bray_symbols::TypeId,
        target: bray_symbols::TypeId,
    ) -> ImplementationSelectionKey {
        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(50));
        let owner = generic_owner(trait_definition.into());

        let substitution = match GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Type(parameter)],
            [GenericArgument::Type(target)],
        ) {
            Ok(substitution) => substitution,
            Err(error) => panic!("trait substitution must validate: {error:?}"),
        };

        let substitution = match semantic_values().intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("trait substitution must be interned: {error:?}"),
        };

        let application = TraitApplicationData::new(trait_definition, substitution);

        let application = match semantic_values().intern_trait_application(application) {
            Ok(application) => application,
            Err(error) => panic!("trait application must be interned: {error:?}"),
        };

        ImplementationSelectionKey::new(source, application)
    }

    fn callable_instance(definition: TraitCallableMemberSymbolId) -> CallableInstanceData {
        let substitution = empty_substitution(definition.into());

        let Some(definition) = CallableDefinitionId::try_new(definition.into()) else {
            panic!("trait callable member must be callable");
        };

        CallableInstanceData::new(definition, substitution)
    }

    fn implementation_instance(symbol: u32) -> bray_symbols::ImplementationInstanceId {
        let definition = NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(symbol));
        let substitution = empty_substitution(definition.into());
        let instance = ImplementationInstanceData::new(definition.into(), substitution);

        match semantic_values().intern_implementation_instance(instance) {
            Ok(instance) => instance,
            Err(error) => panic!("implementation instance must be interned: {error:?}"),
        }
    }

    fn empty_substitution(owner: bray_symbols::AnySymbolId) -> bray_symbols::GenericSubstitutionId {
        let owner = generic_owner(owner);

        let substitution = match GenericSubstitutionData::try_new(owner, [], []) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty substitution must validate: {error:?}"),
        };

        match semantic_values().intern_generic_substitution(substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("empty substitution must be interned: {error:?}"),
        }
    }

    fn generic_owner(owner: bray_symbols::AnySymbolId) -> GenericOwnerId {
        let Some(owner) = GenericOwnerId::try_new(owner) else {
            panic!("test symbol must support generic substitution");
        };

        owner
    }
}
