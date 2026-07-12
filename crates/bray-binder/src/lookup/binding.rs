use bray_symbols::{
    AnySymbolId, LocalScopeId, MemberLookupIndex, MemberLookupResult, MemberVisibility,
    ModuleSymbolId, SymbolGraph,
};

use super::category::ResolvedName;
use super::path::NameAccess;
use crate::unit::BoundUnitLocalBuilder;

pub(super) type NameLookupResult<T> = MemberLookupResult<T, ResolvedName>;

pub(super) fn lookup_unqualified_name(
    unit: &BoundUnitLocalBuilder,
    symbols: &SymbolGraph,
    scope: LocalScopeId,
    module: ModuleSymbolId,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    let mut current = Some(scope);

    while let Some(scope) = current {
        let locals = match unit.local_symbols_named(scope, name) {
            Ok(locals) => locals,
            Err(_) => return MemberLookupResult::Malformed(Box::new([])),
        };

        let surfaces = match unit.surface_symbols_named(scope, name) {
            Ok(surfaces) => surfaces,
            Err(_) => return MemberLookupResult::Malformed(Box::new([])),
        };

        if !locals.is_empty() || !surfaces.is_empty() {
            let mut has_recovered_local = false;

            for local in locals {
                match unit.local_symbol_is_recovered(*local) {
                    Ok(true) => {
                        has_recovered_local = true;
                        break;
                    }
                    Ok(false) => {}
                    Err(_) => return MemberLookupResult::Malformed(Box::new([])),
                }
            }

            let candidates = locals
                .iter()
                .copied()
                .map(ResolvedName::Local)
                .chain(surfaces.iter().copied().map(ResolvedName::Surface))
                .collect::<Vec<_>>();

            if has_recovered_local {
                return MemberLookupResult::Malformed(candidates.into_boxed_slice());
            }

            return match candidates.as_slice() {
                [candidate] => MemberLookupResult::Found(*candidate),
                _ => MemberLookupResult::Ambiguous(candidates.into_boxed_slice()),
            };
        }

        current = match unit.scope_parent(scope) {
            Ok(parent) => parent,
            Err(_) => return MemberLookupResult::Malformed(Box::new([])),
        };
    }

    lookup_surface_name(symbols, module.into(), name, access)
}

pub(super) fn lookup_surface_name(
    symbols: &SymbolGraph,
    owner: AnySymbolId,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    symbols
        .lookup_member_with_access(owner, name, |_, visibility| access.allows(visibility))
        .map(ResolvedName::Surface, ResolvedName::Surface)
}

pub(super) fn lookup_member_index(
    index: &MemberLookupIndex<AnySymbolId>,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    index
        .lookup_with_access(name, |_, visibility| access.allows(visibility))
        .map(ResolvedName::Surface, ResolvedName::Surface)
}

impl NameAccess {
    pub(super) const fn allows(self, visibility: MemberVisibility) -> bool {
        match self {
            Self::Internal => true,
            Self::Public => visibility.is_public(),
        }
    }
}
