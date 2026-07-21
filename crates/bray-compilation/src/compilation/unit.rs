use std::sync::Arc;

use bray_binder::{
    BinderDependency, BinderFactError, BoundUnitBindingError, BoundUnitComputation,
    bind_anonymous_callable, bind_callable_body, bind_constant_template, bind_constraint,
    bind_contract_clause, bind_expression_candidates, bind_predicate_definition,
    bind_runtime_default, semantic_unit_context,
};
use bray_bound_tree::{
    AnyBoundNodeId, BoundUnit, BoundUnitKey, BoundUnitKind, BoundUnitRoot, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, CheckedControlFlowFacts, CheckedExpressionTypes,
    CheckedSemanticSelections, DeclaredValueTypeTemplates, walk_bound_unit_view,
};
use bray_checker::{
    CheckerInfrastructureError, CheckerOutcome, CheckerUnitView, ControlFlowChecker,
    DefaultControlFlowChecker, DefaultExpressionSemanticChecker, ExpressionCandidateSet,
    ExpressionSemanticChecker, SemanticUnitContext,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::SymbolGraph;

use super::binder::{CompilationBinderFacts, bind_declared_value_type_templates};
use super::checker::CompilationCheckerContext;
use super::facts::{CheckedExpressionSemantics, Compilation};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

type ExpressionSemanticComputation = (
    DiagnosticResult<CheckedExpressionSemantics>,
    Box<[BinderDependency]>,
);

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

    /// Returns final expression types and their diagnostics for one bound semantic unit.
    pub fn checked_expression_types(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedExpressionTypes>>, FactQueryError> {
        let published =
            self.checked_expression_types_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns exact semantic selections and their diagnostics for one bound semantic unit.
    pub fn checked_semantic_selections(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedSemanticSelections>>, FactQueryError> {
        let published =
            self.checked_semantic_selections_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns source-declared value type templates and equality constraints for one bound unit.
    pub fn declared_value_type_templates(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<DeclaredValueTypeTemplates>>, FactQueryError> {
        let published =
            self.declared_value_type_templates_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    fn bound_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BoundUnit>>, FactQueryError> {
        self.unit_fact(
            &self.state.bound_units,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let facts = self.binder_facts_for(&key, cancellation)?;
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

                let context = self.checker_context_for(&key, cancellation)?;
                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                check_control_flow(bound.result().value(), &semantic_context, &context)
            },
        )
    }

    pub(super) fn expression_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedExpressionSemantics>>, FactQueryError> {
        // Exact fact dependencies retain the same Arc-backed unit identity independently.
        self.unit_fact(
            &self.state.expression_semantics,
            CompilationFactKey::ExpressionSemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;
                let facts = self.binder_facts_for(&key, cancellation)?;
                let candidates = expression_candidates(&facts, bound.result().value())?;

                let context = self.checker_context_for(&key, cancellation)?;
                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                check_expression_semantics(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    declared.result().value(),
                    candidates.value(),
                    candidates.diagnostics(),
                )
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
            |_| {
                let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

                // The projection owns a stable immutable table while its entries remain Arc-shared.
                let types = semantics.result().value().0.clone();

                // Each public projection retains the diagnostics from the atomic computation.
                let diagnostics = semantics.result().diagnostics().clone();

                Ok((DiagnosticResult::new(types, diagnostics), Box::new([])))
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
            |_| {
                let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

                // The projection owns a stable immutable table while its entries remain Arc-shared.
                let selections = semantics.result().value().1.clone();

                // Each public projection retains the diagnostics from the atomic computation.
                let diagnostics = semantics.result().diagnostics().clone();

                Ok((DiagnosticResult::new(selections, diagnostics), Box::new([])))
            },
        )
    }

    fn declared_value_type_templates_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<DeclaredValueTypeTemplates>>, FactQueryError> {
        self.unit_fact(
            &self.state.declared_value_type_templates,
            CompilationFactKey::DeclaredValueTypeTemplates(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let facts = self.binder_facts_for(&key, cancellation)?;

                let result = bind_declared_value_type_templates(&facts, bound.result().value())
                    .map_err(map_binder_fact_error)?;

                Ok((result, Box::new([])))
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

fn semantic_unit_context_for(
    symbols: &SymbolGraph,
    bound: &BoundUnit,
) -> Result<SemanticUnitContext, FactQueryError> {
    semantic_unit_context(symbols, bound).map_err(FactQueryError::SemanticUnitContext)
}

fn check_control_flow(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
) -> Result<
    (
        DiagnosticResult<CheckedControlFlowFacts>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    let result = match DefaultControlFlowChecker.check_control_flow(unit) {
        CheckerOutcome::Complete(result) => result.map(|result| result.into_facts()),
        CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            return Err(FactQueryError::CheckerInfrastructure(error));
        }
    };

    Ok((result, Box::new([])))
}

fn expression_candidates(
    facts: &CompilationBinderFacts<'_>,
    bound: &BoundUnit,
) -> Result<DiagnosticResult<Vec<ExpressionCandidateSet>>, FactQueryError> {
    let mut candidates = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(bound.view(), root_node(bound.root()), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        match bind_expression_candidates(facts, bound, expression) {
            Ok(result) => {
                let (candidate, candidate_diagnostics) = result.into_parts();

                if !matches!(candidate, ExpressionCandidateSet::NotApplicable(_)) {
                    candidates.push(candidate);
                }

                diagnostics.add_range(candidate_diagnostics.diagnostics().iter().cloned());
            }
            Err(error) => {
                failure = Some(error);

                return BoundWalkControl::Stop;
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = failure {
        return Err(map_binder_fact_error(error));
    }

    match outcome {
        BoundWalkOutcome::Completed => Ok(DiagnosticResult::new(candidates, diagnostics)),
        BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
            Err(FactQueryError::InfrastructureFailure)
        }
    }
}

fn check_expression_semantics(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    declared_types: &DeclaredValueTypeTemplates,
    candidates: &[ExpressionCandidateSet],
    candidate_diagnostics: &DiagnosticBag,
) -> Result<ExpressionSemanticComputation, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    let result = match DefaultExpressionSemanticChecker.check_expression_semantics(
        unit,
        declared_types,
        candidates,
    ) {
        CheckerOutcome::Complete(result) => {
            let (value, diagnostics) = result.into_parts();

            DiagnosticResult::new(value, candidate_diagnostics.merged(&diagnostics))
        }
        CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            return Err(FactQueryError::CheckerInfrastructure(error));
        }
    };

    Ok((result, Box::new([])))
}

const fn root_node(root: BoundUnitRoot) -> AnyBoundNodeId {
    match root {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            AnyBoundNodeId::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
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

    use bray_binder::{SemanticUnitContextError, semantic_unit_context};
    use bray_bound_tree::{
        BoundReferenceTarget, BoundUnitKind, DeclaredValueTypeConstraintKind,
        DeclaredValueTypeTemplates, DeclaredValueTypeTerm, SelectedArgument, SemanticSelection,
    };
    use bray_checker::{CheckerInfrastructureError, CheckerUnitViewError, SemanticUnitContext};
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{NamedTypeSymbolId, SymbolKind, TypeData, TypeExpressionTemplate};

    use super::{Compilation, check_control_flow, semantic_unit_context_for};
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError};
    use crate::test_support::{FactTestGate, compilation, source_callable_body_key};

    #[test]
    fn declared_value_type_templates_publish_lazy_source_evidence_and_constraints() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func identity<const count: usize>(value: [i32; count]) -> [i32; count]\n",
            "{\n",
            "    let local: [i32; count] = value;\n",
            "    const copy: [i32; count] = local;\n",
            "    let callable = lambda(item: [i32; count]) -> [i32; count]\n",
            "    {\n",
            "        item\n",
            "    };\n",
            "    copy\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation
                .state
                .declared_value_type_templates
                .is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        let facts = match compilation.declared_value_type_templates(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("declared value types must publish: {error:?}"),
        };

        assert!(has_value_kind(
            facts.value(),
            SymbolKind::GenericConstParameter
        ));

        assert!(has_value_kind(facts.value(), SymbolKind::CallableParameter));
        assert!(has_value_kind(facts.value(), SymbolKind::LocalConstant));

        assert!(
            facts
                .value()
                .evidence()
                .iter()
                .any(|entry| matches!(entry.term(), DeclaredValueTypeTerm::Pattern(_)))
        );

        assert!(matches!(
            facts.value().callable_result(),
            Some(TypeExpressionTemplate::Array { .. })
        ));

        assert!(has_constraint_kind(
            facts.value(),
            DeclaredValueTypeConstraintKind::Initializer
        ));

        assert!(has_constraint_kind(
            facts.value(),
            DeclaredValueTypeConstraintKind::PatternBinding
        ));

        assert!(has_constraint_kind(
            facts.value(),
            DeclaredValueTypeConstraintKind::DefinitionUse
        ));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::DeclaredValueTypeTemplates(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published declared value types must retain dependencies"),
            Err(error) => panic!("declared value type dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("bound callable must remain available: {error:?}"),
        };

        let [nested] = bound.value().nested_units() else {
            panic!("test callable must retain one anonymous callable");
        };

        let nested = nested.clone();

        let nested_facts = match compilation.declared_value_type_templates(nested.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("anonymous callable value types must publish: {error:?}"),
        };

        assert!(has_value_kind(
            nested_facts.value(),
            SymbolKind::AnonymousCallableParameter
        ));

        assert!(has_value_kind(
            nested_facts.value(),
            SymbolKind::GenericConstParameter
        ));

        assert!(matches!(
            nested_facts.value().callable_result(),
            Some(TypeExpressionTemplate::Array { .. })
        ));

        assert_eq!(
            compilation.state.checked_control_flow.is_published(&nested),
            Ok(false)
        );
    }

    #[test]
    fn declared_value_type_templates_cover_declaration_surface_categories() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "predicate valid(value: i32) = true;\n",
            "struct Holder\n",
            "{\n",
            "    func value() -> i32\n",
            "    {\n",
            "        return 1;\n",
            "    }\n",
            "}\n",
            "func check(value: i32) -> i32 ensures(result == value)\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys_for_test() {
            Ok(keys) => keys,
            Err(error) => panic!("declared unit keys must be available: {error:?}"),
        };

        let constant = facts_for_kind(&compilation, &keys, BoundUnitKind::ConstantTemplate);
        assert!(has_value_kind(constant.value(), SymbolKind::Constant));

        assert!(has_constraint_kind(
            constant.value(),
            DeclaredValueTypeConstraintKind::Initializer
        ));

        let predicate = facts_for_kind(&compilation, &keys, BoundUnitKind::PredicateDefinition);

        assert!(has_value_kind(
            predicate.value(),
            SymbolKind::PredicateParameter
        ));

        let receiver = keys
            .iter()
            .filter(|key| key.kind() == BoundUnitKind::CallableBody)
            .filter_map(|key| compilation.declared_value_type_templates(key.clone()).ok())
            .find(|facts| has_value_kind(facts.value(), SymbolKind::ReceiverParameter))
            .unwrap_or_else(|| panic!("type callable body must publish receiver evidence"));

        assert!(receiver.value().callable_result().is_some());

        let contract = facts_for_kind(&compilation, &keys, BoundUnitKind::ContractClause);

        assert!(has_value_kind(
            contract.value(),
            SymbolKind::PostconditionResult
        ));

        assert!(has_value_kind(
            contract.value(),
            SymbolKind::CallableParameter
        ));
    }

    #[test]
    fn runtime_defaults_publish_their_exact_parameter_type_template() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func defaults(first: i32 = 1, second: [i32; 1] = first)\n",
            "{\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys_for_test() {
            Ok(keys) => keys,
            Err(error) => panic!("declared unit keys must be available: {error:?}"),
        };

        let templates = keys
            .iter()
            .filter(|key| key.kind() == BoundUnitKind::RuntimeDefault)
            .map(|key| {
                let facts = match compilation.declared_value_type_templates(key.clone()) {
                    Ok(facts) => facts,
                    Err(error) => panic!("runtime default types must publish: {error:?}"),
                };

                let [evidence] = facts.value().evidence() else {
                    panic!("runtime default must publish one declared type");
                };

                evidence.template().clone()
            })
            .collect::<Vec<_>>();

        assert_eq!(templates.len(), 2);

        assert!(
            templates
                .iter()
                .any(|template| matches!(template, TypeExpressionTemplate::Resolved(_)))
        );

        assert!(
            templates
                .iter()
                .any(|template| matches!(template, TypeExpressionTemplate::Array { .. }))
        );
    }

    #[test]
    fn recovered_declared_value_type_syntax_is_deterministic_and_panic_free() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func broken(value: i32) -> i32\n",
            "{\n",
            "    let local: = value;\n",
            "    local\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let first = match compilation.declared_value_type_templates(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("recovered declared value types must publish: {error:?}"),
        };

        let second = match compilation.declared_value_type_templates(key) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated recovered request must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        assert!(
            first
                .value()
                .evidence()
                .iter()
                .any(|entry| matches!(entry.term(), DeclaredValueTypeTerm::Pattern(_)))
        );
    }

    #[test]
    fn expression_typing_and_call_selection_converge_into_cached_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = identity(1);\n",
            "}\n",
            "func identity(pos value: i64) -> i64\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation
                .state
                .checked_expression_types
                .is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation
                .state
                .checked_semantic_selections
                .is_published(&key),
            Ok(false)
        );

        let types = match compilation.checked_expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("expression types must publish: {error:?}"),
        };

        let selections = match compilation.checked_semantic_selections(key.clone()) {
            Ok(selections) => selections,
            Err(error) => panic!("semantic selections must publish: {error:?}"),
        };

        assert!(
            types.diagnostics().is_empty(),
            "expression typing must be diagnostic-free: {:?}",
            types.diagnostics()
        );

        assert!(
            selections.diagnostics().is_empty(),
            "semantic selection must be diagnostic-free: {:?}",
            selections.diagnostics()
        );

        let [selection] = selections.value().entries() else {
            panic!("direct call must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("direct call must publish a callable selection");
        };

        let [
            SelectedArgument::Explicit {
                expression: argument,
                ..
            },
        ] = call.arguments()
        else {
            panic!("direct call must retain one explicit argument");
        };

        let Some(call_type) = types.value().expression(selection.expression()) else {
            panic!("selected call must have a final type");
        };

        let Some(argument_type) = types.value().expression(*argument) else {
            panic!("selected argument must have a final type");
        };

        assert!(!call_type.is_recovered());
        assert_eq!(argument_type, call_type);

        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };

        let data = match values.type_data(call_type.ty()) {
            Ok(data) => data,
            Err(error) => panic!("selected result type must be available: {error:?}"),
        };

        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(definition),
            ..
        } = data.as_ref()
        else {
            panic!("selected result must be a named scalar type");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
            Some(RepresentationRole::ScalarI64)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(true)
        );

        let repeated_types = match compilation.checked_expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("repeated expression types must publish: {error:?}"),
        };

        let repeated_selections = match compilation.checked_semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("repeated semantic selections must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&types, &repeated_types));
        assert!(Arc::ptr_eq(&selections, &repeated_selections));
    }

    #[test]
    fn unsupported_generic_calls_defer_without_false_diagnostics() {
        // TODO(BRA-242): Replace this boundary test when generic call candidates are materialized.
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = identity<i64>(1);\n",
            "}\n",
            "func identity<T>(pos value: T) -> T\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        let types = match compilation.checked_expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("deferred expression types must publish: {error:?}"),
        };

        let selections = match compilation.checked_semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("deferred semantic selections must publish: {error:?}"),
        };

        assert!(
            types.diagnostics().is_empty(),
            "deferred expression typing must be diagnostic-free: {:?}",
            types.diagnostics()
        );

        assert!(types.value().is_recovered());
        assert!(selections.diagnostics().is_empty());
        assert!(selections.value().entries().is_empty());
    }

    fn facts_for_kind(
        compilation: &Compilation,
        keys: &[bray_bound_tree::BoundUnitKey],
        kind: BoundUnitKind,
    ) -> Arc<bray_diagnostics::DiagnosticResult<DeclaredValueTypeTemplates>> {
        let key = keys
            .iter()
            .find(|key| key.kind() == kind)
            .unwrap_or_else(|| panic!("test source must publish a {kind:?} unit"));

        match compilation.declared_value_type_templates(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("{kind:?} declared value types must publish: {error:?}"),
        }
    }

    fn has_constraint_kind(
        facts: &DeclaredValueTypeTemplates,
        kind: DeclaredValueTypeConstraintKind,
    ) -> bool {
        facts.constraints().iter().any(|entry| entry.kind() == kind)
    }

    fn has_value_kind(facts: &DeclaredValueTypeTemplates, kind: SymbolKind) -> bool {
        facts.evidence().iter().any(|entry| match entry.term() {
            DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(symbol)) => {
                symbol.kind() == kind
            }
            DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(symbol)) => {
                symbol.kind() == kind
            }
            DeclaredValueTypeTerm::Expression(_) | DeclaredValueTypeTerm::Pattern(_) => false,
        })
    }

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

        let declared_gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .declared_value_type_templates
            .set_test_observer(&key, declared_gate.observer())
        {
            panic!("declared value type fact must accept a test observer: {error:?}");
        }

        let declared = std::thread::scope(|scope| {
            let owner_key = key.clone();
            let owner = scope.spawn(|| compilation.declared_value_type_templates(owner_key));

            declared_gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter_key = key.clone();
            let waiter = scope.spawn(|| compilation.declared_value_type_templates(waiter_key));

            declared_gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            declared_gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(Ok(facts)) => facts,
                Ok(Err(error)) => panic!("concurrent declared value types failed: {error:?}"),
                Err(_) => panic!("concurrent declared value type request panicked"),
            })
        });

        assert!(Arc::ptr_eq(&declared[0], &declared[1]));

        let expression_gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .expression_semantics
            .set_test_observer(&key, expression_gate.observer())
        {
            panic!("expression semantic fact must accept a test observer: {error:?}");
        }

        let (types, selections) = std::thread::scope(|scope| {
            let types_key = key.clone();
            let types = scope.spawn(|| compilation.checked_expression_types(types_key));

            expression_gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let selections_key = key.clone();
            let selections =
                scope.spawn(|| compilation.checked_semantic_selections(selections_key));

            expression_gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            expression_gate.release();

            let types = match types.join() {
                Ok(Ok(types)) => types,
                Ok(Err(error)) => panic!("concurrent expression types failed: {error:?}"),
                Err(_) => panic!("concurrent expression type request panicked"),
            };

            let selections = match selections.join() {
                Ok(Ok(selections)) => selections,
                Ok(Err(error)) => panic!("concurrent semantic selections failed: {error:?}"),
                Err(_) => panic!("concurrent semantic selection request panicked"),
            };

            (types, selections)
        });

        assert_eq!(types.value().unit(), selections.value().unit());
        assert_eq!(types.value().kind(), selections.value().kind());
        assert_eq!(types.diagnostics(), selections.diagnostics());

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

        assert!(compilation.checked_control_flow(key.clone()).is_ok());

        let expression_cancellation = CancellationToken::new();
        let expression_gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .expression_semantics
            .set_test_observer(&key, expression_gate.observer())
        {
            panic!("expression semantic fact must accept a test observer: {error:?}");
        }

        let expression_semantics = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation
                    .expression_semantics_with_cancellation(request_key, &expression_cancellation)
            });

            expression_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            expression_cancellation.cancel();
            expression_gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled expression semantic request panicked"),
            }
        });

        assert!(matches!(
            expression_semantics,
            Err(FactQueryError::Cancelled)
        ));

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation
                .state
                .checked_expression_types
                .is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation
                .state
                .checked_semantic_selections
                .is_published(&key),
            Ok(false)
        );

        assert!(compilation.checked_expression_types(key).is_ok());
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
    fn control_flow_requests_do_not_force_nested_unit_control_flow() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda()\n",
            "    {\n",
            "    };\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("callable body must bind: {error:?}"),
        };

        let [nested] = bound.value().nested_units() else {
            panic!("test callable must contain one nested semantic unit");
        };

        assert_eq!(
            compilation.state.checked_control_flow.is_published(nested),
            Ok(false)
        );

        if let Err(error) = compilation.checked_control_flow(key) {
            panic!("parent control-flow facts must be available: {error:?}");
        }

        assert_eq!(
            compilation.state.checked_control_flow.is_published(nested),
            Ok(false)
        );
    }

    #[test]
    fn invalid_checker_unit_views_preserve_their_typed_infrastructure_error() {
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

        let canonical = match semantic_unit_context(context.symbols(), bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("semantic unit context must be available: {error:?}"),
        };

        let SemanticUnitContext::CallableBody(declaration) = canonical else {
            panic!("callable body must produce a callable-body checker entry");
        };

        let invalid = SemanticUnitContext::Constraint(declaration);
        let result = check_control_flow(bound.value(), &invalid, &context);

        assert!(matches!(
            result,
            Err(FactQueryError::CheckerInfrastructure(
                CheckerInfrastructureError::InvalidUnitView(
                    CheckerUnitViewError::SemanticContextMismatch
                )
            ))
        ));
    }

    #[test]
    fn invalid_semantic_unit_contexts_preserve_their_typed_cause() {
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
            semantic_unit_context_for(symbols, bound.value()),
            Err(FactQueryError::SemanticUnitContext(
                SemanticUnitContextError::MissingOwner
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
