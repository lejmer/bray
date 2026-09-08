use std::sync::Arc;

use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, CallableSymbolId, DependencyRequirementKind,
    ExactSymbolId, GenericParameterSymbolId, ImplementationSymbolId, NamedTypeSymbolId, SymbolKey,
};

use crate::{
    InterfaceDependencyRequirementKind, InterfaceSemanticInternError, InterfaceSymbolReference,
    InterfaceSymbolResolver,
};

pub(super) fn resolve_symbol(
    symbols: &impl InterfaceSymbolResolver,
    reference: &InterfaceSymbolReference,
) -> Result<AnySymbolId, InterfaceSemanticInternError> {
    // Resolution errors own an Arc-backed reference so they can cross the loading boundary.
    symbols
        .resolve(reference)
        .ok_or_else(|| InterfaceSemanticInternError::UnresolvedSymbol(reference.clone()))
}

pub(super) fn resolve_symbol_key(
    symbols: &impl InterfaceSymbolResolver,
    reference: &InterfaceSymbolReference,
) -> Result<SymbolKey, InterfaceSemanticInternError> {
    // Interface references are Arc-backed and errors own their stable reference.
    symbols
        .symbol_key(reference)
        .ok_or_else(|| InterfaceSemanticInternError::UnresolvedSymbol(reference.clone()))
}

pub(super) fn resolve_exact<I: ExactSymbolId>(
    symbols: &impl InterfaceSymbolResolver,
    reference: &InterfaceSymbolReference,
) -> Result<I, InterfaceSemanticInternError> {
    let symbol = resolve_symbol(symbols, reference)?;

    I::try_from_any(symbol).ok_or_else(|| invalid_symbol(reference))
}

pub(super) trait SymbolFamily: Sized {
    fn try_from_any(id: AnySymbolId) -> Option<Self>;
}

impl SymbolFamily for NamedTypeSymbolId {
    fn try_from_any(id: AnySymbolId) -> Option<Self> {
        Self::try_from_any(id)
    }
}

impl SymbolFamily for ImplementationSymbolId {
    fn try_from_any(id: AnySymbolId) -> Option<Self> {
        Self::try_from_any(id)
    }
}

impl SymbolFamily for CallableSymbolId {
    fn try_from_any(id: AnySymbolId) -> Option<Self> {
        Self::try_from_any(id)
    }
}

impl SymbolFamily for GenericParameterSymbolId {
    fn try_from_any(id: AnySymbolId) -> Option<Self> {
        Self::try_from_any(id)
    }
}

pub(super) fn resolve_family<I: SymbolFamily>(
    symbols: &impl InterfaceSymbolResolver,
    reference: &InterfaceSymbolReference,
) -> Result<I, InterfaceSemanticInternError> {
    let symbol = resolve_symbol(symbols, reference)?;

    I::try_from_any(symbol).ok_or_else(|| invalid_symbol(reference))
}

pub(super) fn resolve_constant_definition(
    symbols: &impl InterfaceSymbolResolver,
    reference: &InterfaceSymbolReference,
) -> Result<AnyConstantDefinitionId, InterfaceSemanticInternError> {
    let symbol = resolve_symbol(symbols, reference)?;

    match symbol {
        AnySymbolId::Constant(id) => Ok(id.into()),
        AnySymbolId::TraitConstantMember(id) => Ok(id.into()),
        AnySymbolId::TraitConstantFulfillment(id) => Ok(id.into()),
        _ => Err(invalid_symbol(reference)),
    }
}

pub(super) fn invalid_symbol(reference: &InterfaceSymbolReference) -> InterfaceSemanticInternError {
    // The error owns this Arc-backed reference independently of the decoded table.
    InterfaceSemanticInternError::InvalidSymbolKind(reference.clone())
}

pub(super) fn convert_requirement_kind(
    input: InterfaceDependencyRequirementKind,
) -> DependencyRequirementKind {
    match input {
        InterfaceDependencyRequirementKind::StorageAlive => DependencyRequirementKind::StorageAlive,
        InterfaceDependencyRequirementKind::StorageInitialized => {
            DependencyRequirementKind::StorageInitialized
        }
        InterfaceDependencyRequirementKind::ExclusiveMutationAuthority => {
            DependencyRequirementKind::ExclusiveMutationAuthority
        }
        InterfaceDependencyRequirementKind::BorrowCapabilityActive(kind) => {
            DependencyRequirementKind::BorrowCapabilityActive(kind)
        }
        InterfaceDependencyRequirementKind::ScopedCapabilityLive => {
            DependencyRequirementKind::ScopedCapabilityLive
        }
        InterfaceDependencyRequirementKind::LifecycleObligation(kind) => {
            DependencyRequirementKind::LifecycleObligation(kind)
        }
    }
}

pub(super) fn collect_ids<I: Copy, O>(
    values: &[I],
    resolve: impl Fn(&I) -> Option<O>,
) -> Option<Vec<O>> {
    values.iter().map(resolve).collect()
}

pub(super) fn lookup<I: Copy>(table: &[Option<I>], index: Option<usize>) -> Option<I> {
    table.get(index?)?.as_ref().copied()
}

pub(super) fn resolve_optional_id<I, O>(
    id: Option<I>,
    resolve: impl FnOnce(I) -> Option<O>,
) -> Option<Option<O>> {
    match id {
        Some(id) => resolve(id).map(Some),
        None => Some(None),
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_optional_id;

    #[test]
    fn optional_identity_resolution_distinguishes_absence_from_an_unresolved_reference() {
        assert_eq!(resolve_optional_id(None::<u32>, |_| Some(1)), Some(None));

        assert_eq!(
            resolve_optional_id(Some(2), |id| Some(id + 1)),
            Some(Some(3))
        );

        assert_eq!(resolve_optional_id(Some(2), |_| None::<u32>), None);
    }
}

pub(super) fn finish_table<I>(
    table: Vec<Option<I>>,
) -> Result<Arc<[I]>, InterfaceSemanticInternError> {
    table
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .map(Arc::from)
        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
}
