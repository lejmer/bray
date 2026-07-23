use std::collections::BTreeMap;

use bray_compiler_known::{CatalogDeclarationSurfaceSyntax, CatalogSurfaceElement};

use super::CompilerKnownSymbolProvider;
use crate::{
    AnySymbolId, MemberEntry, MemberLookupIndex, MemberLookupResult, MemberValidity,
    MemberVisibility, SymbolKind, SymbolName,
};

pub(super) fn add_member_entry(
    entries: &mut BTreeMap<AnySymbolId, Vec<MemberEntry<AnySymbolId>>>,
    surface: &CatalogDeclarationSurfaceSyntax,
    symbol: AnySymbolId,
    owner: AnySymbolId,
) {
    if has_internal_visibility(surface) {
        return;
    }

    let Some(name) = member_name(surface, symbol.kind()) else {
        return;
    };

    entries.entry(owner).or_default().push(MemberEntry::new(
        symbol,
        name,
        MemberVisibility::Public,
        MemberValidity::Valid,
    ));
}

pub(super) fn build_member_indexes(
    entries: BTreeMap<AnySymbolId, Vec<MemberEntry<AnySymbolId>>>,
) -> BTreeMap<AnySymbolId, MemberLookupIndex<AnySymbolId>> {
    entries
        .into_iter()
        .map(|(owner, entries)| {
            let index = match MemberLookupIndex::new(entries) {
                Ok(index) => index,
                Err(_) => panic!("compiler-known member entries must have distinct identities"),
            };

            (owner, index)
        })
        .collect()
}

impl CompilerKnownSymbolProvider {
    /// Returns one compiler-known member's ordinary name.
    pub fn member_name(&self, owner: AnySymbolId, member: AnySymbolId) -> Option<&SymbolName> {
        self.member_indexes
            .get(&owner)
            .and_then(|index| index.name(member))
    }

    /// Resolves one compiler-known ordinary member through caller-provided visibility policy.
    pub fn lookup_member_with_access(
        &self,
        owner: AnySymbolId,
        name: &str,
        is_accessible: impl FnMut(AnySymbolId, MemberVisibility) -> bool,
    ) -> MemberLookupResult<AnySymbolId> {
        self.member_indexes
            .get(&owner)
            .map_or(MemberLookupResult::NotFound, |index| {
                index.lookup_with_access(name, is_accessible)
            })
    }
}

fn member_name(surface: &CatalogDeclarationSurfaceSyntax, kind: SymbolKind) -> Option<SymbolName> {
    if matches!(
        kind,
        SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::ImplementationOverload
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
            | SymbolKind::TraitScopeEnterFulfillment
            | SymbolKind::TraitScopeExitFulfillment
    ) {
        return None;
    }

    surface.elements().iter().find_map(|element| match element {
        CatalogSurfaceElement::Token(token) if token.is_identifier() => {
            SymbolName::try_new(token.spelling())
        }
        CatalogSurfaceElement::EnterNode(_)
        | CatalogSurfaceElement::Token(_)
        | CatalogSurfaceElement::ExitNode(_) => None,
    })
}

fn has_internal_visibility(surface: &CatalogDeclarationSurfaceSyntax) -> bool {
    surface.elements().iter().any(|element| {
        matches!(
            element,
            CatalogSurfaceElement::Token(token) if token.is_internal_visibility()
        )
    })
}
