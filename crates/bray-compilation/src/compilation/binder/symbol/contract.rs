// rust-style: allow(module-too-large, reason = "callable contract binding and validation form one publication transaction over shared syntax and symbol state")

use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{
    BinderFactContext, BinderFactError, BinderFactResult, PredicateClauseBindingContext,
    SymbolFactProvider, bind_predicate_clause, bind_trusted_capability_clause,
};
use bray_diagnostics::{
    DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticResult,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableContractClause, CallableContractClauseKind, CallableContractSet,
    CallableContractsFact, CallableExecution, CallablePhaseBehavior, CallableSignatureFact,
    CallableSymbolId, CallableTrust, CheckedConstraint, CurrentRunCancellation,
    DependencyContractTemplateId, GenericConstraintSet, GenericConstraintsFact,
    GenericDeclarationTemplateFact, GenericOwnerId, SymbolFactRequest, SymbolFactResult,
    TrustedCapabilityRequirement, TrustedCapabilitySymbolId, TypeData,
};
use bray_syntax::{
    EnsuresClauseSyntax, RequiresClauseSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    UsesClauseSyntax, WithClauseSyntax, syntax_node_view, walk_direct_child_nodes,
};

use super::binding::CompilationSymbolFactBinding;
use super::cache::CompilationSymbolFacts;
use super::declaration_body::checked_source_predicate_sequence;
use super::environment::type_binder;
use super::surface::{symbol_ordinal, with_declaration_root};
use crate::compilation::binder::CompilationBinderFacts;
use crate::compilation::diagnostics::source_diagnostic;
use crate::fact::SymbolFactCache;

impl CompilationSymbolFactBinding<GenericConstraintsFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<GenericConstraintsFact> {
        &self.generic_constraints
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<GenericConstraintsFact>,
    ) -> BinderFactResult<SymbolFactResult<GenericConstraintsFact>> {
        bind_generic_constraints(context, request.symbol())
    }
}

impl CompilationSymbolFactBinding<CallableContractsFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableContractsFact> {
        &self.callable_contracts
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableContractsFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableContractsFact>> {
        bind_callable_contracts(context, request.owner())
    }
}

fn bind_generic_constraints(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<SymbolFactResult<GenericConstraintsFact>> {
    let generic_owner =
        GenericOwnerId::try_new(owner).ok_or(BinderFactError::DependencyUnavailable)?;

    let template = context.symbol_fact(
        SymbolFactRequest::<GenericDeclarationTemplateFact>::new(generic_owner),
    )?;

    // The published constraint fact owns the Arc-backed template diagnostics independently.
    let template_diagnostics = template.diagnostics().clone();

    if context.imported_fact_address(owner)?.is_some() {
        let constraints = template
            .value()
            .constraints()
            .iter()
            .map(|constraint| {
                constraint
                    .resolved()
                    .ok_or(BinderFactError::DependencyUnavailable)
            })
            .collect::<BinderFactResult<Vec<_>>>()?;

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
                .ok_or(BinderFactError::DependencyUnavailable)?;

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
                        return Err(BinderFactError::DependencyUnavailable);
                    };

                    CheckedConstraint::new(ordinal, *predicate)
                }
                bray_symbols::GenericConstraintTemplate::Resolved(_) => {
                    return Err(BinderFactError::DependencyUnavailable);
                }
            };

            constraints.push(constraint);
        }
    }

    publish_catalog_result(GenericConstraintSet::new(constraints), diagnostics)
}

fn bind_callable_contracts(
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
) -> BinderFactResult<SymbolFactResult<CallableContractsFact>> {
    if let Some(address) = context.imported_fact_address(owner.into_any())? {
        return super::imported::imported_callable_contracts(context, address);
    }

    let clauses = with_declaration_root(context, owner.into_any(), |root| {
        Ok(direct_contract_clauses(root))
    })?;

    let mut predicates = Vec::new();
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
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Ensures(clause) => bind_callable_predicates(
                context,
                owner.into_any(),
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Ensures,
                &mut predicates,
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
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let signature = context.symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(owner))?;

    let (execution, trust) = match signature.value().callable_type() {
        bray_symbols::TypeExpressionTemplate::Callable(callable) => {
            (callable.execution(), callable.trust())
        }
        bray_symbols::TypeExpressionTemplate::Resolved(ty) => {
            let data = context
                .semantic_values
                .type_data(*ty)
                .map_err(|_| BinderFactError::DependencyUnavailable)?;

            match &*data {
                TypeData::Callable(callable) => (callable.execution(), callable.trust()),
                _ => return Err(BinderFactError::DependencyUnavailable),
            }
        }
        _ => return Err(BinderFactError::DependencyUnavailable),
    };

    let definition = bray_symbols::CallableDefinitionId::try_new(owner.into_any())
        .ok_or(BinderFactError::DependencyUnavailable)?;

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
        capabilities
            .value()
            .iter()
            .map(|capability| capability.requirement()),
        dependency,
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

pub(in crate::compilation) fn bind_declared_trusted_capabilities(
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
) -> BinderFactResult<DiagnosticResult<Vec<DeclaredTrustedCapability>>> {
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
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
    clauses: impl IntoIterator<Item = UsesClauseSyntax>,
) -> BinderFactResult<DiagnosticResult<Vec<DeclaredTrustedCapability>>> {
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
    trusted_capabilities: impl IntoIterator<Item = TrustedCapabilityRequirement>,
    dependencies: DependencyContractTemplateId,
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

    let execution_requirements = body
        .into_iter()
        .flat_map(bray_bound_tree::CheckedBodyBehavior::execution_requirements)
        .copied();

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
    );

    match execution {
        CallableExecution::Synchronous => (body_behavior, None),
        CallableExecution::Asynchronous => (
            CallablePhaseBehavior::empty(dependencies),
            Some(body_behavior),
        ),
    }
}

fn validate_trusted_capabilities(
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
    trust: CallableTrust,
    declared: &[DeclaredTrustedCapability],
    used: Option<&[bray_bound_tree::TrustedCapabilityUse]>,
    is_recovered: bool,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<()> {
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
            .ok_or(BinderFactError::DependencyUnavailable)?;

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
        .ok_or(BinderFactError::DependencyUnavailable)?;

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
    context: &CompilationBinderFacts<'_>,
    origins: impl IntoIterator<Item = bray_declarations::SyntaxAnchor>,
    fallback: bray_declarations::SyntaxAnchor,
    kind: DiagnosticKind,
    capability: TrustedCapabilitySymbolId,
) -> BinderFactResult<bray_diagnostics::Diagnostic> {
    let name = context
        .symbols
        .member_name(capability.into())
        .ok_or(BinderFactError::DependencyUnavailable)?;

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
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    expressions: impl IntoIterator<Item = bray_syntax::ExpressionSyntax>,
    kind: CallableContractClauseKind,
    predicates: &mut Vec<CallableContractClause>,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<()> {
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

    let expression_count = expressions.len();

    let owner_key = context
        .symbols
        .symbol_key(owner)
        .cloned()
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let source = context
        .compilation()
        .bound_source(bray_declarations::SyntaxAnchor::from_node(&syntax))
        .map_err(super::binding::binder_error)?;

    let key = bray_bound_tree::BoundUnitKey::contract_clause(owner_key, source)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let checked = checked_source_predicate_sequence(context, key)?;

    if checked.dependency_contracts.len() != expression_count {
        return Err(BinderFactError::DependencyUnavailable);
    }

    *diagnostics = diagnostics.merged(&checked.diagnostics);

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

fn resolve_trait_satisfaction_constraint(
    context: &CompilationBinderFacts<'_>,
    constraint: &bray_symbols::GenericConstraintTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<CheckedConstraint> {
    let Some((subject, application)) = constraint.trait_satisfaction_templates() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let (subject, application) =
        resolve_trait_satisfaction_templates(context, subject, application, diagnostics)?;

    Ok(CheckedConstraint::trait_satisfaction(
        constraint.ordinal(),
        subject,
        application,
    ))
}

fn resolve_type_equality_constraint(
    context: &CompilationBinderFacts<'_>,
    constraint: &bray_symbols::GenericConstraintTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<CheckedConstraint> {
    let Some((left, right)) = constraint.type_equality_templates() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let left = resolve_type_template(context, left, diagnostics)?;
    let right = resolve_type_template(context, right, diagnostics)?;

    Ok(CheckedConstraint::type_equality(
        constraint.ordinal(),
        left,
        right,
    ))
}

fn resolve_type_template(
    context: &CompilationBinderFacts<'_>,
    template: &bray_symbols::TypeExpressionTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<bray_symbols::TypeId> {
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
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    bray_checker::resolve_type_expression_template(context.semantic_values, template, &constants)
        .map_err(|_| BinderFactError::DependencyUnavailable)?
        .ok_or(BinderFactError::DependencyUnavailable)
}

fn resolve_trait_satisfaction_templates(
    context: &CompilationBinderFacts<'_>,
    subject: &bray_symbols::TypeExpressionTemplate,
    application: &bray_symbols::TraitApplicationTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<(bray_symbols::TypeId, bray_symbols::TraitApplicationId)> {
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
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let subject = bray_checker::resolve_type_expression_template(
        context.semantic_values,
        subject,
        &constants,
    )
    .map_err(|_| BinderFactError::DependencyUnavailable)?
    .ok_or(BinderFactError::DependencyUnavailable)?;

    let application = bray_checker::resolve_trait_application_template(
        context.semantic_values,
        application,
        &constants,
    )
    .map_err(|_| BinderFactError::DependencyUnavailable)?
    .ok_or(BinderFactError::DependencyUnavailable)?;

    Ok((subject, application))
}

fn bind_callable_static_constraints(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    expressions: impl IntoIterator<Item = bray_syntax::ExpressionSyntax>,
    predicates: &mut Vec<CallableContractClause>,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<()> {
    for expression in expressions {
        let ordinal = symbol_ordinal(predicates.len())?;

        let clause = if let Some(satisfaction) = expression.trait_satisfaction_constraint() {
            let subject =
                type_binder(context, owner)?.bind_type_expression(satisfaction.subject())?;

            let application =
                type_binder(context, owner)?.bind_trait_application(satisfaction.application())?;

            *diagnostics = diagnostics.merged(subject.diagnostics());
            *diagnostics = diagnostics.merged(application.diagnostics());

            let (subject, application) = resolve_trait_satisfaction_templates(
                context,
                subject.value(),
                application.value(),
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
                return Err(BinderFactError::DependencyUnavailable);
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
) -> BinderFactResult<DiagnosticResult<T>> {
    Ok(DiagnosticResult::new(value, diagnostics))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_binder::SymbolFactProvider;
    use bray_bound_tree::SemanticSelection;
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
        DiagnosticRelatedLocationKind, SeverityKind,
    };
    use bray_symbols::{
        CallableContractTemplate, CallableContractsFact, CallableSymbolId,
        DeclarationPredicateClauseKind, NativeLinkKind, NativeLinkRequirement, SymbolFactRequest,
        SymbolFactResult,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::publish_catalog_result;
    use crate::test_support::{
        compilation, compilation_with_options, diagnostic_kinds, source_callable_body_key,
        source_function,
    };
    use crate::{Compilation, CompilationOptions, WorkerBudget};

    #[test]
    fn binding_diagnostics_remain_owned_by_the_published_fact() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::BindingUnresolvedName,
            SeverityKind::Error,
        );

        let result = publish_catalog_result((), DiagnosticBag::single(diagnostic.clone()));

        let result = result.unwrap_or_else(|error| panic!("fact must publish: {error:?}"));

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

        let [diagnostic] = contracts.diagnostics().diagnostics() else {
            panic!("missing capability must publish one diagnostic");
        };

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

        let [diagnostic] = contracts.diagnostics().diagnostics() else {
            panic!("unused capability must publish one diagnostic");
        };

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

        let [diagnostic] = contracts.diagnostics().diagnostics() else {
            panic!("untrusted capability must publish one diagnostic");
        };

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

        let [diagnostic] = contracts.diagnostics().diagnostics() else {
            panic!("undeclared capability must publish one diagnostic");
        };

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
            .dependency_contract_template_data(predicate.dependency_contract())
            .unwrap_or_else(|error| panic!("predicate dependency must be available: {error:?}"));

        assert!(!dependency.requirements().is_empty());
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

        let [selection] = selections.value().entries() else {
            panic!("caller must publish one selected call");
        };

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
    ) -> Arc<SymbolFactResult<CallableContractsFact>> {
        let function = source_function(compilation, name);

        let facts = compilation
            .binder_facts(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("binder facts must be available: {error:?}"));

        facts
            .symbol_fact(SymbolFactRequest::<CallableContractsFact>::new(
                CallableSymbolId::from(function),
            ))
            .unwrap_or_else(|error| panic!("callable contracts must publish: {error:?}"))
    }
}
