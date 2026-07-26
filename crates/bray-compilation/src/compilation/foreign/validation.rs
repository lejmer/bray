use bray_bound_tree::BoundSourceAnchor;
use bray_checker::{
    TargetAbiValue, TargetCallableAbiRequirement, TargetValidityRequest, TargetValidityRequirement,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticType};
use bray_symbols::{
    CallableAbi, CallableExecution, CallableSignatureTemplate, CallableTrust, DeclaredLayoutMode,
    FunctionSymbolId, NamedTypeSymbolId, SemanticValueStore, StructSymbolId, TypeData,
    TypeExpressionTemplate,
};
use bray_syntax::FunctionDeclarationSyntax;
use bray_target::TargetScalarKind;

use super::super::Compilation;
use super::diagnostic::{diagnostic_abi, source_diagnostic, template_diagnostic_type};
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct CallableBoundarySurface {
    pub(super) abi: CallableAbi,
    trust: CallableTrust,
    execution: CallableExecution,
    parameters: Vec<TypeExpressionTemplate>,
    result: TypeExpressionTemplate,
}

pub(super) fn callable_surface(
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

pub(super) fn validate_callable_surface(
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
        .map(|parameter| target_abi_value(compilation, parameter))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let result = if is_unit_template(compilation, &callable.result)? {
        None
    } else {
        target_abi_value(compilation, &callable.result)?
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
    if compiler_known_representation_role(compilation, definition).is_some() {
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

fn compiler_known_representation_role(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
) -> Option<RepresentationRole> {
    let NamedTypeSymbolId::Struct(definition) = definition else {
        return None;
    };

    compilation
        .available_compiler_known_symbols()
        .symbol_representation(definition)
        .filter(|role| foreign_representation(*role))
}

fn target_abi_value(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
) -> Result<Option<TargetAbiValue>, FactQueryError> {
    let value = match template {
        TypeExpressionTemplate::Resolved(ty) => {
            let data = compilation
                .semantic_value_store()?
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            target_abi_value_from_data(compilation, data.as_ref())
        }
        TypeExpressionTemplate::Named { definition, .. } => {
            target_abi_value_from_named(compilation, *definition)
        }
        TypeExpressionTemplate::Callable(callable) => {
            Some(TargetAbiValue::Callable(callable.abi()))
        }
        TypeExpressionTemplate::Tuple(_)
        | TypeExpressionTemplate::Array { .. }
        | TypeExpressionTemplate::Slice(_)
        | TypeExpressionTemplate::Nullable(_)
        | TypeExpressionTemplate::Borrow { .. }
        | TypeExpressionTemplate::TraitView(_)
        | TypeExpressionTemplate::OwnedIndirection { .. }
        | TypeExpressionTemplate::TypeValuedMemberProjection { .. } => None,
    };

    Ok(value)
}

fn target_abi_value_from_data(
    compilation: &Compilation,
    data: &TypeData,
) -> Option<TargetAbiValue> {
    match data {
        TypeData::Named { definition, .. } => target_abi_value_from_named(compilation, *definition),
        TypeData::Callable(callable) => Some(TargetAbiValue::Callable(callable.abi())),
        TypeData::Error
        | TypeData::Tuple(_)
        | TypeData::Array { .. }
        | TypeData::Slice(_)
        | TypeData::Generator(_)
        | TypeData::Nullable(_)
        | TypeData::Borrow { .. }
        | TypeData::TraitView(_)
        | TypeData::OwnedIndirection { .. }
        | TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. } => None,
    }
}

fn target_abi_value_from_named(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
) -> Option<TargetAbiValue> {
    let role = compiler_known_representation_role(compilation, definition)?;

    if role == RepresentationRole::RawPointer {
        return Some(TargetAbiValue::RawPointer);
    }

    // TODO(BRA-268): Include finalized aggregate layouts in target ABI requirements.
    target_scalar(role).map(TargetAbiValue::Scalar)
}

const fn target_scalar(role: RepresentationRole) -> Option<TargetScalarKind> {
    match role {
        RepresentationRole::ScalarBool => Some(TargetScalarKind::Bool),
        RepresentationRole::ScalarChar => Some(TargetScalarKind::Char),
        RepresentationRole::ScalarI8 => Some(TargetScalarKind::I8),
        RepresentationRole::ScalarI16 => Some(TargetScalarKind::I16),
        RepresentationRole::ScalarI32 => Some(TargetScalarKind::I32),
        RepresentationRole::ScalarI64 => Some(TargetScalarKind::I64),
        RepresentationRole::ScalarI128 => Some(TargetScalarKind::I128),
        RepresentationRole::ScalarU8 => Some(TargetScalarKind::U8),
        RepresentationRole::ScalarU16 => Some(TargetScalarKind::U16),
        RepresentationRole::ScalarU32 => Some(TargetScalarKind::U32),
        RepresentationRole::ScalarU64 => Some(TargetScalarKind::U64),
        RepresentationRole::ScalarU128 => Some(TargetScalarKind::U128),
        RepresentationRole::ScalarIsize => Some(TargetScalarKind::Isize),
        RepresentationRole::ScalarUsize => Some(TargetScalarKind::Usize),
        RepresentationRole::ScalarR16 => Some(TargetScalarKind::R16),
        RepresentationRole::ScalarR32 => Some(TargetScalarKind::R32),
        RepresentationRole::ScalarR64 => Some(TargetScalarKind::R64),
        RepresentationRole::ScalarR128 => Some(TargetScalarKind::R128),
        RepresentationRole::ScalarC32 => Some(TargetScalarKind::C32),
        RepresentationRole::ScalarC64 => Some(TargetScalarKind::C64),
        RepresentationRole::ScalarC128 => Some(TargetScalarKind::C128),
        RepresentationRole::ScalarC256 => Some(TargetScalarKind::C256),
        RepresentationRole::Unit
        | RepresentationRole::Never
        | RepresentationRole::String
        | RepresentationRole::RawPointer
        | RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::PanicReport
        | RepresentationRole::ConversionError
        | RepresentationRole::Future
        | RepresentationRole::Task
        | RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => None,
    }
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
