use std::sync::Arc;

use bray_binder::{BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider};
use bray_bound_tree::{
    BoundCallableTarget, BoundUnitKey, BoundUnitKind, CheckedTemplateKind, SemanticSelection,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticDependencySubjectKind,
    DiagnosticExpressionCategory, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    DeclarationDirectivesQuery, DirectiveKind, StaticDeclaredTypeQuery, StaticInstanceTemplate,
    StaticInstanceTemplateId, StaticInstanceTemplateQuery, StaticStorageDuration, StaticSymbolId,
    SymbolQueryContract, SymbolQueryRequest,
};

use super::binding::CompilationSymbolQueryEvaluator;
use super::cache::CompilationSymbolSemantics;
use super::declaration_body::checked_source_expression;
use super::imported::imported_declaration_template;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<StaticInstanceTemplateQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<StaticInstanceTemplateQuery> {
        &self.static_instance_templates
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<StaticInstanceTemplateQuery>,
    ) -> BindingQueryResult<
        DiagnosticResult<<StaticInstanceTemplateQuery as SymbolQueryContract>::Value>,
    > {
        bind_static_instance_template(context, request.owner())
    }
}

impl crate::compilation::Compilation {
    /// Returns the complete checked open template of one static declaration.
    pub fn static_instance_template(
        &self,
        declaration: StaticSymbolId,
    ) -> Result<Arc<DiagnosticResult<StaticInstanceTemplate>>, crate::fact::FactQueryError> {
        let context = self.binding_context(&self.state.cancellation)?;

        context
            .resolve_symbol_query(SymbolQueryRequest::<StaticInstanceTemplateQuery>::new(
                declaration,
            ))
            .map_err(crate::compilation::binder::binding_query_error)
    }
}

fn bind_static_instance_template(
    context: &CompilationBindingContext<'_>,
    declaration: StaticSymbolId,
) -> BindingQueryResult<DiagnosticResult<StaticInstanceTemplate>> {
    let declared_type = context.resolve_symbol_query(SymbolQueryRequest::<
        StaticDeclaredTypeQuery,
    >::new(declaration))?;

    let imported = context
        .imported_semantic_address(declaration.into())?
        .is_some();

    let directives = if imported {
        None
    } else {
        Some(context.resolve_symbol_query(SymbolQueryRequest::<
            DeclarationDirectivesQuery,
        >::new(declaration.into()))?)
    };

    let thread_local = directives.as_ref().and_then(|directives| {
        directives
            .value()
            .directives()
            .iter()
            .find(|directive| directive.kind() == DirectiveKind::ThreadLocal)
    });

    let source_duration = if thread_local.is_some() {
        StaticStorageDuration::ExactThread
    } else {
        StaticStorageDuration::Product
    };

    let mut diagnostics = DiagnosticBag::merged_all(
        std::iter::once(declared_type.diagnostics())
            .chain(directives.as_ref().map(|directives| directives.diagnostics())),
    );

    if let Some(directive) = thread_local
        && !context
            .compilation()
            .requested_target()
            .profile()
            .properties()
            .native_threads()
    {
        let span = SourceSpan::new(
            directive.syntax().source_id(),
            directive.syntax().full_range(),
        );

        diagnostics.add(
            Diagnostic::new(
                DiagnosticId::new(span.start().bytes()),
                DiagnosticKind::CheckingThreadLocalStaticUnavailable,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_arg(DiagnosticArg::target_triple(
                context
                    .compilation()
                    .requested_target()
                    .profile()
                    .identity()
                    .as_str(),
            )),
        );
    }

    let (duration, dependency_contract, lifecycle_obligations, witness_requirements) =
        static_initializer_behavior(context, declaration, source_duration, &mut diagnostics)?;

    Ok(DiagnosticResult::new(
        StaticInstanceTemplate::new(
            StaticInstanceTemplateId::new(declaration),
            duration,
            declared_type.value().clone(),
            dependency_contract,
            lifecycle_obligations,
            witness_requirements,
        ),
        diagnostics,
    ))
}

fn static_initializer_behavior(
    context: &CompilationBindingContext<'_>,
    declaration: StaticSymbolId,
    source_duration: StaticStorageDuration,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<(
    StaticStorageDuration,
    bray_symbols::DependencyContractTemplateId,
    Vec<bray_symbols::LifecycleObligationKind>,
    Vec<bray_symbols::SymbolKey>,
)> {
    if let Some(key) = context
        .compilation()
        .static_initializer_key(declaration)
        .map_err(|_| BindingQueryError::DependencyUnavailable)?
    {
        let checked = checked_source_expression(context, key.clone())?;

        let behavior = context
            .compilation()
            .body_behavior_with_cancellation(key.clone(), context.cancellation())
            .map_err(super::binding::binder_error)?;

        *diagnostics = DiagnosticBag::merged_all([
            diagnostics,
            &checked.diagnostics,
            behavior.result().diagnostics(),
        ]);

        diagnostics.add_range(validate_static_initializer_calls(context, &key)?);

        diagnostics.add_range(validate_static_dependency_duration(
            context,
            source_duration,
            checked.dependency_contract,
            &key,
        )?);

        return Ok((
            source_duration,
            checked.dependency_contract,
            behavior.result().value().lifecycle_obligations().to_vec(),
            Vec::new(),
        ));
    }

    let address = context
        .imported_semantic_address(declaration.into())?
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let product = imported_declaration_template(
        context,
        address,
        CheckedTemplateKind::ProductStaticInitializer,
    )?;

    let thread = imported_declaration_template(
        context,
        address,
        CheckedTemplateKind::ThreadLocalStaticInitializer,
    )?;

    *diagnostics = DiagnosticBag::merged_all([
        diagnostics,
        product.diagnostics(),
        thread.diagnostics(),
    ]);

    let (duration, template) = match (product.value().as_ref(), thread.value().as_ref()) {
        (Some(template), None) => (StaticStorageDuration::Product, template.template()),
        (None, Some(template)) => (StaticStorageDuration::ExactThread, template.template()),
        _ => return Err(BindingQueryError::DependencyUnavailable),
    };

    Ok((
        duration,
        template.behavior().dependency_contract(),
        template.behavior().lifecycle_obligations().to_vec(),
        template
            .behavior()
            .witnesses()
            .iter()
            .map(|witness| witness.declaration().clone())
        .collect(),
    ))
}

fn validate_static_dependency_duration(
    context: &CompilationBindingContext<'_>,
    duration: StaticStorageDuration,
    contract: bray_symbols::DependencyContractTemplateId,
    key: &BoundUnitKey,
) -> BindingQueryResult<Vec<Diagnostic>> {
    if duration != StaticStorageDuration::Product {
        return Ok(Vec::new());
    }

    let contract = context
        .semantic_values()
        .dependency_contract_template_data(contract)
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    if !contract
        .requirements()
        .iter()
        .any(requirement_reaches_exact_thread)
    {
        return Ok(Vec::new());
    }

    let syntax = key.source().syntax();
    let span = SourceSpan::new(syntax.source_id(), syntax.full_range());

    Ok(vec![
        Diagnostic::new(
            DiagnosticId::new(span.start().bytes()),
            DiagnosticKind::CheckingStaticDependencyOutlivesOwner,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(DiagnosticArg::dependency_subject_kind(
            DiagnosticDependencySubjectKind::ExactThreadStatic,
        )),
    ])
}

fn requirement_reaches_exact_thread(requirement: &bray_symbols::DependencyRequirement) -> bool {
    match requirement {
        bray_symbols::DependencyRequirement::Direct { subject, .. } => matches!(
            subject.subject_root(),
            bray_symbols::DependencySubjectRoot::ExactThreadStatic(_)
        ),
        bray_symbols::DependencyRequirement::Guarded(guarded) => {
            guard_reaches_exact_thread(guarded.guard())
                || guarded
                    .requirements()
                    .iter()
                    .any(requirement_reaches_exact_thread)
        }
    }
}

fn guard_reaches_exact_thread(guard: &bray_symbols::DependencyGuard) -> bool {
    let subject = match guard {
        bray_symbols::DependencyGuard::NullablePresent(subject)
        | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
    };

    matches!(
        subject.subject_root(),
        bray_symbols::DependencySubjectRoot::ExactThreadStatic(_)
    )
}

fn validate_static_initializer_calls(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
) -> BindingQueryResult<Vec<Diagnostic>> {
    let bound = context
        .compilation()
        .bound_unit_with_cancellation(key.clone(), context.cancellation())
        .map_err(super::binding::binder_error)?;

    let semantics = context
        .compilation()
        .expression_semantics_with_cancellation(key.clone(), context.cancellation())
        .map_err(super::binding::binder_error)?;

    let mut diagnostics = Vec::new();

    for entry in semantics.result().value().1.entries() {
        let SemanticSelection::Call(call) = entry.selection() else {
            continue;
        };

        let is_constant = match call.target() {
            BoundCallableTarget::Declaration(callable) => context
                .compilation()
                .is_constant_callable(callable, context.cancellation())
                .map_err(super::binding::binder_error)?,
            BoundCallableTarget::Predicate(_) => true,
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => false,
        };

        if is_constant {
            continue;
        }

        let expression = bound
            .result()
            .value()
            .view()
            .expression(entry.expression())
            .ok_or(BindingQueryError::DependencyUnavailable)?;

        let anchor = expression.origin().source_anchor().syntax();
        let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

        diagnostics.push(
            Diagnostic::new(
                DiagnosticId::new(span.start().bytes()),
                DiagnosticKind::CheckingInvalidConstantExpression,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidConstantExpression,
                span,
            ))
            .with_arg(DiagnosticArg::expression_category(
                DiagnosticExpressionCategory::Call,
            ))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::ConstantExpressionMustBeEvaluable,
            )),
        );
    }

    Ok(diagnostics)
}

impl crate::compilation::Compilation {
    pub(in crate::compilation) fn static_initializer_key(
        &self,
        declaration: StaticSymbolId,
    ) -> Result<Option<BoundUnitKey>, crate::fact::FactQueryError> {
        let symbols = self.symbol_graph()?;

        let Some(owner) = symbols.symbol_key(declaration.into()) else {
            return Ok(None);
        };

        Ok(self.declared_unit_keys()?.into_iter().find(|key| {
            key.kind() == BoundUnitKind::ConstantTemplate && key.declared_owner() == owner
        }))
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{DependencyRequirement, DependencySubjectRoot, StaticStorageDuration};
    use bray_testing::diagnostics_of_kind;

    use super::super::test_support::{source_id, symbol_graph};
    use crate::test_support::{compilation, compilation_with_target_operations};

    #[test]
    fn product_static_publishes_a_checked_open_template() {
        let compilation = compilation("module app; static Answer: i32 = 42;");
        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("static template must publish: {error:?}"));

        assert!(template.diagnostics().is_empty());
        assert_eq!(template.value().id().declaration(), declaration);
        assert_eq!(template.value().duration(), StaticStorageDuration::Product);
        assert!(template.value().declared_type().resolved_type().is_some());
    }

    #[test]
    fn thread_local_static_selects_the_exact_thread_duration() {
        let compilation = compilation("module app; @thread_local static Cache: i32 = 0;");
        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("thread-local template must publish: {error:?}"));

        assert!(template.diagnostics().is_empty());

        assert_eq!(
            template.value().duration(),
            StaticStorageDuration::ExactThread
        );
    }

    #[test]
    fn generic_static_keeps_its_open_constant_initializer() {
        let compilation =
            compilation("module app; static Value<const N: i32>: i32 = N;");

        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("generic static template must publish: {error:?}"));

        assert!(template.diagnostics().is_empty());
    }

    #[test]
    fn thread_local_static_requires_native_thread_target_support() {
        let compilation = compilation_with_target_operations(
            "module app; @thread_local static Cache: i32 = 0;",
            false,
            false,
        );

        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("thread-local template must recover: {error:?}"));

        assert_eq!(
            diagnostics_of_kind(
                template.diagnostics(),
                DiagnosticKind::CheckingThreadLocalStaticUnavailable,
            )
            .len(),
            1
        );
    }

    #[test]
    fn static_initializer_retains_a_shared_product_static_dependency() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static Root: i32 = 1;\n",
            "static Alias: &i32 = &Root;\n",
        ));

        let symbols = symbol_graph(&compilation);

        let declarations = symbols
            .statics()
            .iter()
            .filter(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();

        let [root, alias] = declarations.as_slice() else {
            panic!("test source must contain two static declarations");
        };

        let template = compilation
            .static_instance_template(*alias)
            .unwrap_or_else(|error| panic!("dependent static template must publish: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            template.diagnostics()
        );

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let dependency = values
            .dependency_contract_template_data(template.value().dependency_contract())
            .unwrap_or_else(|error| panic!("dependency contract must publish: {error:?}"));

        assert!(dependency.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                DependencyRequirement::Direct { subject, .. }
                    if subject.subject_root() == DependencySubjectRoot::ProductStatic(*root)
            )
        }));
    }

    #[test]
    fn static_initializer_retains_an_exact_thread_static_dependency() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@thread_local static Root: i32 = 1;\n",
            "@thread_local static Alias: &i32 = &Root;\n",
        ));

        let symbols = symbol_graph(&compilation);

        let declarations = symbols
            .statics()
            .iter()
            .filter(|symbol| symbol.origin() == bray_symbols::SymbolOrigin::Source)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();

        let [root, alias] = declarations.as_slice() else {
            panic!("test source must contain two static declarations");
        };

        let template = compilation
            .static_instance_template(*alias)
            .unwrap_or_else(|error| panic!("dependent static template must publish: {error:?}"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let dependency = values
            .dependency_contract_template_data(template.value().dependency_contract())
            .unwrap_or_else(|error| panic!("dependency contract must publish: {error:?}"));

        assert!(dependency.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                DependencyRequirement::Direct { subject, .. }
                    if subject.subject_root() == DependencySubjectRoot::ExactThreadStatic(*root)
            )
        }));
    }

    #[test]
    fn product_static_reports_an_exact_thread_dependency() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@thread_local static ThreadValue: i32 = 1;\n",
            "static ProductValue: &i32 = &ThreadValue;\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingStaticDependencyOutlivesOwner,
            )
            .len(),
            1
        );
    }

    #[test]
    fn static_storage_supports_shared_reads_and_requires_mutation_authority() {
        let readable = compilation(concat!(
            "module app;\n",
            "static Root: i32 = 1;\n",
            "func read() -> i32\n",
            "{\n",
            "    return Root;\n",
            "}\n",
        ));

        assert!(
            readable.check_diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            readable.check_diagnostics()
        );

        let mutation = compilation(concat!(
            "module app;\n",
            "static Root: i32 = 1;\n",
            "func write()\n",
            "{\n",
            "    Root = 2;\n",
            "}\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                mutation.check_diagnostics(),
                DiagnosticKind::CheckingMissingMutationAuthority,
            )
            .len(),
            1
        );
    }

    #[test]
    fn static_initializer_calls_constant_functions() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func build() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
            "static Value: i32 = build();\n",
        ));

        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("static template must publish: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            template.diagnostics()
        );
    }

    #[test]
    fn static_initializer_reports_runtime_function_calls() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func build() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
            "static Value: i32 = build();\n",
        ));

        let symbols = symbol_graph(&compilation);

        let declaration = source_id(
            symbols.statics(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("static template must recover: {error:?}"));

        assert_eq!(
            diagnostics_of_kind(
                template.diagnostics(),
                DiagnosticKind::CheckingInvalidConstantExpression,
            )
            .len(),
            1
        );
    }
}
