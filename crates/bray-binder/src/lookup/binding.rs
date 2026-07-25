use bray_symbols::{
    AnySymbolId, LocalScopeBoundary, LocalScopeId, MemberLookupResult, MemberVisibility,
    ModuleSymbolId, SymbolGraph,
};

use super::category::ResolvedName;
use super::path::NameAccess;
use crate::unit::BoundUnitLocalBuilder;

pub(super) type NameLookupResult<T> = MemberLookupResult<T, ResolvedName>;

pub(crate) fn lookup_unqualified_name(
    unit: &BoundUnitLocalBuilder,
    symbols: &SymbolGraph,
    scope: LocalScopeId,
    module: Option<ModuleSymbolId>,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    let mut current = Some(scope);

    while let Some(scope) = current {
        if name == "result" {
            let result = match unit.postcondition_result(scope) {
                Ok(result) => result,
                Err(_) => return MemberLookupResult::Malformed(Box::new([])),
            };

            if let Some(result) = result {
                let is_recovered = unit.local_symbol_is_recovered(result.into());
                let result = ResolvedName::Local(result.into());

                return match is_recovered {
                    Ok(true) => MemberLookupResult::Malformed(Box::new([result])),
                    Ok(false) => MemberLookupResult::Found(result),
                    Err(_) => MemberLookupResult::Malformed(Box::new([])),
                };
            }
        }

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

            let mut has_recovered_surface = false;

            for surface in surfaces {
                match symbols.symbol_is_recovered(*surface) {
                    Some(true) => {
                        has_recovered_surface = true;
                        break;
                    }
                    Some(false) => {}
                    None => return MemberLookupResult::Malformed(Box::new([])),
                }
            }

            let candidates = locals
                .iter()
                .copied()
                .map(ResolvedName::Local)
                .chain(surfaces.iter().copied().map(ResolvedName::Surface))
                .collect::<Vec<_>>();

            if has_recovered_local || has_recovered_surface {
                return MemberLookupResult::Malformed(candidates.into_boxed_slice());
            }

            return match candidates.as_slice() {
                [candidate] => MemberLookupResult::Found(*candidate),
                _ => MemberLookupResult::Ambiguous(candidates.into_boxed_slice()),
            };
        }

        let boundary = match unit.scope_boundary(scope) {
            Ok(boundary) => boundary,
            Err(_) => return MemberLookupResult::Malformed(Box::new([])),
        };

        if boundary == LocalScopeBoundary::Callable {
            break;
        }

        current = match unit.scope_parent(scope) {
            Ok(parent) => parent,
            Err(_) => return MemberLookupResult::Malformed(Box::new([])),
        };
    }

    let generic_lookup = symbols
        .symbol_for_key(unit.key().declared_owner())
        .map(|owner| lookup_visible_generic_parameter(symbols, owner, name))
        .unwrap_or(MemberLookupResult::NotFound);

    if !matches!(generic_lookup, MemberLookupResult::NotFound) {
        return generic_lookup;
    }

    let module_lookup = module
        .map(|module| lookup_surface_name(symbols, module.into(), name, access))
        .unwrap_or(MemberLookupResult::NotFound);

    let ambient_lookup = lookup_surface_name(
        symbols,
        symbols.compiler_known_environment().id().into(),
        name,
        access,
    );

    combine_name_lookups(module_lookup, ambient_lookup)
}

fn lookup_visible_generic_parameter(
    symbols: &SymbolGraph,
    owner: AnySymbolId,
    name: &str,
) -> NameLookupResult<ResolvedName> {
    let mut current = Some(owner);

    while let Some(owner) = current {
        let lookup =
            lookup_surface_name(symbols, owner, name, NameAccess::Internal).classify(|candidate| {
                match candidate {
                    ResolvedName::Surface(
                        AnySymbolId::GenericTypeParameter(_)
                        | AnySymbolId::GenericConstParameter(_),
                    ) => Some(candidate),
                    ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
                }
            });

        match lookup {
            MemberLookupResult::NotFound | MemberLookupResult::WrongKind(_) => {}
            result => return result,
        }

        current = symbols.containing_symbol(owner);
    }

    MemberLookupResult::NotFound
}

pub(crate) fn lookup_surface_name(
    symbols: &SymbolGraph,
    owner: AnySymbolId,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    symbols
        .lookup_member_with_access(owner, name, |_, visibility| access.allows(visibility))
        .map(ResolvedName::Surface, ResolvedName::Surface)
}

#[cfg(test)]
pub(super) fn lookup_member_index(
    index: &bray_symbols::MemberLookupIndex<AnySymbolId>,
    name: &str,
    is_accessible: impl FnMut(AnySymbolId, bray_symbols::MemberVisibility) -> bool,
) -> NameLookupResult<ResolvedName> {
    index
        .lookup_with_access(name, is_accessible)
        .map(ResolvedName::Surface, ResolvedName::Surface)
}

pub(crate) fn combine_name_lookups(
    first: NameLookupResult<ResolvedName>,
    second: NameLookupResult<ResolvedName>,
) -> NameLookupResult<ResolvedName> {
    let mut accessible = Vec::new();
    let mut inaccessible = Vec::new();
    let mut has_malformed = false;

    collect_lookup_candidates(
        first,
        &mut accessible,
        &mut inaccessible,
        &mut has_malformed,
    );
    collect_lookup_candidates(
        second,
        &mut accessible,
        &mut inaccessible,
        &mut has_malformed,
    );

    accessible.sort_unstable();
    accessible.dedup();
    inaccessible.sort_unstable();
    inaccessible.dedup();

    if has_malformed {
        return MemberLookupResult::Malformed(accessible.into_boxed_slice());
    }

    match accessible.as_slice() {
        [candidate] => MemberLookupResult::Found(*candidate),
        [] if inaccessible.is_empty() => MemberLookupResult::NotFound,
        [] => MemberLookupResult::Inaccessible(inaccessible.into_boxed_slice()),
        _ => MemberLookupResult::Ambiguous(accessible.into_boxed_slice()),
    }
}

fn collect_lookup_candidates(
    result: NameLookupResult<ResolvedName>,
    accessible: &mut Vec<ResolvedName>,
    inaccessible: &mut Vec<ResolvedName>,
    has_malformed: &mut bool,
) {
    match result {
        MemberLookupResult::Found(candidate) => accessible.push(candidate),
        MemberLookupResult::NotFound => {}
        MemberLookupResult::WrongKind(candidates) | MemberLookupResult::Ambiguous(candidates) => {
            accessible.extend(candidates)
        }
        MemberLookupResult::Inaccessible(candidates) => inaccessible.extend(candidates),
        MemberLookupResult::Malformed(candidates) => {
            *has_malformed = true;
            accessible.extend(candidates);
        }
    }
}

impl NameAccess {
    pub(super) const fn allows(self, visibility: MemberVisibility) -> bool {
        match self {
            Self::Internal => true,
            Self::Public => visibility.is_public(),
        }
    }
}
