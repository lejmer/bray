// rust-style: allow(module-too-large, reason = "callable contract binding and validation form one publication transaction over shared syntax and symbol state")

use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{
    BindingError, BindingQueryContext, BindingQueryError, PredicateClauseBindingContext,
    SymbolQueryProvider, bind_predicate_clause, bind_trusted_capability_clause,
};
use bray_diagnostics::{
    DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticResult,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableContractClause, CallableContractClauseKind, CallableContractSet,
    CallableContractsQuery, CallableExecution, CallableExecutionRequirement, CallablePhaseBehavior,
    CallableSignatureQuery, CallableSymbolId, CallableTrust, CheckedConstraint,
    CurrentRunCancellation, DependencyContractTemplateId, GenericConstraintSet,
    GenericConstraintsQuery, GenericDeclarationTemplateQuery, GenericOwnerId, SymbolOrigin,
    SymbolQueryContract, SymbolQueryRequest, TrustedCapabilityRequirement,
    TrustedCapabilitySymbolId, TypeData,
};
use bray_syntax::{
    EnsuresClauseSyntax, RequiresClauseSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    UsesClauseSyntax, WithClauseSyntax, syntax_node_view, walk_direct_child_nodes,
};

use super::binding::CompilationSymbolQueryEvaluator;
use super::cache::CompilationSymbolSemantics;
use super::declaration_body::{CheckedSourcePredicateSequence, checked_source_predicate_sequence};
use super::environment::type_binder;
use super::surface::{symbol_ordinal, with_declaration_root};
use crate::compilation::binder::{
    BindingQueryResult, CompilationBindingContext,
    semantic_contract_binding_error as binding_contract,
    symbol_query_contract_binding_error as query_contract,
};
use crate::compilation::diagnostics::source_diagnostic;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryViolation, SemanticSymbolCategory,
};
use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<GenericConstraintsQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<GenericConstraintsQuery> {
        &self.generic_constraints
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<GenericConstraintsQuery>,
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<
            <GenericConstraintsQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_generic_constraints(context, request.symbol())
    }
}

impl CompilationSymbolQueryEvaluator<CallableContractsQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<CallableContractsQuery> {
        &self.callable_contracts
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<CallableContractsQuery>,
    ) -> BindingQueryResult<
        bray_diagnostics::DiagnosticResult<
            <CallableContractsQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_callable_contracts(context, request.owner())
    }
}

fn bind_generic_constraints(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<
    bray_diagnostics::DiagnosticResult<
        <GenericConstraintsQuery as bray_symbols::SymbolQueryContract>::Value,
    >,
> {
    let generic_owner = GenericOwnerId::try_new(owner).ok_or_else(|| {
        query_contract(
            owner,
            GenericConstraintsQuery::KIND,
            SemanticQueryViolation::UnexpectedSymbolKind {
                expected: SemanticSymbolCategory::GenericOwner,
                actual: owner.kind(),
            },
        )
    })?;

    let template = context.resolve_symbol_query(SymbolQueryRequest::<
        GenericDeclarationTemplateQuery,
    >::new(generic_owner))?;

    // The published constraint query owns the Arc-backed template diagnostics independently.
    let template_diagnostics = template.diagnostics().clone();

    if context.imported_semantic_address(owner)?.is_some() {
        let constraints = template
            .value()
            .constraints()
            .iter()
            .map(|constraint| {
                constraint.resolved().ok_or_else(|| {
                    query_contract(
                        owner,
                        GenericConstraintsQuery::KIND,
                        SemanticQueryViolation::Missing(SemanticDataKind::GenericConstraint),
                    )
                })
            })
            .collect::<BindingQueryResult<Vec<_>>>()?;

        return publish_catalog_result(
            GenericConstraintSet::new(constraints),
            template_diagnostics,
        );
    }

    let clauses = with_declaration_root(context, owner, |root| Ok(direct_with_clauses(root)))?;

    let mut constraints = Vec::new();
    let mut diagnostics = template_diagnostics;

    for clause in clauses {
        for expression in clause.expressions() {
            let ordinal = symbol_ordinal(constraints.len())?;

            let source = template
                .value()
                .constraints()
                .get(constraints.len())
                .ok_or_else(|| {
                    query_contract(
                        owner,
                        GenericConstraintsQuery::KIND,
                        SemanticQueryViolation::CountMismatch {
                            data: SemanticDataKind::GenericConstraint,
                            expected: constraints.len().saturating_add(1),
                            actual: template.value().constraints().len(),
                        },
                    )
                })?;

            let constraint = match source {
                bray_symbols::GenericConstraintTemplate::TraitSatisfaction { .. } => {
                    resolve_trait_satisfaction_constraint(context, source, &mut diagnostics)?
                }
                bray_symbols::GenericConstraintTemplate::TypeEquality { .. } => {
                    resolve_type_equality_constraint(context, source, &mut diagnostics)?
                }
                bray_symbols::GenericConstraintTemplate::Source { .. } => {
                    let result = bind_predicate_clause(
                        context,
                        owner,
                        syntax_node_view(&clause),
                        [expression],
                        PredicateClauseBindingContext::GenericConstraint,
                    )?;

                    let (predicates, clause_diagnostics) = result.into_parts();

                    diagnostics = diagnostics.merged(&clause_diagnostics);

                    let [predicate] = predicates.as_ref() else {
                        return Err(query_contract(
                            owner,
                            GenericConstraintsQuery::KIND,
                            SemanticQueryViolation::CountMismatch {
                                data: SemanticDataKind::GenericConstraint,
                                expected: 1,
                                actual: predicates.len(),
                            },
                        ));
                    };

                    CheckedConstraint::new(ordinal, *predicate)
                }
                bray_symbols::GenericConstraintTemplate::Resolved(_) => {
                    return Err(query_contract(
                        owner,
                        GenericConstraintsQuery::KIND,
                        SemanticQueryViolation::Unsupported(SemanticDataKind::GenericConstraint),
                    ));
                }
            };

            constraints.push(constraint);
        }
    }

    publish_catalog_result(GenericConstraintSet::new(constraints), diagnostics)
}

fn bind_callable_contracts(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
) -> BindingQueryResult<
    bray_diagnostics::DiagnosticResult<
        <CallableContractsQuery as bray_symbols::SymbolQueryContract>::Value,
    >,
> {
    if let Some(address) = context.imported_semantic_address(owner.into_any())? {
        return super::imported::imported_callable_contracts(context, address);
    }

    let clauses = with_declaration_root(context, owner.into_any(), |root| {
        Ok(direct_contract_clauses(root))
    })?;

    let mut predicates = Vec::new();
    let mut declared_execution_requirements = Vec::new();
    let mut uses_clauses = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for clause in clauses {
        match clause {
            ContractClauseSyntax::Requires(clause) => bind_callable_predicates(
                context,
                owner.into_any(),
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Requires,
                &mut predicates,
                &mut declared_execution_requirements,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Ensures(clause) => bind_callable_predicates(
                context,
                owner.into_any(),
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Ensures,
                &mut predicates,
                &mut declared_execution_requirements,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::With(clause) => bind_callable_static_constraints(
                context,
                owner.into_any(),
                clause.expressions(),
                &mut predicates,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Uses(clause) => uses_clauses.push(clause),
        }
    }

    let capabilities = bind_trusted_capability_clauses(context, owner, uses_clauses)?;

    diagnostics = diagnostics.merged(capabilities.diagnostics());

    let dependency = context
        .semantic_values
        .empty_dependency_contract_template()
        .map_err(crate::compilation::binder::semantic_value_binding_error)?;

    let signature =
        context.resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(owner))?;

    let (execution, trust, properties) = match signature.value().callable_type() {
        bray_symbols::TypeExpressionTemplate::Callable(callable) => (
            callable.execution(),
            callable.trust(),
            callable
                .phase_behaviors()
                .deferred_execution()
                .unwrap_or_else(|| callable.phase_behaviors().invocation())
                .execution_properties()
                .to_vec(),
        ),
        bray_symbols::TypeExpressionTemplate::Resolved(ty) => {
            let data = context.semantic_values.type_data(*ty);

            match &*data {
                TypeData::Callable(callable) => (
                    callable.execution(),
                    callable.trust(),
                    callable
                        .phase_behaviors()
                        .deferred_execution()
                        .unwrap_or_else(|| callable.phase_behaviors().invocation())
                        .execution_properties()
                        .to_vec(),
                ),
                _ => {
                    return Err(binding_contract(
                        SemanticQueryContext::Type(*ty),
                        SemanticQueryViolation::Unsupported(SemanticDataKind::CallableSignature),
                    ));
                }
            }
        }
        _ => {
            return Err(query_contract(
                owner.into_any(),
                CallableContractsQuery::KIND,
                SemanticQueryViolation::Unsupported(SemanticDataKind::CallableSignature),
            ));
        }
    };

    let definition =
        bray_symbols::CallableDefinitionId::try_new(owner.into_any()).ok_or_else(|| {
            query_contract(
                owner.into_any(),
                CallableContractsQuery::KIND,
                SemanticQueryViolation::Unsupported(SemanticDataKind::BoundUnit),
            )
        })?;

    let body_key = context
        .compilation()
        .callable_body_key(definition)
        .map_err(super::binding::binder_error)?;

    let body_behavior = match &body_key {
        Some(key) => {
            let behavior = context
                .compilation()
                .body_behavior_with_cancellation(key.clone(), context.cancellation())
                .map_err(super::binding::binder_error)?;

            diagnostics = diagnostics.merged(behavior.result().diagnostics());

            Some(behavior)
        }
        None => None,
    };

    let used_capabilities = body_behavior
        .as_ref()
        .map(|behavior| behavior.result().value().trusted_capability_uses());

    let capability_recovered = body_behavior
        .as_ref()
        .is_some_and(|behavior| behavior.result().value().is_recovered());

    let (invocation_behavior, deferred_execution_behavior) = callable_phase_behaviors(
        execution,
        &properties,
        capabilities
            .value()
            .iter()
            .map(|capability| capability.requirement()),
        dependency,
        declared_execution_requirements,
        body_behavior
            .as_ref()
            .map(|behavior| behavior.result().value()),
    );

    validate_trusted_capabilities(
        context,
        owner,
        trust,
        capabilities.value(),
        used_capabilities,
        capability_recovered,
        &mut diagnostics,
    )?;

    publish_catalog_result(
        CallableContractSet::new(predicates, invocation_behavior, deferred_execution_behavior),
        diagnostics,
    )
}

pub(in crate::compilation) fn bind_declared_execution_requirements(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<Vec<CallableExecutionRequirement>>> {
    if context.symbols.symbol_origin(owner.into_any()) != Some(SymbolOrigin::Source) {
        return Ok(DiagnosticResult::without_diagnostics(Vec::new()));
    }

    let clauses = with_declaration_root(context, owner.into_any(), |root| {
        Ok(direct_contract_clauses(root))
    })?;

    let mut requirements = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for clause in clauses {
        let ContractClauseSyntax::Requires(clause) = clause else {
            continue;
        };

        let expressions = clause.expressions().collect::<Vec<_>>();

        let checked = checked_callable_predicates(
            context,
            owner.into_any(),
            syntax_node_view(&clause),
            expressions.len(),
        )?;

        diagnostics = diagnostics.merged(&checked.diagnostics);
        requirements.extend(checked.execution_requirements);
    }

    Ok(DiagnosticResult::new(requirements, diagnostics))
}

pub(in crate::compilation) fn bind_declared_trusted_capabilities(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<Vec<DeclaredTrustedCapability>>> {
    let clauses = with_declaration_root(context, owner.into_any(), |root| {
        Ok(direct_contract_clauses(root))
    })?;

    bind_trusted_capability_clauses(
        context,
        owner,
        clauses.into_iter().filter_map(|clause| match clause {
            ContractClauseSyntax::Uses(clause) => Some(clause),
            ContractClauseSyntax::Requires(_)
            | ContractClauseSyntax::Ensures(_)
            | ContractClauseSyntax::With(_) => None,
        }),
    )
}

fn bind_trusted_capability_clauses(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
    clauses: impl IntoIterator<Item = UsesClauseSyntax>,
) -> BindingQueryResult<DiagnosticResult<Vec<DeclaredTrustedCapability>>> {
    let mut capabilities = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for clause in clauses {
        let result = bind_trusted_capability_clause(context, owner.into_any(), &clause)?;

        let (symbols, clause_diagnostics) = result.into_parts();

        diagnostics = diagnostics.merged(&clause_diagnostics);

        for capability in symbols {
            let ordinal = symbol_ordinal(capabilities.len())?;

            capabilities.push(DeclaredTrustedCapability::new(
                TrustedCapabilityRequirement::new(ordinal, capability.symbol()),
                capability.source(),
            ));
        }
    }

    Ok(DiagnosticResult::new(capabilities, diagnostics))
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(in crate::compilation) struct DeclaredTrustedCapability {
    requirement: TrustedCapabilityRequirement,
    source: bray_declarations::SyntaxAnchor,
}

impl DeclaredTrustedCapability {
    const fn new(
        requirement: TrustedCapabilityRequirement,
        source: bray_declarations::SyntaxAnchor,
    ) -> Self {
        Self {
            requirement,
            source,
        }
    }

    pub(in crate::compilation) const fn requirement(self) -> TrustedCapabilityRequirement {
        self.requirement
    }

    const fn capability(self) -> TrustedCapabilitySymbolId {
        self.requirement.capability()
    }

    const fn source(self) -> bray_declarations::SyntaxAnchor {
        self.source
    }
}

fn callable_phase_behaviors(
    execution: CallableExecution,
    properties: &[bray_symbols::ExecutionProperty],
    trusted_capabilities: impl IntoIterator<Item = TrustedCapabilityRequirement>,
    dependencies: DependencyContractTemplateId,
    declared_execution_requirements: impl IntoIterator<Item = CallableExecutionRequirement>,
    body: Option<&bray_bound_tree::CheckedBodyBehavior>,
) -> (CallablePhaseBehavior, Option<CallablePhaseBehavior>) {
    let effects = body
        .into_iter()
        .flat_map(bray_bound_tree::CheckedBodyBehavior::effects)
        .copied();

    let capabilities = body
        .into_iter()
        .flat_map(bray_bound_tree::CheckedBodyBehavior::capabilities)
        .copied();

    let execution_requirements = declared_execution_requirements.into_iter().chain(
        body.into_iter()
            .flat_map(bray_bound_tree::CheckedBodyBehavior::execution_requirements)
            .copied(),
    );

    let lifecycle_obligations = body
        .into_iter()
        .flat_map(bray_bound_tree::CheckedBodyBehavior::lifecycle_obligations)
        .copied();

    let current_run_cancellation = body.map_or(
        CurrentRunCancellation::NotEntered,
        bray_bound_tree::CheckedBodyBehavior::current_run_cancellation,
    );

    let body_behavior = CallablePhaseBehavior::new(
        effects,
        capabilities,
        trusted_capabilities,
        execution_requirements,
        lifecycle_obligations,
        dependencies,
        current_run_cancellation,
    )
    .with_execution_properties(properties.iter().copied());

    match execution {
        CallableExecution::Synchronous => (body_behavior, None),
        CallableExecution::Asynchronous => (
            CallablePhaseBehavior::empty(dependencies),
            Some(body_behavior),
        ),
    }
}

fn validate_trusted_capabilities(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
    trust: CallableTrust,
    declared: &[DeclaredTrustedCapability],
    used: Option<&[bray_bound_tree::TrustedCapabilityUse]>,
    is_recovered: bool,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<()> {
    let has_body = used.is_some();
    let mut declared_by_capability = BTreeMap::<_, BTreeSet<_>>::new();

    for capability in declared {
        declared_by_capability
            .entry(capability.capability())
            .or_default()
            .insert(capability.source());
    }

    let used_by_capability = used
        .unwrap_or_default()
        .iter()
        .map(|use_| (use_.capability(), use_))
        .collect::<BTreeMap<_, _>>();

    let declared = declared_by_capability
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();

    let used = used_by_capability.keys().copied().collect::<BTreeSet<_>>();

    if declared.is_empty() && used.is_empty() {
        return Ok(());
    }

    if trust != CallableTrust::Trusted {
        let anchor = context
            .symbols
            .declaration_syntax_anchor(owner.into_any())
            .ok_or_else(|| {
                query_contract(
                    owner.into_any(),
                    CallableContractsQuery::KIND,
                    SemanticQueryViolation::Missing(SemanticDataKind::SourceAnchor),
                )
            })?;

        for capability in declared.union(&used) {
            diagnostics.add(trusted_capability_diagnostic(
                context,
                trusted_capability_origins(
                    *capability,
                    &declared_by_capability,
                    &used_by_capability,
                ),
                anchor,
                DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable,
                *capability,
            )?);
        }

        return Ok(());
    }

    if !has_body {
        return Ok(());
    }

    let anchor = context
        .symbols
        .declaration_syntax_anchor(owner.into_any())
        .ok_or_else(|| {
            query_contract(
                owner.into_any(),
                CallableContractsQuery::KIND,
                SemanticQueryViolation::Missing(SemanticDataKind::SourceAnchor),
            )
        })?;

    for capability in used.difference(&declared) {
        diagnostics.add(trusted_capability_diagnostic(
            context,
            used_by_capability
                .get(capability)
                .into_iter()
                .flat_map(|use_| use_.sources().iter().map(|source| source.syntax())),
            anchor,
            DiagnosticKind::CheckingUndeclaredTrustedCapability,
            *capability,
        )?);
    }

    if is_recovered {
        return Ok(());
    }

    for capability in declared.difference(&used) {
        diagnostics.add(trusted_capability_diagnostic(
            context,
            declared_by_capability
                .get(capability)
                .into_iter()
                .flat_map(|origins| origins.iter().copied()),
            anchor,
            DiagnosticKind::CheckingUnusedTrustedCapability,
            *capability,
        )?);
    }

    Ok(())
}

fn trusted_capability_diagnostic(
    context: &CompilationBindingContext<'_>,
    origins: impl IntoIterator<Item = bray_declarations::SyntaxAnchor>,
    fallback: bray_declarations::SyntaxAnchor,
    kind: DiagnosticKind,
    capability: TrustedCapabilitySymbolId,
) -> BindingQueryResult<bray_diagnostics::Diagnostic> {
    let name = context
        .symbols
        .member_name(capability.into())
        .ok_or_else(|| {
            binding_contract(
                SemanticQueryContext::Symbol(capability.into()),
                SemanticQueryViolation::Missing(SemanticDataKind::MemberName),
            )
        })?;

    let origins = origins.into_iter().collect::<BTreeSet<_>>();
    let anchor = origins.first().copied().unwrap_or(fallback);
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    let mut diagnostic = source_diagnostic(anchor, kind)
        .with_arg(DiagnosticArg::referenced_name(name.as_str()))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidTrustedCapabilityRequirement,
            span,
        ));

    for origin in origins.into_iter().skip(1) {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::RequirementOrigin,
            SourceSpan::new(origin.source_id(), origin.full_range()),
        ));
    }

    Ok(diagnostic)
}

fn trusted_capability_origins<'a>(
    capability: TrustedCapabilitySymbolId,
    declared: &'a BTreeMap<TrustedCapabilitySymbolId, BTreeSet<bray_declarations::SyntaxAnchor>>,
    used: &'a BTreeMap<TrustedCapabilitySymbolId, &bray_bound_tree::TrustedCapabilityUse>,
) -> impl Iterator<Item = bray_declarations::SyntaxAnchor> + 'a {
    declared
        .get(&capability)
        .into_iter()
        .flat_map(|origins| origins.iter().copied())
        .chain(
            used.get(&capability)
                .into_iter()
                .flat_map(|use_| use_.sources().iter().map(|source| source.syntax())),
        )
}

enum ContractClauseSyntax {
    Requires(RequiresClauseSyntax),
    Ensures(EnsuresClauseSyntax),
    With(WithClauseSyntax),
    Uses(UsesClauseSyntax),
}

fn bind_callable_predicates(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    expressions: impl IntoIterator<Item = bray_syntax::ExpressionSyntax>,
    kind: CallableContractClauseKind,
    predicates: &mut Vec<CallableContractClause>,
    execution_requirements: &mut Vec<CallableExecutionRequirement>,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<()> {
    let expressions = expressions.into_iter().collect::<Vec<_>>();

    if context.symbols.symbol_origin(owner) != Some(bray_symbols::SymbolOrigin::Source) {
        let result = bind_predicate_clause(
            context,
            owner,
            syntax,
            expressions,
            PredicateClauseBindingContext::CallableContract(kind),
        )?;

        let (summaries, clause_diagnostics) = result.into_parts();

        *diagnostics = diagnostics.merged(&clause_diagnostics);

        for summary in summaries {
            let ordinal = symbol_ordinal(predicates.len())?;

            predicates.push(CallableContractClause::new(ordinal, kind, summary));
        }

        return Ok(());
    }

    let checked = checked_callable_predicates(context, owner, syntax, expressions.len())?;

    *diagnostics = diagnostics.merged(&checked.diagnostics);

    if kind == CallableContractClauseKind::Requires {
        execution_requirements.extend(checked.execution_requirements);
    }

    for dependency in checked.dependency_contracts {
        let ordinal = symbol_ordinal(predicates.len())?;

        predicates.push(CallableContractClause::new(
            ordinal,
            kind,
            bray_symbols::PredicateSemanticSummary::new(dependency),
        ));
    }

    Ok(())
}

fn checked_callable_predicates(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    expression_count: usize,
) -> BindingQueryResult<CheckedSourcePredicateSequence> {
    let owner_key = context.symbols.symbol_key(owner).cloned().ok_or_else(|| {
        query_contract(
            owner,
            CallableContractsQuery::KIND,
            SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
        )
    })?;

    let source = context
        .compilation()
        .bound_source(bray_declarations::SyntaxAnchor::from_node(&syntax))
        .map_err(super::binding::binder_error)?;

    let key = bray_bound_tree::BoundUnitKey::contract_clause(owner_key, source).ok_or(
        BindingQueryError::Binding(BindingError::InvalidUnitKey { source, owner }),
    )?;

    let checked = checked_source_predicate_sequence(context, key)?;

    if checked.dependency_contracts.len() != expression_count {
        return Err(query_contract(
            owner,
            CallableContractsQuery::KIND,
            SemanticQueryViolation::CountMismatch {
                data: SemanticDataKind::DependencyContract,
                expected: expression_count,
                actual: checked.dependency_contracts.len(),
            },
        ));
    }

    Ok(checked)
}

fn resolve_trait_satisfaction_constraint(
    context: &CompilationBindingContext<'_>,
    constraint: &bray_symbols::GenericConstraintTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<CheckedConstraint> {
    let query_context = constraint_context(constraint);

    let Some((subject, application)) = constraint.trait_satisfaction_templates() else {
        return Err(binding_contract(
            query_context,
            SemanticQueryViolation::Unsupported(SemanticDataKind::GenericConstraint),
        ));
    };

    let (subject, application) = resolve_trait_satisfaction_templates(
        context,
        subject,
        application,
        query_context,
        diagnostics,
    )?;

    Ok(CheckedConstraint::trait_satisfaction(
        constraint.ordinal(),
        subject,
        application,
    ))
}

fn resolve_type_equality_constraint(
    context: &CompilationBindingContext<'_>,
    constraint: &bray_symbols::GenericConstraintTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<CheckedConstraint> {
    let query_context = constraint_context(constraint);

    let Some((left, right)) = constraint.type_equality_templates() else {
        return Err(binding_contract(
            query_context,
            SemanticQueryViolation::Unsupported(SemanticDataKind::GenericConstraint),
        ));
    };

    let left = resolve_type_template(context, left, query_context.clone(), diagnostics)?;
    let right = resolve_type_template(context, right, query_context, diagnostics)?;

    Ok(CheckedConstraint::type_equality(
        constraint.ordinal(),
        left,
        right,
    ))
}

fn resolve_type_template(
    context: &CompilationBindingContext<'_>,
    template: &bray_symbols::TypeExpressionTemplate,
    query_context: crate::compilation::SemanticQueryContext,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<bray_symbols::TypeId> {
    let mut terms = BTreeMap::new();

    for occurrence in template.constant_expressions() {
        let result = context
            .compilation()
            .embedded_constant_term_with_cancellation(occurrence, context.cancellation())
            .map_err(super::binding::binder_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());
        terms.insert(occurrence.key(), *result.value());
    }

    let constants = bray_checker::CheckedConstantTerms::try_from_terms(terms)
        .map_err(checked_constant_terms_binding_error)?;

    bray_checker::resolve_type_expression_template(context.semantic_values, template, &constants)
        .map_err(BindingQueryError::CheckerInfrastructure)?
        .ok_or_else(|| {
            missing_semantic_data(query_context, crate::compilation::SemanticDataKind::Type)
        })
}

fn resolve_trait_satisfaction_templates(
    context: &CompilationBindingContext<'_>,
    subject: &bray_symbols::TypeExpressionTemplate,
    application: &bray_symbols::TraitApplicationTemplate,
    query_context: crate::compilation::SemanticQueryContext,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<(bray_symbols::TypeId, bray_symbols::TraitApplicationId)> {
    let mut terms = BTreeMap::new();

    for occurrence in subject
        .constant_expressions()
        .into_iter()
        .chain(application.constant_expressions())
    {
        let result = context
            .compilation()
            .embedded_constant_term_with_cancellation(occurrence, context.cancellation())
            .map_err(super::binding::binder_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());
        terms.insert(occurrence.key(), *result.value());
    }

    let constants = bray_checker::CheckedConstantTerms::try_from_terms(terms)
        .map_err(checked_constant_terms_binding_error)?;

    let subject = bray_checker::resolve_type_expression_template(
        context.semantic_values,
        subject,
        &constants,
    )
    .map_err(BindingQueryError::CheckerInfrastructure)?
    .ok_or_else(|| {
        missing_semantic_data(
            query_context.clone(),
            crate::compilation::SemanticDataKind::Type,
        )
    })?;

    let application = bray_checker::resolve_trait_application_template(
        context.semantic_values,
        application,
        &constants,
    )
    .map_err(BindingQueryError::CheckerInfrastructure)?
    .ok_or_else(|| {
        missing_semantic_data(
            query_context,
            crate::compilation::SemanticDataKind::TraitApplication,
        )
    })?;

    Ok((subject, application))
}

fn checked_constant_terms_binding_error(
    cause: bray_checker::CheckedConstantTermsBuildError,
) -> BindingQueryError<crate::fact::FactQueryError> {
    crate::compilation::binder::semantic_query_binding_error(
        crate::compilation::SemanticQueryFailure::CheckedConstantTerms { unit: None, cause },
    )
}

fn constraint_context(
    constraint: &bray_symbols::GenericConstraintTemplate,
) -> crate::compilation::SemanticQueryContext {
    constraint
        .unit_syntax()
        .map(|unit| crate::compilation::SemanticQueryContext::Source(unit.source_id()))
        .unwrap_or(crate::compilation::SemanticQueryContext::Fact(
            crate::fact::CompilationFactKey::CheckDiagnostics,
        ))
}

fn missing_semantic_data(
    context: crate::compilation::SemanticQueryContext,
    data: crate::compilation::SemanticDataKind,
) -> BindingQueryError<crate::fact::FactQueryError> {
    crate::compilation::binder::semantic_contract_binding_error(
        context,
        crate::compilation::SemanticQueryViolation::Missing(data),
    )
}

fn bind_callable_static_constraints(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    expressions: impl IntoIterator<Item = bray_syntax::ExpressionSyntax>,
    predicates: &mut Vec<CallableContractClause>,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<()> {
    for expression in expressions {
        let ordinal = symbol_ordinal(predicates.len())?;

        let clause = if let Some(satisfaction) = expression.trait_satisfaction_constraint() {
            let subject =
                type_binder(context, owner)?.bind_type_expression(satisfaction.subject())?;

            let application =
                type_binder(context, owner)?.bind_trait_application(satisfaction.application())?;

            *diagnostics = diagnostics.merged(subject.diagnostics());
            *diagnostics = diagnostics.merged(application.diagnostics());

            let Some(application) = application.value() else {
                continue;
            };

            let (subject, application) = resolve_trait_satisfaction_templates(
                context,
                subject.value(),
                application,
                crate::compilation::SemanticQueryContext::Symbol(owner),
                diagnostics,
            )?;

            CallableContractClause::trait_satisfaction(ordinal, subject, application)
        } else {
            // The binder owns the expression while the clause view borrows the same syntax node.
            let result = bind_predicate_clause(
                context,
                owner,
                syntax_node_view(&expression),
                [expression.clone()],
                PredicateClauseBindingContext::GenericConstraint,
            )?;

            let (checked, expression_diagnostics) = result.into_parts();

            *diagnostics = diagnostics.merged(&expression_diagnostics);

            let [predicate] = checked.as_ref() else {
                return Err(query_contract(
                    owner,
                    CallableContractsQuery::KIND,
                    SemanticQueryViolation::CountMismatch {
                        data: SemanticDataKind::GenericConstraint,
                        expected: 1,
                        actual: checked.len(),
                    },
                ));
            };

            CallableContractClause::new(ordinal, CallableContractClauseKind::Static, *predicate)
        };

        predicates.push(clause);
    }

    Ok(())
}

fn direct_with_clauses(root: SyntaxNodeView<'_>) -> Vec<WithClauseSyntax> {
    let mut children = Vec::new();

    walk_direct_child_nodes(&root, |node| {
        if node.kind() != SyntaxKind::WithClause {
            return SyntaxWalkControl::Continue;
        }

        let Some(clause) = node.cast::<WithClauseSyntax>() else {
            return SyntaxWalkControl::Stop;
        };

        children.push(clause);

        SyntaxWalkControl::Continue
    });

    children
}

fn direct_contract_clauses(root: SyntaxNodeView<'_>) -> Vec<ContractClauseSyntax> {
    let mut clauses = Vec::new();

    walk_direct_child_nodes(&root, |node| {
        let clause = match node.kind() {
            SyntaxKind::RequiresClause => node
                .cast::<RequiresClauseSyntax>()
                .map(ContractClauseSyntax::Requires),
            SyntaxKind::EnsuresClause => node
                .cast::<EnsuresClauseSyntax>()
                .map(ContractClauseSyntax::Ensures),
            SyntaxKind::WithClause => node
                .cast::<WithClauseSyntax>()
                .map(ContractClauseSyntax::With),
            SyntaxKind::UsesClause => node
                .cast::<UsesClauseSyntax>()
                .map(ContractClauseSyntax::Uses),
            _ => None,
        };

        match clause {
            Some(clause) => {
                clauses.push(clause);

                SyntaxWalkControl::Continue
            }
            None => SyntaxWalkControl::Continue,
        }
    });

    clauses
}

fn publish_catalog_result<T>(
    value: T,
    diagnostics: DiagnosticBag,
) -> BindingQueryResult<DiagnosticResult<T>> {
    Ok(DiagnosticResult::new(value, diagnostics))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_binder::SymbolQueryProvider;
    use bray_bound_tree::SemanticSelection;
    use bray_compiler_known::ImplementationHook;
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
        DiagnosticRelatedLocationKind, SeverityKind,
    };
    use bray_symbols::{
        CallableContractTemplate, CallableContractsQuery, CallablePhaseBehavior, CallableSymbolId,
        DeclarationPredicateClauseKind, NativeLinkKind, NativeLinkRequirement, SymbolQueryRequest,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::publish_catalog_result;
    use crate::test_support::{
        compilation, compilation_with_options, diagnostic_kinds, only_call_selection,
        source_callable_body_key, source_function,
    };
    use crate::{Compilation, CompilationOptions, WorkerBudget};

    #[test]
    fn binding_diagnostics_remain_owned_by_the_published_result() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::BindingUnresolvedName,
            SeverityKind::Error,
        );

        let result = publish_catalog_result((), DiagnosticBag::single(diagnostic.clone()));

        let result = result.unwrap_or_else(|error| panic!("query must publish: {error:?}"));

        assert_eq!(result.diagnostics(), &DiagnosticBag::single(diagnostic));
    }

    #[test]
    fn trusted_callable_contracts_require_every_used_capability() {
        let compilation = trusted_capability_compilation("");

        let contracts = callable_contracts(&compilation, "outer");

        assert_eq!(
            diagnostic_kinds(contracts.diagnostics()),
            [DiagnosticKind::CheckingUndeclaredTrustedCapability]
        );

        let diagnostic = bray_testing::single_diagnostic(contracts.diagnostics());

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::referenced_name("foreign_call")]
        );

        assert!(diagnostic.primary_span().is_some());

        assert_goal_state_diagnostic_kind(
            contracts.diagnostics(),
            DiagnosticKind::CheckingUndeclaredTrustedCapability,
        );
    }

    #[test]
    fn trusted_callable_contracts_reject_unused_capabilities() {
        let compilation = compilation(concat!(
            "trusted module app;\n",
            "trusted func unused()\n",
            "    uses(foreign_call)\n",
            "{\n",
            "}\n",
        ));

        let contracts = callable_contracts(&compilation, "unused");

        assert_eq!(
            diagnostic_kinds(contracts.diagnostics()),
            [DiagnosticKind::CheckingUnusedTrustedCapability]
        );

        assert_goal_state_diagnostic_kind(
            contracts.diagnostics(),
            DiagnosticKind::CheckingUnusedTrustedCapability,
        );

        let diagnostic = bray_testing::single_diagnostic(contracts.diagnostics());

        assert_eq!(
            diagnostic
                .primary_span()
                .unwrap_or_else(|| panic!("unused capability must retain its declaration"))
                .range()
                .len()
                .bytes(),
            12
        );
    }

    #[test]
    fn trusted_capabilities_require_trusted_callable_declarations() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func ordinary()\n",
            "    uses(foreign_call)\n",
            "{\n",
            "}\n",
        ));

        let contracts = callable_contracts(&compilation, "ordinary");

        assert_eq!(
            diagnostic_kinds(contracts.diagnostics()),
            [DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable]
        );

        assert_goal_state_diagnostic_kind(
            contracts.diagnostics(),
            DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable,
        );

        let diagnostic = bray_testing::single_diagnostic(contracts.diagnostics());

        assert_eq!(
            diagnostic
                .primary_span()
                .unwrap_or_else(|| panic!("untrusted capability must retain its declaration"))
                .range()
                .len()
                .bytes(),
            12
        );
    }

    #[test]
    fn undeclared_capability_retains_every_causative_call_origin() {
        let compilation = trusted_capability_compilation_with_body(
            "",
            "    native_call();\n    native_call();\n    native_call();\n",
        );

        let contracts = callable_contracts(&compilation, "outer");

        assert_goal_state_diagnostic_kind(
            contracts.diagnostics(),
            DiagnosticKind::CheckingUndeclaredTrustedCapability,
        );

        let diagnostic = bray_testing::single_diagnostic(contracts.diagnostics());

        assert_eq!(diagnostic.related_locations().len(), 2);

        assert!(
            diagnostic.related_locations().iter().all(|related| {
                related.kind() == DiagnosticRelatedLocationKind::RequirementOrigin
            })
        );
    }

    #[test]
    fn exact_trusted_capability_contracts_validate_without_diagnostics() {
        let compilation = trusted_capability_compilation("    uses(foreign_call)\n");

        let contracts = callable_contracts(&compilation, "outer");

        assert!(contracts.diagnostics().is_empty());
    }

    #[test]
    fn package_diagnostics_request_callable_contract_validation() {
        let compilation = trusted_capability_compilation("");

        assert!(compilation.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingUndeclaredTrustedCapability
        }));
    }

    #[test]
    fn wrappers_do_not_inherit_callee_implementation_capabilities() {
        let compilation = compilation(concat!(
            "trusted module app;\n",
            "trusted func outer()\n",
            "{\n",
            "    inner();\n",
            "}\n",
            "trusted func inner()\n",
            "    uses(foreign_call)\n",
            "{\n",
            "}\n",
        ));

        let contracts = callable_contracts(&compilation, "outer");

        assert!(contracts.diagnostics().is_empty());
    }

    #[test]
    fn callable_contracts_retain_checked_predicate_dependencies() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func checked(pos value: bool)\n",
            "    requires(value)\n",
            "{\n",
            "}\n",
        ));

        let contracts = callable_contracts(&compilation, "checked");

        let [precondition] = contracts.value().invocation_preconditions() else {
            panic!("requires clause must publish one precondition");
        };

        let Some(predicate) = precondition.predicate() else {
            panic!("requires clause must retain predicate meaning");
        };

        let dependency = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"))
            .dependency_contract_template_data(predicate.dependency_contract());

        assert!(!dependency.requirements().is_empty());
    }

    #[test]
    fn execution_predicates_apply_to_the_phase_that_executes_the_body() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func synchronous()\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "}\n",
            "async func asynchronous()\n",
            "    requires(blocking_execution())\n",
            "{\n",
            "}\n",
        ));

        let synchronous = callable_contracts(&compilation, "synchronous");
        let asynchronous = callable_contracts(&compilation, "asynchronous");

        assert_execution_requirement(
            &compilation,
            synchronous.value().invocation_behavior(),
            ImplementationHook::BlockingExecution,
        );

        assert!(synchronous.value().deferred_execution_behavior().is_none());

        assert!(
            asynchronous
                .value()
                .invocation_behavior()
                .execution_requirements()
                .is_empty()
        );

        let deferred = asynchronous
            .value()
            .deferred_execution_behavior()
            .unwrap_or_else(|| panic!("async callable must publish deferred behavior"));

        assert_execution_requirement(
            &compilation,
            deferred,
            ImplementationHook::BlockingExecution,
        );
    }

    fn assert_execution_requirement(
        compilation: &Compilation,
        behavior: &CallablePhaseBehavior,
        expected: ImplementationHook,
    ) {
        let [requirement] = behavior.execution_requirements() else {
            panic!("phase must publish one execution requirement");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .symbol_implementation(requirement.declaration()),
            Some(expected)
        );
    }

    #[test]
    fn selected_calls_retain_preconditions_and_normal_completion_postconditions() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func caller()\n",
            "{\n",
            "    checked(true);\n",
            "}\n",
            "func checked(pos value: bool) -> bool\n",
            "    requires(value)\n",
            "    ensures(result)\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("call selection must publish: {error:?}"));

        let selection = only_call_selection(selections.value());

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("caller selection must be a call");
        };

        let Some(CallableContractTemplate::Source(contract)) = call.contract() else {
            panic!("source call must retain its contract template");
        };

        let [precondition, postcondition] = contract.expressions() else {
            panic!("selected call must retain both contract predicates");
        };

        assert_eq!(
            precondition.kind(),
            DeclarationPredicateClauseKind::Requires
        );

        assert_eq!(
            postcondition.kind(),
            DeclarationPredicateClauseKind::Ensures
        );
    }

    #[test]
    fn callable_contract_predicates_must_be_boolean() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func checked()\n",
            "    requires(1)\n",
            "{\n",
            "}\n",
        ));

        let contracts = callable_contracts(&compilation, "checked");

        assert_eq!(
            diagnostic_kinds(contracts.diagnostics()),
            [DiagnosticKind::CheckingIncompatibleExpressionType],
            "{:#?}",
            contracts.diagnostics()
        );
    }

    fn trusted_capability_compilation(outer_contract: &str) -> Compilation {
        trusted_capability_compilation_with_body(outer_contract, "    native_call();\n")
    }

    fn trusted_capability_compilation_with_body(outer_contract: &str, body: &str) -> Compilation {
        let source = format!(
            concat!(
                "trusted module app;\n",
                "@link(name = \"native\")\n",
                "@symbol(name = \"native_call\")\n",
                "@abi(c)\n",
                "extern trusted func native_call()\n",
                "    uses(foreign_call);\n",
                "trusted func outer()\n",
                "{outer_contract}",
                "{{\n",
                "{body}",
                "}}\n",
            ),
            outer_contract = outer_contract,
            body = body,
        );

        let Some(link) = NonEmptySharedStr::try_new("native") else {
            panic!("test link name must be valid");
        };

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_native_link_inputs([NativeLinkRequirement::new(link, NativeLinkKind::Dynamic)]);

        compilation_with_options(&source, options)
    }

    fn callable_contracts(
        compilation: &Compilation,
        name: &str,
    ) -> Arc<
        bray_diagnostics::DiagnosticResult<
            <CallableContractsQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        let function = source_function(compilation, name);

        let binding_context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("binder queries must be available: {error:?}"));

        binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                CallableSymbolId::from(function),
            ))
            .unwrap_or_else(|error| panic!("callable contracts must publish: {error:?}"))
    }
}
