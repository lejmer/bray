use std::sync::Arc;

use bray_binder::{
    BinderFactError, BinderFactResult, SymbolFactProvider,
    bind_callable_type_directives as bind_callable_type_directive_surface, bind_directive_template,
};
use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_declarations::{ModulePartRecord, SyntaxAnchor};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableTypeDirectiveKey, DeclarationDirectivesFact, DirectiveAttachment,
    DirectiveKind, DirectiveSurface, DirectiveTemplate, ModuleSymbolId, SymbolFactContract,
    SymbolFactRequest, SymbolFactResult, SymbolKeyData,
};
use bray_syntax::{SyntaxNodeView, SyntaxWalkControl, walk_direct_child_nodes};

use super::binding::{CompilationSymbolFactBinding, binder_error};
use super::cache::CompilationSymbolFacts;
use super::surface::{syntax_node_for_anchor, with_declaration_root};
use crate::compilation::Compilation;
use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::{FactQueryError, SymbolFactCache};

impl CompilationSymbolFactBinding<DeclarationDirectivesFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<DeclarationDirectivesFact> {
        &self.declaration_directives
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<DeclarationDirectivesFact>,
    ) -> BinderFactResult<SymbolFactResult<DeclarationDirectivesFact>> {
        bind_declaration_directives(context, request.owner())
    }
}

impl Compilation {
    /// Returns source-backed directives attached to one declaration symbol.
    pub fn declaration_directives(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Arc<DiagnosticResult<DirectiveSurface>>, FactQueryError> {
        if !DeclarationDirectivesFact::KIND.is_applicable_to(symbol) {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let facts = self.binder_facts(&self.state.cancellation)?;

        facts
            .symbol_fact(SymbolFactRequest::<DeclarationDirectivesFact>::new(symbol))
            .map_err(binder_fact_error)
    }

    /// Returns source-backed directives attached to one callable type occurrence.
    pub fn callable_type_directives(
        &self,
        key: CallableTypeDirectiveKey,
    ) -> Result<Arc<DiagnosticResult<DirectiveSurface>>, FactQueryError> {
        self.callable_type_directives_with_cancellation(key, &self.state.cancellation)
    }

    fn callable_type_directives_with_cancellation(
        &self,
        key: CallableTypeDirectiveKey,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<Arc<DiagnosticResult<DirectiveSurface>>, FactQueryError> {
        let cell = self.state.callable_type_directives.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            crate::fact::CompilationFactKey::CallableTypeDirectives(key),
            cancellation,
            || {
                let context = self.binder_facts(cancellation)?;

                let syntax =
                    syntax_node_for_anchor(&context, key.syntax()).map_err(binder_fact_error)?;

                let result = bind_callable_type_directive_surface(key.owner(), syntax)
                    .map_err(binder_fact_error)?;

                let (surface, mut diagnostics) = result.into_parts();

                bind_directive_argument_diagnostics(
                    &context,
                    key.owner(),
                    surface.directives(),
                    &mut diagnostics,
                )
                .map_err(binder_fact_error)?;

                Ok(Arc::new(DiagnosticResult::new(surface, diagnostics)))
            },
        )?;

        Ok(Arc::clone(result))
    }
}

fn bind_declaration_directives(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<SymbolFactResult<DeclarationDirectivesFact>> {
    let mut diagnostics = DiagnosticBag::new();

    let directives = match owner {
        AnySymbolId::Module(module) => bind_module_directives(context, module, &mut diagnostics)?,
        _ => bind_nonmodule_directives(context, owner, &mut diagnostics)?,
    };

    Ok(DiagnosticResult::new(
        DirectiveSurface::new(directives),
        diagnostics,
    ))
}

fn bind_module_directives(
    context: &CompilationBinderFacts<'_>,
    owner: ModuleSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<Vec<DirectiveTemplate>> {
    let module = context
        .symbols
        .module(owner)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let declarations = context.declarations();
    let mut directives = Vec::new();

    for part in module.module_parts() {
        let part = declarations
            .module_part(*part)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let attachment = DirectiveAttachment::ModulePart(part.id());

        bind_anchors(
            context,
            owner.into(),
            attachment,
            part.surface().directives(),
            &mut directives,
            diagnostics,
        )?;
    }

    Ok(directives)
}

pub(in crate::compilation) fn bind_module_part_directives_for_selection(
    context: &CompilationBinderFacts<'_>,
    owner: ModuleSymbolId,
    part: &ModulePartRecord,
) -> BinderFactResult<DiagnosticResult<DirectiveSurface>> {
    let mut directives = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    let anchors = part
        .surface()
        .directives()
        .iter()
        .copied()
        .filter(|anchor| {
            matches!(
                anchor.syntax_kind(),
                bray_syntax::SyntaxKind::TargetDirective | bray_syntax::SyntaxKind::TestDirective
            )
        })
        .collect::<Vec<_>>();

    bind_anchors(
        context,
        owner.into(),
        DirectiveAttachment::ModulePart(part.id()),
        &anchors,
        &mut directives,
        &mut diagnostics,
    )?;

    Ok(DiagnosticResult::new(
        DirectiveSurface::new(directives),
        diagnostics,
    ))
}

fn bind_nonmodule_directives(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<Vec<DirectiveTemplate>> {
    let key = context
        .symbols
        .symbol_key(owner)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    match key.data() {
        SymbolKeyData::SourceDeclaration { declaration, .. } => {
            let declaration = context
                .declarations()
                .declaration(*declaration)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let mut directives = Vec::new();

            bind_anchors(
                context,
                owner,
                DirectiveAttachment::Declaration(owner),
                declaration.surface().directives(),
                &mut directives,
                diagnostics,
            )?;

            Ok(directives)
        }
        SymbolKeyData::CompilerKnownDeclaration { .. } => {
            bind_compiler_known_directives(context, owner, diagnostics)
        }
        SymbolKeyData::External(_) => Ok(Vec::new()),
        SymbolKeyData::Root(_) | SymbolKeyData::Module { .. } | SymbolKeyData::Synthesized(_) => {
            Err(BinderFactError::DependencyUnavailable)
        }
    }
}

fn bind_anchors(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    attachment: DirectiveAttachment,
    anchors: &[SyntaxAnchor],
    directives: &mut Vec<DirectiveTemplate>,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<()> {
    for anchor in anchors {
        let syntax = syntax_node_for_anchor(context, *anchor)?;
        let result = bind_directive_template(owner, attachment, syntax)?;

        let (directive, result_diagnostics) = result.into_parts();

        diagnostics.add_range(result_diagnostics);

        bind_directive_argument_diagnostics(
            context,
            owner,
            std::slice::from_ref(&directive),
            diagnostics,
        )?;

        directives.push(directive);
    }

    Ok(())
}

fn bind_directive_argument_diagnostics(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    directives: &[DirectiveTemplate],
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<()> {
    let owner_key = context
        .symbols
        .symbol_key(owner)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    for directive in directives {
        // Target-gate units own target argument binding and diagnostics.
        if directive.kind() == DirectiveKind::Target {
            continue;
        }

        for argument in directive.arguments() {
            let syntax = argument.expression().syntax();

            let Some(source) = context.compilation().source(syntax.source_id()) else {
                continue;
            };

            // Bare words such as `c` and `stable` are interpreted by the directive's checker.
            if directive_argument_is_bare_symbol(context, syntax)? {
                continue;
            }

            // Each bound argument unit owns the stable Arc-backed declaration identity.
            let key = BoundUnitKey::embedded_constant(
                owner_key.clone(),
                BoundSourceAnchor::new(syntax, source.version()),
            )
            .ok_or(BinderFactError::DependencyUnavailable)?;

            let bound = context
                .compilation()
                .bound_unit_with_cancellation(key, context.cancellation)
                .map_err(binder_error)?;

            diagnostics.add_range(bound.result().diagnostics().iter().cloned());
        }
    }

    Ok(())
}

fn directive_argument_is_bare_symbol(
    context: &CompilationBinderFacts<'_>,
    syntax: SyntaxAnchor,
) -> BinderFactResult<bool> {
    let source = context
        .compilation()
        .source(syntax.source_id())
        .ok_or(BinderFactError::DependencyUnavailable)?;

    Ok(crate::compilation::directive::bare_directive_argument_name(
        context.compilation().syntax_tree_result().syntax_tree(),
        source,
        syntax,
    )
    .is_some())
}

fn bind_compiler_known_directives(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<Vec<DirectiveTemplate>> {
    with_declaration_root(context, owner, |root| {
        let mut directives = Vec::new();
        let mut failed = None;

        walk_direct_child_nodes(&root, |child| {
            if DirectiveKind::try_from_syntax_kind(child.kind()).is_some() {
                push_compiler_known_directive(
                    owner,
                    child,
                    &mut directives,
                    diagnostics,
                    &mut failed,
                );
            } else {
                walk_direct_child_nodes(&child, |directive| {
                    if DirectiveKind::try_from_syntax_kind(directive.kind()).is_some() {
                        push_compiler_known_directive(
                            owner,
                            directive,
                            &mut directives,
                            diagnostics,
                            &mut failed,
                        );
                    }

                    SyntaxWalkControl::Continue
                });
            }

            if failed.is_some() {
                SyntaxWalkControl::Stop
            } else {
                SyntaxWalkControl::Continue
            }
        });

        match failed {
            Some(error) => Err(error),
            None => Ok(directives),
        }
    })
}

fn push_compiler_known_directive(
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    directives: &mut Vec<DirectiveTemplate>,
    diagnostics: &mut DiagnosticBag,
    failed: &mut Option<BinderFactError>,
) {
    match bind_directive_template(owner, DirectiveAttachment::Declaration(owner), syntax) {
        Ok(result) => {
            let (directive, result_diagnostics) = result.into_parts();

            diagnostics.add_range(result_diagnostics);
            directives.push(directive);
        }
        Err(error) => *failed = Some(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_declarations::SyntaxAnchor;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        CallableSignatureFact, CallableSymbolId, CallableTypeDirectiveKey, DirectiveArgumentName,
        DirectiveAttachment, DirectiveKind, ProductKind, SymbolFactRequest, SymbolOrigin,
        TypeExpressionTemplate,
    };
    use bray_syntax::{
        SyntaxCast, SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, TypeExpressionSyntax,
        walk_syntax_tree,
    };

    use super::Compilation;
    use crate::WorkerBudget;
    use crate::compilation::binder::symbol::test_support::{binder_facts, published_fact};
    use crate::fact::CancellationToken;
    use crate::test_support::{
        compilation, compilation_with_product, compilation_with_sources_product_and_worker_budget,
    };

    #[test]
    fn function_directives_preserve_source_order_duplicates_and_argument_forms() {
        let compilation = compilation_with_product(
            concat!(
                "module app;\n",
                "\n",
                "@abi(c)\n",
                "@link(\"first\", kind = \"static\")\n",
                "@link(\"second\")\n",
                "@symbol(\"native_main\")\n",
                "@entrypoint\n",
                "@test\n",
                "func main()\n",
                "{\n",
                "}\n",
            ),
            ProductKind::Test,
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(function) = symbols
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a function");
        };

        let surface = compilation
            .declaration_directives(function.id().into())
            .unwrap_or_else(|error| panic!("function directives must bind: {error:?}"));

        let second = compilation
            .declaration_directives(function.id().into())
            .unwrap_or_else(|error| panic!("function directives must remain available: {error:?}"));

        let directives = surface.value().directives();

        assert!(Arc::ptr_eq(&surface, &second));
        assert!(surface.diagnostics().is_empty());

        assert_eq!(
            directives
                .iter()
                .map(|directive| directive.kind())
                .collect::<Vec<_>>(),
            [
                DirectiveKind::Abi,
                DirectiveKind::Link,
                DirectiveKind::Link,
                DirectiveKind::Symbol,
                DirectiveKind::Entrypoint,
                DirectiveKind::Test,
            ]
        );

        assert_eq!(directives[1].arguments().len(), 2);

        assert_eq!(
            directives[1].arguments()[0].name(),
            &DirectiveArgumentName::Positional
        );

        assert!(matches!(
            directives[1].arguments()[1].name(),
            DirectiveArgumentName::Named(name) if name.as_str() == "kind"
        ));

        assert_eq!(
            expression_text(&compilation, directives[1].arguments()[1].expression()),
            "\"static\""
        );
    }

    #[test]
    fn malformed_directive_arguments_publish_binding_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@link(name = )\n",
            "extern func read();\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(function) = symbols
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a function");
        };

        let surface = compilation
            .declaration_directives(function.id().into())
            .unwrap_or_else(|error| panic!("function directives must bind: {error:?}"));

        bray_testing::assert_goal_state_diagnostic_kind(
            surface.diagnostics(),
            DiagnosticKind::BindingMalformedDirectiveArgument,
        );

        let diagnostic_kinds = surface
            .diagnostics()
            .iter()
            .map(bray_diagnostics::Diagnostic::kind)
            .collect::<Vec<_>>();

        assert_eq!(
            diagnostic_kinds,
            [DiagnosticKind::BindingMalformedDirectiveArgument]
        );
    }

    #[test]
    fn unresolved_directive_argument_names_publish_binding_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(missing.value)\n",
            "union Result\n",
            "{\n",
            "    Ready;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(union) = symbols
            .unions()
            .iter()
            .find(|union| union.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a union");
        };

        let surface = compilation
            .declaration_directives(union.id().into())
            .unwrap_or_else(|error| panic!("union directives must bind: {error:?}"));

        assert_eq!(
            surface
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingUnresolvedName]
        );
    }

    #[test]
    fn module_directives_retain_exact_partial_module_attachments() {
        let compilation = compilation_with_sources_product_and_worker_budget(
            &[
                concat!(
                    "@target(target.scalar.U64)\n",
                    "@link(\"first\")\n",
                    "module app;\n",
                ),
                concat!("@test\n", "@link(\"second\")\n", "module app;\n"),
            ],
            ProductKind::Test,
            WorkerBudget::serial(),
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(module) = symbols
            .modules()
            .iter()
            .find(|module| module.origin() == SymbolOrigin::Source)
        else {
            panic!("test sources must declare a module");
        };

        let [first_part, second_part] = module.module_parts() else {
            panic!("test module must contain two source contributions");
        };

        let surface = compilation
            .declaration_directives(module.id().into())
            .unwrap_or_else(|error| panic!("module directives must bind: {error:?}"));

        let directives = surface.value().directives();

        assert_eq!(directives.len(), 4);

        assert_eq!(
            directives[0].attachment(),
            DirectiveAttachment::ModulePart(*first_part)
        );

        assert_eq!(
            directives[1].attachment(),
            DirectiveAttachment::ModulePart(*first_part)
        );

        assert_eq!(
            directives[2].attachment(),
            DirectiveAttachment::ModulePart(*second_part)
        );

        assert_eq!(
            directives[3].attachment(),
            DirectiveAttachment::ModulePart(*second_part)
        );
    }

    #[test]
    fn type_and_variant_directives_publish_through_the_same_fact() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "@layout(c)\n",
            "@copy\n",
            "struct Value\n",
            "{\n",
            "}\n",
            "\n",
            "@layout(c)\n",
            "union Result\n",
            "{\n",
            "    @tag(1)\n",
            "    Ok;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(structure) = symbols
            .structures()
            .iter()
            .find(|structure| structure.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a struct");
        };

        let Some(union) = symbols
            .unions()
            .iter()
            .find(|union| union.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a union");
        };

        let Some(variant) = symbols
            .union_variants()
            .iter()
            .find(|variant| variant.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a union variant");
        };

        let structure = compilation
            .declaration_directives(structure.id().into())
            .unwrap_or_else(|error| panic!("struct directives must bind: {error:?}"));

        let union = compilation
            .declaration_directives(union.id().into())
            .unwrap_or_else(|error| panic!("union directives must bind: {error:?}"));

        let variant = compilation
            .declaration_directives(variant.id().into())
            .unwrap_or_else(|error| panic!("variant directives must bind: {error:?}"));

        assert_eq!(
            structure
                .value()
                .directives()
                .iter()
                .map(|directive| directive.kind())
                .collect::<Vec<_>>(),
            [DirectiveKind::Layout, DirectiveKind::Copy]
        );

        assert_eq!(union.value().directives()[0].kind(), DirectiveKind::Layout);
        assert_eq!(variant.value().directives()[0].kind(), DirectiveKind::Tag);
    }

    #[test]
    fn callable_type_directives_publish_without_changing_canonical_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func use(callback: @abi(c) func())\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let Some(function) = symbols
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
        else {
            panic!("test source must declare a function");
        };

        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function.id())),
        );

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let parameters = signature
            .value()
            .parameter_type_templates(values)
            .unwrap_or_else(|error| panic!("parameter templates must be available: {error:?}"));

        let [TypeExpressionTemplate::Resolved(_)] = parameters.as_slice() else {
            panic!("constant-free callable parameter must resolve canonically");
        };

        let key =
            CallableTypeDirectiveKey::new(function.id().into(), callable_type_anchor(&compilation));

        let surface = compilation
            .callable_type_directives(key)
            .unwrap_or_else(|error| panic!("callable type directives must bind: {error:?}"));

        let repeated = compilation
            .callable_type_directives(key)
            .unwrap_or_else(|error| {
                panic!("callable type directives must remain cached: {error:?}")
            });

        let [directive] = surface.value().directives() else {
            panic!("callable type must publish one ABI directive");
        };

        assert!(Arc::ptr_eq(&surface, &repeated));
        assert!(surface.diagnostics().is_empty());
        assert_eq!(directive.kind(), DirectiveKind::Abi);

        assert!(matches!(
            directive.attachment(),
            DirectiveAttachment::CallableType(_)
        ));

        assert_eq!(
            expression_text(&compilation, directive.arguments()[0].expression()),
            "c"
        );
    }

    fn callable_type_anchor(compilation: &Compilation) -> SyntaxAnchor {
        let mut result = None;

        walk_syntax_tree(compilation.syntax_tree(), |event| {
            let SyntaxWalkEvent::EnterNode(node) = event else {
                return SyntaxWalkControl::Continue;
            };

            if node.kind() != SyntaxKind::TypeExpression {
                return SyntaxWalkControl::Continue;
            }

            let Some(expression) = TypeExpressionSyntax::cast_from(node) else {
                return SyntaxWalkControl::Continue;
            };

            if expression.func_keyword().is_none() {
                return SyntaxWalkControl::Continue;
            }

            result = Some(SyntaxAnchor::from_node(&expression));

            SyntaxWalkControl::Stop
        });

        match result {
            Some(result) => result,
            None => panic!("test source must contain a callable type"),
        }
    }

    fn expression_text(
        compilation: &Compilation,
        expression: bray_symbols::DeclarationExpressionTemplate,
    ) -> &str {
        let syntax = expression.syntax();

        let Some(source) = compilation.source(syntax.source_id()) else {
            panic!("directive argument source must remain loaded");
        };

        let Some(text) = source.text_slice(syntax.full_range()) else {
            panic!("directive argument range must remain valid");
        };

        text
    }
}
