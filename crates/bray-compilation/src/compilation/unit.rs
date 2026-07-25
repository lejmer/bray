use std::sync::Arc;

use bray_binder::{
    BinderDependency, BinderFactContext, BoundUnitBindingError, BoundUnitComputation,
    bind_anonymous_callable, bind_callable_body, bind_constant_template, bind_constraint,
    bind_contract_clause, bind_embedded_constant, bind_expression_candidates,
    bind_predicate_definition, bind_runtime_default, bind_target_gate, semantic_unit_context,
};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundUnit, BoundUnitKey, BoundUnitKind, BoundUnitRoot,
    BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, CheckedControlFlowFacts,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedPatternFacts,
    CheckedRefinementFacts, CheckedSemanticSelections, DeclaredValueTypeTemplates, LivenessFacts,
    StorageFlowFacts, StoragePlan, walk_bound_unit_view,
};
use bray_checker::{
    CheckerInfrastructureError, CheckerUnitView, ControlFlowChecker, DefaultControlFlowChecker,
    DefaultDependencyContractChecker, DefaultExpressionSemanticChecker, DefaultLivenessAnalyzer,
    DefaultPatternChecker, DefaultRefinementAnalyzer, DefaultStorageFlowChecker,
    DefaultStoragePlanner, DependencyContractChecker, ExpressionCandidateSet,
    ExpressionSemanticChecker, IterationPatternType, LivenessAnalyzer, NestedCallableEvidence,
    PatternCheckInput, PatternChecker, RefinementAnalyzer, SemanticUnitContext, StorageFlowChecker,
    StoragePlanner,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::SymbolGraph;

use super::binder::{CompilationBinderFacts, bind_declared_value_type_templates, type_scope};
use super::checker::{CompilationCheckerContext, checker_result};
use super::facts::{CheckedExpressionSemantics, Compilation};
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact, QueryPriority,
};

type ExpressionSemanticComputation = (
    DiagnosticResult<CheckedExpressionSemantics>,
    Box<[BinderDependency]>,
);

impl Compilation {
    pub(in crate::compilation) fn bound_source(
        &self,
        anchor: bray_declarations::SyntaxAnchor,
    ) -> Result<bray_bound_tree::BoundSourceAnchor, FactQueryError> {
        let source = self
            .source(anchor.source_id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok(bray_bound_tree::BoundSourceAnchor::new(
            anchor,
            source.version(),
        ))
    }

    /// Returns one bound semantic unit and its diagnostics.
    pub fn bound_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<BoundUnit>>, FactQueryError> {
        let published = self.bound_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns one bound semantic unit for a cancellable prioritized request.
    pub fn bound_unit_with_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<DiagnosticResult<BoundUnit>>, FactQueryError> {
        let published =
            self.bound_unit_with_cancellation_and_priority(key, cancellation, priority)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns the control-flow facts and diagnostics for one bound semantic unit.
    pub fn control_flow(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedControlFlowFacts>>, FactQueryError> {
        let published = self.control_flow_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns final expression types and their diagnostics for one bound semantic unit.
    pub fn expression_types(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedExpressionTypes>>, FactQueryError> {
        let published = self.expression_types_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns checked pattern and match-coverage facts for one bound semantic unit.
    pub fn pattern_facts(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedPatternFacts>>, FactQueryError> {
        let published = self.pattern_facts_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns exact semantic selections and their diagnostics for one bound semantic unit.
    pub fn semantic_selections(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedSemanticSelections>>, FactQueryError> {
        let published =
            self.semantic_selections_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns storage identities, evaluated access plans, and their diagnostics for one unit.
    pub fn storage_plan(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<StoragePlan>>, FactQueryError> {
        let published = self.storage_plan_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns durable last-use and lexical scope-boundary decisions for one unit.
    pub fn liveness(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<LivenessFacts>>, FactQueryError> {
        let published = self.liveness_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns flow-sensitive facts available at checked operation occurrences.
    pub fn refinement_facts(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedRefinementFacts>>, FactQueryError> {
        let published = self.refinement_facts_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns checked storage, ownership, movement, and borrow decisions for one unit.
    pub fn storage_flow_facts(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<StorageFlowFacts>>, FactQueryError> {
        let published = self.storage_flow_facts_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns normalized dependency contracts for semantic occurrences in one unit.
    pub fn dependency_contracts(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedDependencyContracts>>, FactQueryError> {
        let published =
            self.dependency_contracts_with_cancellation(key, &self.state.cancellation)?;

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

    pub(in crate::compilation) fn bound_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BoundUnit>>, FactQueryError> {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.bound_unit_with_cancellation_and_priority(key, cancellation, priority)
    }

    fn bound_unit_with_cancellation_and_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<PublishedUnitFact<BoundUnit>>, FactQueryError> {
        self.unit_fact_with_priority(
            &self.state.bound_units,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| {
                let facts = self.binder_facts_for(&key, cancellation)?;
                let unit = self.bound_unit_id(&key)?;

                bind_unit(&facts, unit, key)
                    .map(BoundUnitComputation::into_parts)
                    .map_err(map_binding_error)
            },
        )
    }

    pub(super) fn control_flow_with_cancellation(
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
        self.unit_fact(
            &self.state.expression_semantics,
            CompilationFactKey::ExpressionSemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let provisional = self.provisional_expression_semantics_with_cancellation(
                    key.clone(),
                    cancellation,
                )?;

                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let (pattern_input, iteration_sources, iteration_diagnostics, has_iterations) =
                    self.iteration_pattern_input(&key, bound.result().value(), cancellation)?;

                if !has_iterations {
                    return Ok((provisional.result().as_ref().clone(), Box::new([])));
                }

                let mut result = self.compute_expression_semantics(
                    &key,
                    cancellation,
                    &pattern_input,
                    &iteration_sources,
                )?;

                let (semantics, diagnostics) = result.0.into_parts();

                result.0 =
                    DiagnosticResult::new(semantics, diagnostics.merged(&iteration_diagnostics));

                Ok(result)
            },
        )
    }

    pub(in crate::compilation) fn provisional_expression_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedExpressionSemantics>>, FactQueryError> {
        self.unit_fact(
            &self.state.provisional_expression_semantics,
            CompilationFactKey::ProvisionalExpressionSemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                self.compute_expression_semantics(
                    &key,
                    cancellation,
                    &PatternCheckInput::new(),
                    &[],
                )
            },
        )
    }

    fn compute_expression_semantics(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
        pattern_input: &PatternCheckInput,
        iteration_sources: &[bray_bound_tree::SelectedIterationSource],
    ) -> Result<ExpressionSemanticComputation, FactQueryError> {
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let declared =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let supplemental = self.nested_callable_evidence(bound.result().value(), cancellation)?;

        let facts = self.binder_facts_for(key, cancellation)?;
        let candidates = expression_candidates(&facts, bound.result().value())?;

        let context = self.checker_context_for(key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let unit = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(
                    error,
                ))
            })?;

        let result = checker_result(DefaultExpressionSemanticChecker.check_expression_semantics(
            unit,
            declared.result().value(),
            &supplemental,
            candidates.value(),
            pattern_input,
            iteration_sources,
        ))?;

        let (value, diagnostics) = result.into_parts();
        let result = DiagnosticResult::new(value, candidates.diagnostics().merged(&diagnostics));

        Ok((result, Box::new([])))
    }

    fn nested_callable_evidence(
        &self,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<Vec<NestedCallableEvidence>, FactQueryError> {
        let mut evidence = Vec::new();
        let mut failure = None;

        let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
            if cancellation.is_cancelled() {
                failure = Some(FactQueryError::Cancelled);

                return BoundWalkControl::Stop;
            }

            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(bray_bound_tree::BoundExpression::AnonymousCallable(callable)) =
                bound.view().expression(expression)
            else {
                return BoundWalkControl::Continue;
            };

            // Each independently demandable nested fact owns its cache key.
            let nested_bound =
                match self.bound_unit_with_cancellation(callable.unit().clone(), cancellation) {
                    Ok(nested) => nested,
                    Err(error) => {
                        failure = Some(error);

                        return BoundWalkControl::Stop;
                    }
                };

            let BoundUnitRoot::AnonymousCallable {
                callable: callable_symbol,
                ..
            } = nested_bound.result().value().root()
            else {
                failure = Some(FactQueryError::InfrastructureFailure);

                return BoundWalkControl::Stop;
            };

            let nested = match self.declared_value_type_templates_with_cancellation(
                callable.unit().clone(),
                cancellation,
            ) {
                Ok(nested) => nested,
                Err(error) => {
                    failure = Some(error);

                    return BoundWalkControl::Stop;
                }
            };

            let Some(callable_type) = nested.result().value().callable_type() else {
                failure = Some(FactQueryError::InfrastructureFailure);

                return BoundWalkControl::Stop;
            };

            // The outer expression fact owns this template after the nested fact handle drops.
            evidence.push(NestedCallableEvidence::new(
                expression,
                callable_symbol,
                callable_type.clone(),
            ));

            BoundWalkControl::Continue
        });

        if let Some(error) = failure {
            return Err(error);
        }

        match outcome {
            BoundWalkOutcome::Completed => Ok(evidence),
            BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
                Err(FactQueryError::InfrastructureFailure)
            }
        }
    }

    pub(in crate::compilation) fn expression_types_with_cancellation(
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

    pub(in crate::compilation) fn pattern_facts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedPatternFacts>>, FactQueryError> {
        // Cache identity, unit publication, and dependent queries retain the shared key separately.
        self.unit_fact(
            &self.state.checked_patterns,
            CompilationFactKey::CheckedPatterns(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let types = self.expression_types_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let (input, _, iteration_diagnostics, _) =
                    self.iteration_pattern_input(&key, bound.result().value(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .map_err(|error| {
                            FactQueryError::CheckerInfrastructure(
                                CheckerInfrastructureError::InvalidUnitView(error),
                            )
                        })?;

                let (input, constant_diagnostics) = self.add_constant_pattern_evidence(
                    unit,
                    types.result().value(),
                    selections.result().value(),
                    input,
                    cancellation,
                )?;

                let result = check_patterns(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    types.result().value(),
                    &input,
                )?;

                let (patterns, pattern_diagnostics) = result.into_parts();
                let diagnostics = DiagnosticBag::merged_all([
                    &iteration_diagnostics,
                    &constant_diagnostics,
                    &pattern_diagnostics,
                ]);

                Ok((DiagnosticResult::new(patterns, diagnostics), Box::new([])))
            },
        )
    }

    fn iteration_pattern_input(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            PatternCheckInput,
            Vec<bray_bound_tree::SelectedIterationSource>,
            DiagnosticBag,
            bool,
        ),
        FactQueryError,
    > {
        let mut iterations = Vec::new();

        let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
            if cancellation.is_cancelled() {
                return BoundWalkControl::Stop;
            }

            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(expression) = bound.view().expression(id) else {
                return BoundWalkControl::Stop;
            };

            let pattern = match expression {
                BoundExpression::For(expression) => Some(expression.pattern()),
                BoundExpression::Generator(expression) => Some(expression.pattern()),
                _ => None,
            };

            if let Some(pattern) = pattern {
                iterations.push((id, pattern));
            }

            BoundWalkControl::Continue
        });

        if cancellation.is_cancelled() {
            return Err(FactQueryError::Cancelled);
        }

        if outcome != BoundWalkOutcome::Completed {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let error_type = self
            .semantic_value_store()?
            .intern_type(bray_symbols::TypeData::Error)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mut inputs = Vec::with_capacity(iterations.len());
        let mut sources = Vec::with_capacity(iterations.len());
        let mut diagnostics = DiagnosticBag::new();

        for (expression, pattern) in iterations {
            // Each demand-driven selection query owns its cheaply shared unit key.
            let selection =
                self.iteration_source_with_cancellation(key.clone(), expression, cancellation)?;

            diagnostics = diagnostics.merged(selection.diagnostics());

            match selection.value() {
                Some(selection) => {
                    inputs.push(IterationPatternType::new(
                        pattern,
                        selection.element_type(),
                        false,
                    ));
                    sources.push(selection.clone());
                }
                None => inputs.push(IterationPatternType::new(pattern, error_type, true)),
            }
        }

        let has_iterations = !inputs.is_empty();

        Ok((
            PatternCheckInput::new().with_iteration_patterns(inputs),
            sources,
            diagnostics,
            has_iterations,
        ))
    }

    fn semantic_selections_with_cancellation(
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

    pub(in crate::compilation) fn storage_plan_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<StoragePlan>>, FactQueryError> {
        self.unit_fact(
            &self.state.storage_plans,
            CompilationFactKey::StoragePlan(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;
                let types = self.expression_types_with_cancellation(key.clone(), cancellation)?;
                let patterns = self.pattern_facts_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let (_, iterations, iteration_diagnostics, _) =
                    self.iteration_pattern_input(&key, bound.result().value(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = plan_storage(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    declared.result().value(),
                    types.result().value(),
                    patterns.result().value(),
                    selections.result().value(),
                    &iterations,
                )?;

                let (plan, plan_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    declared.result().diagnostics(),
                    types.result().diagnostics(),
                    patterns.result().diagnostics(),
                    selections.result().diagnostics(),
                    &iteration_diagnostics,
                    &plan_diagnostics,
                ]);

                Ok((DiagnosticResult::new(plan, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn liveness_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<LivenessFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.liveness,
            CompilationFactKey::Liveness(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = analyze_liveness(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    storage.result().value(),
                )?;

                let (facts, liveness_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    storage.result().diagnostics(),
                    &liveness_diagnostics,
                ]);

                Ok((DiagnosticResult::new(facts, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn refinement_facts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedRefinementFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.refinement_facts,
            CompilationFactKey::RefinementFacts(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let patterns = self.pattern_facts_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = analyze_refinements(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    patterns.result().value(),
                    storage.result().value(),
                )?;

                let (facts, refinement_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    patterns.result().diagnostics(),
                    storage.result().diagnostics(),
                    &refinement_diagnostics,
                ]);

                Ok((DiagnosticResult::new(facts, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn storage_flow_facts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<StorageFlowFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.storage_flow_facts,
            CompilationFactKey::StorageFlowFacts(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;

                let refinements =
                    self.refinement_facts_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .map_err(|error| {
                            FactQueryError::CheckerInfrastructure(
                                CheckerInfrastructureError::InvalidUnitView(error),
                            )
                        })?;

                let result = checker_result(DefaultStorageFlowChecker.check_storage_flow(
                    unit,
                    storage.result().value(),
                    liveness.result().value(),
                    refinements.result().value(),
                ))?;

                let (facts, flow_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    storage.result().diagnostics(),
                    liveness.result().diagnostics(),
                    refinements.result().diagnostics(),
                    &flow_diagnostics,
                ]);

                Ok((DiagnosticResult::new(facts, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn dependency_contracts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedDependencyContracts>>, FactQueryError> {
        self.unit_fact(
            &self.state.dependency_contracts,
            CompilationFactKey::DependencyContracts(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;

                let flow = self.storage_flow_facts_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .map_err(|error| {
                            FactQueryError::CheckerInfrastructure(
                                CheckerInfrastructureError::InvalidUnitView(error),
                            )
                        })?;

                let result =
                    checker_result(DefaultDependencyContractChecker.check_dependency_contracts(
                        unit,
                        selections.result().value(),
                        storage.result().value(),
                        flow.result().value(),
                    ))?;

                let (contracts, dependency_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    selections.result().diagnostics(),
                    storage.result().diagnostics(),
                    flow.result().diagnostics(),
                    &dependency_diagnostics,
                ]);

                Ok((DiagnosticResult::new(contracts, diagnostics), Box::new([])))
            },
        )
    }

    pub(super) fn declared_value_type_templates_with_cancellation(
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
                    .map_err(super::binder::binder_fact_error)?;

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
        BoundUnitKind::EmbeddedConstant => bind_embedded_constant(facts, unit, key)?.finish(),
        BoundUnitKind::PredicateDefinition => bind_predicate_definition(facts, unit, key)?.finish(),
        BoundUnitKind::Constraint => bind_constraint(facts, unit, key)?.finish(),
        BoundUnitKind::ContractClause => bind_contract_clause(facts, unit, key)?.finish(),
        BoundUnitKind::TargetGate => bind_target_gate(facts, unit, key)?.finish(),
    }
}

pub(in crate::compilation) fn semantic_unit_context_for(
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

    let result = checker_result(DefaultControlFlowChecker.check_control_flow(unit))?
        .map(|result| result.into_facts());

    Ok((result, Box::new([])))
}

fn expression_candidates(
    facts: &CompilationBinderFacts<'_>,
    bound: &BoundUnit,
) -> Result<DiagnosticResult<Vec<ExpressionCandidateSet>>, FactQueryError> {
    let owner = facts
        .symbols()
        .symbol_for_key(bound.key().declared_owner())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let type_scope = type_scope(facts, owner).map_err(super::binder::binder_fact_error)?;

    let mut candidates = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        match bind_expression_candidates(facts, bound, expression, &type_scope) {
            Ok(result) => {
                let (candidate, candidate_diagnostics) = result.into_parts();

                if !matches!(candidate, ExpressionCandidateSet::NotApplicable(_)) {
                    candidates.push(candidate);
                }

                diagnostics.add_range(candidate_diagnostics);
            }
            Err(error) => {
                failure = Some(error);

                return BoundWalkControl::Stop;
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = failure {
        return Err(super::binder::binder_fact_error(error));
    }

    match outcome {
        BoundWalkOutcome::Completed => Ok(DiagnosticResult::new(candidates, diagnostics)),
        BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
            Err(FactQueryError::InfrastructureFailure)
        }
    }
}

fn check_patterns(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> Result<DiagnosticResult<CheckedPatternFacts>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultPatternChecker.check_patterns(unit, types, input))
}

#[expect(
    clippy::too_many_arguments,
    reason = "storage planning consumes each independently demandable prerequisite directly"
)]
fn plan_storage(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    declared_types: &DeclaredValueTypeTemplates,
    types: &CheckedExpressionTypes,
    patterns: &CheckedPatternFacts,
    selections: &CheckedSemanticSelections,
    iterations: &[bray_bound_tree::SelectedIterationSource],
) -> Result<DiagnosticResult<StoragePlan>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultStoragePlanner.plan_storage(
        unit,
        declared_types,
        types,
        patterns,
        selections,
        iterations,
    ))
}

fn analyze_liveness(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    storage: &StoragePlan,
) -> Result<DiagnosticResult<LivenessFacts>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultLivenessAnalyzer.analyze_liveness(unit, storage))
}

fn analyze_refinements(
    bound: &BoundUnit,
    semantic_context: &SemanticUnitContext,
    context: &CompilationCheckerContext<'_>,
    patterns: &CheckedPatternFacts,
    storage: &StoragePlan,
) -> Result<DiagnosticResult<CheckedRefinementFacts>, FactQueryError> {
    let unit = CheckerUnitView::new(bound, semantic_context, context).map_err(|error| {
        FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(error))
    })?;

    checker_result(DefaultRefinementAnalyzer.analyze_refinements(unit, patterns, storage))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{SemanticUnitContextError, semantic_unit_context};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundCallResult, BoundCallableTarget, BoundDependencySubject,
        BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitKind,
        BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes, DeclaredValueTypeConstraintKind,
        DeclaredValueTypeTemplates, DeclaredValueTypeTerm, PatternOperation, PatternPredicate,
        PatternProjection, RefinementFactKind, SelectedArgument, SemanticSelection,
        StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget,
        StorageIdentity, StorageProjection, walk_bound_unit_view,
    };
    use bray_checker::{CheckerInfrastructureError, CheckerUnitViewError, SemanticUnitContext};
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{
        NamedTypeSymbolId, SymbolKind, SymbolOrdinal, TypeData, TypeExpressionTemplate,
    };

    use super::{Compilation, check_control_flow, semantic_unit_context_for};
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError, QueryPriority};
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget,
        source_callable_body_key,
    };

    #[test]
    fn storage_plans_publish_unit_local_identities_accesses_and_dependencies() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "    y: i32;\n",
            "}\n",
            "func main(input: Point, items: [i32; 4]) -> i32\n",
            "{\n",
            "    let mut value: i32 = input.x;\n",
            "    let { x, y }: Point = input;\n",
            "    let shared = &value;\n",
            "    let exclusive = & mut value;\n",
            "    let first = items[0];\n",
            "    let middle = items[1..3];\n",
            "    value = x;\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.storage_plans.is_published(&key),
            Ok(false)
        );

        let first = match compilation.storage_plan(key.clone()) {
            Ok(plan) => plan,
            Err(error) => panic!("storage planning must publish: {error:?}"),
        };

        assert!(
            first
                .value()
                .identities()
                .iter()
                .any(|identity| { matches!(identity, StorageIdentity::Parameter(_)) })
        );

        assert!(
            first
                .value()
                .identities()
                .iter()
                .any(|identity| { matches!(identity, StorageIdentity::LocalOwned(_)) })
        );

        assert!(
            first
                .value()
                .identities()
                .iter()
                .any(|identity| { matches!(identity, StorageIdentity::Result(_)) })
        );

        assert!(
            first
                .value()
                .identities()
                .iter()
                .any(|identity| { matches!(identity, StorageIdentity::Temporary(_)) })
        );

        assert!(
            first
                .value()
                .bindings()
                .iter()
                .any(|(target, _)| { matches!(target, StorageBindingTarget::Parameter(_)) })
        );

        assert!(
            first
                .value()
                .bindings()
                .iter()
                .any(|(target, _)| { matches!(target, StorageBindingTarget::Local(_)) })
        );

        assert!(
            first
                .value()
                .bindings()
                .iter()
                .any(|(target, _)| { matches!(target, StorageBindingTarget::Result) })
        );

        assert!(first.value().accesses().iter().any(|access| {
            access
                .projections()
                .iter()
                .any(|projection| matches!(projection, StorageProjection::ProductField(_)))
        }));

        assert!(!first.value().borrow_capabilities().is_empty());

        assert!(
            first
                .value()
                .accesses()
                .iter()
                .any(|access| { matches!(access.root(), StorageAccessRoot::Borrow(_)) })
        );

        for purpose in [
            StorageAccessPurpose::Read,
            StorageAccessPurpose::Initialize,
            StorageAccessPurpose::Write,
            StorageAccessPurpose::ValueTransfer,
            StorageAccessPurpose::Assignment,
            StorageAccessPurpose::Member,
            StorageAccessPurpose::Index,
            StorageAccessPurpose::Slice,
            StorageAccessPurpose::Projection,
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Shared),
            StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable),
        ] {
            assert!(
                first
                    .value()
                    .access_plans()
                    .iter()
                    .any(|plan| plan.purpose() == purpose),
                "storage plan must retain {purpose:?}"
            );
        }

        let second = match compilation.storage_plan(key.clone()) {
            Ok(plan) => plan,
            Err(error) => panic!("repeated storage planning must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::StoragePlan(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published storage plans must retain dependencies"),
            Err(error) => panic!("storage-plan dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::CheckedExpressionTypes(
                key.clone()
            ))
        );

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::CheckedPatterns(
                key.clone()
            ))
        );

        assert!(dependencies.contains(
            &crate::fact::CompilationFactKey::CheckedSemanticSelections(key)
        ));
    }

    #[test]
    fn liveness_is_demanded_independently_and_reuses_its_publication() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    let result: i32 = value;\n",
            "    return result;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(compilation.state.liveness.is_published(&key), Ok(false));

        let first = match compilation.liveness(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("liveness analysis must publish: {error:?}"),
        };

        assert!(!first.value().last_uses().is_empty());

        assert!(
            first
                .value()
                .last_uses()
                .iter()
                .all(|last_use| last_use.operation().unit() == first.value().unit())
        );

        let second = match compilation.liveness(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated liveness analysis must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::Liveness(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published liveness must retain dependencies"),
            Err(error) => panic!("liveness dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));
        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key)));
    }

    #[test]
    fn dependency_contracts_are_demanded_independently_and_reuse_their_publication() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.dependency_contracts.is_published(&key),
            Ok(false)
        );

        let first = match compilation.dependency_contracts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("dependency contracts must publish: {error:?}"),
        };

        let second = match compilation.dependency_contracts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated dependency contracts must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        let dependencies = match compilation.state.fact_runtime.dependencies(
            &crate::fact::CompilationFactKey::DependencyContracts(key.clone()),
        ) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published dependency contracts must retain dependencies"),
            Err(error) => panic!("dependency contract dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(
            &crate::fact::CompilationFactKey::CheckedSemanticSelections(key.clone())
        ));

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StorageFlowFacts(key)));
    }

    #[test]
    fn storage_flow_is_lazy_and_reports_overlapping_borrows() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let mut value: i32 = 1;\n",
            "    let shared: &i32 = &value;\n",
            "    let exclusive: & mut i32 = & mut value;\n",
            "    shared;\n",
            "    exclusive;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.storage_flow_facts.is_published(&key),
            Ok(false)
        );

        let facts = match compilation.storage_flow_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            facts.value().operations().iter().any(|operation| {
                operation.status() == bray_bound_tree::StorageOperationStatus::ConflictingBorrow
            }),
            "{facts:?}"
        );

        assert!(
            facts
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingConflictingBorrow)
                .next()
                .is_some()
        );

        let repeated = match compilation.storage_flow_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated storage-flow checking must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&facts, &repeated));

        let dependencies = match compilation.state.fact_runtime.dependencies(
            &crate::fact::CompilationFactKey::StorageFlowFacts(key.clone()),
        ) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published storage flow must retain dependencies"),
            Err(error) => panic!("storage-flow dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::Liveness(key.clone())));

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::RefinementFacts(
                key.clone()
            ))
        );
    }

    #[test]
    fn storage_flow_rejects_mutation_without_parameter_authority() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32)\n",
            "{\n",
            "    value = 2;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(facts.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }));

        assert!(
            facts
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingMissingMutationAuthority)
                .next()
                .is_some()
        );
    }

    #[test]
    fn storage_flow_rejects_mutable_borrow_without_parameter_authority() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32)\n",
            "{\n",
            "    let borrowed: & mut i32 = & mut value;\n",
            "    borrowed;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(facts.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }));

        assert!(
            facts
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingMissingMutationAuthority)
                .next()
                .is_some()
        );
    }

    #[test]
    fn storage_flow_accepts_mutation_through_mutable_borrow_parameter() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: & mut i32)\n",
            "{\n",
            "    value = 2;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            facts.value().operations().iter().all(|operation| {
                !matches!(
                    operation.status(),
                    bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
                        | bray_bound_tree::StorageOperationStatus::ConflictingBorrow
                )
            }),
            "{facts:?}"
        );
    }

    #[test]
    fn storage_flow_rejects_mutation_through_shared_borrow_parameter() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: &i32)\n",
            "{\n",
            "    value = 2;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(facts.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }));
    }

    #[test]
    fn storage_flow_rejects_use_after_move() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "func main(resource: Resource)\n",
            "{\n",
            "    let moved: Resource = resource;\n",
            "    resource;\n",
            "    moved;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            facts
                .value()
                .operations()
                .iter()
                .any(|operation| operation.status()
                    == bray_bound_tree::StorageOperationStatus::Moved),
            "{facts:?}"
        );

        assert!(
            facts
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage)
                .next()
                .is_some()
        );

        assert!(
            facts
                .value()
                .exits()
                .iter()
                .any(|exit| !exit.moved().is_empty())
        );
    }

    #[test]
    fn storage_flow_allows_assignment_to_reinitialize_moved_storage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let mut resource: Resource = Resource { value = 1 };\n",
            "    let moved: Resource = resource;\n",
            "    resource = Resource { value = 2 };\n",
            "    resource;\n",
            "    moved;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            facts.value().operations().iter().all(|operation| {
                !matches!(
                    operation.status(),
                    bray_bound_tree::StorageOperationStatus::Uninitialized
                        | bray_bound_tree::StorageOperationStatus::Moved
                )
            }),
            "{facts:?}"
        );
    }

    #[test]
    fn storage_flow_copies_copyable_value_transfers() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    let copied: i32 = value;\n",
            "    value;\n",
            "    return copied;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.storage_flow_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            facts
                .value()
                .operations()
                .iter()
                .any(|operation| operation.purpose() == StorageAccessPurpose::Copy),
            "{facts:?}"
        );

        assert!(
            facts
                .value()
                .operations()
                .iter()
                .all(|operation| operation.status()
                    != bray_bound_tree::StorageOperationStatus::Moved),
            "{facts:?}"
        );
    }

    #[test]
    fn liveness_converges_conservatively_across_branches_and_loops() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos condition: bool, pos value: i32) -> i32\n",
            "{\n",
            "    let selected: i32 = if condition\n",
            "    {\n",
            "        value\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        value\n",
            "    };\n",
            "    loop\n",
            "    {\n",
            "        if condition\n",
            "        {\n",
            "            break;\n",
            "        };\n",
            "        value;\n",
            "    };\n",
            "    return selected;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.liveness(key) {
            Ok(facts) => facts,
            Err(error) => panic!("cyclic liveness analysis must converge: {error:?}"),
        };

        assert!(!facts.value().last_uses().is_empty());
    }

    #[test]
    fn refinement_facts_are_lazy_cached_and_retain_branch_conditions() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos condition: bool)\n",
            "{\n",
            "    if condition\n",
            "    {\n",
            "        condition;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        condition;\n",
            "    };\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.refinement_facts.is_published(&key),
            Ok(false)
        );

        let first = match compilation.refinement_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("refinement analysis must publish: {error:?}"),
        };

        assert!(
            first.value().occurrences().iter().any(|occurrence| {
                occurrence.facts().iter().any(|fact| {
                    matches!(
                        fact.kind(),
                        RefinementFactKind::Condition { value: true, .. }
                    )
                })
            }),
            "{first:?}"
        );

        let second = match compilation.refinement_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated refinement analysis must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));

        let dependencies = match compilation.state.fact_runtime.dependencies(
            &crate::fact::CompilationFactKey::RefinementFacts(key.clone()),
        ) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published refinements must retain dependencies"),
            Err(error) => panic!("refinement dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));
        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::CheckedPatterns(
                key.clone()
            ))
        );
        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key)));
    }

    #[test]
    fn liveness_retains_storage_required_beyond_nested_scope_exits() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    {\n",
            "        value;\n",
            "    };\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.liveness(key) {
            Ok(facts) => facts,
            Err(error) => panic!("scope liveness analysis must publish: {error:?}"),
        };

        assert!(
            facts
                .value()
                .live_across_scopes()
                .iter()
                .any(|entry| { matches!(entry.subject(), BoundDependencySubject::Storage(_)) })
        );
    }

    #[test]
    fn storage_plans_retain_coherent_alternative_bindings_conservatively() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First(value: i32);\n",
            "    Second(value: i32);\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case First(value = item) | Second(value = item)\n",
            "        {\n",
            "            item;\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let plan = match compilation.storage_plan(key) {
            Ok(plan) => plan,
            Err(error) => panic!("alternative-pattern storage planning must publish: {error:?}"),
        };

        let Some((_, StorageBinding::Identity(storage))) = plan
            .value()
            .bindings()
            .iter()
            .find(|(target, _)| matches!(target, StorageBindingTarget::Local(_)))
        else {
            panic!("coherent alternatives must retain one logical local binding");
        };

        assert!(matches!(
            plan.value().identity(*storage),
            Some(StorageIdentity::Error(_))
        ));

        assert!(plan.value().accesses().iter().any(|access| {
            matches!(access.root(), StorageAccessRoot::Recovery(root) if root == *storage)
        }));
    }

    #[test]
    fn storage_plans_retain_recovery_without_panicking() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32)\n",
            "{\n",
            "    let broken: i32 = ;\n",
            "    value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let plan = match compilation.storage_plan(key.clone()) {
            Ok(plan) => plan,
            Err(error) => panic!("recovered storage planning must publish: {error:?}"),
        };

        assert!(
            plan.value()
                .accesses()
                .iter()
                .any(bray_bound_tree::StorageAccess::is_recovered)
        );

        let liveness = match compilation.liveness(key) {
            Ok(liveness) => liveness,
            Err(error) => panic!("recovered liveness analysis must publish: {error:?}"),
        };

        assert!(liveness.value().is_recovered());
    }

    #[test]
    fn storage_plans_cover_receiver_predicate_and_anonymous_parameters() {
        let compilation = compilation(concat!(
            "module app;\n",
            "predicate accepts(value: i32) = true;\n",
            "struct Counter\n",
            "{\n",
            "    func read() -> i32\n",
            "    {\n",
            "        return 0;\n",
            "    }\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let callable = lambda(value: i32)\n",
            "    {\n",
            "        value;\n",
            "    };\n",
            "}\n",
        ));

        let keys = match compilation.declared_unit_keys_for_test() {
            Ok(keys) => keys,
            Err(error) => panic!("declared unit keys must be available: {error:?}"),
        };

        let receiver = keys
            .iter()
            .filter(|key| key.kind() == BoundUnitKind::CallableBody)
            .filter_map(|key| compilation.storage_plan(key.clone()).ok())
            .find(|plan| {
                plan.value()
                    .identities()
                    .iter()
                    .any(|identity| matches!(identity, StorageIdentity::Receiver(_)))
            })
            .unwrap_or_else(|| panic!("type callable storage must retain its receiver"));

        assert!(receiver.diagnostics().is_empty());

        let predicate_key = keys
            .iter()
            .find(|key| key.kind() == BoundUnitKind::PredicateDefinition)
            .unwrap_or_else(|| panic!("predicate definition key must be available"));

        let predicate = match compilation.storage_plan(predicate_key.clone()) {
            Ok(plan) => plan,
            Err(error) => panic!("predicate storage planning must publish: {error:?}"),
        };

        assert!(
            predicate
                .value()
                .identities()
                .iter()
                .any(|identity| matches!(identity, StorageIdentity::PredicateParameter(_)))
        );

        let main = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(main) {
            Ok(bound) => bound,
            Err(error) => panic!("source callable must bind: {error:?}"),
        };

        let [nested] = bound.value().nested_units() else {
            panic!("source callable must contain one anonymous callable");
        };

        let anonymous = match compilation.storage_plan(nested.clone()) {
            Ok(plan) => plan,
            Err(error) => panic!("anonymous callable storage planning must publish: {error:?}"),
        };

        assert!(
            anonymous
                .value()
                .identities()
                .iter()
                .any(|identity| matches!(identity, StorageIdentity::AnonymousParameter(_)))
        );
    }

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

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key.clone()) {
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

        assert_expression_representation(
            &compilation,
            types.value(),
            selection.expression(),
            RepresentationRole::ScalarI64,
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(true)
        );

        let repeated_types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("repeated expression types must publish: {error:?}"),
        };

        let repeated_selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("repeated semantic selections must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&types, &repeated_types));
        assert!(Arc::ptr_eq(&selections, &repeated_selections));
    }

    #[test]
    fn overload_narrowing_provides_context_before_literal_defaults() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = choose(true, 1);\n",
            "}\n",
            "func choose_wide(pos key: bool, pos value: i64) -> i64\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "func choose_default(pos key: i32, pos value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "overload choose = {choose_wide, choose_default}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("semantic selections must publish: {error:?}"),
        };

        assert!(
            types.diagnostics().is_empty(),
            "overload typing must be diagnostic-free: {:?}",
            types.diagnostics()
        );

        assert!(
            selections.diagnostics().is_empty(),
            "overload selection must be diagnostic-free: {:?}",
            selections.diagnostics()
        );

        let [selection] = selections.value().entries() else {
            panic!("overload call must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("overload call must publish a callable selection");
        };

        let [_, SelectedArgument::Explicit { expression, .. }] = call.arguments() else {
            panic!("overload call must retain both explicit arguments");
        };

        assert_expression_representation(
            &compilation,
            types.value(),
            *expression,
            RepresentationRole::ScalarI64,
        );
    }

    #[test]
    fn named_arguments_share_selection_mapping_with_type_inference() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = select(second = 1, first = true);\n",
            "}\n",
            "func select(pos first: bool, pos second: i64) -> i64\n",
            "{\n",
            "    return second;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("named-argument expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("named-argument selection must publish: {error:?}"),
        };

        let [selection] = selections.value().entries() else {
            panic!("named call must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("named call must publish a callable selection");
        };

        let [
            SelectedArgument::Explicit {
                expression: second,
                ordinal: 1,
                ..
            },
            SelectedArgument::Explicit { ordinal: 0, .. },
        ] = call.arguments()
        else {
            panic!("named call must preserve source order and declaration ordinals");
        };

        assert_expression_representation(
            &compilation,
            types.value(),
            *second,
            RepresentationRole::ScalarI64,
        );
    }

    #[test]
    fn declared_array_types_resolve_their_embedded_constant_lengths() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos source: [i32; 4])\n",
            "{\n",
            "    let copy: [i32; 4] = source;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key) {
            Ok(types) => types,
            Err(error) => panic!("deferred expression types must publish: {error:?}"),
        };

        assert!(
            types.diagnostics().is_empty(),
            "checked declared types must not produce derived diagnostics: {:?}",
            types.diagnostics()
        );

        assert!(!types.value().is_recovered());
    }

    #[test]
    fn explicit_generic_calls_publish_specialized_types_and_selections() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = convert<Item, Item>(0);\n",
            "}\n",
            "func convert<T, U>(pos unused: i32) -> T\n",
            "{\n",
            "}\n",
            "struct Item\n",
            "{\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("generic expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("generic semantic selections must publish: {error:?}"),
        };

        assert!(
            types.diagnostics().is_empty(),
            "generic expression typing must be diagnostic-free: {:?}",
            types.diagnostics()
        );

        assert!(!types.value().is_recovered());
        assert!(selections.diagnostics().is_empty());

        let [selection] = selections.value().entries() else {
            panic!("generic call must publish one semantic selection");
        };

        assert!(matches!(selection.selection(), SemanticSelection::Call(_)));

        let Some(result) = types.value().expression(selection.expression()) else {
            panic!("generic call must have a final type");
        };

        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };

        let result = match values.type_data(result.ty()) {
            Ok(result) => result,
            Err(error) => panic!("generic result type must be available: {error:?}"),
        };

        assert!(matches!(
            result.as_ref(),
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(_),
                ..
            }
        ));
    }

    #[test]
    fn asynchronous_calls_publish_target_specific_future_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let pending = produce();\n",
            "}\n",
            "async func produce() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("asynchronous expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("asynchronous call selection must publish: {error:?}"),
        };

        let [selection] = selections.value().entries() else {
            panic!("asynchronous call must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("asynchronous invocation must publish a call selection");
        };

        let BoundCallResult::LazyFuture(future) = call.resolution().result() else {
            panic!("asynchronous invocation must construct a lazy future");
        };

        let Some(result) = types.value().expression(selection.expression()) else {
            panic!("asynchronous call must have a final type");
        };

        assert_eq!(result.ty(), future.future_type());

        assert_expression_representation(
            &compilation,
            types.value(),
            selection.expression(),
            RepresentationRole::Future,
        );
    }

    #[test]
    fn callable_values_are_classified_from_converged_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let operation: func(pos value: i32) -> i32 = identity;\n",
            "    let result = operation(1);\n",
            "}\n",
            "func identity(pos value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("callable-value expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("callable-value selection must publish: {error:?}"),
        };

        let [selection] = selections.value().entries() else {
            panic!("callable-value invocation must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("callable value must publish a call selection");
        };

        assert!(matches!(call.target(), BoundCallableTarget::Indirect(_)));

        assert!(!types.value().is_recovered());
    }

    #[test]
    fn direct_lambda_calls_use_nested_callable_types_and_identities() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = lambda(pos value: i32) -> i32\n",
            "    {\n",
            "        return value;\n",
            "    }(1);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let types = match compilation.expression_types(key.clone()) {
            Ok(types) => types,
            Err(error) => panic!("lambda expression types must publish: {error:?}"),
        };

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("lambda call selection must publish: {error:?}"),
        };

        assert!(!types.value().is_recovered());

        let [selection] = selections.value().entries() else {
            panic!("direct lambda call must publish one semantic selection");
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("direct lambda invocation must publish a call selection");
        };

        assert!(matches!(call.target(), BoundCallableTarget::Anonymous(_)));
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

    fn assert_expression_representation(
        compilation: &Compilation,
        types: &CheckedExpressionTypes,
        expression: BoundExpressionId,
        expected: RepresentationRole,
    ) {
        let Some(result) = types.expression(expression) else {
            panic!("selected expression must have a final type");
        };

        assert_type_representation(compilation, result.ty(), expected);
    }

    fn assert_type_representation(
        compilation: &Compilation,
        ty: bray_symbols::TypeId,
        expected: RepresentationRole,
    ) {
        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };

        let data = match values.type_data(ty) {
            Ok(data) => data,
            Err(error) => panic!("type must be available: {error:?}"),
        };

        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(definition),
            ..
        } = data.as_ref()
        else {
            panic!("type must use a named scalar representation");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
            Some(expected)
        );
    }

    fn first_pattern_reference(bound: &BoundUnit) -> Option<BoundExpressionId> {
        first_expression(bound, |expression| {
            matches!(expression, BoundExpression::PatternReference(_))
        })
    }

    fn first_expression(
        bound: &BoundUnit,
        mut matches: impl FnMut(&BoundExpression) -> bool,
    ) -> Option<BoundExpressionId> {
        let mut reference = None;

        walk_bound_unit_view(bound.view(), bound.root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            if bound
                .view()
                .expression(expression)
                .is_some_and(&mut matches)
            {
                reference = Some(expression);

                return BoundWalkControl::Stop;
            }

            BoundWalkControl::Continue
        });

        reference
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
        let worker_budget = crate::WorkerBudget::new(2)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"));

        let compilation = compilation_with_sources_and_worker_budget(
            &[r#"module app;
func main()
{
}
"#],
            worker_budget,
        );

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
            let types = scope.spawn(|| compilation.expression_types(types_key));

            expression_gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let selections_key = key.clone();

            let selections = scope.spawn(|| compilation.semantic_selections(selections_key));

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
            let owner = scope.spawn(move || compilation.control_flow(owner_key));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter_key = key.clone();
            let waiter = scope.spawn(move || compilation.control_flow(waiter_key));

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
                compilation.bound_unit_with_priority(
                    request_key,
                    &bound_cancellation,
                    QueryPriority::Interactive,
                )
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
                compilation.control_flow_with_cancellation(request_key, &checked_cancellation)
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

        assert!(compilation.control_flow(key.clone()).is_ok());

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

        assert!(compilation.expression_types(key).is_ok());
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

        let checked = match compilation.control_flow(key) {
            Ok(checked) => checked,
            Err(error) => panic!("recursive callable must check without a cycle: {error:?}"),
        };

        assert!(checked.diagnostics().is_empty());
    }

    #[test]
    fn unit_fact_requests_do_not_force_nested_units() {
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

        assert_eq!(
            compilation.state.storage_plans.is_published(nested),
            Ok(false)
        );

        if let Err(error) = compilation.control_flow(key.clone()) {
            panic!("parent control-flow facts must be available: {error:?}");
        }

        assert_eq!(
            compilation.state.checked_control_flow.is_published(nested),
            Ok(false)
        );

        if let Err(error) = compilation.storage_plan(key) {
            panic!("parent storage plan must be available: {error:?}");
        }

        assert_eq!(
            compilation.state.storage_plans.is_published(nested),
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

    #[test]
    fn pattern_facts_publish_exhaustive_boolean_match_coverage() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case true\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case false\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.checked_patterns.is_published(&key),
            Ok(false)
        );

        let facts = match compilation.pattern_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        assert_eq!(
            compilation.state.checked_patterns.is_published(&key),
            Ok(true)
        );

        let repeated = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated pattern facts must be available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&facts, &repeated));

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(coverage.unreachable_arms().is_empty());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());

        let literal_patterns = facts
            .value()
            .patterns()
            .iter()
            .filter(|pattern| matches!(pattern.test(), Some(PatternPredicate::Literal(_))))
            .collect::<Vec<_>>();

        assert_eq!(literal_patterns.len(), 2);

        assert!(literal_patterns.iter().all(|pattern| {
            pattern.operation() == PatternOperation::Observe
                && matches!(pattern.refinement(), Some(PatternPredicate::Literal(_)))
        }));
    }

    #[test]
    fn pattern_facts_use_constant_paths_for_boolean_coverage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const yes: bool = true;\n",
            "const no: bool = false;\n",
            "func main(value: bool)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case yes\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case no\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("constant-backed pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(coverage.unreachable_arms().is_empty());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());

        assert_eq!(
            facts
                .value()
                .patterns()
                .iter()
                .filter(|pattern| matches!(pattern.test(), Some(PatternPredicate::Constant(_))))
                .count(),
            2
        );
    }

    #[test]
    fn pattern_facts_report_subsumed_constant_alternatives() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const yes: bool = true;\n",
            "func main(value: bool)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case yes | true\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case false\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("constant alternative facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingUnreachablePatternAlternative]
        );
    }

    #[test]
    fn pattern_facts_use_constant_guard_truth_for_coverage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const enabled: bool = true;\n",
            "const disabled: bool = false;\n",
            "func main(value: bool)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case true when disabled\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case true when enabled\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case false\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("constant guard facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert_eq!(coverage.unreachable_arms(), &[0]);

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingUnreachableMatchArm]
        );
    }

    #[test]
    fn pattern_facts_evaluate_closed_local_constant_patterns() {
        let compilation = pattern_compilation(concat!(
            "    const base: bool = true;\n",
            "    const yes: bool = base;\n",
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case yes\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case false\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("local constant pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_reject_constant_paths_with_incompatible_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const one: i32 = 1;\n",
            "func main(value: bool)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case one\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("incompatible constant pattern must recover: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );

        assert!(facts.value().is_recovered());
    }

    #[test]
    fn pattern_facts_report_non_exhaustive_matches() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case true\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingNonExhaustiveMatch]
        );
    }

    #[test]
    fn pattern_facts_report_unreachable_match_arms() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case true\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case true\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case false\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert_eq!(coverage.unreachable_arms(), &[1]);

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingUnreachableMatchArm]
        );
    }

    #[test]
    fn pattern_facts_report_patterns_incompatible_with_the_subject() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case none\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn pattern_facts_reject_refutable_declaration_patterns() {
        let compilation = pattern_compilation("    let true: bool = true;\n");
        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingRefutablePattern]
        );
    }

    #[test]
    fn pattern_facts_publish_exhaustive_nullable_match_coverage() {
        let compilation = pattern_compilation(concat!(
            "    let value: i32? = none;\n",
            "    match value\n",
            "    {\n",
            "        case ?present\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case none\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive());
        assert!(coverage.unreachable_arms().is_empty());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_compose_nested_nullable_coverage() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool? = none;\n",
            "    match value\n",
            "    {\n",
            "        case none\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case ?true\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case ?false\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("nullable pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_publish_exhaustive_closed_union_coverage() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "    Second;\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case .First\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case .Second\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(coverage.unreachable_arms().is_empty());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_resolve_bare_variants_through_the_expected_subject_type() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case First\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bare variant must bind: {error:?}"),
        };

        assert!(bound.value().local_symbols().bindings().is_empty());
        assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("bare variant pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(facts.value().binding_types().is_empty());
        assert!(!facts.value().is_recovered());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn late_typed_match_subjects_resolve_contextual_variants() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    match make_choice()\n",
            "    {\n",
            "        case First\n",
            "        {\n",
            "            First;\n",
            "        }\n",
            "    }\n",
            "}\n",
            "func make_choice() -> Choice\n",
            "{\n",
            "    return .First;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("late subject pattern must bind: {error:?}"),
        };

        let Some(reference) = first_pattern_reference(bound.value()) else {
            panic!("variant arm body must retain a deferred pattern reference");
        };

        let selections = match compilation.semantic_selections(key.clone()) {
            Ok(selections) => selections,
            Err(error) => panic!("late subject selections must be available: {error:?}"),
        };

        assert_eq!(selections.value().expression(reference), None);

        assert!(
            crate::test_support::diagnostic_kinds(selections.diagnostics())
                .contains(&bray_diagnostics::DiagnosticKind::BindingUnresolvedName)
        );

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("late subject pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(facts.value().binding_types().is_empty());
        assert!(!facts.value().is_recovered());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn nested_variant_patterns_resolve_against_the_nested_subject() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Inner\n",
            "{\n",
            "    First;\n",
            "}\n",
            "union Outer\n",
            "{\n",
            "    Wrap(value: Inner);\n",
            "}\n",
            "func main(value: Outer)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case Wrap(value = First)\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("nested pattern facts must be available: {error:?}"),
        };

        assert!(facts.value().binding_types().is_empty());
        assert!(!facts.value().is_recovered());
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn late_typed_bare_bindings_become_visible_after_pattern_resolution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    match make_value()\n",
            "    {\n",
            "        case captured\n",
            "        {\n",
            "            captured;\n",
            "        }\n",
            "    };\n",
            "}\n",
            "func make_value() -> bool\n",
            "{\n",
            "    return true;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("late binding pattern must bind: {error:?}"),
        };

        let Some(binding) = bound
            .value()
            .local_symbols()
            .bindings()
            .iter()
            .find(|binding| binding.name().as_str() == "captured")
            .map(bray_symbols::LocalBindingSymbol::id)
        else {
            panic!("late binding pattern must retain its candidate identity");
        };

        let Some(reference) = first_pattern_reference(bound.value()) else {
            panic!("arm body must retain a deferred pattern reference");
        };

        let selections = match compilation.semantic_selections(key.clone()) {
            Ok(selections) => selections,
            Err(error) => panic!("late binding selections must be available: {error:?}"),
        };

        assert_eq!(
            selections.value().expression(reference),
            Some(&SemanticSelection::Reference(BoundReferenceTarget::Local(
                binding.into()
            )))
        );

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("late binding pattern facts must be available: {error:?}"),
        };

        let Some(binding_type) = facts.value().binding_type(binding) else {
            panic!("resolved binding must publish its checked type");
        };

        assert_type_representation(
            &compilation,
            binding_type.ty(),
            RepresentationRole::ScalarBool,
        );

        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn nearer_locals_shadow_contextual_pattern_candidates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    selected;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    match make_choice()\n",
            "    {\n",
            "        case selected\n",
            "        {\n",
            "            let selected: bool = true;\n",
            "            selected;\n",
            "        }\n",
            "    };\n",
            "}\n",
            "func make_choice() -> Choice\n",
            "{\n",
            "    return .selected;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("shadowed contextual pattern must bind: {error:?}"),
        };

        assert_eq!(first_pattern_reference(bound.value()), None);
        assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

        let types = match compilation.expression_types(key) {
            Ok(types) => types,
            Err(error) => panic!("shadowed contextual pattern types must be available: {error:?}"),
        };

        let Some(reference) = first_expression(bound.value(), |expression| {
            matches!(
                expression,
                BoundExpression::Name(name)
                    if matches!(name.target(), BoundReferenceTarget::Local(_))
            )
        }) else {
            panic!("shadowing local reference must remain bound");
        };

        let Some(result) = types.value().expression(reference) else {
            panic!("shadowing local reference must have a final type");
        };

        assert_type_representation(&compilation, result.ty(), RepresentationRole::ScalarBool);

        assert!(
            !crate::test_support::diagnostic_kinds(types.diagnostics())
                .contains(&bray_diagnostics::DiagnosticKind::BindingUnresolvedName),
            "{:?}",
            types.diagnostics()
        );
    }

    #[test]
    fn pattern_facts_introduce_bare_bindings_after_pattern_name_resolution_fails() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case captured\n",
            "        {\n",
            "            captured;\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bare binding pattern must bind: {error:?}"),
        };

        let Some(captured) = bound
            .value()
            .local_symbols()
            .bindings()
            .iter()
            .find(|binding| binding.name().as_str() == "captured")
            .map(bray_symbols::LocalBindingSymbol::id)
        else {
            panic!("bare pattern must introduce the captured local");
        };

        assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("bare binding pattern facts must be available: {error:?}"),
        };

        let Some(binding) = facts
            .value()
            .binding_types()
            .iter()
            .copied()
            .find(|binding| binding.binding() == captured)
        else {
            panic!("bare binding pattern must publish the captured binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn bare_variants_never_activate_provisional_bindings_for_guards_or_bodies() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case First when First\n",
            "        {\n",
            "            First;\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("bare variant references must recover: {error:?}"),
        };

        assert!(bound.value().local_symbols().bindings().is_empty());

        let mut unresolved = 0;

        walk_bound_unit_view(bound.value().view(), bound.value().root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            if matches!(
                bound.value().view().expression(expression),
                Some(BoundExpression::UnresolvedReference(_))
            ) {
                unresolved += 1;
            }

            BoundWalkControl::Continue
        });

        assert_eq!(unresolved, 2);
    }

    #[test]
    fn pattern_facts_do_not_treat_bare_variant_alternatives_as_bindings() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "    Second;\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case First | Second\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bare variant alternatives must bind: {error:?}"),
        };

        assert!(bound.value().local_symbols().bindings().is_empty());
        assert!(bound.diagnostics().is_empty(), "{:?}", bound.diagnostics());

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("bare variant alternative facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{facts:?}");
        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_binding_reports_ambiguous_lexical_and_subject_candidates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const First: bool = true;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "}\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case First\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("ambiguous pattern must recover: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(bound.diagnostics()),
            [bray_diagnostics::DiagnosticKind::BindingAmbiguousName]
        );

        assert!(bound.value().local_symbols().bindings().is_empty());
    }

    #[test]
    fn pattern_facts_report_arms_after_a_catch_all_as_unreachable() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case _\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case true\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("pattern facts must be available: {error:?}"),
        };

        let [coverage] = facts.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive());
        assert_eq!(coverage.unreachable_arms(), &[1]);
    }

    #[test]
    fn pattern_facts_check_fixed_array_shape_before_proving_irrefutability() {
        let valid = pattern_compilation("    let [first, .., last]: [i32; 3] = [1, 2, 3];\n");
        let invalid = pattern_compilation("    let [first]: [i32; 2] = [1, 2];\n");

        let valid_key = source_callable_body_key(&valid);
        let invalid_key = source_callable_body_key(&invalid);

        let valid_facts = match valid.pattern_facts(valid_key) {
            Ok(facts) => facts,
            Err(error) => panic!("valid array-pattern facts must be available: {error:?}"),
        };

        let invalid_facts = match invalid.pattern_facts(invalid_key) {
            Ok(facts) => facts,
            Err(error) => panic!("invalid array-pattern facts must be available: {error:?}"),
        };

        assert!(
            valid_facts.diagnostics().is_empty(),
            "{:?}",
            valid_facts.diagnostics()
        );

        let [first, last] = valid_facts.value().binding_types() else {
            panic!("array pattern must publish its two binding types");
        };

        assert_eq!(
            first.projection(),
            Some(PatternProjection::ElementFromStart(SymbolOrdinal::new(0)))
        );

        assert_eq!(
            last.projection(),
            Some(PatternProjection::ElementFromEnd(SymbolOrdinal::new(0)))
        );

        assert!(!valid_facts.value().is_recovered());

        assert_eq!(
            crate::test_support::diagnostic_kinds(invalid_facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn pattern_facts_check_product_field_coverage() {
        let valid = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "    y: i32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let { x, y }: Point = Point { x = 1, y = 2 };\n",
            "}\n",
        ));

        let invalid = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "    y: i32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let { x }: Point = Point { x = 1, y = 2 };\n",
            "}\n",
        ));

        let valid_key = source_callable_body_key(&valid);
        let invalid_key = source_callable_body_key(&invalid);

        let valid_facts = match valid.pattern_facts(valid_key) {
            Ok(facts) => facts,
            Err(error) => panic!("valid product-pattern facts must be available: {error:?}"),
        };

        let invalid_facts = match invalid.pattern_facts(invalid_key) {
            Ok(facts) => facts,
            Err(error) => panic!("invalid product-pattern facts must be available: {error:?}"),
        };

        assert!(
            valid_facts.diagnostics().is_empty(),
            "{:?}",
            valid_facts.diagnostics()
        );

        assert_eq!(
            crate::test_support::diagnostic_kinds(invalid_facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn pattern_facts_check_and_project_generic_product_fields() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "func main(input: Wrapper<bool>)\n",
            "{\n",
            "    let { value }: Wrapper<bool> = input;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("generic product-pattern facts must be available: {error:?}"),
        };

        let [binding] = facts.value().binding_types() else {
            panic!("field shorthand must publish one binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert_eq!(binding.operation(), PatternOperation::Consume);

        assert!(matches!(
            binding.projection(),
            Some(PatternProjection::ProductField(_))
        ));

        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_check_and_project_generic_union_payload_fields() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Maybe<T>\n",
            "{\n",
            "    Some(value: T);\n",
            "    None;\n",
            "}\n",
            "func main(input: Maybe<bool>)\n",
            "{\n",
            "    match input\n",
            "    {\n",
            "        case Some(value = value)\n",
            "        {\n",
            "        }\n",
            "\n",
            "        case .None\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("generic payload-pattern facts must be available: {error:?}"),
        };

        let [binding] = facts.value().binding_types() else {
            panic!("payload pattern must publish one binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert!(matches!(
            binding.projection(),
            Some(PatternProjection::ActiveUnionPayloadField { .. })
        ));

        assert!(facts.diagnostics().is_empty(), "{:?}", facts.diagnostics());
    }

    #[test]
    fn pattern_facts_do_not_treat_unknown_named_payload_fields_as_positional() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Maybe<T>\n",
            "{\n",
            "    Some(value: T);\n",
            "}\n",
            "func main(input: Maybe<bool>)\n",
            "{\n",
            "    match input\n",
            "    {\n",
            "        case .Some(other = value)\n",
            "        {\n",
            "        }\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("invalid payload-pattern facts must recover: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn pattern_facts_check_nested_product_patterns_against_field_types() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "    y: i32;\n",
            "}\n",
            "func main(input: Point)\n",
            "{\n",
            "    let { x = none, y }: Point = input;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("nested product-pattern facts must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(facts.diagnostics()),
            [bray_diagnostics::DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn pattern_facts_retain_consuming_match_operations() {
        let compilation = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match consume value\n",
            "    {\n",
            "        case _\n",
            "        {\n",
            "        }\n",
            "    }\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.pattern_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("consuming pattern facts must be available: {error:?}"),
        };

        assert!(
            facts
                .value()
                .patterns()
                .iter()
                .all(|pattern| pattern.operation() == PatternOperation::Consume)
        );
    }

    fn pattern_compilation(body: &str) -> Compilation {
        compilation(&format!("module app;\nfunc main()\n{{\n{body}}}\n",))
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
