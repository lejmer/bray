use std::sync::Arc;

use bray_binder::{
    BinderDependency, BinderFactError, BoundExpressionCheckInput, BoundUnitBindingError,
    BoundUnitComputation, bind_anonymous_callable, bind_callable_body, bind_constant_template,
    bind_constraint, bind_contract_clause, bind_expression_check_input, bind_predicate_definition,
    bind_runtime_default, unit_check_entry_context,
};
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitKind, CheckedControlFlowFacts, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedSemanticSelections,
};
use bray_checker::{
    CheckerInfrastructureError, CheckerOutcome, ControlFlowChecker, DefaultControlFlowChecker,
    DefaultExpressionTypeChecker, DefaultLiteralAdapter, DefaultSemanticSelector,
    ExpressionTypeChecker, ExpressionTypeInput, LiteralAdaptationInput, LiteralAdapter,
    SemanticSelectionInput, SemanticSelector, UnitCheckEntryContext, UnitCheckRequest,
};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::SymbolGraph;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use super::checker::CompilationCheckerContext;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

// Bound-unit keys are Arc-backed identities. Fact queries clone only their shared handles.
impl Compilation {
    /// Returns one bound semantic unit and its diagnostics.
    pub fn bound_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<BoundUnit>>, FactQueryError> {
        let published = self.bound_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns the control-flow facts and diagnostics for one bound semantic unit.
    pub fn checked_control_flow(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedControlFlowFacts>>, FactQueryError> {
        let published =
            self.checked_control_flow_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns canonical expression types and diagnostics for one bound semantic unit.
    pub fn checked_expression_types(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedExpressionTypes>>, FactQueryError> {
        let published =
            self.checked_expression_types_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns exact semantic selections and diagnostics for one bound semantic unit.
    pub fn checked_semantic_selections(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedSemanticSelections>>, FactQueryError> {
        let published =
            self.checked_semantic_selections_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns canonical source-literal values and diagnostics for one bound semantic unit.
    pub fn checked_literal_values(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedLiteralValues>>, FactQueryError> {
        let published =
            self.checked_literal_values_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    fn bound_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BoundUnit>>, FactQueryError> {
        let facts = self.binder_facts_for(&key, cancellation)?;

        self.unit_fact(
            &self.state.bound_units,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            cancellation,
            |_| {
                let unit = self.bound_unit_id(&key)?;

                bind_unit(&facts, unit, key)
                    .map(BoundUnitComputation::into_parts)
                    .map_err(map_binding_error)
            },
        )
    }

    fn checked_control_flow_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedControlFlowFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.checked_control_flow,
            CompilationFactKey::CheckedControlFlow(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                for nested in bound.result().value().nested_units() {
                    self.checked_control_flow_with_cancellation(nested.clone(), cancellation)?;
                }

                let context = self.checker_context_for(&key, cancellation)?;
                let entry = checker_entry_context(context.symbols(), bound.result().value())?;

                check_control_flow(bound.result().value(), &entry, &context)
            },
        )
    }

    fn checked_expression_types_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedExpressionTypes>>, FactQueryError> {
        self.unit_fact(
            &self.state.checked_expression_types,
            CompilationFactKey::CheckedExpressionTypes(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                for nested in bound.result().value().nested_units() {
                    self.checked_expression_types_with_cancellation(nested.clone(), cancellation)?;
                }

                let input =
                    self.expression_check_input_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;
                let entry = checker_entry_context(context.symbols(), bound.result().value())?;

                check_expression_types(
                    bound.result().value(),
                    &entry,
                    &context,
                    input.result().value().types(),
                )
            },
        )
    }

    fn expression_check_input_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BoundExpressionCheckInput>>, FactQueryError> {
        self.unit_fact(
            &self.state.expression_check_inputs,
            CompilationFactKey::ExpressionCheckInput(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let binder_facts = self.binder_facts_for(&key, cancellation)?;
                let input = bind_expression_check_input(
                    &binder_facts,
                    bound.result().value(),
                    self.available_compiler_known_symbols(),
                )
                .map_err(map_binder_fact_error)?;

                Ok((DiagnosticResult::without_diagnostics(input), Box::new([])))
            },
        )
    }

    fn checked_semantic_selections_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedSemanticSelections>>, FactQueryError> {
        self.unit_fact(
            &self.state.checked_semantic_selections,
            CompilationFactKey::CheckedSemanticSelections(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let types =
                    self.checked_expression_types_with_cancellation(key.clone(), cancellation)?;
                let input =
                    self.expression_check_input_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;
                let entry = checker_entry_context(context.symbols(), bound.result().value())?;

                check_semantic_selections(
                    bound.result().value(),
                    &entry,
                    &context,
                    types.result().value(),
                    input.result().value().selections(),
                )
            },
        )
    }

    fn checked_literal_values_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedLiteralValues>>, FactQueryError> {
        self.unit_fact(
            &self.state.checked_literal_values,
            CompilationFactKey::CheckedLiteralValues(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let types =
                    self.checked_expression_types_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;
                let entry = checker_entry_context(context.symbols(), bound.result().value())?;

                adapt_literal_values(
                    bound.result().value(),
                    &entry,
                    &context,
                    types.result().value(),
                )
            },
        )
    }
}

fn bind_unit(
    facts: &CompilationBinderFacts<'_>,
    unit: bray_bound_tree::BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError> {
    match key.kind() {
        BoundUnitKind::CallableBody => bind_callable_body(facts, unit, key)?.finish(),
        BoundUnitKind::AnonymousCallable => bind_anonymous_callable(facts, unit, key)?.finish(),
        BoundUnitKind::RuntimeDefault => bind_runtime_default(facts, unit, key)?.finish(),
        BoundUnitKind::ConstantTemplate => bind_constant_template(facts, unit, key)?.finish(),
        BoundUnitKind::PredicateDefinition => bind_predicate_definition(facts, unit, key)?.finish(),
        BoundUnitKind::Constraint => bind_constraint(facts, unit, key)?.finish(),
        BoundUnitKind::ContractClause => bind_contract_clause(facts, unit, key)?.finish(),
    }
}

fn checker_entry_context(
    symbols: &SymbolGraph,
    bound: &BoundUnit,
) -> Result<UnitCheckEntryContext, FactQueryError> {
    unit_check_entry_context(symbols, bound).map_err(FactQueryError::CheckerEntryContext)
}

fn check_control_flow(
    bound: &BoundUnit,
    entry: &UnitCheckEntryContext,
    context: &CompilationCheckerContext<'_>,
) -> Result<
    (
        DiagnosticResult<CheckedControlFlowFacts>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let request = checker_request(bound, entry, context)?;
    let outcome = DefaultControlFlowChecker
        .check_control_flow(request)
        .map(|result| result.into_facts());

    checker_outcome(outcome)
}

fn check_expression_types(
    bound: &BoundUnit,
    entry: &UnitCheckEntryContext,
    context: &CompilationCheckerContext<'_>,
    input: &ExpressionTypeInput,
) -> Result<
    (
        DiagnosticResult<CheckedExpressionTypes>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let request = checker_request(bound, entry, context)?;
    checker_outcome(DefaultExpressionTypeChecker.check_expression_types(request, input))
}

fn adapt_literal_values(
    bound: &BoundUnit,
    entry: &UnitCheckEntryContext,
    context: &CompilationCheckerContext<'_>,
    types: &CheckedExpressionTypes,
) -> Result<
    (
        DiagnosticResult<CheckedLiteralValues>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let request = checker_request(bound, entry, context)?;
    let input = LiteralAdaptationInput::new(types);

    checker_outcome(DefaultLiteralAdapter.adapt_literals(request, input))
}

fn check_semantic_selections(
    bound: &BoundUnit,
    entry: &UnitCheckEntryContext,
    context: &CompilationCheckerContext<'_>,
    types: &CheckedExpressionTypes,
    input: SemanticSelectionInput,
) -> Result<
    (
        DiagnosticResult<CheckedSemanticSelections>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let request = checker_request(bound, entry, context)?;
    checker_outcome(DefaultSemanticSelector.check_semantic_selections(request, types, input))
}

fn checker_request<'unit>(
    bound: &'unit BoundUnit,
    entry: &'unit UnitCheckEntryContext,
    context: &'unit CompilationCheckerContext<'unit>,
) -> Result<UnitCheckRequest<'unit, CompilationCheckerContext<'unit>>, FactQueryError> {
    UnitCheckRequest::new(bound, entry, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitRequest(error))
    })
}

fn checker_outcome<T>(
    outcome: CheckerOutcome<T>,
) -> Result<(DiagnosticResult<T>, Box<[BinderDependency]>), FactQueryError> {
    match outcome {
        CheckerOutcome::Complete(result) => Ok((result, Box::new([]))),
        CheckerOutcome::Cancelled => Err(FactQueryError::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            Err(FactQueryError::CheckerInfrastructure(error))
        }
    }
}

const fn map_binding_error(error: BoundUnitBindingError) -> FactQueryError {
    match error {
        BoundUnitBindingError::Cancelled => FactQueryError::Cancelled,
        BoundUnitBindingError::InvalidUnitKey
        | BoundUnitBindingError::MissingSyntax
        | BoundUnitBindingError::MissingOwner
        | BoundUnitBindingError::MissingModule
        | BoundUnitBindingError::SemanticValue(_)
        | BoundUnitBindingError::Construction
        | BoundUnitBindingError::Binding
        | BoundUnitBindingError::Assembly => FactQueryError::InfrastructureFailure,
    }
}

const fn map_binder_fact_error(error: BinderFactError) -> FactQueryError {
    match error {
        BinderFactError::Cancelled => FactQueryError::Cancelled,
        BinderFactError::DependencyUnavailable => FactQueryError::InfrastructureFailure,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{UnitCheckEntryContextError, unit_check_entry_context};
    use bray_bound_tree::{BoundExpression, SelectedArgument, SemanticSelection};
    use bray_checker::{CheckerInfrastructureError, UnitCheckEntryContext, UnitCheckRequestError};
    use bray_symbols::{CallableAbi, ConstantValueKind};

    use super::{Compilation, check_control_flow, checker_entry_context};
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError};
    use crate::test_support::{FactTestGate, compilation, source_callable_body_key};

    #[test]
    fn repeated_and_concurrent_requests_share_production_semantic_facts() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let first_bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("first bound-unit request must complete: {error:?}"),
        };
        let second_bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("repeated bound-unit request must complete: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first_bound, &second_bound));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .checked_control_flow
            .set_test_observer(&key, gate.observer())
        {
            panic!("control-flow fact must accept a test observer: {error:?}");
        }

        // Bound-unit keys are Arc-backed immutable identities shared by concurrent requests.
        let checked = std::thread::scope(|scope| {
            let compilation = &compilation;
            let owner_key = key.clone();
            let owner = scope.spawn(move || compilation.checked_control_flow(owner_key));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter_key = key.clone();
            let waiter = scope.spawn(move || compilation.checked_control_flow(waiter_key));

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(Ok(checked)) => checked,
                Ok(Err(error)) => panic!("concurrent semantic request failed: {error:?}"),
                Err(_) => panic!("concurrent semantic request panicked"),
            })
        });

        assert!(
            checked
                .iter()
                .skip(1)
                .all(|fact| Arc::ptr_eq(&checked[0], fact))
        );
    }

    #[test]
    fn direct_calls_publish_exact_selected_invocation_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func recurse(pos value: i32 = 1)\n",
            "{\n",
            "    recurse(2);\n",
            "    recurse();\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);
        let types = match compilation.checked_expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("direct call typing must complete: {error:?}"),
        };
        let selections = match compilation.checked_semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("direct call selection must complete: {error:?}"),
        };

        let [explicit, defaulted] = selections.value().entries() else {
            panic!("both direct calls must publish selections");
        };
        let SemanticSelection::Call(explicit) = explicit.selection() else {
            panic!("the first selected expression must be a call");
        };
        let SemanticSelection::Call(defaulted) = defaulted.selection() else {
            panic!("the second selected expression must be a call");
        };

        let [
            SelectedArgument::Explicit {
                expression,
                parameter,
                conversion,
            },
        ] = explicit.arguments()
        else {
            panic!("the explicit argument must map to its declared parameter");
        };
        let [
            SelectedArgument::Default {
                parameter: default_parameter,
                provider,
            },
        ] = defaulted.arguments()
        else {
            panic!("the omitted argument must retain its default provider");
        };

        assert_eq!(explicit.abi(), CallableAbi::Bray);
        assert_eq!(defaulted.abi(), CallableAbi::Bray);
        assert_eq!(explicit.target(), defaulted.target());
        assert!(explicit.target().declaration().is_some());
        assert_eq!(parameter, default_parameter);

        let symbols = match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("callable symbols must be available: {error:?}"),
        };
        let declared_provider = symbols
            .callable_parameter(*default_parameter)
            .and_then(|parameter| parameter.default_provider());

        assert_eq!(declared_provider, Some(*provider));
        assert_eq!(conversion.source_type(), conversion.target_type());
        assert_eq!(
            types
                .value()
                .expression(*expression)
                .map(|result| result.ty()),
            Some(conversion.source_type())
        );
        assert!(explicit.receiver().is_none());
        assert!(defaulted.receiver().is_none());
        assert!(types.diagnostics().is_empty());
        assert!(selections.diagnostics().is_empty());
    }

    #[test]
    fn source_literals_publish_values_adapted_to_final_expression_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let fixed: i32 = 42;\n",
            "    let target_sized: usize = 42;\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);
        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("literal body must bind: {error:?}"),
        };
        let values = match compilation.checked_literal_values(key) {
            Ok(values) => values,
            Err(error) => panic!("literal adaptation must complete: {error:?}"),
        };

        let [fixed, target_sized] = values.value().entries() else {
            panic!("both source literals must publish canonical values");
        };

        for entry in [fixed, target_sized] {
            assert!(matches!(
                bound.value().view().expression(entry.expression()),
                Some(BoundExpression::Literal(_))
            ));
        }

        let semantic_values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };
        for entry in [fixed, target_sized] {
            let value = match semantic_values.constant_value_data(entry.value()) {
                Ok(value) => value,
                Err(error) => panic!("adapted value must be interned: {error:?}"),
            };

            assert!(matches!(value.kind(), ConstantValueKind::Integer(_)));
        }
    }

    #[test]
    fn cancelled_production_queries_publish_nothing_and_can_be_retried() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);
        let bound_cancellation = CancellationToken::new();
        let bound_gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .bound_units
            .set_test_observer(&key, bound_gate.observer())
        {
            panic!("bound-unit fact must accept a test observer: {error:?}");
        }

        let bound = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation.bound_unit_with_cancellation(request_key, &bound_cancellation)
            });

            bound_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            bound_cancellation.cancel();
            bound_gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled bound-unit request panicked"),
            }
        });

        assert!(matches!(bound, Err(FactQueryError::Cancelled)));
        assert_eq!(compilation.state.bound_units.is_published(&key), Ok(false));

        let bound = compilation.bound_unit(key.clone());

        assert!(bound.is_ok());

        let checked_cancellation = CancellationToken::new();
        let checked_gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .checked_control_flow
            .set_test_observer(&key, checked_gate.observer())
        {
            panic!("control-flow fact must accept a test observer: {error:?}");
        }

        let checked = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation
                    .checked_control_flow_with_cancellation(request_key, &checked_cancellation)
            });

            checked_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            checked_cancellation.cancel();
            checked_gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled control-flow request panicked"),
            }
        });

        assert!(matches!(checked, Err(FactQueryError::Cancelled)));
        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        assert!(compilation.checked_control_flow(key).is_ok());
    }

    #[test]
    fn recursive_callable_references_do_not_form_body_fact_cycles() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func recurse()\n",
            "{\n",
            "    recurse();\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        let checked = match compilation.checked_control_flow(key) {
            Ok(checked) => checked,
            Err(error) => panic!("recursive callable must check without a cycle: {error:?}"),
        };

        assert!(checked.diagnostics().is_empty());
    }

    #[test]
    fn invalid_checker_requests_preserve_their_typed_infrastructure_error() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let canonical = match unit_check_entry_context(context.symbols(), bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("checker entry context must be available: {error:?}"),
        };

        let UnitCheckEntryContext::CallableBody(declaration) = canonical else {
            panic!("callable body must produce a callable-body checker entry");
        };

        let invalid = UnitCheckEntryContext::Constraint(declaration);
        let result = check_control_flow(bound.value(), &invalid, &context);

        assert!(matches!(
            result,
            Err(FactQueryError::CheckerInfrastructure(
                CheckerInfrastructureError::InvalidUnitRequest(
                    UnitCheckRequestError::EntryContextMismatch
                )
            ))
        ));
    }

    #[test]
    fn invalid_checker_entry_contexts_preserve_their_typed_cause() {
        let primary = callable_compilation();
        let key = source_callable_body_key(&primary);

        let bound = match primary.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let foreign = compilation(
            r#"module other;
func other()
{
}
"#,
        );

        let symbols = match foreign.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("foreign symbol graph must be available: {error:?}"),
        };

        assert!(matches!(
            checker_entry_context(symbols, bound.value()),
            Err(FactQueryError::CheckerEntryContext(
                UnitCheckEntryContextError::MissingOwner
            ))
        ));
    }

    fn callable_compilation() -> Compilation {
        compilation(
            r#"module app;
func main()
{
}
"#,
        )
    }
}
