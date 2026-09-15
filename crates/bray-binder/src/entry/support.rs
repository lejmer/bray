use bray_bound_tree::{BoundReferenceTarget, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableExecution, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSignatureTemplateError, CallableSymbolId, LocalScopeId, LocalSymbolRegionId,
    SelfTypeContext, SymbolName, SymbolQueryRequest, TypeData,
};

use super::BoundUnitBindingError;
use crate::binder::Binder;
use crate::binding::BindingError;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::publication::BoundUnitAssemblyError;
use crate::unit::BoundUnitLocalBuilder;
use crate::{BindingQueryContext, SymbolQueryProvider};

pub(super) fn missing_syntax<Upstream>(key: &BoundUnitKey) -> BoundUnitBindingError<Upstream> {
    BoundUnitBindingError::MissingSyntax {
        source: key.source(),
    }
}

pub(super) fn missing_owner<Upstream>(
    key: &BoundUnitKey,
    symbol: Option<AnySymbolId>,
) -> BoundUnitBindingError<Upstream> {
    // Unit keys and symbol keys are Arc-backed stable identities.
    BoundUnitBindingError::MissingOwner {
        source: key.source(),
        owner: key.declared_owner().clone(),
        symbol,
    }
}

pub(super) fn create_binder<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<Binder<'_, C>, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let region = LocalSymbolRegionId::new(unit.raw());
    let start = key.source().syntax().full_range().start();

    let unit = BoundUnitLocalBuilder::new(unit, key, region, start)
        .map_err(BoundUnitBindingError::Construction)?;

    Ok(Binder::new(binding_context, unit))
}

pub(super) fn path_context<C>(
    binder: &Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<PathBindingContext, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let symbol = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .ok_or_else(|| missing_owner(binder.unit().key(), None))?;

    let module = binder
        .binding_context()
        .symbols()
        .containing_module(symbol)
        .ok_or(BoundUnitBindingError::MissingModule {
            source: binder.unit().key().source(),
            owner: symbol,
        })?;

    Ok(PathBindingContext::new(
        scope,
        module.id(),
        module.owner(),
        NameAccess::Internal,
    ))
}

pub(crate) fn push_callable_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<CallableExecution, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let callable = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .and_then(CallableSymbolId::try_from_any)
        .ok_or_else(|| missing_owner(binder.unit().key(), None))?;

    push_callable_inputs_for(binder, scope, callable)
}

pub(crate) fn push_callable_inputs_for<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    callable: CallableSymbolId,
) -> Result<CallableExecution, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let signature = binder
        .binding_context()
        .symbol_semantics()
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
        .map_err(map_query_error)?;

    insert_callable_inputs(
        binder,
        scope,
        signature.value(),
        signature.value().parameters().len(),
    )?;

    signature
        .value()
        .execution(binder.binding_context().semantic_values())
        .map_err(map_signature_error)
}

pub(super) fn insert_callable_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    signature: &CallableSignatureTemplate,
    parameter_count: usize,
) -> Result<(), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    if let Some(receiver) = signature.receiver() {
        let parameter = receiver.parameter().into();
        let ty = receiver_type(binder, receiver.ty())?;

        insert_named_surface(binder, scope, parameter, "self")?;

        binder.record_value_type(BoundReferenceTarget::Surface(parameter), ty);
    }

    if parameter_count == 0 {
        return Ok(());
    }

    let parameter_types = signature
        .parameter_type_templates(binder.binding_context().semantic_values())
        .map_err(map_signature_error)?;

    let parameter_names = signature
        .parameter_names(binder.binding_context().semantic_values())
        .map_err(map_signature_error)?;

    for ((parameter, name), ty) in signature
        .parameters()
        .iter()
        .zip(parameter_names)
        .zip(parameter_types)
        .take(parameter_count)
    {
        let parameter = (*parameter).into();

        insert_named_surface(binder, scope, parameter, name.as_str())?;

        if let Some(ty) = ty.resolved_type() {
            binder.record_value_type(BoundReferenceTarget::Surface(parameter), ty);
        }
    }

    Ok(())
}

fn receiver_type<C>(
    binder: &Binder<'_, C>,
    ty: bray_symbols::TypeId,
) -> Result<bray_symbols::TypeId, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let values = binder.binding_context().semantic_values();

    let data = values.type_data(ty);

    let TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) = data.as_ref() else {
        return Ok(ty);
    };

    values
        .intern_open_named_type(binder.binding_context().symbols(), *definition)
        .map_err(BoundUnitBindingError::SemanticValue)?
        .ok_or(BoundUnitBindingError::Binding(
            BindingError::SymbolRecordUnavailable((*definition).into_any()),
        ))
}

pub(super) fn insert_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
) -> Result<(), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    if let Some(name) = binder
        .binding_context()
        .symbols()
        .symbol_key(symbol)
        .and_then(bray_symbols::SymbolKey::source_declaration_id)
        .and_then(|declaration| {
            binder
                .binding_context()
                .declarations()
                .declaration(declaration)
        })
        .and_then(bray_declarations::DeclarationRecord::name)
        .and_then(bray_declarations::DeclarationName::as_identifier)
    {
        return insert_named_surface(binder, scope, symbol, name);
    }

    // Insertion mutates the binder after releasing the graph borrow. Symbol names are Arc-backed.
    let name = binder
        .binding_context()
        .symbols()
        .member_name(symbol)
        .cloned()
        .ok_or_else(|| missing_owner(binder.unit().key(), Some(symbol)))?;

    insert_named_surface(binder, scope, symbol, name.as_str())
}

pub(super) fn insert_named_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
    name: &str,
) -> Result<(), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let name = SymbolName::try_new(name).ok_or(BoundUnitBindingError::InvalidSurfaceName {
        source: binder.unit().key().source(),
        symbol,
    })?;

    binder.unit_mut().insert_surface_name(scope, name, symbol);

    Ok(())
}

pub(super) fn error_type<C>(
    binding_context: &C,
) -> Result<bray_symbols::TypeId, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    binding_context
        .semantic_values()
        .intern_type(TypeData::Error)
        .map_err(BoundUnitBindingError::SemanticValue)
}

pub(super) fn anchored_descendant<C, T>(binding_context: &C, anchor: SyntaxAnchor) -> Option<T>
where
    C: BindingQueryContext + ?Sized,
    T: bray_syntax::SyntaxCast,
{
    anchor.find_descendant(binding_context.syntax())
}

pub(super) fn map_binding_error<Upstream>(
    error: BindingError<Upstream>,
) -> BoundUnitBindingError<Upstream> {
    match error {
        BindingError::Cancelled => BoundUnitBindingError::Cancelled,
        BindingError::CheckerInfrastructure(error) => {
            BoundUnitBindingError::CheckerInfrastructure(error)
        }
        BindingError::SemanticValue(error) => BoundUnitBindingError::SemanticValue(error),
        BindingError::Upstream(error) => BoundUnitBindingError::Upstream(error),
        BindingError::Construction(error) => BoundUnitBindingError::Construction(error),
        BindingError::Assembly(error) => BoundUnitBindingError::Assembly(error),
        error @ (BindingError::DependencyUnavailable
        | BindingError::MissingSyntax { .. }
        | BindingError::MissingOwner { .. }
        | BindingError::MissingModule { .. }
        | BindingError::InvalidSurfaceName { .. }
        | BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::TransactionContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax
        | BindingError::SyntaxContract(_)
        | BindingError::GenericOwnerUnavailable(_)
        | BindingError::CompilerKnownRepresentationUnavailable(_)
        | BindingError::SymbolRecordUnavailable(_)
        | BindingError::ModulePartRecordUnavailable(_)
        | BindingError::DeclarationRecordUnavailable(_)
        | BindingError::ContextualSelfUnavailable(_)
        | BindingError::UnresolvedTypeTemplate
        | BindingError::InvalidUnitKey { .. }
        | BindingError::CallableParameterCountMismatch { .. }
        | BindingError::CallableParameterOwnerMismatch { .. }
        | BindingError::ReceiverParameterOwnerMismatch { .. }
        | BindingError::ReceiverContextMismatch { .. }
        | BindingError::CallableTypeExpected { .. }
        | BindingError::CallableTypeTemplateExpected(_)
        | BindingError::CompilerKnownHeapStoragePolicyUnavailable
        | BindingError::ImportedPackageUnavailable(_)
        | BindingError::ImportedPathUnavailable { .. }
        | BindingError::BoundWalkStopped(_)
        | BindingError::CallableSignature(_)
        | BindingError::GenericSubstitution(_)) => BoundUnitBindingError::Binding(error),
    }
}

fn map_signature_error<Upstream>(
    error: CallableSignatureTemplateError,
) -> BoundUnitBindingError<Upstream> {
    BoundUnitBindingError::Binding(BindingError::CallableSignature(error))
}

pub(super) fn map_assembly_error<Upstream>(
    error: BoundUnitAssemblyError,
) -> BoundUnitBindingError<Upstream> {
    BoundUnitBindingError::Assembly(error)
}

pub(super) fn map_query_error<Upstream>(
    error: crate::BindingQueryError<Upstream>,
) -> BoundUnitBindingError<Upstream> {
    match error {
        crate::BindingQueryError::Cancelled => BoundUnitBindingError::Cancelled,
        crate::BindingQueryError::CheckerInfrastructure(error) => {
            BoundUnitBindingError::CheckerInfrastructure(error)
        }
        crate::BindingQueryError::SemanticValue(error) => {
            BoundUnitBindingError::SemanticValue(error)
        }
        crate::BindingQueryError::Upstream(error) => BoundUnitBindingError::Upstream(error),
        crate::BindingQueryError::DependencyUnavailable => {
            BoundUnitBindingError::Binding(BindingError::DependencyUnavailable)
        }
        crate::BindingQueryError::MissingSyntax { source } => {
            BoundUnitBindingError::MissingSyntax { source }
        }
        crate::BindingQueryError::MissingOwner {
            source,
            owner,
            symbol,
        } => BoundUnitBindingError::MissingOwner {
            source,
            owner,
            symbol,
        },
        crate::BindingQueryError::MissingModule { source, owner } => {
            BoundUnitBindingError::MissingModule { source, owner }
        }
        crate::BindingQueryError::InvalidSurfaceName { source, symbol } => {
            BoundUnitBindingError::InvalidSurfaceName { source, symbol }
        }
        crate::BindingQueryError::Construction(error) => BoundUnitBindingError::Construction(error),
        crate::BindingQueryError::Binding(error) => map_binding_error(error),
        crate::BindingQueryError::Assembly(error) => BoundUnitBindingError::Assembly(error),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundTreeBuildError, BoundUnitBuildError, BoundUnitId};

    use super::{map_assembly_error, map_binding_error, map_query_error};
    use crate::{
        BindingError, BindingQueryError, BoundUnitAssemblyError, BoundUnitBindingError,
        BoundUnitConstructionError,
    };

    #[test]
    fn unit_binding_mappers_preserve_exact_local_causes() {
        let construction =
            BoundUnitConstructionError::BoundTree(BoundTreeBuildError::ForeignNode {
                expected: BoundUnitId::new(2),
                actual: BoundUnitId::new(7),
                kind: bray_bound_tree::BoundNodeKind::Pattern,
            });

        let assembly =
            BoundUnitAssemblyError::InvalidBoundUnit(BoundUnitBuildError::RootKindMismatch);

        assert_eq!(
            map_binding_error::<u8>(BindingError::Construction(construction)),
            BoundUnitBindingError::Construction(construction)
        );

        assert_eq!(
            map_binding_error::<u8>(BindingError::ControlTargetMismatch),
            BoundUnitBindingError::Binding(BindingError::ControlTargetMismatch)
        );

        assert_eq!(
            map_assembly_error::<u8>(assembly),
            BoundUnitBindingError::Assembly(assembly)
        );

        assert_eq!(
            map_query_error::<u8>(BindingQueryError::Binding(BindingError::RollbackFailed)),
            BoundUnitBindingError::Binding(BindingError::RollbackFailed)
        );
    }
}
