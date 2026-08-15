use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{
    BindingQueryContext, BindingQueryError, BindingQueryResult, NameAccess,
    bind_surface_path_with_re_exports,
};
use bray_declarations::{DeclarationKind, DeclarationRecord};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
    DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, MemberLookupResult, MemberVisibility, ModulePathKey, ModuleReExport,
    ModuleSurface, ModuleSurfaceQuery, ModuleSymbolId, ModuleUsing, SymbolKey, SymbolKeyData,
    SymbolKind, SymbolName, SymbolQueryRequest, SymbolRootKey,
};
use bray_syntax::{ExportDeclarationSyntax, PathSyntax, SourceSyntaxNode, UsingDeclarationSyntax};

use super::binding::CompilationSymbolQueryEvaluator;
use super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<ModuleSurfaceQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<ModuleSurfaceQuery> {
        &self.module_surfaces
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<ModuleSurfaceQuery>,
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<
            <ModuleSurfaceQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        ModuleSurfaceResolver::new(context).bind(request.owner())
    }
}

struct ModuleSurfaceResolver<'binding_context, 'compilation> {
    context: &'binding_context CompilationBindingContext<'compilation>,
    completed: BTreeMap<ModuleSymbolId, DiagnosticResult<ModuleSurface>>,
    active: Vec<ModuleSymbolId>,
    detected_cycles: usize,
}

type BoundModuleUsings = (Vec<ModuleUsing>, BTreeSet<Box<[String]>>);

impl<'binding_context, 'compilation> ModuleSurfaceResolver<'binding_context, 'compilation> {
    fn new(context: &'binding_context CompilationBindingContext<'compilation>) -> Self {
        Self {
            context,
            completed: BTreeMap::new(),
            active: Vec::new(),
            detected_cycles: 0,
        }
    }

    fn bind(
        &mut self,
        module: ModuleSymbolId,
    ) -> BindingQueryResult<DiagnosticResult<ModuleSurface>> {
        if let Some(surface) = self.completed.get(&module) {
            // Request-local memoization avoids rebinding reachable module headers.
            return Ok(surface.clone());
        }

        if self.active.contains(&module) {
            self.detected_cycles = self.detected_cycles.saturating_add(1);

            return empty_surface();
        }

        self.active.push(module);

        let result = self.bind_active_module(module);
        let popped = self.active.pop();

        if popped != Some(module) {
            return Err(BindingQueryError::DependencyUnavailable);
        }

        let result = result?;

        // Re-export names and symbol IDs are compact immutable values reused during this request.
        self.completed.insert(module, result.clone());

        Ok(result)
    }

    fn bind_active_module(
        &mut self,
        module: ModuleSymbolId,
    ) -> BindingQueryResult<DiagnosticResult<ModuleSurface>> {
        let declarations = self.module_member_declaration_ids(module)?;
        let mut diagnostics = DiagnosticBag::new();

        let (usings, internal_paths) = self.bind_usings(module, &declarations, &mut diagnostics)?;

        let re_exports =
            self.bind_re_exports(module, &declarations, &internal_paths, &mut diagnostics)?;

        let surface = ModuleSurface::new(usings, re_exports)
            .map_err(|_| BindingQueryError::DependencyUnavailable)?;

        Ok(DiagnosticResult::new(surface, diagnostics))
    }

    fn bind_usings(
        &mut self,
        module: ModuleSymbolId,
        declarations: &[bray_declarations::DeclarationId],
        diagnostics: &mut DiagnosticBag,
    ) -> BindingQueryResult<BoundModuleUsings> {
        let context = self.context;
        let mut usings = Vec::new();
        let mut internal_paths = BTreeSet::new();

        for declaration in declarations
            .iter()
            .copied()
            .filter_map(|declaration| context.declarations().declaration(declaration))
            .filter(|declaration| declaration.kind() == DeclarationKind::Using)
        {
            let Some(syntax) = declaration
                .syntax_anchor()
                .find_descendant::<UsingDeclarationSyntax>(context.syntax())
            else {
                continue;
            };

            let access = if syntax.internal_keyword().is_some() {
                NameAccess::Internal
            } else {
                NameAccess::Public
            };

            let binding = self.bind_path(module, &syntax.path(), access)?;

            let (result, path_diagnostics) = binding.into_parts();

            *diagnostics = diagnostics.merged(&path_diagnostics);

            if let MemberLookupResult::Found(target) = result {
                if matches!(access, NameAccess::Internal)
                    && let Some(path) = path_components(&syntax.path())
                {
                    internal_paths.insert(path);
                }

                usings.push(ModuleUsing::new(
                    declaration.id(),
                    target,
                    matches!(access, NameAccess::Internal),
                ));
            }
        }

        Ok((usings, internal_paths))
    }

    fn bind_re_exports(
        &mut self,
        module: ModuleSymbolId,
        declarations: &[bray_declarations::DeclarationId],
        internal_paths: &BTreeSet<Box<[String]>>,
        diagnostics: &mut DiagnosticBag,
    ) -> BindingQueryResult<Vec<ModuleReExport>> {
        let context = self.context;
        let mut re_exports = Vec::new();
        let mut exported_names = BTreeMap::new();

        for declaration in declarations
            .iter()
            .copied()
            .filter_map(|declaration| context.declarations().declaration(declaration))
            .filter(|declaration| declaration.kind() == DeclarationKind::Export)
        {
            let Some(syntax) = declaration
                .syntax_anchor()
                .find_descendant::<ExportDeclarationSyntax>(context.syntax())
            else {
                continue;
            };

            let Some(name) = path_name(&syntax.path()) else {
                continue;
            };

            let Some((target, visibility)) =
                self.bind_re_export_target(module, &syntax.path(), internal_paths, diagnostics)?
            else {
                continue;
            };

            let member_lookup = context
                .symbols()
                .lookup_member(module.into(), name.as_str());

            let mut prior_spans = member_lookup_spans(context.symbols(), &member_lookup);

            prior_spans.extend(module_declared_child_spans(
                context.symbols(),
                context.declarations(),
                module,
                name.as_str(),
            ));

            if let Some(prior) = exported_names.get(&name).copied() {
                prior_spans.push(prior);
            }

            prior_spans.sort_unstable();
            prior_spans.dedup();

            if !matches!(member_lookup, MemberLookupResult::NotFound) || !prior_spans.is_empty() {
                diagnostics.add(export_conflict_diagnostic(declaration, &name, prior_spans));

                continue;
            }

            exported_names.insert(
                name.clone(),
                SourceSpan::new(declaration.source_id(), declaration.full_range()),
            );

            re_exports.push(ModuleReExport::new(
                declaration.id(),
                name,
                target,
                visibility,
            ));
        }

        Ok(re_exports)
    }

    fn bind_re_export_target(
        &mut self,
        module: ModuleSymbolId,
        path: &PathSyntax,
        internal_paths: &BTreeSet<Box<[String]>>,
        diagnostics: &mut DiagnosticBag,
    ) -> BindingQueryResult<Option<(AnySymbolId, MemberVisibility)>> {
        let public_binding = self.bind_path(module, path, NameAccess::Public)?;

        let (public_result, public_diagnostics) = public_binding.into_parts();

        match public_result {
            MemberLookupResult::Found(target) => {
                if !self.target_can_be_exported(target)? {
                    diagnostics.add(path_diagnostic(
                        path,
                        DiagnosticKind::BindingInvalidModuleExportTarget,
                    ));

                    return Ok(None);
                }

                Ok(Some((target, MemberVisibility::Public)))
            }
            MemberLookupResult::Inaccessible(_) => {
                let internal_binding = self.bind_path(module, path, NameAccess::Internal)?;

                let (internal_result, internal_diagnostics) = internal_binding.into_parts();

                if let MemberLookupResult::Found(target) = internal_result
                    && path_components(path)
                        .is_some_and(|path| path_has_acknowledged_prefix(&path, internal_paths))
                {
                    if !self.target_can_be_exported(target)? {
                        diagnostics.add(path_diagnostic(
                            path,
                            DiagnosticKind::BindingInvalidModuleExportTarget,
                        ));

                        return Ok(None);
                    }

                    return Ok(Some((target, MemberVisibility::Internal)));
                }

                *diagnostics = diagnostics.merged(
                    if matches!(internal_result, MemberLookupResult::Found(_)) {
                        &public_diagnostics
                    } else {
                        &internal_diagnostics
                    },
                );

                Ok(None)
            }
            _ => {
                *diagnostics = diagnostics.merged(&public_diagnostics);

                Ok(None)
            }
        }
    }

    fn target_can_be_exported(&self, target: AnySymbolId) -> BindingQueryResult<bool> {
        if matches!(
            target.kind(),
            SymbolKind::CompilerKnownEnvironment | SymbolKind::Package
        ) {
            return Ok(false);
        }

        let Some(key) = self.context.symbol_key(target)? else {
            return Ok(false);
        };

        Ok(!key_is_compiler_known(key))
    }

    fn bind_path(
        &mut self,
        module: ModuleSymbolId,
        path: &PathSyntax,
        access: NameAccess,
    ) -> BindingQueryResult<DiagnosticResult<MemberLookupResult<AnySymbolId>>> {
        let cycle_count = self.detected_cycles;
        let context = self.context;

        let result = bind_surface_path_with_re_exports(
            context,
            module,
            path,
            access,
            &mut |owner, name, access| self.lookup(owner, name, access),
        )?;

        if self.detected_cycles == cycle_count {
            return Ok(result);
        }

        Ok(DiagnosticResult::new(
            MemberLookupResult::NotFound,
            DiagnosticBag::single(path_diagnostic(
                path,
                DiagnosticKind::BindingCyclicModuleExport,
            )),
        ))
    }

    fn lookup(
        &mut self,
        module: ModuleSymbolId,
        name: &str,
        access: NameAccess,
    ) -> BindingQueryResult<MemberLookupResult<AnySymbolId>> {
        let surface = self.bind(module)?;

        Ok(match access {
            NameAccess::Public => surface.value().lookup_public(name),
            NameAccess::Internal => surface.value().lookup(name),
        })
    }

    fn module_member_declaration_ids(
        &self,
        module: ModuleSymbolId,
    ) -> BindingQueryResult<Vec<bray_declarations::DeclarationId>> {
        let module = self
            .context
            .symbols()
            .module(module)
            .ok_or(BindingQueryError::DependencyUnavailable)?;

        let mut declarations = Vec::new();

        for part in module.module_parts() {
            let part = self
                .context
                .declarations()
                .module_part(*part)
                .ok_or(BindingQueryError::DependencyUnavailable)?;

            for declaration in part.declarations() {
                declarations.push(*declaration);
            }
        }

        Ok(declarations)
    }
}

fn empty_surface() -> BindingQueryResult<DiagnosticResult<ModuleSurface>> {
    let surface =
        ModuleSurface::new([], []).map_err(|_| BindingQueryError::DependencyUnavailable)?;

    Ok(DiagnosticResult::without_diagnostics(surface))
}

fn path_name(path: &PathSyntax) -> Option<SymbolName> {
    let token = path.identifier_tokens().last()?;
    let text = token.text(path.source().text())?;

    SymbolName::try_new(text)
}

fn path_components(path: &PathSyntax) -> Option<Box<[String]>> {
    if path.is_recovered() {
        return None;
    }

    // The resolver owns canonical path text because typed syntax handles are short-lived views.
    path.identifier_tokens()
        .map(|token| token.text(path.source().text()).map(str::to_owned))
        .collect()
}

fn path_has_acknowledged_prefix(path: &[String], acknowledged: &BTreeSet<Box<[String]>>) -> bool {
    (1..=path.len())
        .rev()
        .any(|length| acknowledged.contains(&path[..length]))
}

fn module_declared_child_spans(
    symbols: &bray_symbols::SymbolGraph,
    declarations: &bray_declarations::DeclarationTable,
    module: ModuleSymbolId,
    name: &str,
) -> Vec<SourceSpan> {
    let Some(module) = symbols.module(module) else {
        return Vec::new();
    };

    if module.path().is_recovered() {
        return Vec::new();
    }

    let Some(path) = ModulePathKey::try_new(module.path().segments().chain([name])) else {
        return Vec::new();
    };

    let Some(child) = symbols.module_by_path(module.owner(), &path) else {
        return Vec::new();
    };

    child
        .module_parts()
        .iter()
        .filter_map(|id| declarations.module_part(*id))
        .map(|part| SourceSpan::new(part.source_id(), part.full_range()))
        .collect()
}

fn member_lookup_spans(
    symbols: &bray_symbols::SymbolGraph,
    result: &MemberLookupResult<AnySymbolId>,
) -> Vec<SourceSpan> {
    let candidates: &[AnySymbolId] = match result {
        MemberLookupResult::Found(candidate) => std::slice::from_ref(candidate),
        MemberLookupResult::NotFound => &[],
        MemberLookupResult::WrongKind(candidates)
        | MemberLookupResult::Ambiguous(candidates)
        | MemberLookupResult::Inaccessible(candidates)
        | MemberLookupResult::Malformed(candidates) => candidates,
    };

    candidates
        .iter()
        .filter_map(|candidate| symbols.declaration_syntax_anchor(*candidate))
        .map(|anchor| SourceSpan::new(anchor.source_id(), anchor.full_range()))
        .collect()
}

fn key_is_compiler_known(key: &SymbolKey) -> bool {
    match key.data() {
        SymbolKeyData::Root(SymbolRootKey::CompilerKnownEnvironment)
        | SymbolKeyData::Module {
            owner: SymbolRootKey::CompilerKnownEnvironment,
            ..
        }
        | SymbolKeyData::CompilerKnownDeclaration { .. } => true,
        SymbolKeyData::Synthesized(key) => key_is_compiler_known(key.subject()),
        SymbolKeyData::Root(SymbolRootKey::Package(_))
        | SymbolKeyData::Module {
            owner: SymbolRootKey::Package(_),
            ..
        }
        | SymbolKeyData::SourceDeclaration { .. }
        | SymbolKeyData::External(_) => false,
    }
}

fn path_diagnostic(path: &PathSyntax, kind: DiagnosticKind) -> Diagnostic {
    let token = path.identifier_tokens().last();

    let range = token
        .as_ref()
        .map(bray_syntax::SyntaxToken::range)
        .unwrap_or_else(|| path.full_range());

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(range.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(SourceSpan::new(path.source().source_id(), range))
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::ModuleExport,
        SourceSpan::new(path.source().source_id(), range),
    ));

    if let Some(token) = token
        && let Some(text) = token.text(path.source().text())
    {
        diagnostic = diagnostic.with_arg(DiagnosticArg::referenced_name(text));
    }

    diagnostic
}

fn export_conflict_diagnostic(
    declaration: &DeclarationRecord,
    name: &SymbolName,
    prior_spans: impl IntoIterator<Item = SourceSpan>,
) -> Diagnostic {
    let primary_span = SourceSpan::new(declaration.source_id(), declaration.full_range());

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(declaration.full_range().start().bytes()),
        DiagnosticKind::BindingConflictingModuleExport,
        SeverityKind::Error,
    )
    .with_primary_span(primary_span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::ModuleExport,
        primary_span,
    ))
    .with_arg(DiagnosticArg::referenced_name(name.as_str()));

    for prior in prior_spans.into_iter().filter(|span| *span != primary_span) {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::ConflictingDeclaration,
            prior,
        ));
    }

    diagnostic
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_symbols::{
        AnySymbolId, MemberLookupResult, ModulePathKey, ModuleSurfaceQuery, PackageIdentity,
        SymbolQueryRequest,
    };
    use bray_testing::test_source_inputs;

    use super::super::test_support::{binding_context, resolved_query, symbol_graph};
    use crate::fact::CancellationToken;
    use crate::{Compilation, CompilationRequest};

    #[test]
    fn using_relationships_and_transitive_re_exports_preserve_target_identity() {
        let compilation = compilation([
            concat!("module a;\n", "\n", "func run()\n", "{\n", "}\n",),
            concat!("module b;\n", "\n", "using a.run;\n", "export a.run;\n",),
            concat!("module c;\n", "\n", "export b.run;\n",),
        ]);

        let symbols = symbol_graph(&compilation);
        let a = module(symbols, "a");
        let b = module(symbols, "b");
        let c = module(symbols, "c");

        let run = symbols
            .functions()
            .iter()
            .find(|function| symbols.containing_symbol(function.id().into()) == Some(a.into()))
            .map(|function| AnySymbolId::from(function.id()))
            .unwrap_or_else(|| panic!("module a must declare run"));

        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let b_surface = resolved_query(
            &binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(b),
        );

        assert_eq!(b_surface.value().usings()[0].target(), run);

        assert_eq!(
            b_surface.value().lookup_public("run"),
            MemberLookupResult::Found(run)
        );

        let c_surface = resolved_query(
            &binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(c),
        );

        assert_eq!(c_surface.value().re_exports()[0].target(), run);

        assert_eq!(
            c_surface.value().lookup_public("run"),
            MemberLookupResult::Found(run)
        );

        assert!(c_surface.diagnostics().is_empty());
    }

    #[test]
    fn internal_re_exports_require_exact_internal_using_acknowledgement() {
        let acknowledged = compilation([
            concat!("module a;\n", "\n", "internal func run()\n", "{\n", "}\n",),
            concat!(
                "module b;\n",
                "\n",
                "using internal a.run;\n",
                "export a.run;\n",
            ),
        ]);

        let symbols = symbol_graph(&acknowledged);
        let b = module(symbols, "b");
        let cancellation = CancellationToken::new();
        let acknowledged_binding_context = binding_context(&acknowledged, &cancellation);

        let surface = resolved_query(
            &acknowledged_binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(b),
        );

        assert!(surface.diagnostics().is_empty());

        assert!(matches!(
            surface.value().lookup_public("run"),
            MemberLookupResult::Inaccessible(_)
        ));

        let unacknowledged = compilation([
            concat!("module a;\n", "\n", "internal func run()\n", "{\n", "}\n",),
            concat!("module b;\n", "\n", "export a.run;\n",),
        ]);

        let symbols = symbol_graph(&unacknowledged);
        let b = module(symbols, "b");
        let cancellation = CancellationToken::new();
        let unacknowledged_binding_context = binding_context(&unacknowledged, &cancellation);

        let surface = resolved_query(
            &unacknowledged_binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(b),
        );

        assert_eq!(
            surface
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingInaccessibleName]
        );

        assert!(surface.value().re_exports().is_empty());

        let non_propagating = compilation([
            concat!("module a;\n", "\n", "internal func run()\n", "{\n", "}\n",),
            concat!(
                "module b;\n",
                "\n",
                "using internal a.run;\n",
                "export a.run;\n",
            ),
            concat!(
                "module c;\n",
                "\n",
                "using internal a.run;\n",
                "export b.run;\n",
            ),
        ]);

        let symbols = symbol_graph(&non_propagating);
        let c = module(symbols, "c");
        let cancellation = CancellationToken::new();
        let non_propagating_binding_context = binding_context(&non_propagating, &cancellation);

        let surface = resolved_query(
            &non_propagating_binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(c),
        );

        assert!(surface.value().re_exports().is_empty());

        assert_eq!(
            surface
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingInaccessibleName]
        );

        let descendant = compilation([
            concat!(
                "internal module details;\n",
                "\n",
                "internal struct Buffer\n",
                "{\n",
                "}\n",
            ),
            concat!(
                "module api;\n",
                "\n",
                "using internal details;\n",
                "export details.Buffer;\n",
            ),
        ]);

        let symbols = symbol_graph(&descendant);
        let api = module(symbols, "api");
        let cancellation = CancellationToken::new();
        let descendant_binding_context = binding_context(&descendant, &cancellation);

        let surface = resolved_query(
            &descendant_binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(api),
        );

        assert!(surface.diagnostics().is_empty());

        assert!(matches!(
            surface.value().lookup_public("Buffer"),
            MemberLookupResult::Inaccessible(_)
        ));
    }

    #[test]
    fn conflicting_and_cyclic_exports_produce_structured_diagnostics() {
        let conflicting = compilation([
            concat!("module a;\n", "\n", "func run()\n", "{\n", "}\n",),
            concat!(
                "module b;\n",
                "\n",
                "func run()\n",
                "{\n",
                "}\n",
                "\n",
                "export a.run;\n",
            ),
        ]);

        let conflicting_diagnostics = assert_surface_diagnostic(
            &conflicting,
            "b",
            DiagnosticKind::BindingConflictingModuleExport,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &conflicting_diagnostics,
            DiagnosticKind::BindingConflictingModuleExport,
        );

        let [conflict] = conflicting_diagnostics.diagnostics() else {
            panic!("one conflicting export diagnostic expected");
        };

        let [prior] = conflict.related_locations() else {
            panic!("conflicting export must retain its prior declaration");
        };

        assert_eq!(
            prior.kind(),
            bray_diagnostics::DiagnosticRelatedLocationKind::ConflictingDeclaration
        );

        let child_module_conflict = compilation([
            concat!("module root;\n", "\n", "export source.child;\n",),
            "module root.child;\n",
            "module source.child;\n",
        ]);

        let child_conflict_diagnostics = assert_surface_diagnostic(
            &child_module_conflict,
            "root",
            DiagnosticKind::BindingConflictingModuleExport,
        );

        let [child_conflict] = child_conflict_diagnostics.diagnostics() else {
            panic!("one child-module conflict diagnostic expected");
        };

        assert_eq!(child_conflict.related_locations().len(), 1);

        let duplicate_export = compilation([
            concat!("module a;\n", "\n", "func run()\n", "{\n", "}\n",),
            concat!("module b;\n", "\n", "export a.run;\n", "export a.run;\n",),
        ]);

        let duplicate_export_diagnostics = assert_surface_diagnostic(
            &duplicate_export,
            "b",
            DiagnosticKind::BindingConflictingModuleExport,
        );

        let [duplicate_export] = duplicate_export_diagnostics.diagnostics() else {
            panic!("one duplicate export diagnostic expected");
        };

        assert_eq!(duplicate_export.related_locations().len(), 1);

        let cyclic = compilation([
            concat!("module a;\n", "\n", "export b.run;\n",),
            concat!("module b;\n", "\n", "export a.run;\n",),
        ]);

        let cyclic_diagnostics =
            assert_surface_diagnostic(&cyclic, "a", DiagnosticKind::BindingCyclicModuleExport);

        bray_testing::assert_goal_state_diagnostic_kind(
            &cyclic_diagnostics,
            DiagnosticKind::BindingCyclicModuleExport,
        );

        let compiler_known = compilation([concat!("module app;\n", "\n", "export i32;\n",)]);

        let invalid_target_diagnostics = assert_surface_diagnostic(
            &compiler_known,
            "app",
            DiagnosticKind::BindingInvalidModuleExportTarget,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &invalid_target_diagnostics,
            DiagnosticKind::BindingInvalidModuleExportTarget,
        );
    }

    #[test]
    fn whole_package_diagnostics_demand_module_surfaces() {
        let compilation = compilation([concat!("module app;\n", "\n", "using missing.item;\n",)]);

        assert!(
            compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::BindingUnresolvedName)
                .next()
                .is_some()
        );
    }

    fn assert_surface_diagnostic(
        compilation: &Compilation,
        module_path: &str,
        expected: DiagnosticKind,
    ) -> DiagnosticBag {
        let symbols = symbol_graph(compilation);
        let module = module(symbols, module_path);
        let cancellation = CancellationToken::new();
        let binding_context = binding_context(compilation, &cancellation);

        let surface = resolved_query(
            &binding_context,
            SymbolQueryRequest::<ModuleSurfaceQuery>::new(module),
        );

        assert!(surface.diagnostics().by_kind(expected).next().is_some());

        surface.diagnostics().clone()
    }

    fn module(symbols: &bray_symbols::SymbolGraph, path: &str) -> bray_symbols::ModuleSymbolId {
        let package = symbols
            .roots()
            .packages()
            .first()
            .copied()
            .unwrap_or_else(|| panic!("test compilation must have a package"));

        let path = ModulePathKey::try_new([path])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        symbols
            .module_by_path(package.into(), &path)
            .map(bray_symbols::ModuleSymbol::id)
            .unwrap_or_else(|| panic!("test module must exist"))
    }

    fn compilation<const N: usize>(sources: [&str; N]) -> Compilation {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let sources = test_source_inputs("test", sources);

        let result = Compilation::load(CompilationRequest::new(package, sources));

        match result {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }
}
