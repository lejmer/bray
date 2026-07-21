use bray_bound_tree::BoundPatternTarget;
use bray_diagnostics::DiagnosticNameKind;
use bray_source::SourceSnapshot;
use bray_symbols::{
    AnySymbolId, CallableOverloadSymbolId, ImportedSymbolSkeleton, MemberLookupIndex,
    MemberLookupResult, MemberVisibility, ModuleOwnerId, ModulePathKey, ModuleSymbol,
    ModuleSymbolId, SymbolGraph, TraitSymbolId,
};
use bray_syntax::{PathSyntax, SourceSyntaxNode, SyntaxToken};

use super::binding::{
    NameLookupResult, combine_name_lookups, lookup_member_index, lookup_surface_name,
    lookup_unqualified_name,
};
use super::category::{
    ResolvedMemberName, ResolvedName, ResolvedTypeName, ResolvedValueName,
    classify_callable_overload, classify_member, classify_trait, classify_type, classify_value,
};
use super::diagnostic::{NameReference, malformed_lookup, report_lookup_result};
use crate::{BinderFactContext, BinderFactResult, ImportedPathRoot, binder::Binder};

/// Visibility policy for one source name reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameAccess {
    /// Only publicly visible declarations can be resolved.
    Public,
    /// Internal declarations can also be resolved.
    Internal,
}

/// Stable semantic roots used while resolving one path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PathBindingContext {
    scope: bray_symbols::LocalScopeId,
    module: Option<ModuleSymbolId>,
    module_owner: ModuleOwnerId,
    access: NameAccess,
}

impl PathBindingContext {
    pub(crate) const fn new(
        scope: bray_symbols::LocalScopeId,
        module: ModuleSymbolId,
        module_owner: ModuleOwnerId,
        access: NameAccess,
    ) -> Self {
        Self {
            scope,
            module: Some(module),
            module_owner,
            access,
        }
    }

    pub(crate) const fn without_module(
        scope: bray_symbols::LocalScopeId,
        module_owner: ModuleOwnerId,
        access: NameAccess,
    ) -> Self {
        Self {
            scope,
            module: None,
            module_owner,
            access,
        }
    }

    pub(crate) const fn scope(self) -> bray_symbols::LocalScopeId {
        self.scope
    }

    pub(crate) const fn module(self) -> Option<ModuleSymbolId> {
        self.module
    }

    pub(crate) const fn access(self) -> NameAccess {
        self.access
    }

    pub(crate) const fn module_owner(self) -> ModuleOwnerId {
        self.module_owner
    }

    pub(crate) const fn with_scope(self, scope: bray_symbols::LocalScopeId) -> Self {
        Self { scope, ..self }
    }
}

struct PathLookup {
    result: NameLookupResult<ResolvedName>,
    reference: Option<NameReference>,
}

pub(crate) fn bind_module_path<C>(
    facts: &C,
    module: ModuleSymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> BinderFactResult<NameLookupResult<ResolvedName>>
where
    C: BinderFactContext + ?Sized,
{
    let Some(module) = facts.symbols().module(module) else {
        return Ok(malformed_lookup());
    };

    let Some(references) = path_references(path) else {
        return Ok(malformed_lookup());
    };

    let Some(first) = references.first() else {
        return Ok(malformed_lookup());
    };

    let components = references
        .iter()
        .map(NameReference::text)
        .collect::<Vec<_>>();

    let imported_root = facts.imported_path_root(&components)?;
    let compiler_known = facts.symbols().compiler_known_environment();

    let ordinary = combine_name_lookups(
        lookup_surface_name(facts.symbols(), module.id().into(), first.text(), access),
        lookup_surface_name(
            facts.symbols(),
            compiler_known.id().into(),
            first.text(),
            access,
        ),
    );

    Ok(bind_path_with_ordinary(
        facts.symbols(),
        imported_root,
        module.owner(),
        access,
        references,
        ordinary,
    )
    .result)
}

impl<C> Binder<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_pattern_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<BoundPatternTarget>> {
        let lookup = self.bind_path(context, path)?;
        let result = lookup.result.classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::Constant(id)) => {
                Some(BoundPatternTarget::Surface(id.into()))
            }
            ResolvedName::Surface(AnySymbolId::UnionVariant(id)) => {
                Some(BoundPatternTarget::Surface(id.into()))
            }
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        if matches!(
            result,
            MemberLookupResult::Ambiguous(_)
                | MemberLookupResult::Inaccessible(_)
                | MemberLookupResult::Malformed(_)
        ) && let Some(reference) = lookup.reference
        {
            report_lookup_result(self, &reference, DiagnosticNameKind::Pattern, &result);
        }

        Ok(result)
    }

    pub(crate) fn bind_assignment_pattern_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<BoundPatternTarget>> {
        let lookup = self.bind_path(context, path)?;
        let result = lookup.result.map(
            |name| match name {
                ResolvedName::Local(id) => BoundPatternTarget::Local(id),
                ResolvedName::Surface(id) => BoundPatternTarget::Surface(id),
            },
            |name| name,
        );

        if let Some(reference) = lookup.reference {
            report_lookup_result(self, &reference, DiagnosticNameKind::Value, &result);
        }

        Ok(result)
    }

    pub(crate) fn bind_pattern_identifier(
        &mut self,
        context: PathBindingContext,
        source: &SourceSnapshot,
        token: SyntaxToken,
        assignment: bool,
    ) -> NameLookupResult<BoundPatternTarget> {
        let Some(reference) = token_reference(source, token) else {
            return malformed_lookup();
        };

        let lookup = lookup_unqualified_name(
            self.unit(),
            self.facts().symbols(),
            context.scope,
            context.module,
            reference.text(),
            context.access,
        );

        let result = if assignment {
            lookup.map(
                |name| match name {
                    ResolvedName::Local(id) => BoundPatternTarget::Local(id),
                    ResolvedName::Surface(id) => BoundPatternTarget::Surface(id),
                },
                |name| name,
            )
        } else {
            lookup.classify(|name| match name {
                ResolvedName::Surface(AnySymbolId::Constant(id)) => {
                    Some(BoundPatternTarget::Surface(id.into()))
                }
                ResolvedName::Surface(AnySymbolId::UnionVariant(id)) => {
                    Some(BoundPatternTarget::Surface(id.into()))
                }
                ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
            })
        };

        if assignment
            || matches!(
                result,
                MemberLookupResult::Ambiguous(_)
                    | MemberLookupResult::Inaccessible(_)
                    | MemberLookupResult::Malformed(_)
            )
        {
            report_lookup_result(
                self,
                &reference,
                if assignment {
                    DiagnosticNameKind::Value
                } else {
                    DiagnosticNameKind::Pattern
                },
                &result,
            );
        }

        result
    }

    pub(crate) fn bind_module_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<ModuleSymbolId>> {
        let lookup = self.bind_path(context, path)?;

        let result = lookup.result.classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::Module(id)) => Some(id),
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        if let Some(reference) = lookup.reference {
            report_lookup_result(self, &reference, DiagnosticNameKind::Module, &result);
        }

        Ok(result)
    }

    pub(crate) fn bind_type_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<ResolvedTypeName>> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Type, classify_type)
    }

    pub(crate) fn bind_trait_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<TraitSymbolId>> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Trait, classify_trait)
    }

    pub(crate) fn bind_value_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<ResolvedValueName>> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Value, classify_value)
    }

    pub(crate) fn bind_surface_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<AnySymbolId>> {
        self.bind_classified_path(
            context,
            path,
            DiagnosticNameKind::Symbol,
            |name| match name {
                ResolvedName::Surface(symbol) => Some(symbol),
                ResolvedName::Local(_) => None,
            },
        )
    }

    pub(crate) fn bind_reference_identifier(
        &mut self,
        context: PathBindingContext,
        source: &SourceSnapshot,
        token: SyntaxToken,
    ) -> NameLookupResult<ResolvedName> {
        let Some(reference) = token_reference(source, token) else {
            return malformed_lookup();
        };

        let result = lookup_unqualified_name(
            self.unit(),
            self.facts().symbols(),
            context.scope,
            context.module,
            reference.text(),
            context.access,
        );

        report_lookup_result(self, &reference, DiagnosticNameKind::Value, &result);

        result
    }

    pub(crate) fn bind_callable_overload_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<NameLookupResult<CallableOverloadSymbolId>> {
        self.bind_classified_path(
            context,
            path,
            DiagnosticNameKind::CallableOverload,
            classify_callable_overload,
        )
    }

    pub(crate) fn bind_member(
        &mut self,
        owner: AnySymbolId,
        source: &SourceSnapshot,
        token: SyntaxToken,
        access: NameAccess,
    ) -> NameLookupResult<ResolvedMemberName> {
        let Some(reference) = token_reference(source, token) else {
            return malformed_lookup();
        };

        let result = lookup_surface_name(self.facts().symbols(), owner, reference.text(), access)
            .classify(classify_member);

        report_lookup_result(self, &reference, DiagnosticNameKind::Member, &result);

        result
    }

    pub(crate) fn bind_member_from_index(
        &mut self,
        index: &MemberLookupIndex<AnySymbolId>,
        source: &SourceSnapshot,
        token: SyntaxToken,
        is_accessible: impl FnMut(AnySymbolId, MemberVisibility) -> bool,
    ) -> NameLookupResult<ResolvedMemberName> {
        let Some(reference) = token_reference(source, token) else {
            return malformed_lookup();
        };

        let result =
            lookup_member_index(index, reference.text(), is_accessible).classify(classify_member);

        report_lookup_result(self, &reference, DiagnosticNameKind::Member, &result);

        result
    }

    fn bind_classified_path<T>(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
        expected: DiagnosticNameKind,
        classify: fn(ResolvedName) -> Option<T>,
    ) -> BinderFactResult<NameLookupResult<T>> {
        let lookup = self.bind_path(context, path)?;
        let result = lookup.result.classify(classify);

        if let Some(reference) = lookup.reference {
            report_lookup_result(self, &reference, expected, &result);
        }

        Ok(result)
    }

    fn bind_path(
        &self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> BinderFactResult<PathLookup> {
        let Some(references) = path_references(path) else {
            return Ok(PathLookup {
                result: malformed_lookup(),
                reference: None,
            });
        };

        let Some(first) = references.first() else {
            return Ok(PathLookup {
                result: malformed_lookup(),
                reference: None,
            });
        };

        let components = references
            .iter()
            .map(NameReference::text)
            .collect::<Vec<_>>();

        let imported_root = self.facts().imported_path_root(&components)?;

        let ordinary = lookup_unqualified_name(
            self.unit(),
            self.facts().symbols(),
            context.scope,
            context.module,
            first.text(),
            context.access,
        );

        Ok(bind_path_with_ordinary(
            self.facts().symbols(),
            imported_root,
            context.module_owner,
            context.access,
            references,
            ordinary,
        ))
    }
}

fn bind_path_with_ordinary(
    symbols: &SymbolGraph,
    imported_root: Option<ImportedPathRoot<'_>>,
    module_owner: ModuleOwnerId,
    access: NameAccess,
    references: Vec<NameReference>,
    ordinary: NameLookupResult<ResolvedName>,
) -> PathLookup {
    let source_prefix = next_module_prefix(symbols, module_owner, None, &references, access);
    let compiler_known_owner = ModuleOwnerId::from(symbols.compiler_known_environment().id());

    let compiler_known_prefix =
        next_module_prefix(symbols, compiler_known_owner, None, &references, access);

    let imported_prefix = imported_root.map(|root| imported_path_prefix(root, &references, access));

    let prefixes = std::iter::once((ordinary, 1))
        .chain(source_prefix.map(module_prefix_as_path_prefix))
        .chain(compiler_known_prefix.map(module_prefix_as_path_prefix))
        .chain(imported_prefix);

    let (result, consumed) = combine_path_prefixes(prefixes);

    bind_remaining_path(
        symbols,
        imported_root.map(ImportedPathRoot::symbols),
        access,
        references,
        result,
        consumed,
    )
}

fn bind_remaining_path(
    symbols: &SymbolGraph,
    imported_symbols: Option<&ImportedSymbolSkeleton>,
    access: NameAccess,
    references: Vec<NameReference>,
    mut result: NameLookupResult<ResolvedName>,
    mut consumed: usize,
) -> PathLookup {
    while consumed < references.len() {
        let owner = match &result {
            MemberLookupResult::Found(ResolvedName::Surface(owner)) => *owner,
            _ => return path_lookup(result, references, consumed.saturating_sub(1)),
        };

        let ordinary = lookup_surface_name_with_imports(
            symbols,
            imported_symbols,
            owner,
            references[consumed].text(),
            access,
        );

        let module_prefix = match owner {
            AnySymbolId::Module(module) => symbols
                .module(module)
                .and_then(|module| {
                    next_module_prefix(
                        symbols,
                        module.owner(),
                        Some(module.path()),
                        &references[consumed..],
                        access,
                    )
                })
                .or_else(|| {
                    imported_symbols
                        .and_then(|symbols| symbols.module(module))
                        .and_then(|module| match module.owner() {
                            ModuleOwnerId::Package(package) => next_imported_module_prefix(
                                imported_symbols?,
                                package,
                                Some(module.path()),
                                &references[consumed..],
                                access,
                            ),
                            ModuleOwnerId::CompilerKnownEnvironment(_) => None,
                        })
                }),
            AnySymbolId::Package(package) => imported_symbols.and_then(|symbols| {
                next_imported_module_prefix(symbols, package, None, &references[consumed..], access)
            }),
            _ => None,
        };

        let prefixes =
            std::iter::once((ordinary, 1)).chain(module_prefix.map(module_prefix_as_path_prefix));

        let (next, length) = combine_path_prefixes(prefixes);

        result = next;
        consumed += length;
    }

    path_lookup(result, references, consumed.saturating_sub(1))
}

fn lookup_surface_name_with_imports(
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

fn imported_path_prefix(
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

fn next_module_prefix(
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

fn next_imported_module_prefix(
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

fn module_prefix_as_path_prefix((_, length, lookup): ModulePrefixLookup) -> PathPrefixLookup {
    (lookup, length)
}

fn combine_path_prefixes(
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

fn path_lookup(
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

fn path_references(path: &PathSyntax) -> Option<Vec<NameReference>> {
    if path.is_recovered() {
        return None;
    }

    path.identifier_tokens()
        .map(|token| token_reference(path.source(), token))
        .collect()
}

fn token_reference(source: &SourceSnapshot, token: SyntaxToken) -> Option<NameReference> {
    if token.is_missing() {
        return None;
    }

    let text = token.text(source.text())?;

    if text.is_empty() {
        return None;
    }

    Some(NameReference::new(text, source.source_id(), token.range()))
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_symbols::{
        AnyLocalSymbolId, AnySymbolId, LocalSymbolRegionId, MemberEntry, MemberLookupIndex,
        MemberLookupResult, MemberValidity, MemberVisibility, SymbolKind, SymbolName, SymbolOrigin,
    };
    use bray_syntax::{PathSyntax, SourceSyntaxNode, SyntaxKind, SyntaxToken};
    use bray_testing::{test_source_at, test_source_store};

    use super::{NameAccess, PathBindingContext};
    use crate::BinderFactContext;
    use crate::binder::{Binder, BindingContext};
    use crate::fact::test_support::TestFixture as FactFixture;
    use crate::lookup::category::{ResolvedName, ResolvedTypeName, ResolvedValueName};
    use crate::lookup::test_support::{path, source_module, text_range};
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn typed_paths_resolve_modules_declarations_overloads_and_members() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const size: bool = true;\n",
            "struct Point\n",
            "{\n",
            "    x: bool;\n",
            "}\n",
            "trait Display\n",
            "{\n",
            "}\n",
            "func choose_value()\n",
            "{\n",
            "}\n",
            "overload choose =\n",
            "{\n",
            "    choose_value\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(30));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_module_path(context, &path("app")),
            Ok(MemberLookupResult::Found(id)) if id == module
        ));

        assert!(matches!(
            binder.bind_type_path(context, &path("Point")),
            Ok(MemberLookupResult::Found(ResolvedTypeName::Named(_)))
        ));

        assert!(matches!(
            binder.bind_trait_path(context, &path("Display")),
            Ok(MemberLookupResult::Found(_))
        ));

        assert!(matches!(
            binder.bind_value_path(context, &path("size")),
            Ok(MemberLookupResult::Found(ResolvedValueName::Constant(_)))
        ));

        assert!(matches!(
            binder.bind_callable_overload_path(context, &path("choose")),
            Ok(MemberLookupResult::Found(_))
        ));

        let Ok(MemberLookupResult::Found(ResolvedTypeName::Named(structure))) =
            binder.bind_type_path(context, &path("Point"))
        else {
            panic!("Point must bind as a named type");
        };

        let structure = structure.into_any();
        let AnySymbolId::Struct(structure) = structure else {
            panic!("Point must retain its struct identity");
        };

        let member_path = path("x");
        let Some(member_token) = member_path.identifier_tokens().next() else {
            panic!("test member path must contain one identifier");
        };

        assert!(matches!(
            binder.bind_member(
                structure.into(),
                member_path.source(),
                member_token,
                NameAccess::Internal,
            ),
            MemberLookupResult::Found(member)
                if matches!(member.symbol(), AnySymbolId::StructField(_))
        ));

        let Some(structure_record) = facts.symbols().structure(structure) else {
            panic!("Point must resolve to its struct record");
        };

        let [field] = structure_record.fields() else {
            panic!("Point must contain one field");
        };

        let Some(field_name) = SymbolName::try_new("x") else {
            panic!("test field name must be valid");
        };

        let associated = match MemberLookupIndex::new([MemberEntry::new(
            (*field).into(),
            field_name,
            MemberVisibility::Public,
            MemberValidity::Valid,
        )]) {
            Ok(index) => index,
            Err(error) => panic!("test associated-member index must build: {error:?}"),
        };

        let Some(associated_token) = member_path.identifier_tokens().next() else {
            panic!("test member path must contain one identifier");
        };

        assert!(matches!(
            binder.bind_member_from_index(
                &associated,
                member_path.source(),
                associated_token,
                |_, visibility| NameAccess::Internal.allows(visibility),
            ),
            MemberLookupResult::Found(member)
                if matches!(member.symbol(), AnySymbolId::StructField(_))
        ));

        let result = finish(binder);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn imported_packages_modules_and_named_implementations_use_typed_path_lookup() {
        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_surface_path(&imported);
    }

    #[test]
    fn imported_reexports_follow_the_exporting_module_lookup_edge() {
        let imported = bray_symbols::testing::imported_reexport_lookup_fixture(
            "dependency",
            "implementations",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_surface_path(&imported);
    }

    #[test]
    fn lexical_names_take_part_in_typed_value_lookup() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let mut unit = builder(&unit_fixture, LocalSymbolRegionId::new(31));
        let root = unit.root_scope();

        let local = push_binding(&mut unit, root, unit_fixture.first, false);

        assert_eq!(unit.activate_local(root, local), Ok(()));

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert_eq!(
            binder.bind_value_path(context, &path("value")),
            Ok(MemberLookupResult::Found(ResolvedValueName::Local(
                AnyLocalSymbolId::from(local)
            )))
        );

        assert!(finish(binder).diagnostics().is_empty());
    }

    #[test]
    fn source_modules_consult_the_ambient_compiler_known_surface() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(35));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::TypeExpression, unit);

        assert!(matches!(
            binder.bind_type_path(context, &path("bool")),
            Ok(MemberLookupResult::Found(ResolvedTypeName::Named(_)))
        ));

        assert!(matches!(
            binder.bind_type_path(context, &path("Unit")),
            Ok(MemberLookupResult::NotFound)
        ));

        assert_eq!(
            finish(binder)
                .diagnostics()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn source_modules_resolve_compiler_known_module_paths() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(45));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_module_path(context, &path("core.memory")),
            Ok(MemberLookupResult::Found(_))
        ));

        assert!(matches!(
            binder.bind_value_path(context, &path("core.memory.copy")),
            Ok(MemberLookupResult::Found(ResolvedValueName::Function(_)))
        ));

        assert!(finish(binder).diagnostics().is_empty());
    }

    #[test]
    fn recovered_surface_names_in_lexical_scopes_remain_malformed() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const size: bool = true;\n",
            "func broken(",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let mut unit = builder(&unit_fixture, LocalSymbolRegionId::new(36));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let Some(recovered) = facts
            .symbols()
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source && function.is_recovered())
            .map(|function| AnySymbolId::from(function.id()))
        else {
            panic!("test graph must contain one recovered source function");
        };

        let Some(name) = SymbolName::try_new("broken") else {
            panic!("test surface name must be valid");
        };

        assert_eq!(unit.insert_surface_name(root, name, recovered), Ok(()));

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_value_path(context, &path("broken")),
            Ok(MemberLookupResult::Malformed(candidates))
                if candidates.as_ref() == [ResolvedName::Surface(recovered)]
        ));

        let result = finish(binder);

        assert_eq!(
            result
                .diagnostics()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingMalformedName]
        );
    }

    #[test]
    fn recovered_lexical_names_remain_malformed_lookup_candidates() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let mut unit = builder(&unit_fixture, LocalSymbolRegionId::new(34));
        let root = unit.root_scope();

        let local = push_binding(&mut unit, root, unit_fixture.first, true);

        assert_eq!(unit.activate_local(root, local), Ok(()));

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_value_path(context, &path("value")),
            Ok(MemberLookupResult::Malformed(candidates))
                if candidates.as_ref() == [ResolvedName::Local(local.into())]
        ));

        let result = finish(binder);

        assert_eq!(
            result
                .diagnostics()
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingMalformedName]
        );
    }

    #[test]
    fn failed_lookup_preserves_shape_and_emits_structured_diagnostics() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const size: bool = true;\n",
            "internal const secret: bool = true;\n",
            "func duplicate()\n",
            "{\n",
            "}\n",
            "const duplicate: bool = true;\n",
            "func broken(",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(32));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let public = PathBindingContext::new(root, module, owner, NameAccess::Public);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert_eq!(
            binder.bind_type_path(public, &path("Missing")),
            Ok(MemberLookupResult::NotFound)
        );

        assert!(matches!(
            binder.bind_type_path(public, &path("size")),
            Ok(MemberLookupResult::WrongKind(_))
        ));

        assert!(matches!(
            binder.bind_value_path(public, &path("secret")),
            Ok(MemberLookupResult::Inaccessible(_))
        ));

        assert!(matches!(
            binder.bind_value_path(public, &path("duplicate")),
            Ok(MemberLookupResult::Ambiguous(_))
        ));

        assert!(matches!(
            binder.bind_value_path(public, &path("broken")),
            Ok(MemberLookupResult::Malformed(_))
        ));

        let result = finish(binder);

        let kinds = result
            .diagnostics()
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                DiagnosticKind::BindingUnresolvedName,
                DiagnosticKind::BindingWrongNameKind,
                DiagnosticKind::BindingInaccessibleName,
                DiagnosticKind::BindingAmbiguousName,
                DiagnosticKind::BindingMalformedName,
            ]
        );

        let wrong_kind = &result.diagnostics().diagnostics()[1];

        assert_eq!(
            wrong_kind.primary_span().map(|span| span.range()),
            Some(TextRange::new(TextSize::ZERO, TextSize::new(4)))
        );

        assert_eq!(wrong_kind.args().len(), 2);
    }

    #[test]
    fn module_paths_apply_visibility_and_recovery() {
        let internal_fixture = FactFixture::from_source(concat!(
            "internal module hidden;\n",
            "const size: bool = true;",
        ));

        let internal_facts = internal_fixture.context();

        let internal_unit_fixture = fixture();
        let internal_unit = builder(&internal_unit_fixture, LocalSymbolRegionId::new(37));
        let internal_root = internal_unit.root_scope();

        let (internal_module, internal_owner) = source_module(&internal_facts);

        let public = PathBindingContext::new(
            internal_root,
            internal_module,
            internal_owner,
            NameAccess::Public,
        );

        let mut internal_request = Binder::new(
            &internal_facts,
            BindingContext::TypeExpression,
            internal_unit,
        );

        assert!(matches!(
            internal_request.bind_module_path(public, &path("hidden")),
            Ok(MemberLookupResult::Inaccessible(_))
        ));

        let recovered_fixture =
            FactFixture::from_source(concat!("module broken\n", "const size: bool = true;",));

        let recovered_facts = recovered_fixture.context();

        let recovered_unit_fixture = fixture();
        let recovered_unit = builder(&recovered_unit_fixture, LocalSymbolRegionId::new(38));
        let recovered_root = recovered_unit.root_scope();

        let (recovered_module, recovered_owner) = source_module(&recovered_facts);

        let internal = PathBindingContext::new(
            recovered_root,
            recovered_module,
            recovered_owner,
            NameAccess::Internal,
        );

        let mut recovered_request = Binder::new(
            &recovered_facts,
            BindingContext::TypeExpression,
            recovered_unit,
        );

        assert!(matches!(
            recovered_request.bind_module_path(internal, &path("broken")),
            Ok(MemberLookupResult::Malformed(_))
        ));
    }

    #[test]
    fn module_prefixes_do_not_take_precedence_over_ordinary_names() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const app: bool = true;\n",
            "struct Point\n",
            "{\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(39));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_value_path(context, &path("app.Point")),
            Ok(MemberLookupResult::Ambiguous(_))
        ));

        assert_eq!(
            finish(binder).diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::BindingAmbiguousName
        );
    }

    #[test]
    fn each_declared_module_boundary_participates_in_ordinary_lookup() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module foo\n",
            "{\n",
            "    const bar: bool = true;\n",
            "}\n",
            "module foo.bar\n",
            "{\n",
            "    const size: bool = true;\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(42));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_module_path(context, &path("foo.bar")),
            Ok(MemberLookupResult::Ambiguous(_))
        ));

        assert!(matches!(
            binder.bind_value_path(context, &path("foo.bar")),
            Ok(MemberLookupResult::Ambiguous(_))
        ));
    }

    #[test]
    fn undeclared_module_prefixes_do_not_create_synthetic_symbols() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module foo.bar\n",
            "{\n",
            "    const size: bool = true;\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(43));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::TypeExpression, unit);

        assert_eq!(
            binder.bind_module_path(context, &path("foo.bar")),
            Ok(MemberLookupResult::Found(module))
        );

        assert!(finish(binder).diagnostics().is_empty());
    }

    #[test]
    fn inaccessible_shorter_modules_do_not_hide_public_dotted_modules() {
        let fact_fixture = FactFixture::from_source(concat!(
            "internal module foo\n",
            "{\n",
            "    const hidden: bool = true;\n",
            "}\n",
            "module foo.bar\n",
            "{\n",
            "    const size: bool = true;\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(44));
        let root = unit.root_scope();

        let (_, owner) = source_module(&facts);

        let public_module = facts.symbols().modules().iter().find(|module| {
            module.origin() == SymbolOrigin::Source && module.path().segments().eq(["foo", "bar"])
        });

        let Some(public_module) = public_module else {
            panic!("test graph must contain the public dotted module");
        };

        let context = PathBindingContext::new(root, public_module.id(), owner, NameAccess::Public);
        let mut binder = Binder::new(&facts, BindingContext::TypeExpression, unit);

        assert_eq!(
            binder.bind_module_path(context, &path("foo.bar")),
            Ok(MemberLookupResult::Found(public_module.id()))
        );

        assert!(finish(binder).diagnostics().is_empty());
    }

    #[test]
    fn associated_member_lookup_uses_candidate_aware_accessibility() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const size: bool = true;\n",
            "struct Point\n",
            "{\n",
            "    x: bool;\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(40));
        let member_path = path("x");

        let Some(member_token) = member_path.identifier_tokens().next() else {
            panic!("test member path must contain one identifier");
        };

        let Some(name) = SymbolName::try_new("x") else {
            panic!("test member name must be valid");
        };

        let Some(field) = facts.symbols().struct_fields().first() else {
            panic!("test graph must contain one struct field");
        };

        let index = match MemberLookupIndex::new([MemberEntry::new(
            field.id().into(),
            name,
            MemberVisibility::Public,
            MemberValidity::Valid,
        )]) {
            Ok(index) => index,
            Err(error) => panic!("test member index must build: {error:?}"),
        };

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            binder.bind_member_from_index(
                &index,
                member_path.source(),
                member_token,
                |candidate, _| candidate != AnySymbolId::from(field.id()),
            ),
            MemberLookupResult::Inaccessible(_)
        ));
    }

    #[test]
    fn missing_path_syntax_recovers_without_repeating_parser_diagnostics() {
        let fact_fixture = FactFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(33));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::TypeExpression, unit);

        let sources = test_source_store([""]);
        let snapshot = test_source_at(&sources, 0).clone();

        let mut missing = PathSyntax::builder(snapshot);

        missing.push_identifier_token(SyntaxToken::missing(
            SyntaxKind::IdentifierToken,
            TextSize::ZERO,
        ));

        assert!(matches!(
            binder.bind_type_path(context, &missing.build()),
            Ok(MemberLookupResult::Malformed(candidates)) if candidates.is_empty()
        ));

        assert!(finish(binder).diagnostics().is_empty());
    }

    #[test]
    fn missing_middle_path_segments_do_not_bind_repaired_paths() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app;\n",
            "const size: bool = true;\n",
            "struct Point\n",
            "{\n",
            "}",
        ));

        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(41));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut binder = Binder::new(&facts, BindingContext::TypeExpression, unit);

        let malformed = path_with_missing_middle();

        assert!(matches!(
            binder.bind_type_path(context, &malformed),
            Ok(MemberLookupResult::Malformed(candidates)) if candidates.is_empty()
        ));

        assert!(finish(binder).diagnostics().is_empty());
    }

    fn path_with_missing_middle() -> PathSyntax {
        let sources = test_source_store(["app..Point"]);
        let snapshot = test_source_at(&sources, 0).clone();

        let mut builder = PathSyntax::builder(snapshot);

        builder.push_identifier_token(SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            text_range(0, 3),
        ));

        builder.push_dot_token(SyntaxToken::new(SyntaxKind::DotToken, text_range(3, 4)));

        builder.push_identifier_token(SyntaxToken::missing(
            SyntaxKind::IdentifierToken,
            TextSize::new(4),
        ));

        builder.push_dot_token(SyntaxToken::new(SyntaxKind::DotToken, text_range(4, 5)));

        builder.push_identifier_token(SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            text_range(5, 10),
        ));

        builder.build()
    }

    fn assert_imported_surface_path(imported: &bray_symbols::testing::ImportedLookupFixture) {
        let fact_fixture = FactFixture::from_source("module app; const ready: bool = true;");
        let facts = fact_fixture.context_with_imported(&imported.symbols);

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(45));
        let root = unit.root_scope();

        let (module, owner) = source_module(&facts);

        let context = PathBindingContext::new(root, module, owner, NameAccess::Public);
        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let implementation_path = path("dependency.api.DisplayVec");

        assert_eq!(
            binder.bind_surface_path(context, &implementation_path),
            Ok(MemberLookupResult::Found(imported.declaration))
        );

        assert!(finish(binder).diagnostics().is_empty());
    }

    fn finish<C: BinderFactContext + ?Sized>(binder: Binder<'_, C>) -> crate::binder::BinderOutput {
        match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("test binding binder must publish: {error:?}"),
        }
    }
}
