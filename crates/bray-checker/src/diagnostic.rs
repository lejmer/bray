use bray_bound_tree::{BoundExpressionId, BoundNodeOrigin, BoundPatternId};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{DiagnosticId, DiagnosticNamedType, DiagnosticType, DiagnosticTypeArgument};
use bray_source::SourceSpan;
use bray_symbols::{
    AvailableCompilerKnownSymbols, ExternalSymbolKey, ExternalSymbolKeyData, GenericArgument,
    GenericSubstitutionId, NamedTypeSymbolId, SymbolKey, SymbolKeyData, TypeData, TypeId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

/// Describes one semantic type through the stable diagnostic vocabulary.
pub fn diagnostic_type<C>(
    context: &C,
    ty: TypeId,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    diagnostic_type_at_depth(context, ty, 0)
}

fn diagnostic_type_at_depth<C>(
    context: &C,
    ty: TypeId,
    depth: usize,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if depth >= 256 {
        return Ok(DiagnosticType::Unknown);
    }

    let data = context
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let diagnostic = match data.as_ref() {
        TypeData::Error => DiagnosticType::Error,
        TypeData::Named {
            definition,
            substitution,
        } => match diagnostic_named_representation(
            context.available_compiler_known_symbols(),
            *definition,
        ) {
            Some(diagnostic) => diagnostic,
            None => diagnostic_named_application(context, *definition, *substitution, depth + 1)?,
        },
        TypeData::TypeParameter(_) => DiagnosticType::TypeParameter,
        TypeData::ContextualSelf(_) => DiagnosticType::ContextualSelf,
        TypeData::TypeValuedMemberProjection { .. } => DiagnosticType::TypeValuedMember,
        TypeData::Tuple(elements) => {
            let count = u64::try_from(elements.len()).unwrap_or(u64::MAX);

            DiagnosticType::Tuple(count)
        }
        TypeData::Array { .. } => DiagnosticType::Array,
        TypeData::Slice(_) => DiagnosticType::Slice,
        TypeData::Generator(_) => DiagnosticType::Generator,
        TypeData::Nullable(_) => DiagnosticType::Nullable,
        TypeData::Borrow { .. } => DiagnosticType::Borrow,
        TypeData::TraitView(_) => DiagnosticType::TraitView,
        TypeData::OwnedIndirection { .. } => DiagnosticType::OwnedIndirection,
        TypeData::Callable(_) => DiagnosticType::Callable,
    };

    Ok(diagnostic)
}

pub(crate) fn expression_span<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    };

    source_span(request, expression.origin())
}

pub(crate) fn pattern_span<C>(
    request: CheckerUnitView<'_, C>,
    pattern: BoundPatternId,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(pattern) = request.view().pattern(pattern) else {
        return Err(CheckerInfrastructureError::InvalidBoundNode {
            node: pattern.into(),
        });
    };

    source_span(request, pattern.origin())
}

pub(crate) fn diagnostic_id(index: usize) -> DiagnosticId {
    DiagnosticId::from_index(index)
}

fn source_span<C>(
    request: CheckerUnitView<'_, C>,
    origin: BoundNodeOrigin,
) -> Result<SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .source(origin.source_anchor())
        .map(|source| source.span())
}

fn diagnostic_named_application<C>(
    context: &C,
    definition: NamedTypeSymbolId,
    substitution: GenericSubstitutionId,
    depth: usize,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(Some(name)) = available_diagnostic_fact(context.member_name(definition.into_any()))?
    else {
        return Ok(DiagnosticType::Unknown);
    };

    let Some(Some(key)) = available_diagnostic_fact(context.symbol_key(definition.into_any()))?
    else {
        return Ok(DiagnosticType::Unknown);
    };

    let substitution = context
        .semantic_values()
        .generic_substitution_data(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let path = symbol_module_path(key)
        .into_iter()
        .flat_map(bray_symbols::ModulePathKey::segments)
        .map(str::to_owned)
        .chain([name.as_str().to_owned()]);

    let arguments = substitution
        .bindings()
        .iter()
        .map(|binding| match binding.argument() {
            GenericArgument::Type(ty) => {
                diagnostic_type_at_depth(context, ty, depth).map(DiagnosticTypeArgument::Type)
            }
            GenericArgument::Constant(_) => Ok(DiagnosticTypeArgument::Constant),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(DiagnosticType::Named(DiagnosticNamedType::new(
        path, arguments,
    )))
}

fn symbol_module_path(key: &SymbolKey) -> Option<&bray_symbols::ModulePathKey> {
    match key.data() {
        SymbolKeyData::Module { path, .. } => Some(path),
        SymbolKeyData::SourceDeclaration { owner, .. } => symbol_module_path(owner),
        SymbolKeyData::Synthesized(key) => symbol_module_path(key.subject()),
        SymbolKeyData::External(key) => external_symbol_module_path(key),
        SymbolKeyData::Root(_) | SymbolKeyData::CompilerKnownDeclaration { .. } => None,
    }
}

fn external_symbol_module_path(key: &ExternalSymbolKey) -> Option<&bray_symbols::ModulePathKey> {
    match key.data() {
        ExternalSymbolKeyData::Module { path, .. } => Some(path),
        ExternalSymbolKeyData::Declaration { owner, .. }
        | ExternalSymbolKeyData::Synthesized { owner, .. } => external_symbol_module_path(owner),
        ExternalSymbolKeyData::Package(_) => None,
    }
}

fn available_diagnostic_fact<T>(
    result: crate::CheckerFactResult<T>,
) -> Result<Option<T>, CheckerInfrastructureError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(crate::CheckerFactError::Cancelled) => Ok(None),
        Err(crate::CheckerFactError::Infrastructure(error)) => Err(error),
    }
}

fn diagnostic_named_representation(
    available: &AvailableCompilerKnownSymbols,
    definition: NamedTypeSymbolId,
) -> Option<DiagnosticType> {
    let NamedTypeSymbolId::Struct(definition) = definition else {
        return None;
    };

    available
        .symbol_representation(definition)
        .and_then(diagnostic_representation)
}

const fn diagnostic_representation(role: RepresentationRole) -> Option<DiagnosticType> {
    match role {
        RepresentationRole::ScalarBool => Some(DiagnosticType::Boolean),
        RepresentationRole::ScalarChar => Some(DiagnosticType::Character),
        RepresentationRole::ScalarI8 => Some(DiagnosticType::I8),
        RepresentationRole::ScalarI16 => Some(DiagnosticType::I16),
        RepresentationRole::ScalarI32 => Some(DiagnosticType::I32),
        RepresentationRole::ScalarI64 => Some(DiagnosticType::I64),
        RepresentationRole::ScalarI128 => Some(DiagnosticType::I128),
        RepresentationRole::ScalarU8 => Some(DiagnosticType::U8),
        RepresentationRole::ScalarU16 => Some(DiagnosticType::U16),
        RepresentationRole::ScalarU32 => Some(DiagnosticType::U32),
        RepresentationRole::ScalarU64 => Some(DiagnosticType::U64),
        RepresentationRole::ScalarU128 => Some(DiagnosticType::U128),
        RepresentationRole::ScalarIsize => Some(DiagnosticType::Isize),
        RepresentationRole::ScalarUsize => Some(DiagnosticType::Usize),
        RepresentationRole::ScalarR16 => Some(DiagnosticType::R16),
        RepresentationRole::ScalarR32 => Some(DiagnosticType::R32),
        RepresentationRole::ScalarR64 => Some(DiagnosticType::R64),
        RepresentationRole::ScalarR128 => Some(DiagnosticType::R128),
        RepresentationRole::ScalarC32 => Some(DiagnosticType::C32),
        RepresentationRole::ScalarC64 => Some(DiagnosticType::C64),
        RepresentationRole::ScalarC128 => Some(DiagnosticType::C128),
        RepresentationRole::ScalarC256 => Some(DiagnosticType::C256),
        RepresentationRole::Unit => Some(DiagnosticType::Unit),
        RepresentationRole::Never => Some(DiagnosticType::Never),
        RepresentationRole::String => Some(DiagnosticType::String),
        RepresentationRole::RawPointer
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
