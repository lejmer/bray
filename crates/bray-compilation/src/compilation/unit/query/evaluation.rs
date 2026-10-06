use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_binder::{BinderDependency, BoundUnitComputation};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundUnit, BoundUnitKey, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, CheckedControlFlow, CheckedExpressionSemantics, CheckedMemoryOperations,
    CheckedPatterns, CheckedSemanticSelections, DeclaredValueTypeTemplates, StoragePlan,
    walk_bound_unit_view,
};
use bray_checker::{
    CheckerInfrastructureError, DefaultExpressionSemanticChecker, ExpressionSemanticChecker,
    IterationPatternType, PatternCheckInput,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::super::support::{
    bind_unit, check_control_flow, check_patterns, expression_candidates, map_binding_error,
    unit_walk_failure,
};
use super::super::view::{
    AsyncAnalysisView, DependencyContractsView, ExpressionTypesView, LiteralValuesView,
    LivenessView, RefinementsView, SemanticSelectionsView, StorageFlowView,
};
use crate::compilation::binder::{bind_declared_value_type_templates, binding_query_error};
use crate::compilation::checker::checker_result;
use crate::compilation::operation::{operation_expressions, operation_type_input};
use crate::compilation::state::Compilation;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult, QueryPriority,
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
        let source = self.source(anchor.source_id()).ok_or_else(|| {
            crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Source(anchor.source_id()),
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::SourceSnapshot,
                ),
            )
        })?;

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

    /// Returns one root and its transitively nested bound units in deterministic preorder.
    pub(in crate::compilation) fn bound_unit_family_with_cancellation(
        &self,
        root: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Arc<DiagnosticResult<BoundUnit>>>, FactQueryError> {
        let mut family = Vec::new();
        let mut pending = vec![root];

        while let Some(key) = pending.pop() {
            cancellation.check()?;

            let bound = self.bound_unit_with_cancellation(key, cancellation)?;

            pending.extend(bound.result().value().nested_units().iter().rev().cloned());

            // The family shares each immutable published result independently of its cache cell.
            family.push(Arc::clone(bound.result()));
        }

        Ok(family)
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

    /// Returns the control-flow analysis and diagnostics for one bound semantic unit.
    pub fn control_flow(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedControlFlow>>, FactQueryError> {
        let published = self.control_flow_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns final expression types and their diagnostics for one bound semantic unit.
    pub fn expression_types(
        &self,
        key: BoundUnitKey,
    ) -> Result<ExpressionTypesView, FactQueryError> {
        let published =
            self.expression_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(ExpressionTypesView::new(Arc::clone(published.result())))
    }

    /// Returns final source-literal values and their diagnostics for one bound semantic unit.
    pub fn literal_values(&self, key: BoundUnitKey) -> Result<LiteralValuesView, FactQueryError> {
        let published =
            self.expression_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(LiteralValuesView::new(Arc::clone(published.result())))
    }

    /// Returns checked pattern and match-coverage analysis for one bound semantic unit.
    pub fn patterns(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedPatterns>>, FactQueryError> {
        let published = self.patterns_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns exact semantic selections and their diagnostics for one bound semantic unit.
    pub fn semantic_selections(
        &self,
        key: BoundUnitKey,
    ) -> Result<SemanticSelectionsView, FactQueryError> {
        let published =
            self.expression_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(SemanticSelectionsView::new(Arc::clone(published.result())))
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
    pub fn liveness(&self, key: BoundUnitKey) -> Result<LivenessView, FactQueryError> {
        let published = self.body_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(LivenessView::new(Arc::clone(published.result())))
    }

    /// Returns flow-sensitive analysis available at checked operation occurrences.
    pub fn refinements(&self, key: BoundUnitKey) -> Result<RefinementsView, FactQueryError> {
        let published = self.body_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(RefinementsView::new(Arc::clone(published.result())))
    }

    /// Returns checked storage, ownership, movement, and borrow decisions for one unit.
    pub fn storage_flow(&self, key: BoundUnitKey) -> Result<StorageFlowView, FactQueryError> {
        let published = self.body_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(StorageFlowView::new(Arc::clone(published.result())))
    }

    /// Returns normalized dependency contracts for semantic occurrences in one unit.
    pub fn dependency_contracts(
        &self,
        key: BoundUnitKey,
    ) -> Result<DependencyContractsView, FactQueryError> {
        let published = self.body_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(DependencyContractsView::new(Arc::clone(published.result())))
    }

    /// Returns compiler-provided memory operations selected for one bound semantic unit.
    pub fn memory_operations(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedMemoryOperations>>, FactQueryError> {
        let published = self.memory_operations_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns async frame, suspension, task, and cleanup analysis for one unit.
    pub fn async_analysis(&self, key: BoundUnitKey) -> Result<AsyncAnalysisView, FactQueryError> {
        let published = self.body_semantics_with_cancellation(key, &self.state.cancellation)?;

        Ok(AsyncAnalysisView::new(Arc::clone(published.result())))
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
    ) -> Result<Arc<PublishedUnitResult<BoundUnit>>, FactQueryError> {
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
    ) -> Result<Arc<PublishedUnitResult<BoundUnit>>, FactQueryError> {
        self.unit_query_with_priority(
            &self.state.bound_units,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| {
                let binding_context = self.binding_context_for(&key, cancellation)?;
                let unit = self.bound_unit_id(&key)?;

                bind_unit(&binding_context, unit, key)
                    .map(BoundUnitComputation::into_parts)
                    .map_err(map_binding_error)
            },
        )
    }

    pub(in crate::compilation) fn control_flow_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedControlFlow>>, FactQueryError> {
        self.unit_query(
            &self.state.checked_control_flow,
            CompilationFactKey::CheckedControlFlow(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                check_control_flow(bound.result().value(), &semantic_context, &context)
            },
        )
    }

    pub(in crate::compilation) fn expression_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedExpressionSemantics>>, FactQueryError> {
        self.unit_query(
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
                    self.iteration_inputs(&key, bound.result().value(), cancellation)?;

                let binding_context = self.binding_context_for(&key, cancellation)?;

                let operations =
                    operation_expressions(&binding_context, bound.result().value(), cancellation)?;

                if !has_iterations && operations.is_empty() {
                    return Ok((provisional.result().as_ref().clone(), Box::new([])));
                }

                let mut operation_resolutions = Vec::new();

                loop {
                    let operation_input = operation_type_input(&operation_resolutions)
                        .with_iteration_sources(iteration_sources.iter().cloned());

                    let mut result = self.compute_expression_semantics(
                        &key,
                        cancellation,
                        &pattern_input,
                        &operation_input,
                    )?;

                    let (next, operation_diagnostics) = self.operation_inputs(
                        &key,
                        bound.result().value(),
                        result.0.value(),
                        &operations,
                        &operation_resolutions,
                        cancellation,
                    )?;

                    if next == operation_resolutions {
                        let (semantics, diagnostics) = result.0.into_parts();

                        result.0 = DiagnosticResult::new(
                            semantics,
                            DiagnosticBag::merged_all([
                                &diagnostics,
                                &iteration_diagnostics,
                                &operation_diagnostics,
                            ]),
                        );

                        return Ok(result);
                    }

                    operation_resolutions = next;
                }
            },
        )
    }

    pub(in crate::compilation) fn provisional_expression_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedExpressionSemantics>>, FactQueryError> {
        self.unit_query(
            &self.state.provisional_expression_semantics,
            CompilationFactKey::ProvisionalExpressionSemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                self.compute_expression_semantics(
                    &key,
                    cancellation,
                    &PatternCheckInput::new(),
                    &bray_checker::ExpressionTypeInput::new(),
                )
            },
        )
    }

    fn compute_expression_semantics(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
        pattern_input: &PatternCheckInput,
        operation_input: &bray_checker::ExpressionTypeInput,
    ) -> Result<ExpressionSemanticComputation, FactQueryError> {
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let declared =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let supplemental = self.nested_callable_evidence(bound.result().value(), cancellation)?;

        let binding_context = self.binding_context_for(key, cancellation)?;
        let candidates = expression_candidates(&binding_context, bound.result().value())?;

        let context = self.checker_context_for(key, cancellation)?;

        let semantic_context = semantic_unit_context(context.symbols(), bound.result().value());

        let unit =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context);

        let result = checker_result(DefaultExpressionSemanticChecker.check_expression_semantics(
            unit,
            declared.result().value(),
            &supplemental,
            candidates.value(),
            pattern_input,
            operation_input,
        ))?;

        let ((types, selections, literals), diagnostics) = result.into_parts();

        let (reference_selections, reference_diagnostics) = self.named_reference_selections(
            key,
            cancellation,
            bound.result().value(),
            &types,
            &semantic_context,
            &context,
        )?;

        let selections = CheckedSemanticSelections::try_new(
            bound.result().value(),
            &types,
            selections
                .entries()
                .iter()
                .cloned()
                .chain(reference_selections),
        )
        .map_err(|error| {
            FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::SemanticSelection(
                error,
            ))
        })?;

        let value =
            CheckedExpressionSemantics::try_new(types, selections, literals).map_err(|error| {
                FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::SemanticSnapshot(
                    error,
                ))
            })?;

        let diagnostics = DiagnosticBag::merged_all([
            candidates.diagnostics(),
            &diagnostics,
            &reference_diagnostics,
        ]);

        let result = DiagnosticResult::new(value, diagnostics);

        Ok((result, Box::new([])))
    }

    pub(in crate::compilation) fn patterns_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedPatterns>>, FactQueryError> {
        // Cache identity, unit publication, and dependent queries retain the shared key separately.
        self.unit_query(
            &self.state.checked_patterns,
            CompilationFactKey::CheckedPatterns(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                // Each cached query owns its key while this branch retains it for later queries.
                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                let (input, _, iteration_diagnostics, _) =
                    self.iteration_inputs(&key, bound.result().value(), cancellation)?;

                let input = input.with_declared_pattern_types(declared.result().value());

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let unit = bray_checker::CheckerUnitView::new(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                );

                let (input, constant_diagnostics) = self.add_constant_pattern_evidence(
                    unit,
                    expressions.result().value().types(),
                    expressions.result().value().selections(),
                    input,
                    cancellation,
                )?;

                let result = check_patterns(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    expressions.result().value().types(),
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

    fn iteration_inputs(
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
        let mut cancellation_failure = None;
        let mut missing_expression = None;

        let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
            if let Err(error) = cancellation.check() {
                cancellation_failure = Some(error);

                return BoundWalkControl::Stop;
            }

            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(expression) = bound.view().expression(id) else {
                missing_expression = Some(id);

                return BoundWalkControl::Stop;
            };

            if expression.iteration_source().is_none() {
                return BoundWalkControl::Continue;
            }

            let pattern = match expression {
                BoundExpression::For(expression) => Some(expression.pattern()),
                BoundExpression::Generator(expression) => Some(expression.pattern()),
                _ => None,
            };

            iterations.push((id, pattern));

            BoundWalkControl::Continue
        });

        if let Some(error) = cancellation_failure {
            return Err(error);
        }

        if let Some(expression) = missing_expression {
            return Err(super::super::support::unit_contract_failure(
                key,
                crate::compilation::SemanticQueryViolation::MissingBoundNode(expression.into()),
            ));
        }

        if outcome != BoundWalkOutcome::Completed {
            return Err(unit_walk_failure(key, outcome));
        }

        let error_type = self
            .semantic_value_store()?
            .intern_type(bray_symbols::TypeData::Error)
            .map_err(FactQueryError::SemanticValueStore)?;

        let has_iterations = !iterations.is_empty();
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
                    if let Some(pattern) = pattern {
                        inputs.push(IterationPatternType::new(
                            pattern,
                            selection.element_type(),
                            false,
                        ));
                    }

                    sources.push(selection.clone());
                }
                None => {
                    if let Some(pattern) = pattern {
                        inputs.push(IterationPatternType::new(pattern, error_type, true));
                    }
                }
            }
        }

        Ok((
            PatternCheckInput::new().with_iteration_patterns(inputs),
            sources,
            diagnostics,
            has_iterations,
        ))
    }

    pub(in crate::compilation) fn declared_value_type_templates_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<DeclaredValueTypeTemplates>>, FactQueryError> {
        self.unit_query(
            &self.state.declared_value_type_templates,
            CompilationFactKey::DeclaredValueTypeTemplates(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let binding_context = self.binding_context_for(&key, cancellation)?;

                let result =
                    bind_declared_value_type_templates(&binding_context, bound.result().value())
                        .map_err(binding_query_error)?;

                Ok((result, Box::new([])))
            },
        )
    }
}
