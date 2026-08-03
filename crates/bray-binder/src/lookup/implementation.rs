use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticNameKind, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, MemberLookupResult, ModuleSymbolId, NamedTraitImplementationSymbolId,
};
use bray_syntax::{PathSyntax, SourceSyntaxNode, UsingDeclarationSyntax};

use super::ResolvedName;
use super::diagnostic::lookup_diagnostic;
use super::path::token_reference;
use super::path::{NameAccess, bind_module_path};
use crate::{BinderFactContext, BinderFactResult};

/// A named implementation selected by one explicit source `using` declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundImplementationUsing {
    implementation: NamedTraitImplementationSymbolId,
    using_declaration: SyntaxAnchor,
}

impl BoundImplementationUsing {
    /// Returns the exact named implementation selected by the declaration.
    pub const fn implementation(self) -> NamedTraitImplementationSymbolId {
        self.implementation
    }

    /// Returns the source declaration that selected the implementation.
    pub const fn using_declaration(self) -> SyntaxAnchor {
        self.using_declaration
    }
}

/// Binds one module-level path as a named trait implementation.
pub fn bind_named_trait_implementation_path<C>(
    facts: &C,
    module: ModuleSymbolId,
    path: &PathSyntax,
    access: NameAccess,
) -> BinderFactResult<MemberLookupResult<NamedTraitImplementationSymbolId, AnySymbolId>>
where
    C: BinderFactContext + ?Sized,
{
    Ok(classify_named_trait_implementation(bind_module_path(
        facts, module, path, access,
    )?))
}

/// Binds one explicit `using` declaration when it names a trait implementation.
///
/// Valid `using` declarations for other symbol categories produce no implementation
/// activation and no diagnostic.
pub fn bind_implementation_using<C>(
    facts: &C,
    module: ModuleSymbolId,
    declaration: &UsingDeclarationSyntax,
) -> BinderFactResult<DiagnosticResult<Option<BoundImplementationUsing>>>
where
    C: BinderFactContext + ?Sized,
{
    if facts.is_cancelled() {
        return Err(crate::BinderFactError::Cancelled);
    }

    let access = if declaration.internal_keyword().is_some() {
        NameAccess::Internal
    } else {
        NameAccess::Public
    };

    let path = declaration.path();
    let result = bind_named_trait_implementation_path(facts, module, &path, access)?;

    let value = match &result {
        MemberLookupResult::Found(implementation) => Some(BoundImplementationUsing {
            implementation: *implementation,
            using_declaration: SyntaxAnchor::from_node(declaration),
        }),
        MemberLookupResult::NotFound
        | MemberLookupResult::WrongKind(_)
        | MemberLookupResult::Ambiguous(_)
        | MemberLookupResult::Inaccessible(_)
        | MemberLookupResult::Malformed(_) => None,
    };

    let diagnostic = match &result {
        MemberLookupResult::Ambiguous(candidates)
        | MemberLookupResult::Inaccessible(candidates)
            if contains_named_trait_implementation(candidates) =>
        {
            path.identifier_tokens()
                .last()
                .and_then(|token| token_reference(path.source(), token))
                .and_then(|reference| {
                    lookup_diagnostic(&reference, DiagnosticNameKind::Member, &result)
                })
        }
        MemberLookupResult::Found(_)
        | MemberLookupResult::NotFound
        | MemberLookupResult::WrongKind(_)
        | MemberLookupResult::Ambiguous(_)
        | MemberLookupResult::Inaccessible(_)
        | MemberLookupResult::Malformed(_) => None,
    };

    Ok(DiagnosticResult::new(
        value,
        diagnostic.map_or_else(DiagnosticBag::new, DiagnosticBag::single),
    ))
}

fn contains_named_trait_implementation(candidates: &[AnySymbolId]) -> bool {
    candidates
        .iter()
        .any(|candidate| matches!(candidate, AnySymbolId::NamedTraitImplementation(_)))
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

    use super::{bind_implementation_using, bind_named_trait_implementation_path};
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

        assert_imported_implementation_path(&imported, "dependency.api.DisplayVec");
    }

    #[test]
    fn explicit_usings_bind_imported_named_implementations_with_their_source_anchor() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "using dependency.api.DisplayVec;\n",
            "const ready: bool = true;\n",
        ));

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;
        let declaration = using_declaration(&facts);

        let result = bind_implementation_using(&facts, module, &declaration)
            .unwrap_or_else(|error| panic!("implementation using must bind: {error:?}"));

        assert!(result.diagnostics().is_empty());

        let Some(bound) = result.value() else {
            panic!("named implementation using must publish activation evidence");
        };

        assert_eq!(
            bound.implementation(),
            named_implementation(imported.declaration)
        );

        assert_eq!(
            bound.using_declaration(),
            bray_declarations::SyntaxAnchor::from_node(&declaration)
        );
    }

    #[test]
    fn ordinary_usings_do_not_become_implementation_activation_errors() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "using dependency.api.run;\n",
            "const ready: bool = true;\n",
        ));

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::Function,
            "run",
        );

        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;
        let declaration = using_declaration(&facts);

        let result = bind_implementation_using(&facts, module, &declaration)
            .unwrap_or_else(|error| panic!("ordinary using probe must complete: {error:?}"));

        assert!(result.value().is_none());
        assert!(result.diagnostics().is_empty());
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

        assert_imported_implementation_path(&imported, "dependency.api.DisplayVec");
    }

    #[test]
    fn dotted_package_identities_consume_the_complete_path_prefix() {
        let imported = bray_symbols::testing::imported_lookup_fixture(
            "test.package",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_implementation_path(&imported, "test.package.api.DisplayVec");
    }

    #[test]
    fn undeclared_imported_module_prefixes_route_to_logical_modules() {
        let imported = bray_symbols::testing::imported_lookup_fixture_at_module(
            "dependency",
            ["foo", "bar"],
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_implementation_path(&imported, "dependency.foo.bar.DisplayVec");
    }

    #[test]
    fn declared_imported_module_prefixes_route_in_source_order() {
        let imported = bray_symbols::testing::imported_lookup_fixture_with_declared_prefixes(
            "dependency",
            ["foo", "bar"],
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        assert_imported_implementation_path(&imported, "dependency.foo.bar.DisplayVec");
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

        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;

        assert_eq!(
            bind_named_trait_implementation_path(
                &facts,
                module,
                &path("DisplayVec"),
                NameAccess::Public,
            ),
            Ok(MemberLookupResult::NotFound)
        );
    }

    #[test]
    fn imported_declarations_of_the_wrong_kind_retain_the_exact_candidate() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "using dependency.api.DisplayVec;\n",
            "const ready: bool = true;",
        ));

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::Function,
            "DisplayVec",
        );

        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;

        assert_eq!(
            bind_named_trait_implementation_path(
                &facts,
                module,
                &path("dependency.api.DisplayVec"),
                NameAccess::Public,
            ),
            Ok(MemberLookupResult::WrongKind(Box::new([
                imported.declaration,
            ])))
        );
    }

    #[test]
    fn imported_path_lookup_is_repeatable_and_thread_safe() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "using dependency.api.DisplayVec;\n",
            "const ready: bool = true;",
        ));

        let imported = bray_symbols::testing::imported_lookup_fixture(
            "dependency",
            "api",
            SymbolKind::NamedTraitImplementation,
            "DisplayVec",
        );

        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;
        let implementation_path = path("dependency.api.DisplayVec");

        let bind = || {
            bind_named_trait_implementation_path(
                &facts,
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

        assert!(matches!(expected, Ok(MemberLookupResult::Found(_))));
    }

    fn named_implementation(symbol: AnySymbolId) -> bray_symbols::NamedTraitImplementationSymbolId {
        match symbol {
            AnySymbolId::NamedTraitImplementation(implementation) => implementation,
            _ => panic!("test declaration must be a named trait implementation"),
        }
    }

    fn using_declaration<C>(facts: &C) -> bray_syntax::UsingDeclarationSyntax
    where
        C: BinderFactContext + ?Sized,
    {
        facts
            .syntax()
            .source_units()
            .first()
            .and_then(|unit| unit.using_declarations().next())
            .unwrap_or_else(|| panic!("fixture must contain one using declaration"))
    }

    fn assert_imported_implementation_path(
        imported: &bray_symbols::testing::ImportedLookupFixture,
        source_path: &str,
    ) {
        let source = format!("module app;\nusing {source_path};\nconst ready: bool = true;");

        let fixture = TestFixture::from_source(&source);
        let facts = fixture.context_with_imported(&imported.symbols);
        let module = source_module(&facts).0;

        assert_eq!(
            bind_named_trait_implementation_path(
                &facts,
                module,
                &path(source_path),
                NameAccess::Public,
            ),
            Ok(MemberLookupResult::Found(named_implementation(
                imported.declaration
            )))
        );
    }
}
