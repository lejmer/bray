use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    BorrowKind, DeclaredCopyContract, GenericArgument, GenericParameterSymbolId, TypeData, TypeId,
};

use crate::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerUnitView,
};

pub(super) struct CopyabilityResolver<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'analysis, C>,
    cache: BTreeMap<TypeId, bool>,
    active: BTreeSet<TypeId>,
    diagnostics: DiagnosticBag,
}

impl<'analysis, C> CopyabilityResolver<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(request: CheckerUnitView<'analysis, C>) -> Self {
        Self {
            request,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(super) fn resolve(&mut self, ty: TypeId) -> CheckerFactResult<bool> {
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

    fn resolve_uncached(&mut self, ty: TypeId) -> CheckerFactResult<bool> {
        let data = self.request.semantic_values().type_data(ty).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
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
                let symbols = self.request.available_compiler_known_symbols();

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
                                | bray_compiler_known::RepresentationRole::RawPointer
                        ));
                }

                let representation = self.request.declared_type_representation(*definition)?;

                // The storage-flow result owns diagnostics independently of representation facts.
                self.diagnostics
                    .add_range(representation.diagnostics().iter().cloned());

                match representation.value().copy_contract() {
                    DeclaredCopyContract::Absent => Ok(false),
                    DeclaredCopyContract::Unconditional => Ok(true),
                    DeclaredCopyContract::Conditional => {
                        let substitution = self
                            .request
                            .semantic_values()
                            .generic_substitution_data(*substitution)
                            .map_err(|_| {
                                CheckerFactError::Infrastructure(
                                    CheckerInfrastructureError::SemanticValueUnavailable,
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
            | TypeData::TypeValuedMemberProjection { .. }
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
