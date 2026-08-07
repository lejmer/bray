use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::BoundSourceAnchor;
use bray_checker::{
    TargetCallableAbiRequirement, TargetValidityRequest, TargetValidityRequirement,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticType};
use bray_symbols::{
    CallableAbi, CallableExecution, CallableSignatureTemplate, CallableTrust, DeclaredLayoutMode,
    FunctionSymbolId, GenericArgument, NamedTypeSymbolId, SemanticValueStore, StructFieldTypeFact,
    StructSymbolId, SymbolFactRequest, TypeData, TypeExpressionTemplate, TypeId,
};
use bray_syntax::FunctionDeclarationSyntax;

use super::super::super::Compilation;
use super::super::diagnostic::{diagnostic_abi, source_diagnostic, template_diagnostic_type};
use super::abi::{compiler_known_representation, target_abi_value};
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation::foreign) struct CallableBoundarySurface {
    pub(in crate::compilation::foreign) abi: CallableAbi,
    trust: CallableTrust,
    pub(in crate::compilation::foreign) execution: CallableExecution,
    pub(in crate::compilation::foreign) parameters: Vec<TypeExpressionTemplate>,
    pub(in crate::compilation::foreign) result: TypeExpressionTemplate,
}

pub(in crate::compilation::foreign) fn callable_surface(
    values: &SemanticValueStore,
    signature: &CallableSignatureTemplate,
) -> Result<CallableBoundarySurface, FactQueryError> {
    let parameters = signature
        .parameter_type_templates(values)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let (abi, trust, execution) = match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => {
            (callable.abi(), callable.trust(), callable.execution())
        }
        TypeExpressionTemplate::Resolved(ty) => {
            let data = values
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            (callable.abi(), callable.trust(), callable.execution())
        }
        _ => return Err(FactQueryError::InfrastructureFailure),
    };

    Ok(CallableBoundarySurface {
        abi,
        trust,
        execution,
        parameters,
        // The boundary view owns its result template independently of the signature fact.
        result: signature.result().clone(),
    })
}

pub(in crate::compilation::foreign) fn validate_callable_surface(
    compilation: &Compilation,
    function: FunctionSymbolId,
    anchor: bray_declarations::SyntaxAnchor,
    syntax: &FunctionDeclarationSyntax,
    callable: &CallableBoundarySurface,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), FactQueryError> {
    let abi = callable.abi;

    if abi == CallableAbi::Bray {
        return Ok(());
    }

    if callable.execution == CallableExecution::Asynchronous {
        diagnostics.add(source_diagnostic(
            anchor,
            bray_diagnostics::DiagnosticKind::CheckingForeignCallableExecutionUnsupported,
        ));
    }

    let symbols = compilation.symbol_graph()?;

    let record = symbols
        .function(function)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    if !record.generic_type_parameters().is_empty() || !record.generic_const_parameters().is_empty()
    {
        diagnostics.add(
            source_diagnostic(
                anchor,
                bray_diagnostics::DiagnosticKind::CheckingForeignAbiTypeUnsupported,
            )
            .with_arg(DiagnosticArg::actual_type(DiagnosticType::TypeParameter))
            .with_arg(DiagnosticArg::callable_abi(diagnostic_abi(abi))),
        );
    }

    for parameter in &callable.parameters {
        validate_foreign_type(
            compilation,
            parameter,
            abi,
            anchor,
            cancellation,
            diagnostics,
        )?;
    }

    if !is_unit_template(compilation, &callable.result)? {
        validate_foreign_type(
            compilation,
            &callable.result,
            abi,
            anchor,
            cancellation,
            diagnostics,
        )?;
    }

    let source = compilation
        .source(anchor.source_id())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let parameters = callable
        .parameters
        .iter()
        .map(|parameter| target_abi_value(compilation, parameter, cancellation))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let result = if is_unit_template(compilation, &callable.result)? {
        None
    } else {
        target_abi_value(compilation, &callable.result, cancellation)?
    };

    let request = TargetValidityRequest::new(
        BoundSourceAnchor::new(anchor, source.version()),
        TargetValidityRequirement::CallableAbi(TargetCallableAbiRequirement::new(
            abi, parameters, result,
        )),
    );

    let target = compilation.target_validity_with_cancellation(request, cancellation)?;

    diagnostics.add_range(target.diagnostics().iter().cloned());

    if syntax.function_modifiers().extern_token().is_some()
        && callable.trust != CallableTrust::Trusted
    {
        diagnostics.add(source_diagnostic(
            anchor,
            bray_diagnostics::DiagnosticKind::CheckingForeignCallableRequiresTrusted,
        ));
    }

    Ok(())
}

pub(in crate::compilation::foreign) fn validate_platform_service_surface(
    compilation: &Compilation,
    role: bray_runtime_interface::PlatformServiceRole,
    anchor: bray_declarations::SyntaxAnchor,
    callable: &CallableBoundarySurface,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), FactQueryError> {
    let expected = role.signature();

    let parameters_match = callable.parameters.len() == expected.parameters().len()
        && callable
            .parameters
            .iter()
            .zip(expected.parameters())
            .map(|(actual, expected)| {
                platform_abi_type_matches(compilation, actual, *expected, cancellation)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .all(|matches| matches);

    let result_matches = platform_abi_type_matches(
        compilation,
        &callable.result,
        expected.result(),
        cancellation,
    )?;

    if callable.abi != CallableAbi::C
        || callable.execution != CallableExecution::Synchronous
        || !parameters_match
        || !result_matches
    {
        diagnostics.add(source_diagnostic(
            anchor,
            bray_diagnostics::DiagnosticKind::CheckingPlatformServiceSignatureMismatch,
        ));
    }

    Ok(())
}

fn platform_abi_type_matches(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    expected: bray_runtime_interface::PlatformAbiType,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    let Some(ty) = resolve_template_type(compilation, template, cancellation)? else {
        return Ok(false);
    };

    match expected {
        bray_runtime_interface::PlatformAbiType::U32 => {
            type_has_representation(compilation, ty, RepresentationRole::ScalarU32)
        }
        bray_runtime_interface::PlatformAbiType::U64 => {
            type_has_representation(compilation, ty, RepresentationRole::ScalarU64)
        }
        bray_runtime_interface::PlatformAbiType::I64 => {
            type_has_representation(compilation, ty, RepresentationRole::ScalarI64)
        }
        bray_runtime_interface::PlatformAbiType::PointerU8 => {
            raw_pointer_targets(compilation, ty, RepresentationRole::ScalarU8)
        }
        bray_runtime_interface::PlatformAbiType::PointerU32 => {
            raw_pointer_targets(compilation, ty, RepresentationRole::ScalarU32)
        }
        bray_runtime_interface::PlatformAbiType::PointerU64 => {
            raw_pointer_targets(compilation, ty, RepresentationRole::ScalarU64)
        }
        bray_runtime_interface::PlatformAbiType::PointerI64 => {
            raw_pointer_targets(compilation, ty, RepresentationRole::ScalarI64)
        }
        bray_runtime_interface::PlatformAbiType::Path => c_struct_matches(
            compilation,
            ty,
            &[
                AbiField::Pointer(RepresentationRole::ScalarU8),
                AbiField::Scalar(RepresentationRole::ScalarU64),
            ],
            cancellation,
        ),
        bray_runtime_interface::PlatformAbiType::NativeText => c_struct_matches(
            compilation,
            ty,
            &[
                AbiField::Pointer(RepresentationRole::ScalarU8),
                AbiField::Scalar(RepresentationRole::ScalarU64),
            ],
            cancellation,
        ),
        bray_runtime_interface::PlatformAbiType::FileOptions => c_struct_matches(
            compilation,
            ty,
            &[
                AbiField::Scalar(RepresentationRole::ScalarU32),
                AbiField::Scalar(RepresentationRole::ScalarU32),
                AbiField::Scalar(RepresentationRole::ScalarU64),
            ],
            cancellation,
        ),
        bray_runtime_interface::PlatformAbiType::FileMetadataPointer => {
            let Some(metadata) = raw_pointer_target(compilation, ty)? else {
                return Ok(false);
            };

            c_struct_matches(
                compilation,
                metadata,
                &[
                    AbiField::Scalar(RepresentationRole::ScalarU32),
                    AbiField::Scalar(RepresentationRole::ScalarU32),
                    AbiField::Scalar(RepresentationRole::ScalarU64),
                    AbiField::Scalar(RepresentationRole::ScalarI64),
                    AbiField::Scalar(RepresentationRole::ScalarU32),
                    AbiField::Scalar(RepresentationRole::ScalarU32),
                ],
                cancellation,
            )
        }
        bray_runtime_interface::PlatformAbiType::Status => {
            platform_status_matches(compilation, ty, cancellation)
        }
    }
}

fn resolve_template_type(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    cancellation: &CancellationToken,
) -> Result<Option<TypeId>, FactQueryError> {
    let constants = compilation
        .checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

    bray_checker::resolve_type_expression_template(
        compilation.semantic_value_store()?,
        template,
        constants.value(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)
}

fn type_has_representation(
    compilation: &Compilation,
    ty: TypeId,
    expected: RepresentationRole,
) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(false);
    };

    Ok(compiler_known_representation(compilation, *definition) == Some(expected))
}

fn raw_pointer_targets(
    compilation: &Compilation,
    ty: TypeId,
    expected_target: RepresentationRole,
) -> Result<bool, FactQueryError> {
    let Some(target) = raw_pointer_target(compilation, ty)? else {
        return Ok(false);
    };

    type_has_representation(compilation, target, expected_target)
}

fn raw_pointer_target(
    compilation: &Compilation,
    ty: TypeId,
) -> Result<Option<TypeId>, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named {
        definition,
        substitution,
    } = data.as_ref()
    else {
        return Ok(None);
    };

    if compiler_known_representation(compilation, *definition)
        != Some(RepresentationRole::RawPointer)
    {
        return Ok(None);
    }

    let substitution = values
        .generic_substitution_data(*substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let [binding] = substitution.bindings() else {
        return Ok(None);
    };

    let GenericArgument::Type(target) = binding.argument() else {
        return Ok(None);
    };

    Ok(Some(target))
}

#[derive(Clone, Copy)]
enum AbiField {
    Scalar(RepresentationRole),
    Pointer(RepresentationRole),
}

fn c_struct_matches(
    compilation: &Compilation,
    ty: TypeId,
    expected_fields: &[AbiField],
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named {
        definition: NamedTypeSymbolId::Struct(structure),
        substitution,
    } = data.as_ref()
    else {
        return Ok(false);
    };

    let representation = compilation
        .declared_type_representation_with_cancellation((*structure).into(), cancellation)?;

    if representation.value().layout() != DeclaredLayoutMode::C {
        return Ok(false);
    }

    let facts = compilation.binder_facts(cancellation)?;

    let structure = facts
        .symbols()
        .structure(*structure)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    if !structure.generic_type_parameters().is_empty()
        || !structure.generic_const_parameters().is_empty()
        || structure.fields().len() != expected_fields.len()
    {
        return Ok(false);
    }

    let substitution = values
        .generic_substitution_data(*substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    if !substitution.bindings().is_empty() {
        return Ok(false);
    }

    for (field, expected) in structure.fields().iter().zip(expected_fields) {
        let field = facts
            .symbol_fact(SymbolFactRequest::<StructFieldTypeFact>::new(*field))
            .map_err(super::super::super::binder::binder_fact_error)?;

        let Some(field_ty) = resolve_template_type(compilation, field.value(), cancellation)?
        else {
            return Ok(false);
        };

        let matches = match expected {
            AbiField::Scalar(role) => type_has_representation(compilation, field_ty, *role)?,
            AbiField::Pointer(role) => raw_pointer_targets(compilation, field_ty, *role)?,
        };

        if !matches {
            return Ok(false);
        }
    }

    Ok(true)
}

fn platform_status_matches(
    compilation: &Compilation,
    ty: TypeId,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named {
        definition: NamedTypeSymbolId::Struct(structure),
        substitution,
    } = data.as_ref()
    else {
        return Ok(false);
    };

    let representation = compilation
        .declared_type_representation_with_cancellation((*structure).into(), cancellation)?;

    if representation.value().layout() != DeclaredLayoutMode::C {
        return Ok(false);
    }

    let facts = compilation.binder_facts(cancellation)?;

    let structure = facts
        .symbols()
        .structure(*structure)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    if !structure.generic_type_parameters().is_empty()
        || !structure.generic_const_parameters().is_empty()
        || structure.fields().len() != 3
    {
        return Ok(false);
    }

    let substitution = values
        .generic_substitution_data(*substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    if !substitution.bindings().is_empty() {
        return Ok(false);
    }

    for (field, expected) in structure.fields().iter().zip([
        RepresentationRole::ScalarU32,
        RepresentationRole::ScalarU32,
        RepresentationRole::ScalarI64,
    ]) {
        let field = facts
            .symbol_fact(SymbolFactRequest::<StructFieldTypeFact>::new(*field))
            .map_err(super::super::super::binder::binder_fact_error)?;

        let Some(field_ty) = resolve_template_type(compilation, field.value(), cancellation)?
        else {
            return Ok(false);
        };

        if !type_has_representation(compilation, field_ty, expected)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn validate_foreign_type(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    abi: CallableAbi,
    anchor: bray_declarations::SyntaxAnchor,
    cancellation: &CancellationToken,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), FactQueryError> {
    cancellation.check()?;

    if foreign_type_is_supported(compilation, template, abi, cancellation)? {
        return Ok(());
    }

    let diagnostic = source_diagnostic(
        anchor,
        bray_diagnostics::DiagnosticKind::CheckingForeignAbiTypeUnsupported,
    )
    .with_arg(DiagnosticArg::actual_type(template_diagnostic_type(
        compilation,
        template,
    )?))
    .with_arg(DiagnosticArg::callable_abi(diagnostic_abi(abi)));

    diagnostics.add(diagnostic);

    Ok(())
}

fn foreign_type_is_supported(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    abi: CallableAbi,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    match template {
        TypeExpressionTemplate::Resolved(ty) => {
            let data = compilation
                .semantic_value_store()?
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            foreign_type_data_is_supported(compilation, data.as_ref(), abi, cancellation)
        }
        TypeExpressionTemplate::Named { definition, .. } => {
            named_type_is_supported(compilation, *definition, abi, cancellation)
        }
        TypeExpressionTemplate::Callable(callable) => {
            Ok(callable.abi() == abi && callable.execution() == CallableExecution::Synchronous)
        }
        TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::TraitView(_)
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. } => Ok(false),
    }
}

fn foreign_type_data_is_supported(
    compilation: &Compilation,
    data: &TypeData,
    abi: CallableAbi,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    match data {
        TypeData::Named { definition, .. } => {
            named_type_is_supported(compilation, *definition, abi, cancellation)
        }
        TypeData::Callable(callable) => {
            Ok(callable.abi() == abi && callable.execution() == CallableExecution::Synchronous)
        }
        TypeData::Error => Ok(true),
        TypeData::Tuple(_)
        | TypeData::Array { .. }
        | TypeData::Slice(_)
        | TypeData::Generator(_)
        | TypeData::Nullable(_)
        | TypeData::Borrow { .. }
        | TypeData::TraitView(_)
        | TypeData::OwnedIndirection { .. }
        | TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. } => Ok(false),
    }
}

fn named_type_is_supported(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
    abi: CallableAbi,
    cancellation: &CancellationToken,
) -> Result<bool, FactQueryError> {
    if compiler_known_representation(compilation, definition).is_some_and(foreign_representation) {
        return Ok(true);
    }

    let representation =
        compilation.declared_type_representation_with_cancellation(definition, cancellation)?;

    let target = match abi {
        CallableAbi::C => compilation
            .selected_target()
            .target()
            .profile()
            .facts()
            .abis()
            .c_contract(),
        CallableAbi::System => compilation
            .selected_target()
            .target()
            .profile()
            .facts()
            .abis()
            .system_contract(),
        CallableAbi::Bray => None,
    };

    let Some(target) = target else {
        return Ok(false);
    };

    Ok(match representation.value().layout() {
        DeclaredLayoutMode::C => target.c_layout(),
        DeclaredLayoutMode::Transparent => target.transparent_layout(),
        DeclaredLayoutMode::Default | DeclaredLayoutMode::Stable => false,
    })
}

fn is_unit_template(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
) -> Result<bool, FactQueryError> {
    let TypeExpressionTemplate::Resolved(ty) = template else {
        return Ok(false);
    };

    let unit = compilation
        .available_compiler_known_symbols()
        .representation_symbol::<StructSymbolId>(RepresentationRole::Unit)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let data = compilation
        .semantic_value_store()?
        .type_data(*ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    Ok(matches!(
        data.as_ref(),
        TypeData::Named { definition, .. }
            if *definition == NamedTypeSymbolId::Struct(unit)
    ))
}

const fn foreign_representation(role: RepresentationRole) -> bool {
    matches!(
        role,
        RepresentationRole::ScalarBool
            | RepresentationRole::ScalarChar
            | RepresentationRole::ScalarI8
            | RepresentationRole::ScalarI16
            | RepresentationRole::ScalarI32
            | RepresentationRole::ScalarI64
            | RepresentationRole::ScalarI128
            | RepresentationRole::ScalarU8
            | RepresentationRole::ScalarU16
            | RepresentationRole::ScalarU32
            | RepresentationRole::ScalarU64
            | RepresentationRole::ScalarU128
            | RepresentationRole::ScalarIsize
            | RepresentationRole::ScalarUsize
            | RepresentationRole::ScalarR16
            | RepresentationRole::ScalarR32
            | RepresentationRole::ScalarR64
            | RepresentationRole::ScalarR128
            | RepresentationRole::ScalarC32
            | RepresentationRole::ScalarC64
            | RepresentationRole::ScalarC128
            | RepresentationRole::ScalarC256
            | RepresentationRole::RawPointer
    )
}
