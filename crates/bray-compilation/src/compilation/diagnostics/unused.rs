use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{
    NameAccess, SymbolQueryProvider, access_path_tokens, can_access_symbol, using_covers_path,
    using_scope_contains,
};
use bray_bound_tree::BoundReferenceTarget;
use bray_declarations::{DeclarationKind, SyntaxAnchor};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, ImplementationCoherenceDomainKey, ModuleSurfaceQuery, SymbolOrigin,
    SymbolQueryRequest,
};
use bray_syntax::{AccessExpressionSyntax, PathSyntax, SourceSyntaxNode, SyntaxKind};

use crate::compilation::Compilation;
use crate::compilation::tooling::declaration_for_syntax;
use crate::fact::{CancellationToken, FactQueryError, QueryPriority};

impl Compilation {
    pub(super) fn unused_using_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let declarations = self.product_source_graph()?.declarations();

        if !declarations
            .declarations()
            .iter()
            .any(|declaration| declaration.kind() == DeclarationKind::Using)
        {
            return Ok(DiagnosticBag::new());
        }

        let symbols = self.symbol_graph()?;
        let mut references_by_module = BTreeMap::new();

        for source in self.sources() {
            let index = self
                .source_reference_index(
                    source.source_id(),
                    cancellation,
                    QueryPriority::Background,
                )?
                .expect("loaded source has a reference index");

            for (target, references) in &index.references {
                let BoundReferenceTarget::Surface(target) = target else {
                    continue;
                };

                for reference in references {
                    cancellation.check()?;

                    let syntax = reference.syntax();

                    let Some(declaration) = declaration_for_syntax(declarations, syntax) else {
                        continue;
                    };

                    if matches!(
                        declaration.kind(),
                        DeclarationKind::Using | DeclarationKind::Module
                    ) || syntax == declaration.syntax_anchor()
                    {
                        continue;
                    }

                    let module = self.source_module_for_declaration(symbols, declaration)?;
                    let components = reference_path(syntax, self.syntax_tree());

                    references_by_module
                        .entry(module.id())
                        .or_insert_with(Vec::new)
                        .push((*target, components));
                }
            }
        }

        let binder = self.binding_context(cancellation)?;
        let mut diagnostics = DiagnosticBag::new();

        // Participation retains this Arc-backed package identity and its exact source evidence.
        let domain = ImplementationCoherenceDomainKey::new(self.package_identity().clone());

        let participation =
            self.implementation_participation_with_cancellation(domain, cancellation)?;

        let activating_usings = participation
            .value()
            .implementations()
            .iter()
            .flat_map(|implementation| {
                implementation
                    .evidence()
                    .using_declarations()
                    .iter()
                    .copied()
            })
            .collect::<BTreeSet<_>>();

        for module in symbols
            .modules()
            .iter()
            .filter(|module| module.origin() == SymbolOrigin::Source)
        {
            cancellation.check()?;

            let surface = binder
                .resolve_symbol_query(SymbolQueryRequest::<ModuleSurfaceQuery>::new(module.id()))
                .map_err(crate::compilation::binder::binding_query_error)?;

            for using in surface.value().usings() {
                let declaration = declarations
                    .declaration(using.declaration())
                    .expect("validated using has a declaration");

                if activating_usings.contains(&declaration.syntax_anchor()) {
                    continue;
                }

                let internal_scope = using.permits_internal_access()
                    && !matches!(using.target(), AnySymbolId::Package(_))
                    && symbols
                        .symbol_origin(using.target())
                        .is_none_or(|origin| origin == SymbolOrigin::Imported);

                let imported = if internal_scope {
                    binder
                        .imported_symbols()
                        .map_err(crate::compilation::binder::binding_query_error)?
                } else {
                    None
                };

                let used = references_by_module
                    .get(&module.id())
                    .is_some_and(|references| {
                        references.iter().any(|(target, path)| {
                            if internal_scope
                                && !can_access_symbol(
                                    Some(symbols),
                                    imported,
                                    *target,
                                    NameAccess::Local,
                                )
                                && using_scope_contains(symbols, imported, using.target(), *target)
                            {
                                return true;
                            }

                            let components = path.iter().map(String::as_str).collect::<Vec<_>>();

                            (matches!(target, AnySymbolId::Module(_) | AnySymbolId::Package(_))
                                || *target == using.target())
                                && using_covers_path(declaration, &components)
                        })
                    });

                if used {
                    continue;
                }

                let anchor = declaration.syntax_anchor();
                let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

                let name = declaration
                    .name()
                    .and_then(|name| name.as_path())
                    .expect("validated using has a path")
                    .dotted();

                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticId::new(anchor.full_range().start().bytes()),
                        DiagnosticKind::BindingUnusedUsing,
                        SeverityKind::Warning,
                    )
                    .with_primary_span(span)
                    .with_arg(DiagnosticArg::referenced_name(name))
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::NameReference,
                        span,
                    ))
                    .with_note(DiagnosticNote::new(DiagnosticNoteKind::RemoveUnusedUsing)),
                );
            }
        }

        Ok(diagnostics)
    }
}

fn reference_path(anchor: SyntaxAnchor, tree: &bray_syntax::SyntaxTree) -> Vec<String> {
    if anchor.syntax_kind() == SyntaxKind::AccessExpression {
        let Some(access) = anchor.find_descendant::<AccessExpressionSyntax>(tree) else {
            return Vec::new();
        };

        return access_path_tokens(&access)
            .iter()
            .filter_map(|token| token.text(access.source().text()).map(str::to_owned))
            .collect();
    }

    let Some(path) = anchor.find_descendant::<PathSyntax>(tree) else {
        return Vec::new();
    };

    path.identifier_tokens()
        .filter_map(|token| token.text(path.source().text()).map(str::to_owned))
        .collect()
}
