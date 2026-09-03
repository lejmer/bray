use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundNodeOrigin, BoundPatternId,
    BoundStructuredExpressionKind,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    DiagnosticExpressionCategory, DiagnosticId, DiagnosticNamedType, DiagnosticType,
    DiagnosticTypeArgument,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AvailableCompilerKnownSymbols, ExternalSymbolKey, ExternalSymbolKeyData, GenericArgument,
    GenericSubstitutionId, NamedTypeSymbolId, SymbolKey, SymbolKeyData, TypeData, TypeId,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

pub(crate) const fn expression_category(
    expression: &BoundExpression,
) -> DiagnosticExpressionCategory {
    match expression {
        BoundExpression::Block(_) => DiagnosticExpressionCategory::Block,
        BoundExpression::Literal(_) => DiagnosticExpressionCategory::Literal,
        BoundExpression::Name(_) | BoundExpression::UnresolvedReference(_) => {
            DiagnosticExpressionCategory::NameReference
        }
        BoundExpression::PatternReference(_) => DiagnosticExpressionCategory::PatternReference,
        BoundExpression::Unary(_) => DiagnosticExpressionCategory::UnaryOperation,
        BoundExpression::Binary(_) => DiagnosticExpressionCategory::BinaryOperation,
        BoundExpression::Assignment(_) => DiagnosticExpressionCategory::Assignment,
        BoundExpression::Call(_) | BoundExpression::ErrorCall(_) => {
            DiagnosticExpressionCategory::Call
        }
        BoundExpression::Conversion(_) | BoundExpression::ErrorConversion(_) => {
            DiagnosticExpressionCategory::Conversion
        }
        BoundExpression::AnonymousCallable(_) => DiagnosticExpressionCategory::AnonymousCallable,
        BoundExpression::Await(_) => DiagnosticExpressionCategory::Await,
        BoundExpression::Structured(expression) => match expression.kind() {
            BoundStructuredExpressionKind::Tuple
            | BoundStructuredExpressionKind::Array
            | BoundStructuredExpressionKind::RepeatedArray
            | BoundStructuredExpressionKind::Range
            | BoundStructuredExpressionKind::Unit
            | BoundStructuredExpressionKind::Absence => DiagnosticExpressionCategory::Aggregate,
            BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex => DiagnosticExpressionCategory::Indexing,
            BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::ResultPropagation => {
                DiagnosticExpressionCategory::Propagation
            }
            BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::GeneralGenerator => {
                DiagnosticExpressionCategory::Generator
            }
            BoundStructuredExpressionKind::TypeFormConstruction => {
                DiagnosticExpressionCategory::Construction
            }
            BoundStructuredExpressionKind::PatternTest
            | BoundStructuredExpressionKind::Condition
            | BoundStructuredExpressionKind::PatternBinding
            | BoundStructuredExpressionKind::Conditional
            | BoundStructuredExpressionKind::While
            | BoundStructuredExpressionKind::Loop
            | BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::Borrow
            | BoundStructuredExpressionKind::TrustBoundary
            | BoundStructuredExpressionKind::Assertion
            | BoundStructuredExpressionKind::Catch
            | BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold
            | BoundStructuredExpressionKind::Panic => DiagnosticExpressionCategory::ControlFlow,
        },
        BoundExpression::StructConstruction(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_) => DiagnosticExpressionCategory::Construction,
        BoundExpression::MemberAccess(_) => DiagnosticExpressionCategory::MemberAccess,
        BoundExpression::TraitQualifiedMember(_) => DiagnosticExpressionCategory::TraitMemberAccess,
        BoundExpression::ControlTransfer(_)
        | BoundExpression::For(_)
        | BoundExpression::Match(_) => DiagnosticExpressionCategory::ControlFlow,
        BoundExpression::Generator(_) => DiagnosticExpressionCategory::Generator,
        BoundExpression::Error(_) => DiagnosticExpressionCategory::Recovered,
    }
}

/// Describes one semantic type through the stable diagnostic vocabulary.
pub fn diagnostic_type<C>(
    context: &C,
    ty: TypeId,
) -> Result<DiagnosticType, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    diagnostic_type_at_depth(context, ty, 0)
}

fn diagnostic_type_at_depth<C>(
    context: &C,
    ty: TypeId,
    depth: usize,
) -> Result<DiagnosticType, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    if depth >= 256 {
        return Ok(DiagnosticType::Unknown);
    }

    let data = context.semantic_values().type_data(ty).map_err(|error| {
        CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
    })?;

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
        TypeData::Array { .. } | TypeData::FlexibleArray(_) => DiagnosticType::Array,
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

pub(crate) fn bound_node_origin<C>(
    request: CheckerUnitView<'_, C>,
    node: AnyBoundNodeId,
) -> Option<BoundNodeOrigin>
where
    C: CheckerRequestContext + ?Sized,
{
    match node {
        AnyBoundNodeId::Expression(expression) => request
            .view()
            .expression(expression)
            .map(BoundExpression::origin),
        AnyBoundNodeId::Pattern(pattern) => request
            .view()
            .pattern(pattern)
            .map(bray_bound_tree::BoundPattern::origin),
        AnyBoundNodeId::Block(block) => request
            .view()
            .block(block)
            .map(bray_bound_tree::BoundBlock::origin),
        AnyBoundNodeId::CallableBody(body) => request
            .view()
            .callable_body(body)
            .copied()
            .map(bray_bound_tree::BoundCallableBody::origin),
    }
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
) -> Result<DiagnosticType, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(Some(name)) = available_diagnostic_value(context.member_name(definition.into_any()))?
    else {
        return Ok(DiagnosticType::Unknown);
    };

    let Some(Some(key)) = available_diagnostic_value(context.symbol_key(definition.into_any()))?
    else {
        return Ok(DiagnosticType::Unknown);
    };

    let substitution = context
        .semantic_values()
        .generic_substitution_data(substitution)
        .map_err(|error| {
            CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
        })?;

    let path = diagnostic_symbol_path(key, name.as_str());

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

fn diagnostic_symbol_path(key: &SymbolKey, name: &str) -> Vec<String> {
    let mut path = Vec::new();

    if let SymbolKeyData::External(key) = key.data() {
        path.push(key.package_identity().as_str().to_owned());
    }

    path.extend(
        symbol_module_path(key)
            .into_iter()
            .flat_map(bray_symbols::ModulePathKey::segments)
            .map(str::to_owned),
    );

    path.push(name.to_owned());

    path
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

fn available_diagnostic_value<T, Upstream>(
    result: crate::CheckerQueryResult<T, Upstream>,
) -> Result<Option<T>, CheckerQueryError<Upstream>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(crate::CheckerQueryError::Cancelled) => Ok(None),
        Err(crate::CheckerQueryError::Infrastructure(error)) => {
            Err(CheckerQueryError::Infrastructure(error))
        }
        Err(crate::CheckerQueryError::Upstream(error)) => Err(CheckerQueryError::Upstream(error)),
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

pub(crate) const fn diagnostic_representation(role: RepresentationRole) -> Option<DiagnosticType> {
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
        | RepresentationRole::Range
        | RepresentationRole::DevicePointer
        | RepresentationRole::Atomic
        | RepresentationRole::Uninit
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

#[cfg(test)]
mod tests {
    use bray_symbols::{
        ExternalSymbolKey, ModulePathKey, PackageIdentity, SymbolKey, SymbolKind, SymbolName,
    };

    use super::diagnostic_symbol_path;

    #[test]
    fn imported_diagnostic_paths_include_the_defining_package() {
        let first = imported_type_path("first.package");
        let second = imported_type_path("second.package");

        assert_eq!(first, ["first.package", "shared", "Value"]);
        assert_eq!(second, ["second.package", "shared", "Value"]);
        assert_ne!(first, second);
    }

    fn imported_type_path(package: &str) -> Vec<String> {
        let package = PackageIdentity::try_new(package)
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let module = ModulePathKey::try_new(["shared"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let module = ExternalSymbolKey::module(ExternalSymbolKey::package(package), module)
            .unwrap_or_else(|| panic!("test module key must be valid"));

        let name = SymbolName::try_new("Value")
            .unwrap_or_else(|| panic!("test symbol name must be valid"));

        let declaration = ExternalSymbolKey::named(module, SymbolKind::Struct, name)
            .unwrap_or_else(|| panic!("test declaration key must be valid"));

        diagnostic_symbol_path(&SymbolKey::external(declaration), "Value")
    }
}
