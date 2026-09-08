use bray_binder::BindingQueryContext;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableConditionSet, CallableConditions, CallableConditionsQuery, CallableContractClauseKind,
    CallableSymbolId, SymbolQueryRequest,
};
use bray_syntax::syntax_node_view;

use super::binding::CompilationSymbolQueryEvaluator;
use super::cache::CompilationSymbolSemantics;
use super::contract::{
    ContractClauseSyntax, bind_callable_predicates, bind_callable_static_constraints,
    direct_contract_clauses,
};
use super::surface::with_declaration_root;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};
use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<CallableConditionsQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<CallableConditionsQuery> {
        &self.callable_conditions
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<CallableConditionsQuery>,
    ) -> BindingQueryResult<DiagnosticResult<CallableConditionSet>> {
        bind_callable_conditions(context, request.owner())
    }
}

fn bind_callable_conditions(
    context: &CompilationBindingContext<'_>,
    owner: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<CallableConditionSet>> {
    if let Some(address) = context.imported_semantic_address(owner.into_any())? {
        let imported = super::imported::imported_callable_contracts(context, address)?;

        let (contract, diagnostics) = imported.into_parts();

        // The declaration query independently retains the imported contract's immutable conditions.
        return Ok(DiagnosticResult::new(
            contract.conditions().clone(),
            diagnostics,
        ));
    }

    let clauses = with_declaration_root(context, owner.into_any(), |root| {
        Ok(direct_contract_clauses(root))
    })?;

    bind_condition_clauses(context, owner.into_any(), clauses)
}

pub(in crate::compilation::binder) fn bind_callable_type_contract(
    context: &CompilationBindingContext<'_>,
    owner: bray_symbols::AnySymbolId,
    source: bray_declarations::SyntaxAnchor,
    signature: &bray_symbols::CallableTypeTemplate,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableContractSet>> {
    let root = context
        .syntax()
        .find_node(
            source.source_id(),
            source.syntax_kind(),
            source.full_range(),
            source.is_recovered(),
        )
        .ok_or(bray_binder::BindingQueryError::Binding(
            bray_binder::BindingError::SyntaxContract(source),
        ))?;

    let clauses = direct_contract_clauses(root);

    let requirements =
        super::contract::bind_execution_requirement_clauses(context, owner, &clauses)?;

    let capabilities = super::contract::bind_trusted_capability_clauses(
        context,
        owner,
        clauses.iter().filter_map(|clause| match clause {
            // Syntax wrappers share their immutable tree nodes.
            ContractClauseSyntax::Uses(clause) => Some(clause.clone()),
            _ => None,
        }),
    )?;

    let conditions = bind_condition_clauses(context, owner, clauses)?;

    let mut diagnostics = DiagnosticBag::merged_all([
        conditions.diagnostics(),
        capabilities.diagnostics(),
        requirements.diagnostics(),
    ]);

    super::contract::validate_trusted_capabilities(
        context,
        || Ok(source),
        signature.trust(),
        capabilities.value(),
        None,
        false,
        &mut diagnostics,
    )?;

    let dependencies = context
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(crate::compilation::binder::semantic_value_binding_error)?;

    let (invocation, deferred) = super::contract::callable_phase_behaviors(
        signature.execution(),
        capabilities
            .value()
            .iter()
            .map(|capability| capability.requirement()),
        dependencies,
        requirements.value().iter().copied(),
        None,
    );

    let (conditions, _) = conditions.into_parts();

    Ok(DiagnosticResult::new(
        bray_symbols::CallableContractSet::new(conditions, invocation, deferred),
        diagnostics,
    ))
}

fn bind_condition_clauses(
    context: &CompilationBindingContext<'_>,
    owner: bray_symbols::AnySymbolId,
    clauses: Vec<ContractClauseSyntax>,
) -> BindingQueryResult<DiagnosticResult<CallableConditionSet>> {
    let mut predicates = Vec::new();
    let mut execution_guarantees = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for clause in clauses {
        match clause {
            ContractClauseSyntax::Requires(clause) => bind_callable_predicates(
                context,
                owner,
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Requires,
                &mut predicates,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Ensures(clause) => bind_callable_predicates(
                context,
                owner,
                syntax_node_view(&clause),
                clause.expressions(),
                CallableContractClauseKind::Ensures,
                &mut predicates,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::With(clause) => bind_callable_static_constraints(
                context,
                owner,
                clause.expressions(),
                &mut predicates,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Executes(clause) => super::guarantee::bind_execution_guarantees(
                &clause,
                None,
                &mut execution_guarantees,
                &mut diagnostics,
            ),
            ContractClauseSyntax::When(clause) => super::guarantee::bind_guarded_guarantees(
                context,
                owner,
                &clause,
                None,
                &mut predicates,
                &mut execution_guarantees,
                &mut diagnostics,
            )?,
            ContractClauseSyntax::Uses(_) => {}
        }
    }

    Ok(DiagnosticResult::new(
        CallableConditionSet::new(predicates).with_execution_guarantees(execution_guarantees),
        diagnostics,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::SymbolQueryProvider;
    use bray_symbols::{
        CallableConditions, CallableConditionsQuery, CallableContractsQuery, CallableSymbolId,
        ExecutionProperty, SymbolQueryRequest,
    };

    use crate::test_support::{compilation, source_callable_body_key, source_function};

    #[test]
    fn callable_type_capabilities_remain_on_their_execution_phase() {
        use bray_binder::BindingQueryContext;

        for (syntax, asynchronous) in [
            ("trusted func() uses(foreign_call)", false),
            ("trusted async func() uses(foreign_call)", true),
        ] {
            let compilation = compilation(&format!(
                "trusted module app; func outer(operation: {syntax}) {{}}"
            ));

            let context = compilation
                .binding_context(&compilation.state.cancellation)
                .unwrap();

            let owner = source_function(&compilation, "outer").into();
            let occurrences = callable_occurrences(&context);

            assert_eq!(occurrences.len(), 1);

            let source = occurrences[0];

            let signature = context
                .callable_contract_input_signature(owner, source)
                .unwrap();

            let contract = context
                .callable_type_contract(owner, source, signature.value())
                .unwrap();

            assert!(
                contract.diagnostics().is_empty(),
                "{:?}",
                contract.diagnostics()
            );

            let phase = if asynchronous {
                assert!(
                    contract
                        .value()
                        .invocation_behavior()
                        .trusted_capabilities()
                        .is_empty()
                );

                contract.value().deferred_execution_behavior().unwrap()
            } else {
                assert!(contract.value().deferred_execution_behavior().is_none());

                contract.value().invocation_behavior()
            };

            assert_eq!(phase.trusted_capabilities().len(), 1);

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn callable_type_capabilities_require_a_trusted_callable_type() {
        let compilation =
            compilation("trusted module app; func outer(operation: func() uses(foreign_call)) {}");

        let diagnostics = compilation.check_diagnostics();

        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.kind() ==
            bray_diagnostics::DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable
            })
            .unwrap_or_else(|| panic!("{diagnostics:?}"));

        assert!(diagnostic.primary_span().is_some());

        assert_eq!(
            diagnostic.args(),
            &[bray_diagnostics::DiagnosticArg::referenced_name(
                "foreign_call"
            )]
        );
    }

    #[test]
    fn callable_occurrences_retain_their_own_guard_and_result_inputs() {
        use bray_binder::BindingQueryContext;

        let compilation = compilation(
            "module app; func outer(pos outer_value: i32, \
             operation: func(pos ready: bool) -> bool when(ready) { executes(pure, total) ensures(result) }) \
             { let callback = lambda(pos ready: bool) -> bool \
             when(ready) { executes(pure, total) ensures(result) } { return ready; }; }",
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = source_function(&compilation, "outer").into();
        let occurrences = callable_occurrences(&context);

        assert_eq!(occurrences.len(), 2);

        for source in occurrences {
            let signature = context
                .callable_contract_input_signature(owner, source)
                .unwrap();

            let result = context
                .callable_type_contract(owner, source, signature.value())
                .unwrap();

            assert!(
                result.diagnostics().is_empty(),
                "{:?}",
                result.diagnostics()
            );

            assert_eq!(result.value().entry_guards().len(), 1);
            assert_eq!(result.value().guarded_postconditions().len(), 1);
            assert_eq!(result.value().execution_guarantees().len(), 2);
            assert_input_conditions(&compilation, result.value().conditions(), 1);
        }
    }

    fn assert_input_conditions(
        compilation: &crate::Compilation,
        conditions: &bray_symbols::CallableConditionSet,
        result_ordinal: u32,
    ) {
        use bray_symbols::{ConstantTermData, ProofOutcome, SymbolOrdinal};

        let values = compilation.semantic_value_store().unwrap();

        let argument = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let result = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(
                result_ordinal,
            )))
            .unwrap();

        let guard = conditions.entry_guards()[0]
            .predicate()
            .unwrap()
            .condition()
            .unwrap();

        let postcondition = conditions.guarded_postconditions()[0]
            .predicate()
            .unwrap()
            .condition()
            .unwrap();

        assert_eq!(
            bray_checker::prove_condition(values, &[(argument, true)], guard),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            bray_checker::prove_condition(values, &[(argument, true)], postcondition),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            bray_checker::prove_condition(values, &[(result, true)], postcondition),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            bray_checker::prove_condition(values, &[(result, true)], guard),
            Ok(ProofOutcome::Unknown)
        );
    }

    fn callable_occurrences(
        context: &crate::compilation::binder::CompilationBindingContext<'_>,
    ) -> Vec<bray_declarations::SyntaxAnchor> {
        use bray_binder::BindingQueryContext;
        use bray_declarations::SyntaxAnchor;

        use bray_syntax::{
            LambdaExpressionSyntax, SyntaxWalkControl, SyntaxWalkEvent, TypeExpressionSyntax,
            walk_syntax_node,
        };

        let mut occurrences = Vec::new();

        for source in context.syntax().source_units() {
            walk_syntax_node(source, |event| {
                if let SyntaxWalkEvent::EnterNode(node) = event
                    && (node.cast::<LambdaExpressionSyntax>().is_some()
                        || node
                            .cast::<TypeExpressionSyntax>()
                            .is_some_and(|ty| ty.func_keyword().is_some()))
                {
                    occurrences.push(SyntaxAnchor::from_node(&node));
                }

                SyntaxWalkControl::Continue
            });
        }

        occurrences
    }

    #[test]
    fn callable_type_and_lambda_contracts_cannot_capture_enclosing_parameters() {
        use bray_binder::BindingQueryContext;

        let compilation = compilation(
            "module app; func outer(pos captured: bool, \
             operation: func(pos ready: bool) -> bool when(captured) { executes(pure) }) \
             { let callback = lambda(pos ready: bool) -> bool \
             when(captured) { executes(pure) } { return ready; }; }",
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = source_function(&compilation, "outer").into();
        let occurrences = callable_occurrences(&context);

        assert_eq!(occurrences.len(), 2);

        for source in occurrences {
            let signature = context
                .callable_contract_input_signature(owner, source)
                .unwrap();

            let result = context
                .callable_type_contract(owner, source, signature.value())
                .unwrap();

            assert!(
                result.diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind() == bray_diagnostics::DiagnosticKind::BindingUnresolvedName
                        && diagnostic.args().contains(
                            &bray_diagnostics::DiagnosticArg::referenced_name("captured"),
                        )
                }),
                "{:?}",
                result.diagnostics()
            );
        }
    }

    #[test]
    fn callable_type_and_lambda_entry_guards_have_no_result_input() {
        use bray_binder::BindingQueryContext;

        let compilation = compilation(
            "module app; func outer(operation: func(pos ready: bool) -> bool \
             when(result) { executes(pure) }) { let callback = \
             lambda(pos ready: bool) -> bool when(result) { executes(pure) } { return ready; }; }",
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = source_function(&compilation, "outer").into();
        let occurrences = callable_occurrences(&context);

        assert_eq!(occurrences.len(), 2);

        for source in occurrences {
            let signature = context
                .callable_contract_input_signature(owner, source)
                .unwrap();

            let result = context
                .callable_type_contract(owner, source, signature.value())
                .unwrap();

            assert!(
                result.diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind() == bray_diagnostics::DiagnosticKind::BindingUnresolvedName
                        && diagnostic
                            .args()
                            .contains(&bray_diagnostics::DiagnosticArg::referenced_name("result"))
                        && diagnostic.primary_span().is_some()
                }),
                "{:?}",
                result.diagnostics()
            );

            assert!(
                result.value().entry_guards()[0]
                    .predicate()
                    .and_then(|predicate| predicate.condition())
                    .is_none()
            );
        }
    }

    #[test]
    fn named_callable_contract_types_retain_conditions() {
        use bray_binder::BindingQueryContext;
        use bray_symbols::{CallableContractTypeQuery, SymbolOrigin, TypeData};

        let compilation = compilation(
            "module app; callable Checked = func(pos ready: bool) -> bool \
             when(ready) { executes(pure, total) ensures(result) };",
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = context
            .symbols()
            .callable_contracts()
            .iter()
            .find(|symbol| matches!(symbol.origin(), SymbolOrigin::Source))
            .unwrap()
            .id();

        let result = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractTypeQuery>::new(owner))
            .unwrap();

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let ty = context
            .semantic_values()
            .type_data(result.value().resolved_type().unwrap())
            .unwrap();

        let TypeData::Callable(callable) = ty.as_ref() else {
            panic!("callable type expected")
        };

        assert_eq!(callable.entry_guards().len(), 1);
        assert_eq!(callable.guarded_postconditions().len(), 1);
        assert_eq!(callable.execution_guarantees().len(), 2);
    }

    #[test]
    fn generic_callable_contract_inputs_keep_their_formal_ordinals_after_substitution() {
        use bray_binder::BindingQueryContext;

        use bray_symbols::{
            CallableContractTypeQuery, GenericArgument, GenericOwnerId, GenericSubstitutionData,
            SymbolOrigin, TypeData,
        };

        let compilation = compilation(
            "module app; callable Checked<T> = func(pos ready: bool, value: T) -> bool \
             when(ready) { executes(pure, total) ensures(result) };",
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = context
            .symbols()
            .callable_contracts()
            .iter()
            .find(|symbol| matches!(symbol.origin(), SymbolOrigin::Source))
            .unwrap()
            .id();

        let result = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractTypeQuery>::new(owner))
            .unwrap();

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let values = context.semantic_values();
        let ty = result.value().resolved_type().unwrap();
        let data = values.type_data(ty).unwrap();

        let TypeData::Callable(callable) = data.as_ref() else {
            panic!("callable expected")
        };

        let parameters =
            super::super::generic_parameter_ids(context.symbols(), owner.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(
                    GenericOwnerId::try_new(owner.into()).unwrap(),
                    parameters,
                    [GenericArgument::Type(callable.parameters()[0].ty())],
                )
                .unwrap(),
            )
            .unwrap();

        let substituted = values
            .type_data(values.substitute_type(ty, substitution).unwrap())
            .unwrap();

        let TypeData::Callable(substituted) = substituted.as_ref() else {
            panic!("callable expected")
        };

        assert_eq!(
            substituted.parameters()[1].ty(),
            callable.parameters()[0].ty()
        );

        assert_input_conditions(&compilation, callable.conditions(), 2);
        assert_input_conditions(&compilation, substituted.conditions(), 2);
    }

    #[test]
    fn callable_conditions_are_cached_without_checking_the_ordinary_body() {
        let compilation = compilation(
            "module app; func check(pos ready: bool) -> bool \
             when(ready) { executes(pure, total) ensures(result) } \
             { return unknown_body_function(); }",
        );

        let owner = CallableSymbolId::from(source_function(&compilation, "check"));
        let body = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.body_semantics.is_published(&body),
            Ok(false)
        );

        let context = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let request = SymbolQueryRequest::<CallableConditionsQuery>::new(owner);
        let first = context.resolve_symbol_query(request).unwrap();
        let second = context.resolve_symbol_query(request).unwrap();

        assert!(Arc::ptr_eq(&first, &second));

        assert_eq!(
            compilation.state.body_semantics.is_published(&body),
            Ok(false)
        );

        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
        assert_eq!(first.value().entry_guards().len(), 1);
        assert_eq!(first.value().guarded_postconditions().len(), 1);

        assert_eq!(
            first
                .value()
                .execution_guarantees()
                .iter()
                .map(|guarantee| guarantee.property())
                .collect::<Vec<_>>(),
            [ExecutionProperty::Pure, ExecutionProperty::Total],
        );

        let complete = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(owner))
            .unwrap();

        assert_eq!(
            compilation.state.body_semantics.is_published(&body),
            Ok(true)
        );

        assert_eq!(complete.value().conditions(), first.value());
        assert!(compilation.check_diagnostics().has_errors());
    }
}
