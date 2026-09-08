use bray_diagnostics::DiagnosticBag;
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableContractClause, CallableContractClauseKind, CallableExecutionGuarantee,
    SymbolOrdinal,
};
use bray_syntax::{ExecutesClauseSyntax, SourceSyntaxNode, WhenClauseSyntax, syntax_node_view};

use super::contract::{ContractClauseSyntax, bind_callable_predicates, direct_contract_clauses};
use super::surface::symbol_ordinal;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

pub(super) fn bind_execution_guarantees(
    clause: &ExecutesClauseSyntax,
    guard: Option<SymbolOrdinal>,
    guarantees: &mut Vec<CallableExecutionGuarantee>,
    diagnostics: &mut DiagnosticBag,
) {
    for property in clause.properties() {
        let token = property.identifier_token();

        let Some(name) = token.text(property.source().text()) else {
            continue;
        };

        let source = SourceSpan::new(property.source().source_id(), token.range());

        match bray_checker::check_execution_property(name, source) {
            Ok(property) => guarantees.push(CallableExecutionGuarantee::new(property, guard)),
            Err(diagnostic) => {
                *diagnostics = diagnostics.merged(&DiagnosticBag::single(diagnostic))
            }
        }
    }
}

pub(super) fn bind_guarded_guarantees(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    clause: &WhenClauseSyntax,
    parent: Option<SymbolOrdinal>,
    predicates: &mut Vec<CallableContractClause>,
    guarantees: &mut Vec<CallableExecutionGuarantee>,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<()> {
    let start = predicates.len();
    let ordinal = symbol_ordinal(start)?;

    bind_callable_predicates(
        context,
        owner,
        syntax_node_view(clause),
        [clause.condition()],
        CallableContractClauseKind::Guard,
        predicates,
        diagnostics,
    )?;

    for predicate in &mut predicates[start..] {
        *predicate = predicate.with_guard(parent);
    }

    for nested in direct_contract_clauses(syntax_node_view(clause)) {
        match nested {
            ContractClauseSyntax::Ensures(clause) => {
                let start = predicates.len();

                bind_callable_predicates(
                    context,
                    owner,
                    syntax_node_view(&clause),
                    clause.expressions(),
                    CallableContractClauseKind::Ensures,
                    predicates,
                    diagnostics,
                )?;

                for predicate in &mut predicates[start..] {
                    *predicate = predicate.with_guard(Some(ordinal));
                }
            }
            ContractClauseSyntax::Executes(clause) => {
                bind_execution_guarantees(&clause, Some(ordinal), guarantees, diagnostics)
            }
            ContractClauseSyntax::When(clause) => bind_guarded_guarantees(
                context,
                owner,
                &clause,
                Some(ordinal),
                predicates,
                guarantees,
                diagnostics,
            )?,
            // The parser retains forbidden group contents as diagnosed skipped syntax.
            ContractClauseSyntax::Requires(_)
            | ContractClauseSyntax::With(_)
            | ContractClauseSyntax::Uses(_) => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_binder::SymbolQueryProvider;
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_symbols::CallableConditions;
    use bray_symbols::{
        CallableContractsQuery, CallableSymbolId, ExecutionProperty, SymbolOrdinal,
        SymbolQueryRequest,
    };

    use crate::test_support::{compilation, source_function};

    #[test]
    fn guarded_contract_binding_retains_entry_ancestry_and_conditional_postconditions() {
        let compilation = compilation(
            "module app; func check(pos ready: bool) -> bool \
            when(ready) { executes(pure, total) ensures(result) \
                when(false) { executes(total) ensures(!result) } } \
            ensures(true) { return ready; }",
        );

        let owner = CallableSymbolId::from(source_function(&compilation, "check"));

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let result = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(owner))
            .unwrap();

        let contract = result.value();

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        assert_eq!(
            contract
                .conditions()
                .entry_guards()
                .iter()
                .map(|clause| (clause.ordinal(), clause.guard()))
                .collect::<Vec<_>>(),
            [
                (SymbolOrdinal::new(0), None),
                (SymbolOrdinal::new(2), Some(SymbolOrdinal::new(0)))
            ]
        );

        assert_eq!(
            contract
                .conditions()
                .guarded_postconditions()
                .iter()
                .map(|clause| clause.guard())
                .collect::<Vec<_>>(),
            [Some(SymbolOrdinal::new(0)), Some(SymbolOrdinal::new(2))]
        );

        assert_eq!(
            contract
                .conditions()
                .normal_completion_postconditions()
                .len(),
            1
        );

        assert_eq!(
            contract
                .conditions()
                .execution_guarantees()
                .iter()
                .map(|guarantee| (guarantee.property(), guarantee.guard()))
                .collect::<Vec<_>>(),
            [
                (ExecutionProperty::Pure, Some(SymbolOrdinal::new(0))),
                (ExecutionProperty::Total, Some(SymbolOrdinal::new(0))),
                (ExecutionProperty::Total, Some(SymbolOrdinal::new(2)))
            ]
        );
    }

    #[test]
    fn guarded_contract_binding_rejects_unknown_execution_properties_with_the_identifier() {
        let compilation =
            compilation("module app; func check() when(true) { executes(constant) } {}");

        let owner = CallableSymbolId::from(source_function(&compilation, "check"));

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let result = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(owner))
            .unwrap();

        let diagnostic = bray_testing::single_diagnostic(result.diagnostics());

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingUnknownExecutionProperty
        );

        assert_eq!(
            diagnostic.args(),
            [DiagnosticArg::referenced_name("constant")]
        );

        assert!(diagnostic.primary_span().is_some());

        assert!(
            result
                .value()
                .conditions()
                .execution_guarantees()
                .is_empty()
        );
    }

    #[test]
    fn guarded_contract_binding_does_not_bind_result_at_execution_entry() {
        let compilation = compilation(
            "module app; func check() -> bool when(result) { executes(pure) } { return true; }",
        );

        assert!(
            compilation
                .semantic_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BindingUnresolvedName)
        );
    }
}
