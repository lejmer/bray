// rust-style: allow(module-too-large, reason = "trait fulfillment compatibility is one recursive structural comparison across every contract surface")

use std::collections::BTreeMap;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableContractClause, CallableContractSet, CallableContractsQuery,
    CallableParameterTypeTemplate, CallablePhaseBehavior, CallableSignatureQuery,
    CallableSignatureTemplate, CallableSymbolId, CallableTypeTemplate, ConstantTermData,
    GenericArgument, GenericConstParameterDeclaredTypeQuery, GenericConstraintsQuery,
    GenericDeclarationTemplateQuery, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateQuery, RuntimeDefaultPresence, SymbolQueryRequest,
    TraitApplicationId, TraitMemberFulfillmentId, TraitMemberRequirementId,
    TraitTypeFulfillmentValueQuery, TypeData, TypeExpressionTemplate,
};

use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::FactQueryError;

use super::constraint::{constraints_are_compatible, substitute_requirement_trait_application};
use super::mismatch::{
    CallableBehaviorComponent, CallableBehaviorPhase, CallableContractClauseCategory,
    CallableContractMismatch, CallableContractSurface, GenericParameterCategory,
    GenericSurfaceMismatch, TraitFulfillmentMismatch,
};
use super::types::{
    dependency_contracts_are_compatible, substitute_requirement_type, type_templates_are_compatible,
};

macro_rules! resolve_query {
    ($binding_context:expr, $diagnostics:expr, $contract:ty, $owner:expr) => {{
        let result = $binding_context
            .resolve_symbol_query(SymbolQueryRequest::<$contract>::new($owner))
            .map_err(binding_query_error)?;

        *$diagnostics = $diagnostics.merged(result.diagnostics());

        result
    }};
}
pub(in crate::compilation::implementation::conformance) struct CompatibilityContext<
    'binding_context,
    'compilation,
> {
    symbols: &'binding_context bray_symbols::SymbolGraph,
    imported: Option<&'binding_context bray_symbols::ImportedSymbolSkeleton>,
    values: &'binding_context bray_symbols::SemanticValueStore,
    binding_context: &'binding_context CompilationBindingContext<'compilation>,
    subject: bray_symbols::TypeId,
    trait_application: TraitApplicationId,
    type_bindings:
        &'binding_context BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
}

impl<'binding_context, 'compilation> CompatibilityContext<'binding_context, 'compilation> {
    pub(in crate::compilation::implementation::conformance) fn new(
        symbols: &'binding_context bray_symbols::SymbolGraph,
        imported: Option<&'binding_context bray_symbols::ImportedSymbolSkeleton>,
        values: &'binding_context bray_symbols::SemanticValueStore,
        binding_context: &'binding_context CompilationBindingContext<'compilation>,
        subject: bray_symbols::TypeId,
        trait_application: TraitApplicationId,
        type_bindings: &'binding_context BTreeMap<
            bray_symbols::TraitTypeMemberSymbolId,
            TypeExpressionTemplate,
        >,
    ) -> Self {
        Self {
            symbols,
            imported,
            values,
            binding_context,
            subject,
            trait_application,
            type_bindings,
        }
    }
}

pub(in crate::compilation::implementation::conformance) fn fulfillment_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: TraitMemberRequirementId,
    fulfillment: TraitMemberFulfillmentId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<TraitFulfillmentMismatch>, FactQueryError> {
    let values = context.values;
    let binding_context = context.binding_context;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;

    match (requirement, fulfillment) {
        (
            TraitMemberRequirementId::Callable(requirement),
            TraitMemberFulfillmentId::Callable(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Constant(requirement),
            TraitMemberFulfillmentId::Constant(fulfillment),
        ) => {
            let fulfillment_context =
                fulfillment_self_context(binding_context, fulfillment.into())?;

            let requirement = resolve_query!(
                binding_context,
                diagnostics,
                bray_symbols::TraitConstantMemberDeclaredTypeQuery,
                requirement
            );

            let fulfillment = resolve_query!(
                binding_context,
                diagnostics,
                bray_symbols::TraitConstantFulfillmentDeclaredTypeQuery,
                fulfillment
            );

            let compatible = type_templates_are_compatible(
                values,
                context.subject,
                trait_application,
                fulfillment_context,
                None,
                requirement.value(),
                fulfillment.value(),
                type_bindings,
            )?;

            Ok(
                (!compatible).then(|| TraitFulfillmentMismatch::ConstantType {
                    required: requirement.value().clone(),
                    provided: fulfillment.value().clone(),
                }),
            )
        }
        (TraitMemberRequirementId::Type(_), TraitMemberFulfillmentId::Type(fulfillment)) => {
            let value = resolve_query!(
                binding_context,
                diagnostics,
                TraitTypeFulfillmentValueQuery,
                fulfillment
            );

            Ok(
                (value.value().resolved_type().is_none() && value.diagnostics().has_errors())
                    .then_some(TraitFulfillmentMismatch::TypeValueUnavailable),
            )
        }
        (
            TraitMemberRequirementId::Predicate(requirement),
            TraitMemberFulfillmentId::Predicate(fulfillment),
        ) => predicate_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Finalizer(requirement),
            TraitMemberFulfillmentId::Finalizer(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::Destructor(requirement),
            TraitMemberFulfillmentId::Destructor(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::ScopeEnter(requirement),
            TraitMemberFulfillmentId::ScopeEnter(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        (
            TraitMemberRequirementId::ScopeExit(requirement),
            TraitMemberFulfillmentId::ScopeExit(fulfillment),
        ) => callable_is_compatible(context, requirement.into(), fulfillment.into(), diagnostics),
        _ => Ok(Some(TraitFulfillmentMismatch::MemberCategory)),
    }
}

pub(in crate::compilation::implementation::conformance) fn subject_lifecycle_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: TraitMemberRequirementId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<TraitFulfillmentMismatch>, FactQueryError> {
    let requirement = match requirement {
        TraitMemberRequirementId::Finalizer(requirement) => requirement.into(),
        TraitMemberRequirementId::Destructor(requirement) => requirement.into(),
        _ => return Ok(Some(TraitFulfillmentMismatch::MemberCategory)),
    };

    callable_is_compatible(context, requirement, fulfillment, diagnostics)
}

fn callable_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<TraitFulfillmentMismatch>, FactQueryError> {
    let symbols = context.symbols;
    let imported = context.imported;
    let values = context.values;
    let binding_context = context.binding_context;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;
    let fulfillment_context = fulfillment_self_context(binding_context, fulfillment.into_any())?;

    let requirement_signature = resolve_query!(
        binding_context,
        diagnostics,
        CallableSignatureQuery,
        requirement
    );

    let fulfillment_signature = resolve_query!(
        binding_context,
        diagnostics,
        CallableSignatureQuery,
        fulfillment
    );

    let (generic_mismatch, generic_substitution) = generic_surfaces_are_compatible(
        values,
        context.subject,
        binding_context,
        trait_application,
        requirement.into_any(),
        fulfillment.into_any(),
        type_bindings,
        diagnostics,
    )?;

    if let Some(mismatch) = generic_mismatch {
        return Ok(Some(TraitFulfillmentMismatch::Generic(mismatch)));
    }

    let requirement_type = callable_type_template(
        values,
        requirement.into_any(),
        requirement_signature.value(),
    )?;

    let fulfillment_type = callable_type_template(
        values,
        fulfillment.into_any(),
        fulfillment_signature.value(),
    )?;

    let required_receiver = requirement_signature
        .value()
        .receiver()
        .map(|receiver| receiver.mode());

    let provided_receiver = fulfillment_signature
        .value()
        .receiver()
        .map(|receiver| receiver.mode());

    if required_receiver != provided_receiver {
        return Ok(Some(TraitFulfillmentMismatch::Receiver {
            required: required_receiver,
            provided: provided_receiver,
        }));
    }

    if requirement_type.constness() != fulfillment_type.constness() {
        return Ok(Some(TraitFulfillmentMismatch::CallableConstness {
            required: requirement_type.constness(),
            provided: fulfillment_type.constness(),
        }));
    }

    if requirement_type.execution() != fulfillment_type.execution() {
        return Ok(Some(TraitFulfillmentMismatch::CallableExecution {
            required: requirement_type.execution(),
            provided: fulfillment_type.execution(),
        }));
    }

    if requirement_type.trust() != fulfillment_type.trust() {
        return Ok(Some(TraitFulfillmentMismatch::CallableTrust {
            required: requirement_type.trust(),
            provided: fulfillment_type.trust(),
        }));
    }

    if requirement_type.abi() != fulfillment_type.abi() {
        return Ok(Some(TraitFulfillmentMismatch::CallableAbi {
            required: requirement_type.abi(),
            provided: fulfillment_type.abi(),
        }));
    }

    let storage_constructor =
        bray_compiler_known::CompilerKnownDeclarationKey::try_new("StorageCreate")
            .and_then(|key| {
                binding_context
                    .compilation()
                    .available_compiler_known_symbols()
                    .declaration_symbol::<bray_symbols::TraitCallableMemberSymbolId>(&key)
            })
            .is_some_and(|member| requirement == member.into());

    // Storage construction retains the required value prefix and declares policy-specific
    // arguments on the fulfillment. Ordinary trait callables still require identical arity.
    if requirement_type.parameters().len() != fulfillment_type.parameters().len()
        && (!storage_constructor
            || fulfillment_type.parameters().len() < requirement_type.parameters().len())
    {
        return Ok(Some(TraitFulfillmentMismatch::CallableParameterCount {
            required: requirement_type.parameters().len(),
            provided: fulfillment_type.parameters().len(),
        }));
    }

    for (ordinal, (requirement, fulfillment)) in requirement_type
        .parameters()
        .iter()
        .zip(fulfillment_type.parameters())
        .enumerate()
    {
        if requirement.name() != fulfillment.name() {
            return Ok(Some(TraitFulfillmentMismatch::CallableParameterName {
                ordinal,
                required: requirement.name().as_str().to_owned(),
                provided: fulfillment.name().as_str().to_owned(),
            }));
        }

        if requirement.position() != fulfillment.position() {
            return Ok(Some(TraitFulfillmentMismatch::CallableParameterPosition {
                ordinal,
                required: requirement.position(),
                provided: fulfillment.position(),
            }));
        }

        if requirement.mode() != fulfillment.mode() {
            return Ok(Some(TraitFulfillmentMismatch::CallableParameterMode {
                ordinal,
                required: requirement.mode(),
                provided: fulfillment.mode(),
            }));
        }

        if !type_templates_are_compatible(
            values,
            context.subject,
            trait_application,
            fulfillment_context,
            generic_substitution,
            requirement.ty(),
            fulfillment.ty(),
            type_bindings,
        )? {
            return Ok(Some(TraitFulfillmentMismatch::CallableParameterType {
                ordinal,
                required: requirement.ty().clone(),
                provided: fulfillment.ty().clone(),
            }));
        }
    }

    if !type_templates_are_compatible(
        values,
        context.subject,
        trait_application,
        fulfillment_context,
        generic_substitution,
        requirement_type.result(),
        fulfillment_type.result(),
        type_bindings,
    )? {
        return Ok(Some(TraitFulfillmentMismatch::CallableResultType {
            required: requirement_type.result().clone(),
            provided: fulfillment_type.result().clone(),
        }));
    }

    if let Some(mismatch) = parameter_default_mismatch(
        symbols,
        imported,
        requirement_signature.value().parameters(),
        fulfillment_signature
            .value()
            .parameters()
            .get(..requirement_signature.value().parameters().len())
            .ok_or_else(|| {
                callable_signature_error(
                    fulfillment.into_any(),
                    bray_symbols::CallableSignatureTemplateError::InvalidCallableType,
                )
            })?,
    )? {
        return Ok(Some(mismatch));
    }

    let contract_mismatch = callable_contract_mismatch(
        values,
        context.subject,
        binding_context,
        trait_application,
        generic_substitution,
        requirement,
        fulfillment,
        diagnostics,
    )?;

    Ok(contract_mismatch.map(TraitFulfillmentMismatch::CallableContract))
}

fn callable_type_template(
    values: &bray_symbols::SemanticValueStore,
    callable: bray_symbols::AnySymbolId,
    signature: &CallableSignatureTemplate,
) -> Result<CallableTypeTemplate, FactQueryError> {
    match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.clone()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values.type_data(*ty);

            let TypeData::Callable(callable_data) = data.as_ref() else {
                return Err(callable_signature_error(
                    callable,
                    bray_symbols::CallableSignatureTemplateError::InvalidCallableType,
                ));
            };

            let parameter_types = signature
                .parameter_type_templates(values)
                .map_err(|cause| callable_signature_error(callable, cause))?;

            let parameters =
                callable_data
                    .parameters()
                    .iter()
                    .zip(parameter_types)
                    .map(|(parameter, ty)| {
                        CallableParameterTypeTemplate::new(
                            parameter.name().clone(),
                            parameter.position(),
                            parameter.mode(),
                            ty,
                        )
                    });

            Ok(CallableTypeTemplate::new(
                parameters,
                signature.result().clone(),
                callable_data.constness(),
                callable_data.trust(),
                callable_data.abi(),
                callable_data.dependency_contracts(),
            )
            .with_variadic(callable_data.is_variadic())
            .with_phase_behaviors(callable_data.phase_behaviors().clone()))
        }
        _ => Err(callable_signature_error(
            callable,
            bray_symbols::CallableSignatureTemplateError::InvalidCallableType,
        )),
    }
}

fn callable_signature_error(
    callable: bray_symbols::AnySymbolId,
    cause: bray_symbols::CallableSignatureTemplateError,
) -> FactQueryError {
    crate::compilation::SemanticQueryFailure::CallableSignature {
        callable: Some(callable),
        cause,
    }
    .into()
}

fn generic_surfaces_are_compatible(
    values: &bray_symbols::SemanticValueStore,
    subject: bray_symbols::TypeId,
    binding_context: &CompilationBindingContext<'_>,
    trait_application: TraitApplicationId,
    requirement: bray_symbols::AnySymbolId,
    fulfillment: bray_symbols::AnySymbolId,
    type_bindings: &BTreeMap<bray_symbols::TraitTypeMemberSymbolId, TypeExpressionTemplate>,
    diagnostics: &mut DiagnosticBag,
) -> Result<
    (
        Option<GenericSurfaceMismatch>,
        Option<GenericSubstitutionId>,
    ),
    FactQueryError,
> {
    let fulfillment_context = fulfillment_self_context(binding_context, fulfillment)?;

    let Some(requirement_owner) = GenericOwnerId::try_new(requirement) else {
        return Ok((None, None));
    };

    let Some(fulfillment_owner) = GenericOwnerId::try_new(fulfillment) else {
        return Ok((Some(GenericSurfaceMismatch::FulfillmentIsNotGeneric), None));
    };

    let requirement = resolve_query!(
        binding_context,
        diagnostics,
        GenericDeclarationTemplateQuery,
        requirement_owner
    );

    let fulfillment = resolve_query!(
        binding_context,
        diagnostics,
        GenericDeclarationTemplateQuery,
        fulfillment_owner
    );

    if requirement.value().parameters().len() != fulfillment.value().parameters().len() {
        return Ok((
            Some(GenericSurfaceMismatch::ParameterCount {
                required: requirement.value().parameters().len(),
                provided: fulfillment.value().parameters().len(),
            }),
            None,
        ));
    }

    let mut arguments = Vec::with_capacity(fulfillment.value().parameters().len());

    for (ordinal, (requirement_parameter, fulfillment_parameter)) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
        .enumerate()
    {
        match (requirement_parameter, fulfillment_parameter) {
            (GenericParameterSymbolId::Type(_), GenericParameterSymbolId::Type(fulfillment)) => {
                let ty = values
                    .intern_type(TypeData::TypeParameter(*fulfillment))
                    .map_err(FactQueryError::SemanticValueStore)?;

                arguments.push(GenericArgument::Type(ty));
            }
            (GenericParameterSymbolId::Const(_), GenericParameterSymbolId::Const(fulfillment)) => {
                let term = values
                    .intern_constant_term(ConstantTermData::Parameter(*fulfillment))
                    .map_err(FactQueryError::SemanticValueStore)?;

                arguments.push(GenericArgument::Constant(term));
            }
            (requirement, fulfillment) => {
                let category = |parameter: &GenericParameterSymbolId| match parameter {
                    GenericParameterSymbolId::Type(_) => GenericParameterCategory::Type,
                    GenericParameterSymbolId::Const(_) => GenericParameterCategory::Constant,
                };

                return Ok((
                    Some(GenericSurfaceMismatch::ParameterCategory {
                        ordinal,
                        required: category(requirement),
                        provided: category(fulfillment),
                    }),
                    None,
                ));
            }
        }
    }

    let substitution = GenericSubstitutionData::try_new(
        requirement_owner,
        requirement.value().parameters().iter().copied(),
        arguments,
    )
    .map_err(
        |cause| crate::compilation::SemanticQueryFailure::GenericSubstitution {
            owner: Some(requirement_owner),
            cause,
        },
    )?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)?;

    for (ordinal, (requirement, fulfillment)) in requirement
        .value()
        .parameters()
        .iter()
        .zip(fulfillment.value().parameters())
        .enumerate()
    {
        let (
            GenericParameterSymbolId::Const(requirement),
            GenericParameterSymbolId::Const(fulfillment),
        ) = (requirement, fulfillment)
        else {
            continue;
        };

        let requirement_type = resolve_query!(
            binding_context,
            diagnostics,
            GenericConstParameterDeclaredTypeQuery,
            *requirement
        );

        let fulfillment_type = resolve_query!(
            binding_context,
            diagnostics,
            GenericConstParameterDeclaredTypeQuery,
            *fulfillment
        );

        if !type_templates_are_compatible(
            values,
            subject,
            trait_application,
            fulfillment_context,
            Some(substitution),
            requirement_type.value(),
            fulfillment_type.value(),
            type_bindings,
        )? {
            return Ok((
                Some(GenericSurfaceMismatch::ConstantParameterType {
                    ordinal,
                    required: requirement_type.value().clone(),
                    provided: fulfillment_type.value().clone(),
                }),
                None,
            ));
        }
    }

    let requirement_constraints = resolve_query!(
        binding_context,
        diagnostics,
        GenericConstraintsQuery,
        requirement_owner
    );

    let fulfillment_constraints = resolve_query!(
        binding_context,
        diagnostics,
        GenericConstraintsQuery,
        fulfillment_owner
    );

    if let Some(mismatch) = constraints_are_compatible(
        values,
        subject,
        trait_application,
        substitution,
        requirement_constraints.value(),
        fulfillment_constraints.value(),
    )? {
        return Ok((Some(GenericSurfaceMismatch::Constraints(mismatch)), None));
    }

    Ok((None, Some(substitution)))
}

fn parameter_default_mismatch(
    symbols: &bray_symbols::SymbolGraph,
    imported: Option<&bray_symbols::ImportedSymbolSkeleton>,
    requirement: &[bray_symbols::CallableParameterSymbolId],
    fulfillment: &[bray_symbols::CallableParameterSymbolId],
) -> Result<Option<TraitFulfillmentMismatch>, FactQueryError> {
    if requirement.len() != fulfillment.len() {
        return Ok(Some(TraitFulfillmentMismatch::CallableParameterCount {
            required: requirement.len(),
            provided: fulfillment.len(),
        }));
    }

    for (ordinal, (requirement, fulfillment)) in requirement.iter().zip(fulfillment).enumerate() {
        let requirement = symbols
            .callable_parameter(*requirement)
            .or_else(|| imported.and_then(|symbols| symbols.callable_parameter(*requirement)))
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol((*requirement).into()),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Symbol,
                    ),
                )
            })?;

        let fulfillment = symbols
            .callable_parameter(*fulfillment)
            .or_else(|| imported.and_then(|symbols| symbols.callable_parameter(*fulfillment)))
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol((*fulfillment).into()),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Symbol,
                    ),
                )
            })?;

        let required = matches!(
            requirement.default_presence(),
            RuntimeDefaultPresence::Present
        );

        let provided = matches!(
            fulfillment.default_presence(),
            RuntimeDefaultPresence::Present
        );

        if required != provided {
            return Ok(Some(TraitFulfillmentMismatch::CallableParameterDefault {
                ordinal,
                required,
                provided,
            }));
        }
    }

    Ok(None)
}

fn callable_contract_mismatch(
    values: &bray_symbols::SemanticValueStore,
    subject: bray_symbols::TypeId,
    binding_context: &CompilationBindingContext<'_>,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: CallableSymbolId,
    fulfillment: CallableSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<CallableContractMismatch>, FactQueryError> {
    let requirement = resolve_query!(
        binding_context,
        diagnostics,
        CallableContractsQuery,
        requirement
    );

    let fulfillment = resolve_query!(
        binding_context,
        diagnostics,
        CallableContractsQuery,
        fulfillment
    );

    contract_set_mismatch(
        values,
        subject,
        trait_application,
        generic_substitution,
        requirement.value(),
        fulfillment.value(),
    )
}

fn contract_set_mismatch(
    values: &bray_symbols::SemanticValueStore,
    subject: bray_symbols::TypeId,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &CallableContractSet,
    fulfillment: &CallableContractSet,
) -> Result<Option<CallableContractMismatch>, FactQueryError> {
    if let Some(mismatch) = contract_clause_mismatch(
        values,
        subject,
        trait_application,
        generic_substitution,
        requirement.invocation_preconditions(),
        fulfillment.invocation_preconditions(),
        CallableContractSurface::InvocationPreconditions,
    )? {
        return Ok(Some(mismatch));
    }

    if let Some(mismatch) = contract_clause_mismatch(
        values,
        subject,
        trait_application,
        generic_substitution,
        requirement.static_constraints(),
        fulfillment.static_constraints(),
        CallableContractSurface::StaticConstraints,
    )? {
        return Ok(Some(mismatch));
    }

    if let Some(mismatch) = contract_clause_mismatch(
        values,
        subject,
        trait_application,
        generic_substitution,
        requirement.normal_completion_postconditions(),
        fulfillment.normal_completion_postconditions(),
        CallableContractSurface::CompletionPostconditions,
    )? {
        return Ok(Some(mismatch));
    }

    if let Some(component) = phase_behavior_mismatch(
        values,
        trait_application,
        generic_substitution,
        requirement.invocation_behavior(),
        fulfillment.invocation_behavior(),
    )? {
        return Ok(Some(CallableContractMismatch::Behavior {
            phase: CallableBehaviorPhase::Invocation,
            component,
        }));
    }

    match (
        requirement.deferred_execution_behavior(),
        fulfillment.deferred_execution_behavior(),
    ) {
        (Some(requirement), Some(fulfillment)) => Ok(phase_behavior_mismatch(
            values,
            trait_application,
            generic_substitution,
            requirement,
            fulfillment,
        )?
        .map(|component| CallableContractMismatch::Behavior {
            phase: CallableBehaviorPhase::DeferredExecution,
            component,
        })),
        (None, None) => Ok(None),
        _ => Ok(Some(CallableContractMismatch::DeferredExecutionPresence)),
    }
}

fn contract_clause_mismatch(
    values: &bray_symbols::SemanticValueStore,
    subject: bray_symbols::TypeId,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &[CallableContractClause],
    fulfillment: &[CallableContractClause],
    surface: CallableContractSurface,
) -> Result<Option<CallableContractMismatch>, FactQueryError> {
    if requirement.len() != fulfillment.len() {
        return Ok(Some(CallableContractMismatch::ClauseCount {
            surface,
            required: requirement.len(),
            provided: fulfillment.len(),
        }));
    }

    for (index, (requirement, fulfillment)) in requirement.iter().zip(fulfillment).enumerate() {
        if requirement.ordinal() != fulfillment.ordinal() {
            return Ok(Some(CallableContractMismatch::ClauseOrdinal {
                surface,
                index,
            }));
        }

        if requirement.kind() != fulfillment.kind() {
            return Ok(Some(CallableContractMismatch::ClauseKind {
                surface,
                index,
            }));
        }

        let mismatch = match (requirement.value(), fulfillment.value()) {
            (
                bray_symbols::CallableContractClauseValue::Predicate(requirement),
                bray_symbols::CallableContractClauseValue::Predicate(fulfillment),
            ) => (!dependency_contracts_are_compatible(
                values,
                trait_application,
                generic_substitution,
                requirement.dependency_contract(),
                fulfillment.dependency_contract(),
            )?)
            .then_some(CallableContractMismatch::PredicateDependencies { surface, index }),
            (
                bray_symbols::CallableContractClauseValue::TraitSatisfaction {
                    subject: requirement_subject,
                    application: requirement_application,
                },
                bray_symbols::CallableContractClauseValue::TraitSatisfaction {
                    subject: fulfillment_subject,
                    application: fulfillment_application,
                },
            ) => (!(substitute_requirement_type(
                values,
                subject,
                trait_application,
                generic_substitution,
                requirement_subject,
            )? == fulfillment_subject
                && substitute_requirement_trait_application(
                    values,
                    trait_application,
                    generic_substitution,
                    requirement_application,
                )? == fulfillment_application))
                .then_some(CallableContractMismatch::TraitSatisfaction { surface, index }),
            (required, provided) => Some(CallableContractMismatch::ClauseCategory {
                surface,
                index,
                required: callable_clause_category(&required),
                provided: callable_clause_category(&provided),
            }),
        };

        if mismatch.is_some() {
            return Ok(mismatch);
        }
    }

    Ok(None)
}

fn phase_behavior_mismatch(
    values: &bray_symbols::SemanticValueStore,
    trait_application: TraitApplicationId,
    generic_substitution: Option<GenericSubstitutionId>,
    requirement: &CallablePhaseBehavior,
    fulfillment: &CallablePhaseBehavior,
) -> Result<Option<CallableBehaviorComponent>, FactQueryError> {
    if requirement.effects() != fulfillment.effects() {
        return Ok(Some(CallableBehaviorComponent::Effects));
    }

    if requirement.capabilities() != fulfillment.capabilities() {
        return Ok(Some(CallableBehaviorComponent::Capabilities));
    }

    if fulfillment.trusted_capabilities().iter().any(|provided| {
        !requirement
            .trusted_capabilities()
            .iter()
            .any(|required| required.capability() == provided.capability())
    }) {
        return Ok(Some(CallableBehaviorComponent::TrustedCapabilities));
    }

    if requirement.execution_requirements() != fulfillment.execution_requirements() {
        return Ok(Some(CallableBehaviorComponent::ExecutionRequirements));
    }

    if requirement.lifecycle_obligations() != fulfillment.lifecycle_obligations() {
        return Ok(Some(CallableBehaviorComponent::LifecycleObligations));
    }

    let dependencies_match = dependency_contracts_are_compatible(
        values,
        trait_application,
        generic_substitution,
        requirement.dependency_contract(),
        fulfillment.dependency_contract(),
    )?;

    Ok((!dependencies_match).then_some(CallableBehaviorComponent::Dependencies))
}

const fn callable_clause_category(
    value: &bray_symbols::CallableContractClauseValue,
) -> CallableContractClauseCategory {
    match value {
        bray_symbols::CallableContractClauseValue::Predicate(_) => {
            CallableContractClauseCategory::Predicate
        }
        bray_symbols::CallableContractClauseValue::TraitSatisfaction { .. } => {
            CallableContractClauseCategory::TraitSatisfaction
        }
    }
}

fn predicate_is_compatible(
    context: &CompatibilityContext<'_, '_>,
    requirement: PredicateDefinitionSymbolId,
    fulfillment: PredicateDefinitionSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<TraitFulfillmentMismatch>, FactQueryError> {
    let symbols = context.symbols;
    let imported = context.imported;
    let values = context.values;
    let binding_context = context.binding_context;
    let trait_application = context.trait_application;
    let type_bindings = context.type_bindings;
    let fulfillment_context = fulfillment_self_context(binding_context, fulfillment.into_any())?;

    let requirement_signature = resolve_query!(
        binding_context,
        diagnostics,
        PredicateSignatureTemplateQuery,
        requirement
    );

    let fulfillment_signature = resolve_query!(
        binding_context,
        diagnostics,
        PredicateSignatureTemplateQuery,
        fulfillment
    );

    let (generic_mismatch, generic_substitution) = generic_surfaces_are_compatible(
        values,
        context.subject,
        binding_context,
        trait_application,
        requirement.into_any(),
        fulfillment.into_any(),
        type_bindings,
        diagnostics,
    )?;

    if let Some(mismatch) = generic_mismatch {
        return Ok(Some(TraitFulfillmentMismatch::Generic(mismatch)));
    }

    if requirement_signature.value().is_trusted() != fulfillment_signature.value().is_trusted() {
        return Ok(Some(TraitFulfillmentMismatch::PredicateTrust {
            required: requirement_signature.value().is_trusted(),
            provided: fulfillment_signature.value().is_trusted(),
        }));
    }

    if requirement_signature.value().parameters().len()
        != fulfillment_signature.value().parameters().len()
    {
        return Ok(Some(TraitFulfillmentMismatch::PredicateParameterCount {
            required: requirement_signature.value().parameters().len(),
            provided: fulfillment_signature.value().parameters().len(),
        }));
    }

    for (ordinal, (requirement, fulfillment)) in requirement_signature
        .value()
        .parameters()
        .iter()
        .zip(fulfillment_signature.value().parameters())
        .enumerate()
    {
        let requirement_name = symbols
            .member_name(requirement.parameter().into())
            .or_else(|| {
                imported.and_then(|symbols| symbols.member_name(requirement.parameter().into()))
            })
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol(
                        requirement.parameter().into(),
                    ),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::MemberName,
                    ),
                )
            })?;

        let fulfillment_name = symbols
            .member_name(fulfillment.parameter().into())
            .or_else(|| {
                imported.and_then(|symbols| symbols.member_name(fulfillment.parameter().into()))
            })
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::contract(
                    crate::compilation::SemanticQueryContext::Symbol(
                        fulfillment.parameter().into(),
                    ),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::MemberName,
                    ),
                )
            })?;

        if requirement_name != fulfillment_name {
            return Ok(Some(TraitFulfillmentMismatch::PredicateParameterName {
                ordinal,
                required: requirement_name.as_str().to_owned(),
                provided: fulfillment_name.as_str().to_owned(),
            }));
        }

        if !type_templates_are_compatible(
            values,
            context.subject,
            trait_application,
            fulfillment_context,
            generic_substitution,
            requirement.ty(),
            fulfillment.ty(),
            type_bindings,
        )? {
            return Ok(Some(TraitFulfillmentMismatch::PredicateParameterType {
                ordinal,
                required: requirement.ty().clone(),
                provided: fulfillment.ty().clone(),
            }));
        }
    }

    Ok(None)
}

fn fulfillment_self_context(
    binding_context: &CompilationBindingContext<'_>,
    fulfillment: bray_symbols::AnySymbolId,
) -> Result<Option<bray_symbols::SelfTypeContext>, FactQueryError> {
    binding_context
        .containing_symbol(fulfillment)
        .map(|owner| owner.and_then(bray_symbols::SelfTypeContext::try_new))
        .map_err(binding_query_error)
}
