use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    BorrowKind, DeclaredCopyContract, GenericArgument, GenericParameterSymbolId, TypeData, TypeId,
};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerQueryResult,
    CheckerRequestContext, CheckerUnitView, SemanticUnitContext,
};

/// Checks whether one semantic type has a copy contract in the supplied static context.
pub fn type_is_copyable<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> CheckerOutcome<bool, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let mut resolver = CopyabilityResolver::new(request);

    copyability_outcome(&mut resolver, ty)
}

/// Checks whether one semantic type has a copy contract in an explicit declaration context.
pub fn type_is_copyable_in_context<C>(
    context: &C,
    semantic_context: &SemanticUnitContext,
    ty: TypeId,
) -> CheckerOutcome<bool, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    if context.cancellation().is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let mut resolver = CopyabilityResolver::for_context(context, semantic_context);

    copyability_outcome(&mut resolver, ty)
}

/// Checks whether one closed semantic type has a copy contract.
pub fn closed_type_is_copyable<C>(context: &C, ty: TypeId) -> CheckerOutcome<bool, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    if context.cancellation().is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let mut resolver = CopyabilityResolver::without_context(context);

    copyability_outcome(&mut resolver, ty)
}

fn copyability_outcome<C>(
    resolver: &mut CopyabilityResolver<'_, C>,
    ty: TypeId,
) -> CheckerOutcome<bool, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    match resolver.resolve(ty) {
        Ok(copyable) => {
            let diagnostics = std::mem::take(&mut resolver.diagnostics);

            CheckerOutcome::Complete(bray_diagnostics::DiagnosticResult::new(
                copyable,
                diagnostics,
            ))
        }
        Err(CheckerQueryError::Cancelled) => CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        Err(CheckerQueryError::Upstream(error)) => CheckerOutcome::UpstreamFailure(error),
    }
}

pub(super) struct CopyabilityResolver<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    context: &'analysis C,
    semantic_context: Option<&'analysis SemanticUnitContext>,
    cache: BTreeMap<TypeId, bool>,
    active: BTreeSet<TypeId>,
    diagnostics: DiagnosticBag,
}

impl<'analysis, C> CopyabilityResolver<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(request: CheckerUnitView<'analysis, C>) -> Self {
        Self::for_context(request.context(), request.semantic_context())
    }

    fn for_context(
        context: &'analysis C,
        semantic_context: &'analysis SemanticUnitContext,
    ) -> Self {
        Self {
            context,
            semantic_context: Some(semantic_context),
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn without_context(context: &'analysis C) -> Self {
        Self {
            context,
            semantic_context: None,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(super) fn resolve(&mut self, ty: TypeId) -> CheckerQueryResult<bool, C::UpstreamError> {
        if let Some(copyable) = self.cache.get(&ty) {
            return Ok(*copyable);
        }

        if !self.active.insert(ty) {
            return Ok(false);
        }

        let copyable = self.resolve_uncached(ty)?;

        self.active.remove(&ty);
        self.cache.insert(ty, copyable);

        Ok(copyable)
    }

    pub(super) fn into_parts(self) -> (BTreeSet<TypeId>, DiagnosticBag) {
        let copyable = self
            .cache
            .into_iter()
            .filter_map(|(ty, copyable)| copyable.then_some(ty))
            .collect();

        (copyable, self.diagnostics)
    }

    fn resolve_uncached(&mut self, ty: TypeId) -> CheckerQueryResult<bool, C::UpstreamError> {
        let data = self.context.semantic_values().type_data(ty).map_err(|error| {
            CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
        })?;

        match data.as_ref() {
            TypeData::Error => Ok(true),
            TypeData::Borrow {
                kind: BorrowKind::Shared,
                ..
            }
            | TypeData::Callable(_) => Ok(true),
            TypeData::Tuple(elements) => {
                for element in elements.iter().copied() {
                    if !self.resolve(element)? {
                        return Ok(false);
                    }
                }

                Ok(true)
            }
            TypeData::Array { element, .. } | TypeData::Nullable(element) => self.resolve(*element),
            TypeData::Named {
                definition,
                substitution,
            } => {
                let symbols = self.context.available_compiler_known_symbols();

                if let Some(role) = match definition {
                    bray_symbols::NamedTypeSymbolId::Struct(definition) => {
                        symbols.symbol_representation(*definition)
                    }
                    bray_symbols::NamedTypeSymbolId::Union(definition) => {
                        symbols.symbol_representation(*definition)
                    }
                } {
                    return Ok(role.numeric_kind().is_some()
                        || matches!(
                            role,
                            bray_compiler_known::RepresentationRole::ScalarBool
                                | bray_compiler_known::RepresentationRole::ScalarChar
                                | bray_compiler_known::RepresentationRole::Unit
                                | bray_compiler_known::RepresentationRole::Never
                                | bray_compiler_known::RepresentationRole::String
                                | bray_compiler_known::RepresentationRole::Range
                                | bray_compiler_known::RepresentationRole::RawPointer
                                | bray_compiler_known::RepresentationRole::DevicePointer
                        ));
                }

                let representation = self.context.declared_type_representation(*definition)?;

                // The storage-flow result owns diagnostics independently of representation queries.
                self.diagnostics
                    .add_range(representation.diagnostics().iter().cloned());

                match representation.value().copy_contract() {
                    DeclaredCopyContract::Absent => Ok(false),
                    DeclaredCopyContract::Unconditional => Ok(true),
                    DeclaredCopyContract::Conditional => {
                        let substitution = self
                            .context
                            .semantic_values()
                            .generic_substitution_data(*substitution)
                            .map_err(|error| {
                                CheckerQueryError::Infrastructure(
                                    CheckerInfrastructureError::SemanticValueStore(error),
                                )
                            })?;

                        for parameter in representation.value().copy_dependencies() {
                            let Some(GenericArgument::Type(argument)) = substitution
                                .argument_for(GenericParameterSymbolId::Type(*parameter))
                            else {
                                return Ok(false);
                            };

                            if !self.resolve(argument)? {
                                return Ok(false);
                            }
                        }

                        Ok(true)
                    }
                }
            }
            TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. } => match self.semantic_context {
                Some(semantic_context) => self
                    .context
                    .statically_establishes_copyability(semantic_context, ty),
                None => Ok(false),
            },
            TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Borrow {
                kind: BorrowKind::Mutable,
                ..
            }
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. } => Ok(false),
        }
    }
}
