use bray_bound_tree::{BoundReferenceTarget, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableExecution, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSymbolId, LocalScopeId, LocalSymbolRegionId, SelfTypeContext, SymbolName,
    SymbolQueryRequest, TypeData,
};

use super::BoundUnitBindingError;
use crate::binder::Binder;
use crate::binding::BindingError;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::publication::BoundUnitAssemblyError;
use crate::unit::{BoundUnitConstructionError, BoundUnitLocalBuilder};
use crate::{BindingQueryContext, SymbolQueryProvider};

pub(super) fn create_binder<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<Binder<'_, C>, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
{
    let region = LocalSymbolRegionId::new(unit.raw());
    let start = key.source().syntax().full_range().start();

    let unit = BoundUnitLocalBuilder::new(unit, key, region, start)
        .map_err(|_| BoundUnitBindingError::Construction)?;

    Ok(Binder::new(binding_context, unit))
}

pub(super) fn path_context<C>(
    binder: &Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<PathBindingContext, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
{
    let symbol = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let module = binder
        .binding_context()
        .symbols()
        .containing_module(symbol)
        .ok_or(BoundUnitBindingError::MissingModule)?;

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
) -> Result<CallableExecution, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let callable = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .and_then(CallableSymbolId::try_from_any)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    push_callable_inputs_for(binder, scope, callable)
}

pub(crate) fn push_callable_inputs_for<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    callable: CallableSymbolId,
) -> Result<CallableExecution, BoundUnitBindingError>
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
        .map_err(|_| BoundUnitBindingError::Construction)
}

pub(super) fn insert_callable_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    signature: &CallableSignatureTemplate,
    parameter_count: usize,
) -> Result<(), BoundUnitBindingError>
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
        .map_err(|_| BoundUnitBindingError::Binding)?;

    let parameter_names = signature
        .parameter_names(binder.binding_context().semantic_values())
        .map_err(|_| BoundUnitBindingError::Binding)?;

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
) -> Result<bray_symbols::TypeId, BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
{
    let values = binder.binding_context().semantic_values();

    let data = values
        .type_data(ty)
        .map_err(BoundUnitBindingError::SemanticValue)?;

    let TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) = data.as_ref() else {
        return Ok(ty);
    };

    values
        .intern_open_named_type(binder.binding_context().symbols(), *definition)
        .map_err(BoundUnitBindingError::SemanticValue)?
        .ok_or(BoundUnitBindingError::Binding)
}

pub(super) fn insert_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
) -> Result<(), BoundUnitBindingError>
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
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    insert_named_surface(binder, scope, symbol, name.as_str())
}

pub(super) fn insert_named_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
    name: &str,
) -> Result<(), BoundUnitBindingError>
where
    C: BindingQueryContext + ?Sized,
{
    let name = SymbolName::try_new(name).ok_or(BoundUnitBindingError::MissingOwner)?;

    binder
        .unit_mut()
        .insert_surface_name(scope, name, symbol)
        .map_err(|_| BoundUnitBindingError::Construction)
}

pub(super) fn error_type<C>(
    binding_context: &C,
) -> Result<bray_symbols::TypeId, BoundUnitBindingError>
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

pub(super) fn map_binding_error(error: BindingError) -> BoundUnitBindingError {
    match error {
        BindingError::Cancelled => BoundUnitBindingError::Cancelled,
        BindingError::Construction(BoundUnitConstructionError::BoundTree(_))
        | BindingError::Construction(BoundUnitConstructionError::LocalSymbol(_))
        | BindingError::Construction(BoundUnitConstructionError::LocalAlreadyActivated(_))
        | BindingError::Construction(
            BoundUnitConstructionError::AnonymousCallableBoundaryMismatch
            | BoundUnitConstructionError::AnonymousCallableAlreadyAssigned { .. }
            | BoundUnitConstructionError::AnonymousCallableParameterAlreadyAssigned { .. }
            | BoundUnitConstructionError::AnonymousCallableSourceMismatch { .. }
            | BoundUnitConstructionError::AnonymousCallableSourceVersionMismatch { .. },
        ) => BoundUnitBindingError::Construction,
        BindingError::DependencyUnavailable
        | BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::TransactionContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax => BoundUnitBindingError::Binding,
    }
}

pub(super) fn map_assembly_error(error: BoundUnitAssemblyError) -> BoundUnitBindingError {
    match error {
        BoundUnitAssemblyError::InvalidBoundUnit(_) => BoundUnitBindingError::Assembly,
    }
}

pub(super) const fn map_query_error(error: crate::BindingQueryError) -> BoundUnitBindingError {
    match error {
        crate::BindingQueryError::Cancelled => BoundUnitBindingError::Cancelled,
        crate::BindingQueryError::DependencyUnavailable => BoundUnitBindingError::Binding,
    }
}
