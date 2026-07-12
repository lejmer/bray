use bray_diagnostics::DiagnosticNameKind;
use bray_source::SourceSnapshot;
use bray_symbols::{
    AnySymbolId, CallableOverloadSymbolId, MemberLookupIndex, MemberLookupResult, MemberVisibility,
    ModuleOwnerId, ModulePathKey, ModuleSymbol, ModuleSymbolId, TraitSymbolId,
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
use crate::{BinderFactContext, request::BinderRequestContext};

/// Visibility policy for one source name reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NameAccess {
    Public,
    Internal,
}

/// Stable semantic roots used while resolving one path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PathBindingContext {
    scope: bray_symbols::LocalScopeId,
    module: ModuleSymbolId,
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
            module,
            module_owner,
            access,
        }
    }
}

struct PathLookup {
    result: NameLookupResult<ResolvedName>,
    reference: Option<NameReference>,
}

impl<C> BinderRequestContext<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(crate) fn bind_module_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> NameLookupResult<ModuleSymbolId> {
        let Some(references) = path_references(path) else {
            return malformed_lookup();
        };

        let Some(reference) = whole_path_reference(path, &references) else {
            return malformed_lookup();
        };

        let Some(path_key) =
            ModulePathKey::try_new(references.iter().map(|reference| reference.text()))
        else {
            return malformed_lookup();
        };

        let result = match self
            .facts()
            .symbols()
            .module_by_path(context.module_owner, &path_key)
        {
            Some(module) => module_name_lookup(module, context.access),
            None => MemberLookupResult::NotFound,
        }
        .classify(|name| match name {
            ResolvedName::Surface(AnySymbolId::Module(id)) => Some(id),
            ResolvedName::Local(_) | ResolvedName::Surface(_) => None,
        });

        report_lookup_result(self, &reference, DiagnosticNameKind::Module, &result);

        result
    }

    pub(crate) fn bind_type_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> NameLookupResult<ResolvedTypeName> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Type, classify_type)
    }

    pub(crate) fn bind_trait_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> NameLookupResult<TraitSymbolId> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Trait, classify_trait)
    }

    pub(crate) fn bind_value_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> NameLookupResult<ResolvedValueName> {
        self.bind_classified_path(context, path, DiagnosticNameKind::Value, classify_value)
    }

    pub(crate) fn bind_callable_overload_path(
        &mut self,
        context: PathBindingContext,
        path: &PathSyntax,
    ) -> NameLookupResult<CallableOverloadSymbolId> {
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
    ) -> NameLookupResult<T> {
        let lookup = self.bind_path(context, path);
        let result = lookup.result.classify(classify);

        if let Some(reference) = lookup.reference {
            report_lookup_result(self, &reference, expected, &result);
        }

        result
    }

    fn bind_path(&self, context: PathBindingContext, path: &PathSyntax) -> PathLookup {
        let Some(references) = path_references(path) else {
            return PathLookup {
                result: malformed_lookup(),
                reference: None,
            };
        };

        let module_prefix = longest_module_prefix(
            self.facts().symbols(),
            context.module_owner,
            &references,
            context.access,
        );

        let mut references = references.into_iter();
        let Some(first) = references.next() else {
            return PathLookup {
                result: malformed_lookup(),
                reference: None,
            };
        };

        let ordinary = lookup_unqualified_name(
            self.unit(),
            self.facts().symbols(),
            context.scope,
            context.module,
            first.text(),
            context.access,
        );

        let mut selected_module = None;
        let mut result = match module_prefix {
            Some((module, length, module_lookup)) => {
                selected_module = Some((module, length));
                combine_name_lookups(ordinary, module_lookup)
            }
            None => ordinary,
        };
        let mut result_reference = first;

        if let Some((module, length)) = selected_module {
            match result {
                MemberLookupResult::Found(ResolvedName::Surface(AnySymbolId::Module(id)))
                    if id == module =>
                {
                    for _ in 1..length {
                        let Some(reference) = references.next() else {
                            return PathLookup {
                                result: malformed_lookup(),
                                reference: None,
                            };
                        };

                        result_reference = reference;
                    }
                }
                MemberLookupResult::Found(_) => {}
                _ => {
                    return PathLookup {
                        result,
                        reference: Some(result_reference),
                    };
                }
            }
        }

        for reference in references {
            let owner = match &result {
                MemberLookupResult::Found(ResolvedName::Surface(owner)) => *owner,
                _ => {
                    return PathLookup {
                        result,
                        reference: Some(result_reference),
                    };
                }
            };

            result = lookup_surface_name(
                self.facts().symbols(),
                owner,
                reference.text(),
                context.access,
            );

            result_reference = reference;
        }

        PathLookup {
            result,
            reference: Some(result_reference),
        }
    }
}

fn longest_module_prefix(
    symbols: &bray_symbols::SymbolGraph,
    owner: ModuleOwnerId,
    references: &[NameReference],
    access: NameAccess,
) -> Option<(ModuleSymbolId, usize, NameLookupResult<ResolvedName>)> {
    for length in (1..=references.len()).rev() {
        let path = ModulePathKey::try_new(references[..length].iter().map(NameReference::text))?;

        if let Some(module) = symbols.module_by_path(owner, &path) {
            return Some((module.id(), length, module_name_lookup(module, access)));
        }
    }

    None
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

fn whole_path_reference(path: &PathSyntax, references: &[NameReference]) -> Option<NameReference> {
    let first = references.first()?;
    let last = references.last()?;
    let range =
        bray_source::TextRange::new(first.span().range().start(), last.span().range().end());

    Some(NameReference::new(
        references
            .iter()
            .map(NameReference::text)
            .collect::<Vec<_>>()
            .join("."),
        path.source().source_id(),
        range,
    ))
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_symbols::{
        AnyLocalSymbolId, AnySymbolId, LocalSymbolRegionId, MemberEntry, MemberLookupIndex,
        MemberLookupResult, MemberValidity, MemberVisibility, ModuleOwnerId, SymbolName,
        SymbolOrigin,
    };
    use bray_syntax::{PathSyntax, SourceSyntaxNode, SyntaxKind, SyntaxToken};
    use bray_testing::{test_source_at, test_source_store};

    use super::{NameAccess, PathBindingContext};
    use crate::BinderFactContext;
    use crate::fact::test_support::TestFixture as FactFixture;
    use crate::lookup::category::{ResolvedName, ResolvedTypeName, ResolvedValueName};
    use crate::request::{BinderRequestContext, BindingContext};
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn typed_paths_resolve_modules_declarations_overloads_and_members() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app; ",
            "const Size: bool = true; ",
            "struct Point { x: bool; } ",
            "trait Display {} ",
            "func choose_value() {} ",
            "overload choose = {choose_value}"
        ));

        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(30));
        let root = unit.root_scope();
        let (module, owner) = source_module(&facts);
        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            request.bind_module_path(context, &path("app")),
            MemberLookupResult::Found(id) if id == module
        ));

        assert!(matches!(
            request.bind_type_path(context, &path("Point")),
            MemberLookupResult::Found(ResolvedTypeName::Named(_))
        ));

        assert!(matches!(
            request.bind_trait_path(context, &path("Display")),
            MemberLookupResult::Found(_)
        ));

        assert!(matches!(
            request.bind_value_path(context, &path("Size")),
            MemberLookupResult::Found(ResolvedValueName::Constant(_))
        ));

        assert!(matches!(
            request.bind_callable_overload_path(context, &path("choose")),
            MemberLookupResult::Found(_)
        ));

        let MemberLookupResult::Found(ResolvedTypeName::Named(structure)) =
            request.bind_type_path(context, &path("Point"))
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
            request.bind_member(
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
            request.bind_member_from_index(
                &associated,
                member_path.source(),
                associated_token,
                |_, visibility| NameAccess::Internal.allows(visibility),
            ),
            MemberLookupResult::Found(member)
                if matches!(member.symbol(), AnySymbolId::StructField(_))
        ));

        let result = finish(request);

        assert!(result.diagnostics().is_empty());
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
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert_eq!(
            request.bind_value_path(context, &path("value")),
            MemberLookupResult::Found(ResolvedValueName::Local(AnyLocalSymbolId::from(local)))
        );

        assert!(finish(request).diagnostics().is_empty());
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
        let mut request = BinderRequestContext::new(&facts, BindingContext::TypeExpression, unit);

        assert!(matches!(
            request.bind_type_path(context, &path("bool")),
            MemberLookupResult::Found(ResolvedTypeName::Named(_))
        ));

        assert!(finish(request).diagnostics().is_empty());
    }

    #[test]
    fn recovered_surface_names_in_lexical_scopes_remain_malformed() {
        let fact_fixture =
            FactFixture::from_source("module app; const Size: bool = true; func broken(");
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
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            request.bind_value_path(context, &path("broken")),
            MemberLookupResult::Malformed(candidates)
                if candidates.as_ref() == [ResolvedName::Surface(recovered)]
        ));

        let result = finish(request);

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
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            request.bind_value_path(context, &path("value")),
            MemberLookupResult::Malformed(candidates)
                if candidates.as_ref() == [ResolvedName::Local(local.into())]
        ));

        let result = finish(request);

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
            "module app; ",
            "const Size: bool = true; ",
            "internal const Secret: bool = true; ",
            "func duplicate() {} ",
            "const duplicate: bool = true; ",
            "func broken("
        ));

        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(32));
        let root = unit.root_scope();
        let (module, owner) = source_module(&facts);
        let public = PathBindingContext::new(root, module, owner, NameAccess::Public);
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert_eq!(
            request.bind_type_path(public, &path("Missing")),
            MemberLookupResult::NotFound
        );

        assert!(matches!(
            request.bind_type_path(public, &path("Size")),
            MemberLookupResult::WrongKind(_)
        ));

        assert!(matches!(
            request.bind_value_path(public, &path("Secret")),
            MemberLookupResult::Inaccessible(_)
        ));

        assert!(matches!(
            request.bind_value_path(public, &path("duplicate")),
            MemberLookupResult::Ambiguous(_)
        ));

        assert!(matches!(
            request.bind_value_path(public, &path("broken")),
            MemberLookupResult::Malformed(_)
        ));

        let result = finish(request);
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
        let internal_fixture =
            FactFixture::from_source("internal module hidden; const Size: bool = true;");
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
        let mut internal_request = BinderRequestContext::new(
            &internal_facts,
            BindingContext::TypeExpression,
            internal_unit,
        );

        assert!(matches!(
            internal_request.bind_module_path(public, &path("hidden")),
            MemberLookupResult::Inaccessible(_)
        ));

        let recovered_fixture = FactFixture::from_source("module broken const Size: bool = true;");
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
        let mut recovered_request = BinderRequestContext::new(
            &recovered_facts,
            BindingContext::TypeExpression,
            recovered_unit,
        );

        assert!(matches!(
            recovered_request.bind_module_path(internal, &path("broken")),
            MemberLookupResult::Malformed(_)
        ));
    }

    #[test]
    fn module_prefixes_do_not_take_precedence_over_ordinary_names() {
        let fact_fixture =
            FactFixture::from_source("module app; const app: bool = true; struct Point {}");
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(39));
        let root = unit.root_scope();
        let (module, owner) = source_module(&facts);
        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            request.bind_value_path(context, &path("app.Point")),
            MemberLookupResult::Ambiguous(_)
        ));

        assert_eq!(
            finish(request).diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::BindingAmbiguousName
        );
    }

    #[test]
    fn associated_member_lookup_uses_candidate_aware_accessibility() {
        let fact_fixture = FactFixture::from_source(concat!(
            "module app; ",
            "const Size: bool = true; ",
            "struct Point { x: bool; }"
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

        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        assert!(matches!(
            request.bind_member_from_index(
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
        let mut request = BinderRequestContext::new(&facts, BindingContext::TypeExpression, unit);

        let sources = test_source_store([""]);
        let snapshot = test_source_at(&sources, 0).clone();
        let mut missing = PathSyntax::builder(snapshot);

        missing.push_identifier_token(SyntaxToken::missing(
            SyntaxKind::IdentifierToken,
            TextSize::ZERO,
        ));

        assert!(matches!(
            request.bind_type_path(context, &missing.build()),
            MemberLookupResult::Malformed(candidates) if candidates.is_empty()
        ));

        assert!(finish(request).diagnostics().is_empty());
    }

    #[test]
    fn missing_middle_path_segments_do_not_bind_repaired_paths() {
        let fact_fixture =
            FactFixture::from_source("module app; const Size: bool = true; struct Point {}");
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(41));
        let root = unit.root_scope();
        let (module, owner) = source_module(&facts);
        let context = PathBindingContext::new(root, module, owner, NameAccess::Internal);
        let mut request = BinderRequestContext::new(&facts, BindingContext::TypeExpression, unit);

        let malformed = path_with_missing_middle();

        assert!(matches!(
            request.bind_type_path(context, &malformed),
            MemberLookupResult::Malformed(candidates) if candidates.is_empty()
        ));

        assert!(finish(request).diagnostics().is_empty());
    }

    fn source_module<C: BinderFactContext + ?Sized>(
        facts: &C,
    ) -> (bray_symbols::ModuleSymbolId, ModuleOwnerId) {
        let Some(module) = facts
            .symbols()
            .modules()
            .iter()
            .find(|module| module.origin() == SymbolOrigin::Source)
        else {
            panic!("test graph must contain one source module");
        };

        (module.id(), module.owner())
    }

    fn path(text: &str) -> PathSyntax {
        let sources = test_source_store([text]);
        let snapshot = test_source_at(&sources, 0).clone();
        let mut builder = PathSyntax::builder(snapshot);
        let mut segment_start = 0;

        for (index, byte) in text.bytes().enumerate() {
            if byte != b'.' {
                continue;
            }

            builder.push_identifier_token(SyntaxToken::new(
                SyntaxKind::IdentifierToken,
                text_range(segment_start, index),
            ));

            builder.push_dot_token(SyntaxToken::new(
                SyntaxKind::DotToken,
                text_range(index, index + 1),
            ));

            segment_start = index + 1;
        }

        builder.push_identifier_token(SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            text_range(segment_start, text.len()),
        ));

        builder.build()
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

    fn text_range(start: usize, end: usize) -> TextRange {
        let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
            panic!("test path offsets must fit in TextSize");
        };

        TextRange::new(TextSize::new(start), TextSize::new(end))
    }

    fn finish<C: BinderFactContext + ?Sized>(
        request: BinderRequestContext<'_, C>,
    ) -> crate::request::BinderRequestResult {
        match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("test binding request must publish: {error:?}"),
        }
    }
}
