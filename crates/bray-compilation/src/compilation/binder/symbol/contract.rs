use bray_binder::{
    BinderFactError, BinderFactResult, PredicateClauseBindingContext, SymbolFactProvider,
    bind_predicate_clause, bind_trusted_capability_clause,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractClause, CallableContractClauseKind, CallableContractSet,
    CallableContractsFact, CallableExecution, CallablePhaseBehavior, CallableSignatureFact,
    CallableSymbolId, CheckedConstraint, CurrentRunCancellation, DependencyContractTemplateId,
    GenericConstraintSet, GenericConstraintsFact, SymbolFactRequest, SymbolFactResult, SymbolGraph,
    TrustedCapabilityRequirement, TypeData,
};
use bray_syntax::{
    EnsuresClauseSyntax, RequiresClauseSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    UsesClauseSyntax, WithClauseSyntax, syntax_node_view, walk_direct_child_nodes,
};

use super::binding::CompilationSymbolFactBinding;
use super::cache::CompilationSymbolFacts;
use super::surface::{compiler_known_surface, symbol_ordinal, with_declaration_root};
use crate::compilation::binder::CompilationBinderFacts;
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
    let clauses = with_declaration_root(context, owner, |root| Ok(direct_with_clauses(root)))?;

    let mut constraints = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for clause in clauses {
        let result = bind_predicate_clause(
            context,
            owner,
            syntax_node_view(&clause),
            clause.expressions(),
            PredicateClauseBindingContext::GenericConstraint,
        )?;

        let (predicates, clause_diagnostics) = result.into_parts();

        diagnostics = diagnostics.merged(&clause_diagnostics);

        for predicate in predicates {
            let ordinal = symbol_ordinal(constraints.len())?;

            constraints.push(CheckedConstraint::new(ordinal, predicate));
        }
    }

    publish_catalog_result(GenericConstraintSet::new(constraints), diagnostics)
}

fn bind_callable_contracts(
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
) -> BinderFactResult<SymbolFactResult<CallableContractsFact>> {
    let fragment = compiler_known_fragment(context.symbols, owner.into_any())?;
    let clauses = direct_contract_clauses(fragment.root());

    let mut predicates = Vec::new();
    let mut capabilities = Vec::new();
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
            ContractClauseSyntax::With(clause) => bind_callable_predicates(
                context,
                owner.into_any(),
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Static,
                &mut predicates,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Uses(clause) => {
                let result = bind_trusted_capability_clause(context, owner.into_any(), &clause)?;
                let (symbols, clause_diagnostics) = result.into_parts();
                diagnostics = diagnostics.merged(&clause_diagnostics);

                for symbol in symbols {
                    let ordinal = symbol_ordinal(capabilities.len())?;

                    capabilities.push(TrustedCapabilityRequirement::new(ordinal, symbol));
                }
            }
        }
    }

    let dependency = context
        .semantic_values
        .empty_dependency_contract_template()
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let signature = context.symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(owner))?;

    let execution = match signature.value().callable_type() {
        bray_symbols::TypeExpressionTemplate::Callable(callable) => callable.execution(),
        bray_symbols::TypeExpressionTemplate::Resolved(ty) => {
            let data = context
                .semantic_values
                .type_data(*ty)
                .map_err(|_| BinderFactError::DependencyUnavailable)?;

            match &*data {
                TypeData::Callable(callable) => callable.execution(),
                _ => return Err(BinderFactError::DependencyUnavailable),
            }
        }
        _ => return Err(BinderFactError::DependencyUnavailable),
    };

    let (invocation_behavior, deferred_execution_behavior) =
        callable_phase_behaviors(execution, capabilities, dependency);

    publish_catalog_result(
        CallableContractSet::new(predicates, invocation_behavior, deferred_execution_behavior),
        diagnostics,
    )
}

fn callable_phase_behaviors(
    execution: CallableExecution,
    trusted_capabilities: Vec<TrustedCapabilityRequirement>,
    dependencies: DependencyContractTemplateId,
) -> (CallablePhaseBehavior, Option<CallablePhaseBehavior>) {
    let body_behavior = CallablePhaseBehavior::new(
        [],
        [],
        trusted_capabilities,
        [],
        [],
        dependencies,
        CurrentRunCancellation::NotEntered,
    );

    match execution {
        CallableExecution::Synchronous => (body_behavior, None),
        CallableExecution::Asynchronous => (
            CallablePhaseBehavior::empty(dependencies),
            Some(body_behavior),
        ),
    }
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
    if !diagnostics.is_empty() {
        return Err(BinderFactError::DependencyUnavailable);
    }

    Ok(DiagnosticResult::without_diagnostics(value))
}

fn compiler_known_fragment(
    symbols: &SymbolGraph,
    symbol: AnySymbolId,
) -> BinderFactResult<bray_syntax::PreparsedSyntaxFragment> {
    compiler_known_surface(symbols, symbol)?
        .syntax_fragment()
        .map_err(|_| BinderFactError::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use bray_binder::BinderFactError;
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::publish_catalog_result;

    #[test]
    fn catalog_binding_diagnostics_do_not_enter_user_fact_results() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::BindingUnresolvedName,
            SeverityKind::Error,
        );

        let result = publish_catalog_result((), DiagnosticBag::single(diagnostic));

        assert_eq!(result, Err(BinderFactError::DependencyUnavailable));
    }
}
