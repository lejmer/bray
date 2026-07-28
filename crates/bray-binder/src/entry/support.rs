use bray_bound_tree::{BoundReferenceTarget, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableExecution, CallableSignatureFact, CallableSignatureTemplate,
    CallableSymbolId, LocalScopeId, LocalSymbolRegionId, SymbolFactRequest, SymbolName, TypeData,
};

use super::BoundUnitBindingError;
use crate::binder::Binder;
use crate::binding::BindingError;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::publication::BoundUnitAssemblyError;
use crate::unit::{BoundUnitConstructionError, BoundUnitLocalBuilder};
use crate::{BinderFactContext, SymbolFactProvider};

pub(super) fn create_binder<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<Binder<'_, C>, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let region = LocalSymbolRegionId::new(unit.raw());
    let start = key.source().syntax().full_range().start();

    let unit = BoundUnitLocalBuilder::new(unit, key, region, start)
        .map_err(|_| BoundUnitBindingError::Construction)?;

    Ok(Binder::new(facts, unit))
}

pub(super) fn path_context<C>(
    binder: &Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<PathBindingContext, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let symbol = binder
        .facts()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let module = binder
        .facts()
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

pub(super) fn push_callable_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<CallableExecution, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let callable = binder
        .facts()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .and_then(CallableSymbolId::try_from_any)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let signature = binder
        .facts()
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
        .map_err(map_fact_error)?;

    insert_callable_inputs(
        binder,
        scope,
        signature.value(),
        signature.value().parameters().len(),
    )?;

    signature
        .value()
        .execution(binder.facts().semantic_values())
        .map_err(|_| BoundUnitBindingError::Construction)
}

pub(super) fn insert_callable_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    signature: &CallableSignatureTemplate,
    parameter_count: usize,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    if let Some(receiver) = signature.receiver() {
        let parameter = receiver.parameter().into();

        insert_named_surface(binder, scope, parameter, "self")?;
        binder.record_value_type(BoundReferenceTarget::Surface(parameter), receiver.ty());
    }

    if parameter_count == 0 {
        return Ok(());
    }

    let parameter_types = signature
        .parameter_type_templates(binder.facts().semantic_values())
        .map_err(|_| BoundUnitBindingError::Binding)?;

    for (parameter, ty) in signature
        .parameters()
        .iter()
        .zip(parameter_types)
        .take(parameter_count)
    {
        let parameter = (*parameter).into();

        insert_source_surface(binder, scope, parameter)?;

        if let Some(ty) = ty.resolved_type() {
            binder.record_value_type(BoundReferenceTarget::Surface(parameter), ty);
        }
    }

    Ok(())
}

pub(super) fn insert_source_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let declaration = binder
        .facts()
        .symbols()
        .symbol_key(symbol)
        .and_then(bray_symbols::SymbolKey::source_declaration_id)
        .and_then(|declaration| binder.facts().declarations().declaration(declaration))
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let name = declaration
        .name()
        .and_then(bray_declarations::DeclarationName::as_identifier)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    insert_named_surface(binder, scope, symbol, name)
}

pub(super) fn insert_named_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
    name: &str,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let name = SymbolName::try_new(name).ok_or(BoundUnitBindingError::MissingOwner)?;

    binder
        .unit_mut()
        .insert_surface_name(scope, name, symbol)
        .map_err(|_| BoundUnitBindingError::Construction)
}

pub(super) fn error_type<C>(facts: &C) -> Result<bray_symbols::TypeId, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    facts
        .semantic_values()
        .intern_type(TypeData::Error)
        .map_err(BoundUnitBindingError::SemanticValue)
}

pub(super) fn anchored_descendant<C, T>(facts: &C, anchor: SyntaxAnchor) -> Option<T>
where
    C: BinderFactContext + ?Sized,
    T: bray_syntax::SyntaxCast,
{
    anchor.find_descendant(facts.syntax())
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

pub(super) const fn map_fact_error(error: crate::BinderFactError) -> BoundUnitBindingError {
    match error {
        crate::BinderFactError::Cancelled => BoundUnitBindingError::Cancelled,
        crate::BinderFactError::DependencyUnavailable => BoundUnitBindingError::Binding,
    }
}
