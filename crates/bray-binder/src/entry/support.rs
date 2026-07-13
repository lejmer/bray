use bray_bound_tree::{BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalSymbolRegionId, TypeData};
use bray_syntax::{SyntaxCast, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_tree};

use super::BoundUnitBindingError;
use crate::BinderFactContext;
use crate::binding::BindingError;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::publication::BoundUnitAssemblyError;
use crate::request::{BinderRequestContext, BindingContext};
use crate::unit::{BoundUnitConstructionError, BoundUnitLocalBuilder};

pub(super) fn request<'facts, C>(
    facts: &'facts C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<BinderRequestContext<'facts, C>, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let region = LocalSymbolRegionId::new(unit.raw());
    let start = key.source().syntax().full_range().start();

    let unit = BoundUnitLocalBuilder::new(unit, key, region, start)
        .map_err(|_| BoundUnitBindingError::Construction)?;

    Ok(BinderRequestContext::new(facts, context, unit))
}

pub(super) fn path_context<C>(
    request: &BinderRequestContext<'_, C>,
    scope: bray_symbols::LocalScopeId,
) -> Result<PathBindingContext, BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let declaration = request
        .unit()
        .key()
        .declared_owner()
        .source_declaration_id()
        .ok_or(BoundUnitBindingError::InvalidUnitKey)?;

    let symbol = request
        .facts()
        .symbols()
        .symbol_for_declaration(declaration)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let module = request
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
    T: SyntaxCast,
{
    let mut result = None;
    let mut anchor_depth = None;

    walk_syntax_tree(facts.syntax(), |event| {
        match event {
            SyntaxWalkEvent::EnterNode(node) => {
                if let Some(depth) = anchor_depth.as_mut() {
                    *depth += 1;
                } else if node.source().source_id() == anchor.source_id()
                    && node.kind() == anchor.syntax_kind()
                    && node.full_range() == anchor.full_range()
                {
                    anchor_depth = Some(1);
                }

                if anchor_depth.is_some() && node.kind() == T::KIND {
                    result = node.cast();

                    return SyntaxWalkControl::Stop;
                }
            }
            SyntaxWalkEvent::ExitNode(_) => {
                if let Some(depth) = anchor_depth.as_mut() {
                    *depth -= 1;

                    if *depth == 0 {
                        anchor_depth = None;
                    }
                }
            }
            SyntaxWalkEvent::Token(_) => {}
        }

        SyntaxWalkControl::Continue
    });

    result
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
        BindingError::IdentityCapacityExceeded
        | BindingError::RollbackFailed
        | BindingError::CandidateContextMismatch
        | BindingError::ControlTargetMismatch
        | BindingError::UnsupportedSyntax => BoundUnitBindingError::Binding,
    }
}

pub(super) fn map_assembly_error(error: BoundUnitAssemblyError) -> BoundUnitBindingError {
    match error {
        BoundUnitAssemblyError::InvalidBoundUnit(_) => BoundUnitBindingError::Assembly,
    }
}
