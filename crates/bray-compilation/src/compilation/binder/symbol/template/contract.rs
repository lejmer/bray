use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractExpressionTemplate, CallableContractTemplate, CallableSymbolId,
    DeclarationCapabilityTemplate, DeclarationExpressionTemplate, DeclarationPredicateClauseKind,
    ExecutionGuaranteeTemplate, SymbolOrdinal,
};
use bray_syntax::{
    ExecutesClauseSyntax, ExpressionSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    UsesClauseSyntax, WhenClauseSyntax, syntax_node_view, walk_direct_child_nodes,
};

use super::super::imported::imported_callable_contract;
use super::super::surface::{symbol_ordinal, with_declaration_root};
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

pub(super) fn bind_callable_contract_template(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<CallableContractTemplate>> {
    let symbol = owner.into_any();

    if let Some(address) = context.imported_semantic_address(symbol)? {
        return imported_callable_contract(context, address);
    }

    with_declaration_root(context, symbol, |root| {
        let mut expressions = Vec::new();
        let mut capabilities = Vec::new();
        let mut guarantees = Vec::new();

        collect_contract_children(
            symbol,
            root,
            None,
            &mut expressions,
            &mut capabilities,
            &mut guarantees,
        )?;

        Ok(DiagnosticResult::without_diagnostics(
            CallableContractTemplate::source(owner, expressions, capabilities, guarantees),
        ))
    })
}

fn collect_contract_children(
    owner: AnySymbolId,
    root: SyntaxNodeView<'_>,
    guard: Option<SymbolOrdinal>,
    expressions: &mut Vec<CallableContractExpressionTemplate>,
    capabilities: &mut Vec<DeclarationCapabilityTemplate>,
    guarantees: &mut Vec<ExecutionGuaranteeTemplate>,
) -> BindingQueryResult<()> {
    let mut result = Ok(());

    walk_direct_child_nodes(&root, |node| {
        result = collect_contract_child(owner, node, guard, expressions, capabilities, guarantees);

        if result.is_ok() {
            SyntaxWalkControl::Continue
        } else {
            SyntaxWalkControl::Stop
        }
    });

    result
}

fn collect_contract_child(
    owner: AnySymbolId,
    node: SyntaxNodeView<'_>,
    guard: Option<SymbolOrdinal>,
    expressions: &mut Vec<CallableContractExpressionTemplate>,
    capabilities: &mut Vec<DeclarationCapabilityTemplate>,
    guarantees: &mut Vec<ExecutionGuaranteeTemplate>,
) -> BindingQueryResult<()> {
    if let Some(group) = node.cast::<WhenClauseSyntax>() {
        let ordinal = symbol_ordinal(expressions.len())?;

        expressions.push(
            CallableContractExpressionTemplate::new(
                ordinal,
                DeclarationPredicateClauseKind::Guard,
                SyntaxAnchor::from_node(&group),
                DeclarationExpressionTemplate::new(
                    owner,
                    SyntaxAnchor::from_node(&group.condition()),
                ),
            )
            .with_guard(guard),
        );

        return collect_contract_children(
            owner,
            syntax_node_view(&group),
            Some(ordinal),
            expressions,
            capabilities,
            guarantees,
        );
    }

    if let Some(clause) = node.cast::<ExecutesClauseSyntax>() {
        guarantees.extend(clause.properties().map(|property| {
            ExecutionGuaranteeTemplate::new(SyntaxAnchor::from_node(&property), guard)
        }));

        return Ok(());
    }

    if let Some(clause) = node.cast::<UsesClauseSyntax>() {
        for path in clause.paths() {
            capabilities.push(DeclarationCapabilityTemplate::new(
                symbol_ordinal(capabilities.len())?,
                SyntaxAnchor::from_node(&path),
            ));
        }

        return Ok(());
    }

    let kind = match node.kind() {
        SyntaxKind::RequiresClause => DeclarationPredicateClauseKind::Requires,
        SyntaxKind::EnsuresClause => DeclarationPredicateClauseKind::Ensures,
        SyntaxKind::WithClause => DeclarationPredicateClauseKind::Static,
        _ => return Ok(()),
    };

    let mut result = Ok(());

    walk_direct_child_nodes(&node, |child| {
        let Some(expression) = child.cast::<ExpressionSyntax>() else {
            return SyntaxWalkControl::Continue;
        };

        let ordinal = match symbol_ordinal(expressions.len()) {
            Ok(ordinal) => ordinal,
            Err(error) => {
                result = Err(error);

                return SyntaxWalkControl::Stop;
            }
        };

        expressions.push(
            CallableContractExpressionTemplate::new(
                ordinal,
                kind,
                SyntaxAnchor::from_node(&node),
                DeclarationExpressionTemplate::new(owner, SyntaxAnchor::from_node(&expression)),
            )
            .with_guard(guard),
        );

        SyntaxWalkControl::Continue
    });

    result
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableContractTemplateQuery, CallableSymbolId, DeclarationPredicateClauseKind,
        SymbolOrdinal, SymbolQueryRequest,
    };

    use crate::compilation::binder::symbol::test_support::{
        binding_context, resolved_query, source_id, symbol_graph,
    };
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;

    #[test]
    fn source_templates_retain_guard_ancestry_without_promoting_declarations_to_proofs() {
        let compilation = compilation(
            r#"
            module app;

            func check(pos ready: bool)
                requires(true)
                when(ready)
                {
                    executes(pure, total)
                    ensures(true)
                    when(false)
                    {
                        executes(total)
                        ensures(false)
                    }
                }
                executes(total)
                ensures(true) {}
            "#,
        );

        let symbols = symbol_graph(&compilation);

        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let cancellation = CancellationToken::new();
        let context = binding_context(&compilation, &cancellation);

        let result = resolved_query(
            &context,
            SymbolQueryRequest::<CallableContractTemplateQuery>::new(CallableSymbolId::from(
                function,
            )),
        );

        let template = result.value().source_template().unwrap();

        let clauses = template
            .expressions()
            .iter()
            .map(|expression| (expression.kind(), expression.guard()))
            .collect::<Vec<_>>();

        assert_eq!(
            clauses,
            vec![
                (DeclarationPredicateClauseKind::Requires, None),
                (DeclarationPredicateClauseKind::Guard, None),
                (
                    DeclarationPredicateClauseKind::Ensures,
                    Some(SymbolOrdinal::new(1))
                ),
                (
                    DeclarationPredicateClauseKind::Guard,
                    Some(SymbolOrdinal::new(1))
                ),
                (
                    DeclarationPredicateClauseKind::Ensures,
                    Some(SymbolOrdinal::new(3))
                ),
                (DeclarationPredicateClauseKind::Ensures, None),
            ]
        );

        assert_eq!(
            template
                .execution_guarantees()
                .iter()
                .map(|guarantee| guarantee.guard())
                .collect::<Vec<_>>(),
            vec![
                Some(SymbolOrdinal::new(1)),
                Some(SymbolOrdinal::new(1)),
                Some(SymbolOrdinal::new(3)),
                None
            ]
        );

        assert!(result.diagnostics().is_empty());
    }
}
