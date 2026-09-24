use super::dependencies::{
    static_dependencies_from_contract, validate_static_dependency_duration,
    witness_requirements_from_contract,
};
use crate::compilation::binder::BindingQueryResult;
use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundUnitKey, CheckedTemplateKind, CheckedTemplateOperation, SemanticSelection,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    CallableContractsQuery, CallableSymbolId, DeclarationDirectivesQuery, DirectiveKind,
    StaticDeclaredTypeQuery, StaticInstanceTemplate, StaticInstanceTemplateId,
    StaticInstanceTemplateQuery, StaticStorageDuration, StaticSymbolId, SymbolQueryContract,
    SymbolQueryRequest, TypeAssociatedLifecycleSlot, TypeData,
};

use super::super::binding::CompilationSymbolQueryEvaluator;
use super::super::cache::CompilationSymbolSemantics;
use super::super::declaration_body::checked_source_expression;
use super::super::imported::imported_declaration_template;
use super::super::initializer::validate_static_initializer_template;
use crate::compilation::binder::{
    CompilationBindingContext, semantic_contract_binding_error as binding_contract,
    symbol_query_contract_binding_error as query_contract,
};
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryViolation, SemanticSymbolCategory,
};
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
        Some(context.resolve_symbol_query(
            SymbolQueryRequest::<DeclarationDirectivesQuery>::new(declaration.into()),
        )?)
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
        std::iter::once(declared_type.diagnostics()).chain(
            directives
                .as_ref()
                .map(|directives| directives.diagnostics()),
        ),
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
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidDeclaration,
                span,
            ))
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

    let (
        duration,
        dependency_contract,
        lifecycle_obligations,
        witness_requirements,
        mut lifecycle_dependencies,
    ) = static_initializer_behavior(context, declaration, source_duration, &mut diagnostics)?;

    lifecycle_dependencies.extend(static_type_lifecycle_dependencies(
        context,
        declared_type.value(),
        &mut diagnostics,
    )?);

    lifecycle_dependencies.sort_unstable();
    lifecycle_dependencies.dedup();

    diagnostics.add_range(validate_static_lifecycle_graph(
        context,
        declaration,
        &lifecycle_dependencies,
    )?);

    Ok(DiagnosticResult::new(
        StaticInstanceTemplate::new(
            StaticInstanceTemplateId::new(declaration),
            duration,
            declared_type.value().clone(),
            dependency_contract,
            lifecycle_dependencies,
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
    Vec<StaticSymbolId>,
)> {
    let native = context
        .compilation()
        .foreign_static_contract_with_cancellation(declaration, context.cancellation())
        .map_err(super::super::binding::binder_error)?;

    diagnostics.add_range(native.diagnostics().iter().cloned());

    if native.value().as_ref().is_some_and(|contract| {
        contract.direction() == bray_symbols::ForeignCallableDirection::Import
    }) {
        let dependency_contract = context
            .semantic_values()
            .empty_dependency_contract_template()
            .map_err(crate::compilation::binder::semantic_value_binding_error)?;

        return Ok((
            source_duration,
            dependency_contract,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }

    if let Some(boundary) = context
        .compilation()
        .imported_native_boundary_with_cancellation(declaration.into(), context.cancellation())
        .map_err(super::super::binding::binder_error)?
        && let bray_package_interface::InterfaceNativeBoundaryKind::Static { duration, .. } =
            boundary.kind()
    {
        let dependency_contract = context
            .semantic_values()
            .empty_dependency_contract_template()
            .map_err(crate::compilation::binder::semantic_value_binding_error)?;

        return Ok((
            duration,
            dependency_contract,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }

    if let Some(key) = context
        .compilation()
        .static_initializer_key(declaration)
        .map_err(super::super::binding::binder_error)?
    {
        let checked = checked_source_expression(context, key.clone())?;

        let behavior = context
            .compilation()
            .body_behavior_with_cancellation(key.clone(), context.cancellation())
            .map_err(super::super::binding::binder_error)?;

        *diagnostics = DiagnosticBag::merged_all([
            diagnostics,
            &checked.diagnostics,
            behavior.result().diagnostics(),
        ]);

        diagnostics.add_range(validate_static_initializer_template(context, &key)?);

        let semantics = context
            .compilation()
            .expression_semantics_with_cancellation(key.clone(), context.cancellation())
            .map_err(super::super::binding::binder_error)?;

        if semantics
            .result()
            .value()
            .selections()
            .entries()
            .iter()
            .any(|entry| {
                matches!(
                    entry.selection(),
                    SemanticSelection::StaticReference(reference)
                        if reference.template().declaration() == declaration
                            && reference.closed_instance().is_none()
                )
            })
        {
            diagnostics.add(static_source_diagnostic(
                &key,
                DiagnosticKind::CheckingStaticSpecializationDivergence,
            ));
        }

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
            witness_requirements_from_contract(context, checked.dependency_contract)?,
            static_dependencies_from_contract(context, checked.dependency_contract)?,
        ));
    }

    let address = context
        .imported_semantic_address(declaration.into())?
        .ok_or_else(|| {
            query_contract(
                declaration.into(),
                StaticInstanceTemplateQuery::KIND,
                SemanticQueryViolation::Missing(SemanticDataKind::ImportedTemplate),
            )
        })?;

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

    *diagnostics =
        DiagnosticBag::merged_all([diagnostics, product.diagnostics(), thread.diagnostics()]);

    let product_template = product.value().as_ref();
    let thread_template = thread.value().as_ref();

    let (duration, template) = match (product_template, thread_template) {
        (Some(template), None) => (StaticStorageDuration::Product, template.template()),
        (None, Some(template)) => (StaticStorageDuration::ExactThread, template.template()),
        _ => {
            return Err(query_contract(
                declaration.into(),
                StaticInstanceTemplateQuery::KIND,
                SemanticQueryViolation::CountMismatch {
                    data: SemanticDataKind::ImportedTemplate,
                    expected: 1,
                    actual: usize::from(product_template.is_some())
                        .saturating_add(usize::from(thread_template.is_some())),
                },
            ));
        }
    };

    let mut lifecycle_dependencies =
        static_dependencies_from_contract(context, template.behavior().dependency_contract())?;

    for node in template.nodes() {
        if let CheckedTemplateOperation::Declaration { declaration, .. } = node.operation()
            && let Some(bray_symbols::AnySymbolId::Static(dependency)) =
                context.symbols().symbol_for_key(declaration)
        {
            lifecycle_dependencies.push(dependency);
        }
    }

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
        lifecycle_dependencies,
    ))
}

fn static_type_lifecycle_dependencies(
    context: &CompilationBindingContext<'_>,
    declared_type: &bray_symbols::TypeExpressionTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<Vec<StaticSymbolId>> {
    let Some(ty) = declared_type.resolved_type() else {
        return Ok(Vec::new());
    };

    let data = context.semantic_values().type_data(ty);

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(Vec::new());
    };

    let surface = context
        .compilation()
        .type_associated_surface_result_with_cancellation(*definition, context.cancellation())
        .map_err(super::super::binding::binder_error)?;

    *diagnostics = diagnostics.merged(surface.diagnostics());

    let mut dependencies = Vec::new();

    for lifecycle in surface.value().lifecycle_members() {
        if !matches!(
            lifecycle.slot(),
            TypeAssociatedLifecycleSlot::Finalizer | TypeAssociatedLifecycleSlot::Destructor
        ) {
            continue;
        }

        let callable = CallableSymbolId::try_from_any(lifecycle.id()).ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Type(ty),
                SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: SemanticSymbolCategory::Callable,
                    actual: lifecycle.id().kind(),
                },
            )
        })?;

        let contracts = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))?;

        *diagnostics = diagnostics.merged(contracts.diagnostics());

        dependencies.extend(static_dependencies_from_contract(
            context,
            contracts
                .value()
                .phase_behaviors()
                .invocation()
                .dependency_contract(),
        )?);

    }

    dependencies.sort_unstable();
    dependencies.dedup();

    Ok(dependencies)
}

fn validate_static_lifecycle_graph(
    context: &CompilationBindingContext<'_>,
    declaration: StaticSymbolId,
    dependencies: &[StaticSymbolId],
) -> BindingQueryResult<Vec<Diagnostic>> {
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();

    if !static_lifecycle_is_cyclic(
        context,
        declaration,
        declaration,
        dependencies,
        &mut active,
        &mut complete,
    )? {
        return Ok(Vec::new());
    }

    let Some(key) = context
        .compilation()
        .static_initializer_key(declaration)
        .map_err(super::super::binding::binder_error)?
    else {
        return Ok(Vec::new());
    };

    Ok(vec![static_source_diagnostic(
        &key,
        DiagnosticKind::CheckingStaticLifecycleCycle,
    )])
}

fn static_lifecycle_is_cyclic(
    context: &CompilationBindingContext<'_>,
    root: StaticSymbolId,
    current: StaticSymbolId,
    root_dependencies: &[StaticSymbolId],
    active: &mut BTreeSet<StaticSymbolId>,
    complete: &mut BTreeSet<StaticSymbolId>,
) -> BindingQueryResult<bool> {
    if complete.contains(&current) {
        return Ok(false);
    }

    if !active.insert(current) {
        return Ok(true);
    }

    let dependencies = if current == root {
        root_dependencies.to_vec()
    } else {
        raw_static_lifecycle_dependencies(context, current)?
    };

    for dependency in dependencies {
        if static_lifecycle_is_cyclic(
            context,
            root,
            dependency,
            root_dependencies,
            active,
            complete,
        )? {
            return Ok(true);
        }
    }

    active.remove(&current);
    complete.insert(current);

    Ok(false)
}

fn raw_static_lifecycle_dependencies(
    context: &CompilationBindingContext<'_>,
    declaration: StaticSymbolId,
) -> BindingQueryResult<Vec<StaticSymbolId>> {
    let mut diagnostics = DiagnosticBag::new();

    let (_, _, _, _, mut dependencies) = static_initializer_behavior(
        context,
        declaration,
        StaticStorageDuration::Product,
        &mut diagnostics,
    )?;

    let declared_type = context.resolve_symbol_query(SymbolQueryRequest::<
        StaticDeclaredTypeQuery,
    >::new(declaration))?;

    dependencies.extend(static_type_lifecycle_dependencies(
        context,
        declared_type.value(),
        &mut diagnostics,
    )?);

    dependencies.sort_unstable();
    dependencies.dedup();

    Ok(dependencies)
}

fn static_source_diagnostic(key: &BoundUnitKey, kind: DiagnosticKind) -> Diagnostic {
    let syntax = key.source().syntax();
    let span = SourceSpan::new(syntax.source_id(), syntax.full_range());

    Diagnostic::new(
        DiagnosticId::new(span.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::InvalidConstantExpression,
        span,
    ))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::SemanticSelection;
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{DependencyRequirement, DependencySubjectRoot, StaticStorageDuration};
    use bray_testing::{assert_goal_state_diagnostic_kind, diagnostics_of_kind};

    use super::super::super::test_support::{source_id, symbol_graph};
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
        let compilation = compilation("module app; static Value<const N: i32>: i32 = N;");

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

        assert_goal_state_diagnostic_kind(
            template.diagnostics(),
            DiagnosticKind::CheckingThreadLocalStaticUnavailable,
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

        let dependency =
            values.dependency_contract_template_data(template.value().dependency_contract());

        assert!(dependency.requirements().iter().any(|requirement| {
            matches!(
                requirement,
                DependencyRequirement::Direct { subject, .. }
                    if subject.subject_root() == DependencySubjectRoot::ProductStatic(*root)
            )
        }));

        assert_eq!(template.value().lifecycle_dependencies(), &[*root]);
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

        let dependency =
            values.dependency_contract_template_data(template.value().dependency_contract());

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

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingStaticDependencyOutlivesOwner,
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
    fn static_initializer_accepts_array_literals() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static VALUES: [u32; 2] = [1, 2];\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            compilation.check_diagnostics(),
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

    #[test]
    fn static_initializer_reports_direct_static_value_reads() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static Root: i32 = 1;\n",
            "static Copy: i32 = Root;\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingInvalidConstantExpression,
            )
            .len(),
            1
        );
    }

    #[test]
    fn generic_static_reference_selects_one_closed_instance_key() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Marker {}\n",
            "struct Marked {}\n",
            "impl Marked(Marker) {}\n",
            "static Value<T>: i32 with(T: Marker) = 1;\n",
            "static Selected: &i32 = &Value<Marked>;\n",
        ));

        let symbols = symbol_graph(&compilation);

        let selected = symbols
            .statics()
            .iter()
            .find(|symbol| {
                symbol.origin() == bray_symbols::SymbolOrigin::Source
                    && symbol.generic_type_parameters().is_empty()
            })
            .map(|symbol| symbol.id())
            .unwrap_or_else(|| panic!("selected static must exist"));

        let key = compilation
            .static_initializer_key(selected)
            .unwrap_or_else(|error| panic!("initializer key must publish: {error:?}"))
            .unwrap_or_else(|| panic!("selected static must have an initializer"));

        let semantics = compilation
            .expression_semantics_with_cancellation(key, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("initializer semantics must publish: {error:?}"));

        let instances = semantics
            .result()
            .value()
            .selections()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::StaticReference(reference) => reference.closed_instance(),
                _ => None,
            })
            .collect::<Vec<_>>();

        let [instance] = instances.as_slice() else {
            panic!("one closed static instance must be selected: {instances:?}");
        };

        assert_eq!(instance.template().declaration(), symbols.statics()[0].id());
        assert_eq!(instance.selected_witnesses().len(), 1);
    }

    #[test]
    fn distinct_const_generic_static_references_select_distinct_instances() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static VALUE<const N: u64>: u64 = N;\n",
            "func values() -> (u64, u64)\n",
            "{\n",
            "    return(VALUE<7>, VALUE<11>);\n",
            "}\n",
        ));

        let key = compilation
            .declared_unit_keys_for_test()
            .unwrap_or_else(|error| panic!("declared units must publish: {error:?}"))
            .into_iter()
            .find(|key| key.kind() == bray_bound_tree::BoundUnitKind::CallableBody)
            .unwrap_or_else(|| panic!("callable body must exist"));

        let semantics = compilation
            .expression_semantics_with_cancellation(key, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("callable semantics must publish: {error:?}"));

        let instances = semantics
            .result()
            .value()
            .selections()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::StaticReference(reference) => reference.closed_instance(),
                _ => None,
            })
            .collect::<Vec<_>>();

        let [first, second] = instances.as_slice() else {
            panic!("two closed static instances must be selected: {instances:?}");
        };

        assert_ne!(first, second);
        assert_ne!(first.substitution(), second.substitution());
    }

    #[test]
    fn static_type_cleanup_retains_its_callable_dependencies() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static Root: i32 = 1;\n",
            "struct Resource {}\n",
            "impl Resource\n",
            "{\n",
            "    finalize()\n",
            "    {\n",
            "        let value = Root;\n",
            "        value;\n",
            "    }\n",
            "}\n",
            "static Stored: Resource = Resource {};\n",
        ));

        let symbols = symbol_graph(&compilation);
        let root = symbols.statics()[0].id();
        let stored = symbols.statics()[1].id();

        let template = compilation
            .static_instance_template(stored)
            .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            template.diagnostics()
        );

        assert_eq!(template.value().lifecycle_dependencies(), [root]);
    }

    #[test]
    fn static_type_cleanup_retains_indirect_callable_dependencies() {
        let compilation = compilation(
            r#"
            module app;
            static Root: i32 = 1;
            func read_root()
            {
                let value = Root;
                value;
            }
            struct Resource {}
            impl Resource
            {
                finalize() { read_root(); }
                destruct() { read_root(); }
            }
            static Stored: Resource = Resource {};
            "#,
        );

        let symbols = symbol_graph(&compilation);
        let root = symbols.statics()[0].id();
        let stored = symbols.statics()[1].id();

        let template = compilation
            .static_instance_template(stored)
            .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            template.diagnostics()
        );

        assert_eq!(template.value().lifecycle_dependencies(), [root]);
    }

    #[test]
    fn static_type_cleanup_retains_recursive_callable_dependencies() {
        let compilation = compilation(
            r#"
            module app;
            static Root: i32 = 1;
            func read_root(pos depth: i32)
            {
                if depth > 0 { read_root(depth - 1); }
                let value = Root;
                value;
            }
            struct Resource {}
            impl Resource
            {
                finalize() { read_root(1); }
            }
            static Stored: Resource = Resource {};
            "#,
        );

        let symbols = symbol_graph(&compilation);
        let root = symbols.statics()[0].id();
        let stored = symbols.statics()[1].id();

        let template = compilation
            .static_instance_template(stored)
            .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

        assert!(template.diagnostics().is_empty(), "{:#?}", template.diagnostics());
        assert_eq!(template.value().lifecycle_dependencies(), [root]);
    }

    #[test]
    fn static_type_cleanup_retains_selected_implementation_dependencies() {
        let compilation = compilation(
            r#"
            module app;
            static Root: i32 = 1;
            trait Readable { func read(); }
            struct Subject {}
            impl Subject(Readable)
            {
                func read()
                {
                    let value = Root;
                    value;
                }
            }
            struct Resource {}
            impl Resource
            {
                finalize() { Subject {}(Readable).read(); }
            }
            static Stored: Resource = Resource {};
            "#,
        );

        let symbols = symbol_graph(&compilation);
        let root = symbols.statics()[0].id();
        let stored = symbols.statics()[1].id();

        let template = compilation
            .static_instance_template(stored)
            .unwrap_or_else(|error| panic!("stored static template must publish: {error:?}"));

        assert!(template.diagnostics().is_empty(), "{:#?}", template.diagnostics());
        assert_eq!(template.value().lifecycle_dependencies(), [root]);
    }

    #[test]
    fn closed_generic_static_reference_checks_declaration_constraints() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Argument {}\n",
            "static Value<T>: i32 with(false) = 1;\n",
            "static Selected: &i32 = &Value<Argument>;\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingStaticConstraintUnsatisfied,
            )
            .len(),
            1
        );

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingStaticConstraintUnsatisfied,
        );
    }

    #[test]
    fn static_lifecycle_dependencies_report_cycles() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static First: &i32 = &Second;\n",
            "static Second: &i32 = &First;\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingStaticLifecycleCycle,
            )
            .len(),
            2
        );

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingStaticLifecycleCycle,
        );
    }

    #[test]
    fn static_lifecycle_dependencies_through_helpers_report_cycles() {
        let compilation = compilation(
            r#"
            module app;
            static First: FirstResource = FirstResource {};
            static Second: SecondResource = SecondResource {};
            func read_first() { let value = First; value; }
            func read_second() { let value = Second; value; }
            struct FirstResource {}
            impl FirstResource
            {
                finalize() { read_second(); }
            }
            struct SecondResource {}
            impl SecondResource
            {
                finalize() { read_first(); }
            }
            "#,
        );

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingStaticLifecycleCycle,
            )
            .len(),
            2
        );

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingStaticLifecycleCycle,
        );
    }

    #[test]
    fn open_recursive_static_specialization_reports_divergence() {
        let compilation = compilation(concat!(
            "module app;\n",
            "static Loop<const N: i32>: &i32 = &Loop<N>;\n",
        ));

        assert_eq!(
            diagnostics_of_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingStaticSpecializationDivergence,
            )
            .len(),
            1
        );

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingStaticSpecializationDivergence,
        );
    }
}
