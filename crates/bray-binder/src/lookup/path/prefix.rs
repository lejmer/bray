use bray_source::SourceSnapshot;
use bray_symbols::{
    AnySymbolId, ImportedSymbolSkeleton, MemberLookupResult, ModuleOwnerId, ModulePathKey,
    ModuleSymbol, ModuleSymbolId, SymbolGraph,
};
use bray_syntax::{PathSyntax, SourceSyntaxNode, SyntaxToken};

use super::super::binding::{NameLookupResult, combine_name_lookups, lookup_surface_name};
use super::super::category::ResolvedName;
use super::super::diagnostic::NameReference;
use super::core::NameAccess;
use crate::ImportedPathRoot;

pub(super) struct PathLookup {
    pub(super) result: NameLookupResult<ResolvedName>,
    pub(super) reference: Option<NameReference>,
}

pub(super) fn lookup_surface_name_with_imports(
    symbols: &SymbolGraph,
    imported_symbols: Option<&ImportedSymbolSkeleton>,
    owner: AnySymbolId,
    name: &str,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    if symbols.symbol_key(owner).is_some() {
        return lookup_surface_name(symbols, owner, name, access);
    }

    imported_symbols.map_or(MemberLookupResult::NotFound, |symbols| {
        symbols
            .lookup(owner, name)
            .map(ResolvedName::Surface, ResolvedName::Surface)
    })
}

type ModulePrefixLookup = (ModuleSymbolId, usize, NameLookupResult<ResolvedName>);
type PathPrefixLookup = (NameLookupResult<ResolvedName>, usize);

pub(super) fn imported_path_prefix(
    root: ImportedPathRoot<'_>,
    references: &[NameReference],
    access: NameAccess,
) -> PathPrefixLookup {
    let remaining = &references[root.consumed_components()..];

    next_imported_module_prefix(root.symbols(), root.package(), None, remaining, access).map_or(
        (
            MemberLookupResult::Found(ResolvedName::Surface(root.package().into())),
            root.consumed_components(),
        ),
        |(_, length, lookup)| (lookup, root.consumed_components() + length),
    )
}

pub(super) fn next_module_prefix(
    symbols: &SymbolGraph,
    owner: ModuleOwnerId,
    parent: Option<&ModulePathKey>,
    references: &[NameReference],
    access: NameAccess,
) -> Option<ModulePrefixLookup> {
    let mut malformed = None;
    let mut inaccessible = None;

    // Undeclared prefixes are routes rather than symbols. Prefer the shortest usable declared
    // module, then retain the shortest malformed or inaccessible route when none is usable.
    for length in 1..=references.len() {
        let parent_segments = parent.into_iter().flat_map(ModulePathKey::segments);
        let child_segments = references[..length].iter().map(NameReference::text);
        let path = ModulePathKey::try_new(parent_segments.chain(child_segments))?;

        let Some(module) = symbols.module_by_path(owner, &path) else {
            continue;
        };

        let lookup = module_name_lookup(module, access);

        match &lookup {
            MemberLookupResult::Found(_) => return Some((module.id(), length, lookup)),
            MemberLookupResult::Malformed(_) if malformed.is_none() => {
                malformed = Some((module.id(), length, lookup));
            }
            MemberLookupResult::Inaccessible(_) if inaccessible.is_none() => {
                inaccessible = Some((module.id(), length, lookup));
            }
            MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Malformed(_)
            | MemberLookupResult::Inaccessible(_) => {}
        }
    }

    malformed.or(inaccessible)
}

pub(super) fn source_module_prefix(
    symbols: &SymbolGraph,
    owner: ModuleOwnerId,
    parent: Option<&ModulePathKey>,
    reference: &NameReference,
    access: NameAccess,
) -> NameLookupResult<ResolvedName> {
    next_module_prefix(
        symbols,
        owner,
        parent,
        std::slice::from_ref(reference),
        access,
    )
    .map_or(MemberLookupResult::NotFound, |(_, _, lookup)| lookup)
}

pub(super) fn next_imported_module_prefix(
    symbols: &ImportedSymbolSkeleton,
    package: bray_symbols::PackageSymbolId,
    parent: Option<&ModulePathKey>,
    references: &[NameReference],
    access: NameAccess,
) -> Option<ModulePrefixLookup> {
    for length in 1..=references.len() {
        let parent_segments = parent.into_iter().flat_map(ModulePathKey::segments);
        let child_segments = references[..length].iter().map(NameReference::text);
        let path = ModulePathKey::try_new(parent_segments.chain(child_segments))?;

        let Some(module) = symbols.module_by_path(package, &path) else {
            continue;
        };

        let lookup = module_name_lookup(module, access);

        return Some((module.id(), length, lookup));
    }

    None
}

pub(super) fn module_prefix_as_path_prefix(
    (_, length, lookup): ModulePrefixLookup,
) -> PathPrefixLookup {
    (lookup, length)
}

pub(super) fn combine_path_prefixes(
    prefixes: impl IntoIterator<Item = PathPrefixLookup>,
) -> (NameLookupResult<ResolvedName>, usize) {
    let mut result = MemberLookupResult::NotFound;
    let mut selected = None;

    for (lookup, length) in prefixes {
        let candidate = match &lookup {
            MemberLookupResult::Found(candidate) => Some(*candidate),
            MemberLookupResult::NotFound
            | MemberLookupResult::WrongKind(_)
            | MemberLookupResult::Ambiguous(_)
            | MemberLookupResult::Inaccessible(_)
            | MemberLookupResult::Malformed(_) => None,
        };

        result = combine_name_lookups(result, lookup);

        selected = match (&result, candidate) {
            (MemberLookupResult::Found(found), Some(candidate)) if *found == candidate => {
                Some((candidate, length))
            }
            (MemberLookupResult::Found(found), _) => {
                selected.filter(|(selected, _)| selected == found)
            }
            _ => None,
        };
    }

    let consumed = selected.map_or(1, |(_, length)| length);

    (result, consumed)
}

pub(super) fn path_lookup(
    result: NameLookupResult<ResolvedName>,
    references: Vec<NameReference>,
    reference_index: usize,
) -> PathLookup {
    PathLookup {
        result,
        reference: references.into_iter().nth(reference_index),
    }
}

fn module_name_lookup(module: &ModuleSymbol, access: NameAccess) -> NameLookupResult<ResolvedName> {
    let candidate = ResolvedName::Surface(module.id().into());

    if !access.allows(module.visibility()) {
        MemberLookupResult::Inaccessible(vec![candidate].into_boxed_slice())
    } else if module.is_recovered() {
        MemberLookupResult::Malformed(vec![candidate].into_boxed_slice())
    } else {
        MemberLookupResult::Found(candidate)
    }
}

pub(super) fn path_references(path: &PathSyntax) -> Option<Vec<NameReference>> {
    if path.is_recovered() {
        return None;
    }

    path.identifier_tokens()
        .map(|token| token_reference(path.source(), token))
        .collect()
}

pub(crate) fn token_reference(
    source: &SourceSnapshot,
    token: SyntaxToken,
) -> Option<NameReference> {
    if token.is_missing() {
        return None;
    }

    let text = token.text(source.text())?;

    if text.is_empty() {
        return None;
    }

    Some(NameReference::new(text, source.source_id(), token.range()))
}
