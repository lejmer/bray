use bray_symbols::{
    AnySymbolId, ImportedSymbolSkeleton, MemberLookupResult, ModuleSymbolId,
    NamedTraitImplementationSymbolId, SymbolGraph,
};
use bray_syntax::PathSyntax;

use super::ResolvedName;
use super::path::{NameAccess, bind_module_path};

/// Binds one module-level path as a named trait implementation.
pub fn bind_named_trait_implementation_path(
    symbols: &SymbolGraph,
    imported_symbols: Option<&ImportedSymbolSkeleton>,
    module: ModuleSymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> MemberLookupResult<NamedTraitImplementationSymbolId, AnySymbolId> {
    classify_named_trait_implementation(bind_module_path(
        symbols,
        imported_symbols,
        module,
        path,
        access,
    ))
}

fn classify_named_trait_implementation(
    result: MemberLookupResult<ResolvedName>,
) -> MemberLookupResult<NamedTraitImplementationSymbolId, AnySymbolId> {
    match result {
        MemberLookupResult::Found(ResolvedName::Surface(
            AnySymbolId::NamedTraitImplementation(implementation),
        )) => MemberLookupResult::Found(implementation),
        MemberLookupResult::Found(ResolvedName::Surface(candidate)) => {
            MemberLookupResult::WrongKind(Box::new([candidate]))
        }
        MemberLookupResult::Found(ResolvedName::Local(_)) => {
            MemberLookupResult::Malformed(Box::new([]))
        }
        MemberLookupResult::NotFound => MemberLookupResult::NotFound,
        MemberLookupResult::WrongKind(candidates) => MemberLookupResult::WrongKind(
            surface_candidates(candidates).unwrap_or_else(|| Box::new([])),
        ),
        MemberLookupResult::Ambiguous(candidates) => surface_candidates(candidates)
            .map(MemberLookupResult::Ambiguous)
            .unwrap_or_else(|| MemberLookupResult::Malformed(Box::new([]))),
        MemberLookupResult::Inaccessible(candidates) => surface_candidates(candidates)
            .map(MemberLookupResult::Inaccessible)
            .unwrap_or_else(|| MemberLookupResult::Malformed(Box::new([]))),
        MemberLookupResult::Malformed(candidates) => MemberLookupResult::Malformed(
            surface_candidates(candidates).unwrap_or_else(|| Box::new([])),
        ),
    }
}

fn surface_candidates(candidates: Box<[ResolvedName]>) -> Option<Box<[AnySymbolId]>> {
    candidates
        .into_vec()
        .into_iter()
        .map(|candidate| match candidate {
            ResolvedName::Surface(candidate) => Some(candidate),
            ResolvedName::Local(_) => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}

#[cfg(test)]
mod tests {
    use bray_symbols::{AnySymbolId, MemberLookupResult, SymbolKind};

    use super::bind_named_trait_implementation_path;
    use crate::BinderFactContext;
    use crate::fact::test_support::TestFixture;
    use crate::lookup::NameAccess;
    use crate::lookup::test_support::{path, source_module};

    #[test]
    fn direct_imported_declarations_bind_through_their_package_path() {
        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_implementation_path(&imported);
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

        assert_imported_implementation_path(&imported);
    }

    #[test]
    fn imported_declarations_are_not_unqualified_names() {
        let fixture = TestFixture::from_source("module app; const ready: bool = true;");

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        let facts = fixture.context();
        let module = source_module(&facts).0;

        assert_eq!(
            bind_named_trait_implementation_path(
                facts.symbols(),
                Some(&imported.symbols),
                module,
                &path("DisplayVec"),
                NameAccess::Public,
            ),
            MemberLookupResult::NotFound
        );
    }

    #[test]
    fn imported_path_lookup_is_repeatable_and_thread_safe() {
        let fixture = TestFixture::from_source("module app; const ready: bool = true;");

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        let facts = fixture.context();
        let module = source_module(&facts).0;
        let implementation_path = path("dependency.api.DisplayVec");

        let bind = || {
            bind_named_trait_implementation_path(
                facts.symbols(),
                Some(&imported.symbols),
                module,
                &implementation_path,
                NameAccess::Public,
            )
        };

        let expected = bind();

        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(bind);
            let second = scope.spawn(bind);

            (
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first lookup thread must not panic")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second lookup thread must not panic")),
            )
        });

        assert_eq!(first, expected);
        assert_eq!(second, expected);
        assert!(matches!(expected, MemberLookupResult::Found(_)));
    }

    fn named_implementation(symbol: AnySymbolId) -> bray_symbols::NamedTraitImplementationSymbolId {
        match symbol {
            AnySymbolId::NamedTraitImplementation(implementation) => implementation,
            _ => panic!("test declaration must be a named trait implementation"),
        }
    }

    fn assert_imported_implementation_path(
        imported: &bray_symbols::testing::ImportedLookupFixture,
    ) {
        let fixture = TestFixture::from_source("module app; const ready: bool = true;");
        let facts = fixture.context();
        let module = source_module(&facts).0;

        assert_eq!(
            bind_named_trait_implementation_path(
                facts.symbols(),
                Some(&imported.symbols),
                module,
                &path("dependency.api.DisplayVec"),
                NameAccess::Public,
            ),
            MemberLookupResult::Found(named_implementation(imported.declaration))
        );
    }
}
