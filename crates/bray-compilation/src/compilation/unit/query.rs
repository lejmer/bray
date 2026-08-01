use std::sync::Arc;

use bray_binder::{BinderDependency, BoundUnitComputation};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundUnit, BoundUnitKey, BoundUnitRoot, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, CheckedAsyncFacts, CheckedControlFlowFacts,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues,
    CheckedMemoryOperations, CheckedPatternFacts, CheckedRefinementFacts,
    CheckedSemanticSelections, DeclaredValueTypeTemplates, LivenessFacts, StorageFlowFacts,
    StoragePlan, walk_bound_unit_view,
};
use bray_checker::{
    AsyncChecker, CheckerInfrastructureError, CheckerUnitView, DefaultAsyncChecker,
    DefaultExpressionSemanticChecker, ExpressionSemanticChecker, IterationPatternType,
    NestedCallableEvidence, PatternCheckInput,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::{
    bind_unit, check_control_flow, check_patterns, expression_candidates, map_binding_error,
    semantic_unit_context_for,
};
use crate::compilation::binder::{bind_declared_value_type_templates, binder_fact_error};
use crate::compilation::checker::checker_result;
use crate::compilation::facts::{CheckedExpressionSemantics, Compilation};
use crate::compilation::operation::operation_type_input;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact, QueryPriority,
    UnitFactCache,
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

    /// Returns final source-literal values and their diagnostics for one bound semantic unit.
    pub fn literal_values(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedLiteralValues>>, FactQueryError> {
        let published = self.literal_values_with_cancellation(key, &self.state.cancellation)?;

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

    /// Returns compiler-provided memory operations selected for one bound semantic unit.
    pub fn memory_operations(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedMemoryOperations>>, FactQueryError> {
        let published = self.memory_operations_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns async frame, suspension, task, and cleanup facts for one unit.
    pub fn async_facts(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedAsyncFacts>>, FactQueryError> {
        let published = self.async_facts_with_cancellation(key, &self.state.cancellation)?;

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

    pub(in crate::compilation) fn control_flow_with_cancellation(
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

    pub(in crate::compilation) fn expression_semantics_with_cancellation(
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
                    self.iteration_inputs(&key, bound.result().value(), cancellation)?;

                let (operation_resolutions, operation_diagnostics, has_operations) =
                    self.operation_inputs(&key, bound.result().value(), cancellation)?;

                if !has_iterations && !has_operations {
                    return Ok((provisional.result().as_ref().clone(), Box::new([])));
                }

                let operation_input = operation_type_input(&operation_resolutions)
                    .with_iteration_sources(iteration_sources.iter().cloned());

                let mut result = self.compute_expression_semantics(
                    &key,
                    cancellation,
                    &pattern_input,
                    &operation_input,
                )?;

                let (semantics, diagnostics) = result.0.into_parts();

                result.0 = DiagnosticResult::new(
                    semantics,
                    DiagnosticBag::merged_all([
                        &diagnostics,
                        &iteration_diagnostics,
                        &operation_diagnostics,
                    ]),
                );

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
            operation_input,
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

            let Some(BoundExpression::AnonymousCallable(callable)) =
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
        self.expression_semantic_projection(
            &self.state.checked_expression_types,
            CompilationFactKey::CheckedExpressionTypes,
            key,
            cancellation,
            |semantics| &semantics.0,
        )
    }

    pub(in crate::compilation) fn literal_values_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedLiteralValues>>, FactQueryError> {
        self.expression_semantic_projection(
            &self.state.checked_literal_values,
            CompilationFactKey::CheckedLiteralValues,
            key,
            cancellation,
            |semantics| &semantics.2,
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
                    self.iteration_inputs(&key, bound.result().value(), cancellation)?;

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

    pub(in crate::compilation) fn semantic_selections_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedSemanticSelections>>, FactQueryError> {
        self.expression_semantic_projection(
            &self.state.checked_semantic_selections,
            CompilationFactKey::CheckedSemanticSelections,
            key,
            cancellation,
            |semantics| &semantics.1,
        )
    }

    fn expression_semantic_projection<T>(
        &self,
        cache: &UnitFactCache<T>,
        fact_key: fn(BoundUnitKey) -> CompilationFactKey,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        project: fn(&CheckedExpressionSemantics) -> &T,
    ) -> Result<Arc<PublishedUnitFact<T>>, FactQueryError>
    where
        T: Clone + Send + Sync,
    {
        // Cache identity, unit publication, and the atomic computation retain the shared key.
        self.unit_fact(
            cache,
            fact_key(key.clone()),
            key.clone(),
            cancellation,
            |_| {
                let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

                // The projection owns its immutable table after the atomic fact handle drops.
                let value = project(semantics.result().value()).clone();
                let diagnostics = semantics.result().diagnostics().clone();

                Ok((DiagnosticResult::new(value, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn async_facts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedAsyncFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.async_facts,
            CompilationFactKey::AsyncFacts(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let types = self.expression_types_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;

                let dependencies =
                    self.dependency_contracts_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;

                let refinements =
                    self.refinement_facts_with_cancellation(key.clone(), cancellation)?;

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

                let result = checker_result(DefaultAsyncChecker.check_async_facts(
                    unit,
                    types.result().value(),
                    selections.result().value(),
                    liveness.result().value(),
                    dependencies.result().value(),
                    storage.result().value(),
                    refinements.result().value(),
                    flow.result().value(),
                ))?;

                let (facts, async_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    types.result().diagnostics(),
                    selections.result().diagnostics(),
                    liveness.result().diagnostics(),
                    dependencies.result().diagnostics(),
                    storage.result().diagnostics(),
                    refinements.result().diagnostics(),
                    flow.result().diagnostics(),
                    &async_diagnostics,
                ]);

                Ok((DiagnosticResult::new(facts, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn declared_value_type_templates_with_cancellation(
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
                    .map_err(binder_fact_error)?;

                Ok((result, Box::new([])))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{SemanticUnitContextError, semantic_unit_context};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundCallResult, BoundCallableTarget, BoundDependencySubject,
        BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitKind,
        BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes, CheckedMemoryOperationKind,
        ConstructionTarget, ConversionTarget, DeclaredValueTypeConstraintKind,
        DeclaredValueTypeTemplates, DeclaredValueTypeTerm, IndexTarget, PatternOperation,
        PatternPredicate, PatternProjection, RefinementFactKind, SelectedArgument,
        SelectedOperation, SemanticSelection, StorageAccessPurpose, StorageAccessRoot,
        StorageBinding, StorageBindingTarget, StorageIdentity, StorageProjection,
        walk_bound_unit_view,
    };
    use bray_checker::{CheckerInfrastructureError, CheckerUnitViewError, SemanticUnitContext};
    use bray_compiler_known::{ImplementationHook, RepresentationRole};
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        ConstantValueKind, NamedTypeSymbolId, SymbolKind, SymbolOrdinal, TypeData,
        TypeExpressionTemplate,
    };

    use super::{Compilation, check_control_flow, semantic_unit_context_for};
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError, QueryPriority};
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget,
        compilation_with_target_operations, source_callable_body_key,
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
    fn literal_values_are_adapted_once_to_final_types_and_selected_target() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let fixed: i8 = 42;\n",
            "    let target_sized: usize = 42;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.checked_literal_values.is_published(&key),
            Ok(false)
        );

        let values = match compilation.literal_values(key.clone()) {
            Ok(values) => values,
            Err(error) => panic!("literal values must publish: {error:?}"),
        };

        assert_eq!(
            compilation.state.checked_literal_values.is_published(&key),
            Ok(true)
        );

        assert_eq!(values.value().entries().len(), 2);

        assert_eq!(
            values.value().target_integer_width_bits(),
            compilation.selected_target().target().integer_width_bits()
        );

        let semantic_values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must publish: {error:?}"),
        };

        for entry in values.value().entries() {
            let value = semantic_values
                .constant_value_data(entry.value())
                .unwrap_or_else(|error| panic!("literal value must resolve: {error:?}"));

            assert!(matches!(value.kind(), ConstantValueKind::Integer(_)));
        }

        assert!(values.diagnostics().is_empty());
    }

    #[test]
    fn unrepresentable_literals_publish_recovery_values_and_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let value: i8 = 128;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let values = match compilation.literal_values(key) {
            Ok(values) => values,
            Err(error) => panic!("invalid literal values must recover: {error:?}"),
        };

        assert_eq!(
            values
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingConstantLiteralNotRepresentable)
                .count(),
            1
        );

        let [entry] = values.value().entries() else {
            panic!("source must publish one literal value");
        };

        let semantic_values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must publish: {error:?}"),
        };

        let value = semantic_values
            .constant_value_data(entry.value())
            .unwrap_or_else(|error| panic!("recovery literal value must resolve: {error:?}"));

        assert!(matches!(value.kind(), ConstantValueKind::Error));
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
                .by_kind(DiagnosticKind::CheckingConflictingBorrow)
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
    fn async_facts_report_awaits_in_synchronous_callables() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    await child();\n",
            "}\n",
            "async func child() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = match compilation.async_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("async facts must publish: {error:?}"),
        };

        assert_eq!(facts.value().suspensions().len(), 1);

        assert!(
            facts
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
                .next()
                .is_some()
        );

        assert!(
            compilation
                .semantic_diagnostics()
                .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
                .next()
                .is_some()
        );
    }

    #[test]
    fn async_facts_respect_anonymous_callable_execution_modes() {
        for (modifier, expected_diagnostic) in [("", true), ("async ", false)] {
            let compilation = compilation(&format!(
                concat!(
                    "module app;\n",
                    "async func main()\n",
                    "{{\n",
                    "    await {modifier}lambda()\n",
                    "    {{\n",
                    "        await child();\n",
                    "    }}();\n",
                    "}}\n",
                    "async func child()\n",
                    "{{\n",
                    "}}\n",
                ),
                modifier = modifier,
            ));

            let main = source_callable_body_key(&compilation);

            let bound = compilation
                .bound_unit(main)
                .unwrap_or_else(|error| panic!("source callable must bind: {error:?}"));

            let [anonymous] = bound.value().nested_units() else {
                panic!("source callable must contain one anonymous callable");
            };

            let facts = compilation
                .async_facts(anonymous.clone())
                .unwrap_or_else(|error| panic!("anonymous async facts must publish: {error:?}"));

            let has_diagnostic = facts
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
                .next()
                .is_some();

            assert_eq!(has_diagnostic, expected_diagnostic, "{modifier:?}");
        }
    }

    #[test]
    fn opaque_future_parameters_do_not_imply_recovery() {
        let compilation = compilation(concat!(
            "module app;\n",
            "async func main(pending: Future<i32>) -> i32\n",
            "{\n",
            "    await pending\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let facts = compilation
            .async_facts(key)
            .unwrap_or_else(|error| panic!("async facts must publish: {error:?}"));

        let [suspension] = facts.value().suspensions() else {
            panic!("the direct await must publish one suspension point");
        };

        assert!(suspension.deferred_calls().is_empty());
        assert!(!suspension.is_recovered(), "{suspension:?}");
    }

    #[test]
    fn async_facts_follow_deferred_calls_through_local_future_bindings() {
        let compilation = compilation(concat!(
            "module app;\n",
            "async func main() -> i32\n",
            "{\n",
            "    let retained: i32 = 1;\n",
            "    let pending = child();\n",
            "    let ignored: i32 = await pending;\n",
            "    return retained;\n",
            "}\n",
            "async func child() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let liveness = match compilation.liveness(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("liveness facts must publish: {error:?}"),
        };

        let facts = match compilation.async_facts(key.clone()) {
            Ok(facts) => facts,
            Err(error) => panic!("async facts must publish: {error:?}"),
        };

        let storage = match compilation.storage_plan(key.clone()) {
            Ok(storage) => storage,
            Err(error) => panic!("storage plan must publish: {error:?}"),
        };

        let [suspension] = facts.value().suspensions() else {
            panic!("the direct await must publish one suspension point");
        };

        assert_eq!(suspension.deferred_calls().len(), 1);
        assert!(!suspension.is_recovered());
        assert!(!facts.value().frame_dependencies().is_empty());

        assert_eq!(
            suspension.retained_subjects(),
            facts.value().frame_dependencies()
        );

        assert!(
            liveness
                .value()
                .live_across_suspensions()
                .iter()
                .all(|entry| facts
                    .value()
                    .frame_dependencies()
                    .contains(&entry.subject()))
        );

        assert!(
            facts
                .value()
                .scope_exits()
                .iter()
                .all(|exit| exit.cancellation_broadcast() == exit.lifecycle_resolution())
        );

        assert!(
            facts
                .value()
                .scope_exits()
                .iter()
                .any(|exit| !exit.cancellation_broadcast().is_empty())
        );

        let cleanup_roles = facts
            .value()
            .scope_exits()
            .iter()
            .flat_map(|exit| exit.cancellation_broadcast())
            .filter_map(|access| storage.value().access(*access))
            .filter_map(|access| type_representation(&compilation, access.reached_type()))
            .collect::<Vec<_>>();

        assert!(cleanup_roles.contains(&RepresentationRole::Future));

        assert!(
            cleanup_roles
                .iter()
                .all(|role| *role == RepresentationRole::Future)
        );

        let repeated = match compilation.async_facts(key) {
            Ok(facts) => facts,
            Err(error) => panic!("repeated async facts must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&facts, &repeated));
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
                .by_kind(DiagnosticKind::CheckingMissingMutationAuthority)
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
                .by_kind(DiagnosticKind::CheckingMissingMutationAuthority)
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
                .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
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
    fn storage_plans_retain_exact_branch_dependent_alternative_bindings() {
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

        let Some(StorageIdentity::Alternative { alternative, .. }) =
            plan.value().identity(*storage)
        else {
            panic!("coherent alternatives must retain an exact logical alias");
        };

        let Some(alternative) = plan.value().alternative(alternative) else {
            panic!("logical alias must retain its branch accesses");
        };

        assert_eq!(alternative.accesses().len(), 2);

        assert!(
            plan.value()
                .accesses()
                .iter()
                .all(|access| !access.is_recovered())
        );

        let Some((logical_access, _)) = plan.value().access_entries().find(|(_, access)| {
            matches!(access.root(), StorageAccessRoot::Storage(root) if root == *storage)
        }) else {
            panic!("the matched region must access the logical alias");
        };

        assert_eq!(
            plan.value().relationship(logical_access, logical_access),
            bray_bound_tree::StorageRelationship::Identical
        );

        assert!(alternative.accesses().iter().all(|branch| {
            plan.value().relationship(logical_access, *branch)
                == bray_bound_tree::StorageRelationship::PotentiallyOverlapping
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

        let control = match compilation.control_flow(key.clone()) {
            Ok(control) => control,
            Err(error) => panic!("recovered control flow must publish: {error:?}"),
        };

        assert!(control.value().is_recovered());

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
    fn memory_operations_publish_lazily_from_compiler_known_hooks() {
        let source = concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let pointer = core.memory.null<i32>();\n",
            "}\n",
        );

        let compilation = compilation_with_target_operations(source, true, false);

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.memory_operations.is_published(&key),
            Ok(false)
        );

        let selections = match compilation.semantic_selections(key.clone()) {
            Ok(selections) => selections,
            Err(error) => panic!("semantic selections must publish: {error:?}"),
        };

        let [selection] = selections.value().entries() else {
            panic!(
                "null pointer call must select one callable: {:?}",
                selections.value().entries()
            );
        };

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("null pointer call must select a call");
        };

        let Some(definition) = call.target().declaration() else {
            panic!("null pointer call must select a declaration");
        };

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .symbol_implementation(definition.symbol()),
            Some(ImplementationHook::RawPointerNull)
        );

        let first = match compilation.memory_operations(key.clone()) {
            Ok(operations) => operations,
            Err(error) => panic!("memory operations must publish: {error:?}"),
        };

        assert!(
            first.diagnostics().is_empty(),
            "memory operation checking must be diagnostic-free: {:?}",
            first.diagnostics()
        );

        let [operation] = first.value().operations() else {
            panic!("null pointer call must publish one memory operation");
        };

        assert!(matches!(
            operation.kind(),
            CheckedMemoryOperationKind::Null { .. }
        ));

        let repeated = match compilation.memory_operations(key) {
            Ok(operations) => operations,
            Err(error) => panic!("repeated memory operations must publish: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &repeated));

        let flow = match compilation.storage_flow_facts(source_callable_body_key(&compilation)) {
            Ok(flow) => flow,
            Err(error) => panic!("storage flow must publish: {error:?}"),
        };

        let [decision] = flow.value().memory_operations() else {
            panic!("storage flow must retain the null operation decision");
        };

        assert_eq!(
            decision.status(),
            bray_bound_tree::MemoryOperationStatus::Valid
        );
    }

    #[test]
    fn compiler_known_callable_parameters_supply_default_templates() {
        let source = concat!(
            "trusted module app;\n",
            "trusted func main() uses(manual_alloc)\n",
            "{\n",
            "    let pointer = trusted core.memory.allocate(bytes = 16, align = 4);\n",
            "}\n",
        );

        let compilation = compilation_with_target_operations(source, true, true);

        let operations = match compilation.memory_operations(source_callable_body_key(&compilation))
        {
            Ok(operations) => operations,
            Err(error) => panic!("allocation operation must publish: {error:?}"),
        };

        let [operation] = operations.value().operations() else {
            panic!("allocation call must publish one memory operation");
        };

        assert_eq!(operation.kind(), CheckedMemoryOperationKind::Allocate);

        let flow = compilation
            .storage_flow_facts(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("allocation storage flow must publish: {error:?}"));

        assert!(flow.diagnostics().is_empty(), "{:?}", flow.diagnostics());
    }

    #[test]
    fn unavailable_memory_operations_publish_structured_target_diagnostics() {
        let compilation = compilation_with_target_operations(
            concat!(
                "module app;\n",
                "func main()\n",
                "{\n",
                "    let pointer = core.memory.null<i32>();\n",
                "}\n",
            ),
            false,
            false,
        );

        let operations = match compilation.memory_operations(source_callable_body_key(&compilation))
        {
            Ok(operations) => operations,
            Err(error) => panic!("memory operations must publish: {error:?}"),
        };

        assert!(operations.value().operations().is_empty());

        let mut diagnostics = operations.diagnostics().iter();

        let Some(diagnostic) = diagnostics.next() else {
            panic!("unavailable operation must publish one diagnostic");
        };

        assert!(diagnostics.next().is_none());

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable
        );
    }

    #[test]
    fn invalid_memory_obligations_publish_structured_semantic_diagnostics() {
        let cases = [
            (
                concat!(
                    "trusted module app;\n",
                    "trusted func main() -> u8 uses(raw_memory)\n",
                    "{\n",
                    "    let pointer = core.memory.null<u8>();\n",
                    "    return core.memory.read<u8>(pointer);\n",
                    "}\n",
                ),
                DiagnosticKind::CheckingMissingTrustedMemoryFacts,
            ),
            (
                concat!(
                    "trusted module app;\n",
                    "trusted func main() -> u8 uses(raw_memory)\n",
                    "{\n",
                    "    let pointer = core.memory.null<u8>();\n",
                    "    return trusted core.memory.read<u8>(pointer);\n",
                    "}\n",
                ),
                DiagnosticKind::CheckingUninitializedRawStorage,
            ),
            (
                concat!(
                    "trusted module app;\n",
                    "trusted func main() uses(manual_alloc, raw_memory, unchecked_init)\n",
                    "{\n",
                    "    let pointer = trusted core.memory.allocate(bytes = 0, align = 1);\n",
                    "\n",
                    "    trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
                    "    trusted core.memory.write<u8>(pointer, 1);\n",
                    "}\n",
                ),
                DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
            ),
            (
                concat!(
                    "trusted module app;\n",
                    "trusted func main() uses(manual_alloc, raw_memory, unchecked_init)\n",
                    "{\n",
                    "    let pointer = trusted core.memory.allocate(bytes = 1, align = 1);\n",
                    "\n",
                    "    trusted core.memory.write<u8>(pointer, 1);\n",
                    "    trusted core.memory.deallocate(pointer = pointer, bytes = 1, align = 1);\n",
                    "}\n",
                ),
                DiagnosticKind::CheckingDeallocationWithOutstandingObligations,
            ),
        ];

        for (source, expected) in cases {
            let compilation = compilation_with_target_operations(source, true, true);

            let flow = compilation
                .storage_flow_facts(source_callable_body_key(&compilation))
                .unwrap_or_else(|error| {
                    panic!("invalid memory storage flow must publish: {error:?}")
                });

            assert!(
                flow.diagnostics().by_kind(expected).next().is_some(),
                "{expected:?} must be reported: {:?}",
                flow.diagnostics()
            );
        }
    }

    #[test]
    fn same_named_source_callables_are_not_memory_operations() {
        let compilation = compilation_with_target_operations(
            concat!(
                "module app;\n",
                "func main()\n",
                "{\n",
                "    let value: i32 = allocate();\n",
                "}\n",
                "func allocate() -> i32\n",
                "{\n",
                "    return 1;\n",
                "}\n",
            ),
            true,
            true,
        );

        let operations = match compilation.memory_operations(source_callable_body_key(&compilation))
        {
            Ok(operations) => operations,
            Err(error) => panic!("source call memory operations must publish: {error:?}"),
        };

        assert!(operations.value().operations().is_empty());
        assert!(operations.diagnostics().is_empty());
    }

    #[test]
    fn construction_selections_retain_struct_and_union_targets() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "}\n",
            "union Maybe\n",
            "{\n",
            "    Some(value: i32);\n",
            "    None;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let point = Point { x = 1 };\n",
            "    let present: Maybe = .Some(value = 1);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("construction selections must publish: {error:?}"),
        };

        let targets = selections.value().entries().iter().filter_map(|entry| {
            let SemanticSelection::Operation(SelectedOperation::Construction(construction)) =
                entry.selection()
            else {
                return None;
            };

            Some(construction.target())
        });

        assert_eq!(
            targets
                .filter(|target| matches!(target, ConstructionTarget::Struct(_)))
                .count(),
            1
        );

        assert_eq!(
            selections
                .value()
                .entries()
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.selection(),
                        SemanticSelection::Operation(SelectedOperation::Construction(
                            construction
                        )) if matches!(
                            construction.target(),
                            ConstructionTarget::UnionVariant(_)
                        )
                    )
                })
                .count(),
            1
        );

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn invalid_source_construction_reports_incompatible_candidate() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Point\n",
            "{\n",
            "    x: i32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let point = Point { y = 1 };\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("invalid construction selections must recover: {error:?}"),
        };

        assert_eq!(
            selections
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
                .count(),
            1,
            "{:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn built_in_index_and_conversion_selections_retain_exact_rules() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(pos values: [i32; 2])\n",
            "{\n",
            "    let widened = values[0] as i64;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("operation selections must publish: {error:?}"),
        };

        assert!(selections.value().entries().iter().any(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Operation(SelectedOperation::Index {
                    target: IndexTarget::ArrayElement,
                    ..
                })
            )
        }));

        assert!(selections.value().entries().iter().any(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Operation(SelectedOperation::Conversion(conversion))
                    if matches!(conversion.target(), ConversionTarget::BuiltInScalar)
            )
        }));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn source_index_selections_preserve_slice_bounds_and_custom_storage() {
        let compilation = compilation(
            r#"module app;

struct Values
{
    value: i32;
}

impl Values(SliceIndex<i32>)
{
    type Output = i32;

    func slice(pos start: i32?, pos end: i32?) -> i32
    {
        return 0;
    }
}

func select(pos values: Values) -> i32
{
    let lower = values[1..];
    let upper = values[..2];

    return values[1..2];
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("custom slice selection must publish: {error:?}"));

        assert!(selections.diagnostics().is_empty(), "{selections:?}");

        assert_eq!(
            selections
                .value()
                .entries()
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.selection(),
                        SemanticSelection::Operation(SelectedOperation::Index {
                            target: IndexTarget::Custom { .. },
                            ..
                        })
                    )
                })
                .count(),
            3
        );

        let storage = compilation
            .storage_plan(key)
            .unwrap_or_else(|error| panic!("custom slice storage must publish: {error:?}"));

        let ranges = storage
            .value()
            .accesses()
            .iter()
            .filter_map(|access| match access.projections().last() {
                Some(StorageProjection::SliceRange { start, end }) => Some((start, end)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(ranges.len(), 3);

        assert!(
            ranges
                .iter()
                .any(|(start, end)| start.is_some() && end.is_none())
        );

        assert!(
            ranges
                .iter()
                .any(|(start, end)| start.is_none() && end.is_some())
        );

        assert!(
            ranges
                .iter()
                .any(|(start, end)| start.is_some() && end.is_some())
        );
    }

    #[test]
    fn operation_selection_reports_invalid_indexing() {
        let compilation = compilation(
            r#"module app;

func select(pos value: i32) -> i32
{
    return value[0];
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let semantics = compilation
            .expression_types(key)
            .unwrap_or_else(|error| panic!("invalid indexing must remain checkable: {error:?}"));

        assert!(semantics.diagnostics().has_errors());
    }

    #[test]
    fn operation_selection_reports_invalid_conversions() {
        let compilation = compilation(
            r#"module app;

struct Value
{
}

func convert(pos value: Value) -> i32
{
    return value as i32;
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let semantics = compilation
            .expression_types(key)
            .unwrap_or_else(|error| panic!("invalid conversion must remain checkable: {error:?}"));

        assert!(semantics.diagnostics().has_errors());
    }

    #[test]
    fn box_construction_selection_retains_the_storage_implementation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let stored: box i32 = box(1);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("box construction selection must publish: {error:?}"),
        };

        assert!(selections.value().entries().iter().any(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Operation(SelectedOperation::Construction(construction))
                    if matches!(construction.target(), ConstructionTarget::TypeForm { .. })
            )
        }));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );
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
        assert_eq!(type_representation(compilation, ty), Some(expected));
    }

    fn type_representation(
        compilation: &Compilation,
        ty: bray_symbols::TypeId,
    ) -> Option<RepresentationRole> {
        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };

        let data = match values.type_data(ty) {
            Ok(data) => data,
            Err(error) => panic!("type must be available: {error:?}"),
        };

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return None;
        };

        match definition {
            NamedTypeSymbolId::Struct(definition) => compilation
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
            NamedTypeSymbolId::Union(definition) => compilation
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
        }
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
            [DiagnosticKind::CheckingUnreachablePatternAlternative]
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
            [DiagnosticKind::CheckingUnreachableMatchArm]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
            [DiagnosticKind::CheckingNonExhaustiveMatch]
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
            [DiagnosticKind::CheckingUnreachableMatchArm]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
            [DiagnosticKind::CheckingRefutablePattern]
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
                .contains(&DiagnosticKind::BindingUnresolvedName)
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
                .contains(&DiagnosticKind::BindingUnresolvedName),
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
            [DiagnosticKind::BindingAmbiguousName]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
            [DiagnosticKind::CheckingIncompatiblePattern]
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
