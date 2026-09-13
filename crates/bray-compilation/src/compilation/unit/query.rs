use std::sync::Arc;

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

use super::support::{
    bind_unit, check_control_flow, check_patterns, checker_unit_view, expression_candidates,
    map_binding_error, semantic_unit_context_for, unit_walk_failure,
};
use super::view::{
    AsyncAnalysisView, DependencyContractsView, ExpressionTypesView, LiteralValuesView,
    LivenessView, RefinementsView, SemanticSelectionsView, StorageFlowView,
};
use crate::compilation::binder::{bind_declared_value_type_templates, binding_query_error};
use crate::compilation::checker::checker_result;
use crate::compilation::operation::operation_type_input;
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
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

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

                let (mut operation_resolutions, mut operation_diagnostics, has_operations) =
                    self.operation_inputs(&key, bound.result().value(), cancellation)?;

                if !has_iterations && !has_operations {
                    return Ok((provisional.result().as_ref().clone(), Box::new([])));
                }

                loop {
                    let operation_input = operation_type_input(&operation_resolutions)
                        .with_iteration_sources(iteration_sources.iter().cloned());

                    let mut result = self.compute_expression_semantics(
                        &key,
                        cancellation,
                        &pattern_input,
                        &operation_input,
                    )?;

                    let (additional, diagnostics) = self.additional_operation_inputs(
                        &key,
                        bound.result().value(),
                        result.0.value(),
                        &operation_resolutions,
                        cancellation,
                    )?;

                    operation_diagnostics = operation_diagnostics.merged(&diagnostics);

                    if additional.is_empty() {
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

                    operation_resolutions.extend(additional);
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

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let unit = checker_unit_view(bound.result().value(), &semantic_context, &context)?;

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
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit = checker_unit_view(bound.result().value(), &semantic_context, &context)?;

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
            return Err(super::support::unit_contract_failure(
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{SemanticUnitContextError, semantic_unit_context};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundCallResult, BoundCallableTarget, BoundDependencyRequirement,
        BoundDependencySubject, BoundExpression, BoundExpressionId, BoundReferenceTarget,
        BoundUnit, BoundUnitKind, BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes,
        CheckedMemoryOperationKind, ConstructionTarget, ConversionTarget,
        DeclaredValueTypeConstraintKind, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
        IndexTarget, PatternOperation, PatternPredicate, PatternProjection, RefinementKind,
        SelectedArgument, SelectedOperation, SemanticSelection, StorageAccessPurpose,
        StorageAccessRoot, StorageBinding, StorageBindingTarget, StorageIdentity,
        StorageProjection, walk_bound_unit_view,
    };
    use bray_checker::{CheckerInfrastructureError, CheckerUnitViewError, SemanticUnitContext};
    use bray_compiler_known::{CompilerKnownOperationRole, ImplementationHook, RepresentationRole};
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
        DiagnosticConstructionInputRejection, DiagnosticKind, DiagnosticPatternMissingCase,
        DiagnosticRelatedLocationKind, DiagnosticSelectionCandidateIdentity,
        DiagnosticSelectionCandidateSignature, DiagnosticSelectionRejectionReason,
        DiagnosticSelectionRejections, DiagnosticStorageProjection, DiagnosticStorageRoot,
        DiagnosticType,
    };
    use bray_ir::{MirBinaryOperator, MirNullableQueryKind, MirOperationKind};
    use bray_messages::DiagnosticRenderer;
    use bray_source::SourceSpan;
    use bray_symbols::{
        ConstantValueKind, NamedTypeSymbolId, PackageIdentity, SymbolKind, SymbolOrdinal, TypeData,
        TypeExpressionTemplate,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{Compilation, check_control_flow, semantic_unit_context_for};
    use crate::CompilationRequest;
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError, QueryPriority};
    use crate::test_support::{
        FactTestGate, compilation, compilation_with_sources_and_worker_budget,
        compilation_with_target_operations, only_call_selection, source_callable_body_key,
        source_function_body_key, source_input, source_trait_callable_fulfillment_body_key,
        source_type_callable_member_body_key,
    };

    fn selection_rejections(diagnostic: &Diagnostic) -> &DiagnosticSelectionRejections {
        let Some(DiagnosticArgValue::SelectionRejections(rejections)) = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::SelectionRejections)
            .map(DiagnosticArg::value)
        else {
            panic!("selection diagnostic must retain rejected candidates: {diagnostic:?}");
        };

        rejections
    }

    #[test]
    fn returned_assignments_preserve_disjoint_fields() {
        for returned in [
            "pair.first",
            "first(pair)",
            "tuple_first((pair.first, pair.second))",
        ] {
            let source = r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    mut second: &bool;
                }

                func select(pos caller: &bool, pos temporary: &bool) -> &bool
                {
                    let mut pair = Pair { first = caller, second = caller };
                    pair.second = temporary;

                    return RETURNED;
                }

                func first(pos pair: Pair) -> &bool
                {
                    return pair.first;
                }

                func tuple_first(pos values: (&bool, &bool)) -> &bool
                {
                    let (value, _) = values;

                    return value;
                }

                func good(pos caller: &bool) -> &bool
                {
                    let local: bool = true;

                    return select(caller, &local);
                }
            "#;

            let compilation = compilation(&source.replace("RETURNED", returned));

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn returned_assignments_through_calls_reject_local_storage() {
        let compilation = compilation(
            r#"
                module app;

                struct Holder
                {
                    mut value: &bool;
                }

                func read(pos holder: Holder) -> &bool
                {
                    return holder.value;
                }

                func bad(pos caller: &bool) -> &bool
                {
                    let local: bool = true;
                    let mut holder = Holder { value = caller };
                    holder.value = &local;

                    return read(holder);
                }
            "#,
        );

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "bad"))
            .unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }

    #[test]
    fn returned_assignments_follow_parameter_array_and_alias_storage() {
        for body in [
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    result.value = second;

                    return result;
                }
            "#,
            r#"
                func replace(pos result: Holder, pos second: &bool) -> Holder
                {
                    let mut values = [result];

                    values[0] = Holder
                    {
                        value = second
                    };

                    return values[0];
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let owner = &mut result;
                    let alias = owner;

                    alias.value = second;

                    return result;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    result.value = second;

                    return Holder { value = read(result) };
                }

                func read(pos holder: Holder) -> &bool
                {
                    return holder.value;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut owner = &mut result;
                    let mut alias = owner;
                    alias.value = second;

                    return result;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let (owner,) = (&mut result,);
                    owner.value = second;

                    return result;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let mut owner = &mut result;
                    owner = Holder { value = second };

                    return result;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    let [owner] = [&mut result];
                    owner.value = second;

                    return result;
                }
            "#,
            r#"
                func replace(pos mut result: Holder, pos second: &bool) -> Holder
                {
                    if let (owner,) = (&mut result,)
                    {
                        owner.value = second;
                    }

                    return result;
                }
            "#,
        ] {
            let source = format!(
                r#"
                    module app;

                    struct Holder
                    {{
                        mut value: &bool;
                    }}

                    {body}

                    func bad(pos caller: &bool) -> Holder
                    {{
                        let local: bool = true;

                        return replace(Holder {{ value = caller }}, &local);
                    }}
                "#
            );

            let caller_owned = compilation(&source.replace("&local", "caller"));

            assert!(
                caller_owned.check_diagnostics().is_empty(),
                "{:?}",
                caller_owned.check_diagnostics()
            );

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "bad"))
                .unwrap();

            assert!(
                flow.diagnostics().iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingEscapingStorageDependency),
                "{source}\n{:?}",
                flow.diagnostics()
            );

            bray_testing::assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }

    #[test]
    fn returned_iteration_values_reject_local_storage() {
        let source = r#"
            module app;

            struct Items
            {
                value: &bool;
            }

            struct Cursor
            {
                value: &bool;
                mut available: bool;
            }

            impl &Items(Iterable)
            {
                type Element = &bool;
                type Cursor = Cursor;

                consume func iterate() -> Cursor
                {
                    return Cursor
                    {
                        value = self.value,
                        available = true
                    };
                }
            }

            impl Cursor(Iterator)
            {
                type Element = &bool;

                mut func next() -> (&bool)?
                {
                    if !self.available
                    {
                        return none;
                    }

                    self.available = false;
                    return self.value;
                }
            }

            func first(pos values: &Items) -> (&bool)?
            {
                for value in values
                {
                    return value;
                }

                return none;
            }

            func bad() -> (&bool)?
            {
                let local: bool = true;

                let items = Items
                {
                    value = &local
                };

                return first(&items);
            }
        "#;

        let compilation = compilation(source);
        let key = source_function_body_key(&compilation, "bad");
        let flow = compilation.storage_flow(key).unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }

    #[test]
    fn returned_propagation_respects_yield_regions_and_independent_errors() {
        for (body, result, input, argument, valid) in [
            (
                r#"
                    func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
                    {
                        let result: Result<bool, &bool> =
                        {
                            let value = try input;

                            yield Ok(value);
                        };

                        return result;
                    }
                "#,
                "Result<bool, &bool>",
                "Result<bool, &bool>",
                "Error(&local)",
                false,
            ),
            (
                r#"
                    func forward(pos input: Result<(&bool), bool>) -> Result<bool, bool>
                    {
                        let value = try input;

                        let output: Result<bool, bool> = Ok(true);

                        return output;
                    }
                "#,
                "Result<bool, bool>",
                "Result<(&bool), bool>",
                "Ok(&local)",
                true,
            ),
        ] {
            let source = format!(
                r#"
                    module app;

                    {body}

                    func caller() -> {result}
                    {{
                        let local: bool = true;

                        let input: {input} = {argument};

                        return forward(input);
                    }}
                "#
            );

            let compilation = compilation(&source);

            if valid {
                assert!(
                    compilation.check_diagnostics().is_empty(),
                    "{:?}",
                    compilation.check_diagnostics()
                );
            } else {
                let flow = compilation
                    .storage_flow(source_function_body_key(&compilation, "caller"))
                    .unwrap();

                bray_testing::assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }

    #[test]
    fn returned_propagated_errors_reject_local_storage() {
        let source = r#"
            module app;

            func forward(pos input: Result<bool, &bool>) -> Result<bool, &bool>
            {
                let value = try input;

                return Ok(value);
            }

            func bad() -> Result<bool, &bool>
            {
                let local: bool = true;

                return forward(Error(&local));
            }
        "#;

        let compilation = compilation(source);
        let key = source_function_body_key(&compilation, "bad");
        let flow = compilation.storage_flow(key).unwrap();

        bray_testing::assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingEscapingStorageDependency,
        );
    }

    #[test]
    fn returned_values_reject_local_storage_dependencies() {
        for source in [
            r#"
                module app;

                struct Holder
                {
                    mut value: &bool;
                }

                func replace(pos first: &bool, pos second: &bool) -> Holder
                {
                    let mut result = Holder { value = first };
                    result.value = second;

                    return result;
                }

                func bad(pos caller: &bool) -> Holder
                {
                    let local: bool = true;

                    return replace(caller, &local);
                }
            "#,
            r#"
                module app;

                func bad() -> &bool
                {
                    let value: bool = true;

                    return &value;
                }
            "#,
            r#"
                module app;

                func bad() -> &mut bool
                {
                    let mut value: bool = true;

                    return &mut value;
                }
            "#,
            r#"
                module app;

                func bad() -> &bool
                {
                    let value: bool = true;
                    let alias = &value;

                    return alias;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return Holder
                    {
                        value = &value
                    };
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return value;
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return same(&value);
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: &mut T) -> &mut T
                {
                    return value;
                }

                func bad() -> &mut bool
                {
                    let mut value: bool = true;

                    return same(&mut value);
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: T) -> T
                {
                    return value;
                }

                struct Holder
                {
                    value: &bool;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return same(
                        Holder
                        {
                            value = &value
                        }
                    );
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return
                    {
                        yield value;
                    };
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return same(&value);
                }
            "#,
            r#"
                module app;

                func project<T>(pos value: &box T) -> &T
                {
                    return match value
                    {
                        case box(inner)
                        {
                            yield &inner;
                        }
                    };
                }

                func bad() -> &bool
                {
                    let value = box(true);

                    return project(&value);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let alias = same(&value);

                    return &alias.value;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let alias = &value;

                    return project(alias);
                }
            "#,
            r#"
                module app;

                func bad() -> (&bool)?
                {
                    let value: bool = true;
                    let mut result: (&bool)? = none;

                    result = &value;
                    return result;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;

                    func borrow() -> &bool
                    {
                        return &self.value;
                    }
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return value.borrow();
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: &bool;

                    consume func take() -> &bool
                    {
                        return self.value;
                    }
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    let holder = Holder
                    {
                        value = &value
                    };

                    return holder.take();
                }
            "#,
            r#"
                module app;

                func choose(pos value: &bool, other: &bool = value) -> &bool
                {
                    return other;
                }

                func bad() -> &bool
                {
                    let value: bool = true;

                    return choose(&value);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &(same(value)).value;
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return project(&value);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func wrap(pos transient: &bool, pos anchor: &bool) -> Holder
                {
                    return Holder
                    {
                        value = anchor
                    };
                }

                func choose(pos transient: &bool, pos anchor: &bool, value: Holder = wrap(transient, anchor)) -> Holder
                {
                    return value;
                }

                func bad() -> Holder
                {
                    let value: bool = true;

                    return choose(&value, &value);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                struct Owner
                {
                    source: &Holder;

                    consume func take() -> &bool
                    {
                        return project(self.source);
                    }
                }

                func bad() -> &bool
                {
                    let value = Holder
                    {
                        value = true
                    };

                    let owner = Owner
                    {
                        source = &value
                    };

                    return owner.take();
                }
            "#,
            r#"
                module app;

                func first(pos values: &[bool; 2]) -> &bool
                {
                    return &values[0];
                }

                func bad() -> &bool
                {
                    let values = [true, false];

                    return first(&values);
                }
            "#,
            r#"
                module app;

                func bad(pos value: bool) -> &bool
                {
                    return &value;
                }
            "#,
            r#"
                module app;

                func call(pos function: func(pos value: &bool) -> &bool, pos value: &bool) -> &bool
                {
                    return function(value);
                }

                func bad(pos function: func(pos value: &bool) -> &bool) -> &bool
                {
                    let local: bool = true;

                    return call(function, &local);
                }
            "#,
            r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func wrap(pos first_value: &bool, pos second_value: &bool) -> &bool
                {
                    return first(
                        Pair
                        {
                            first = first_value,
                            second = second_value
                        }
                    );
                }

                func bad(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return wrap(&local, anchor);
                }
            "#,
        ] {
            let compilation = compilation(source);
            let key = source_function_body_key(&compilation, "bad");

            let flow = compilation
                .storage_flow(key)
                .expect("storage flow must publish");

            assert!(
                flow.diagnostics().iter().any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingEscapingStorageDependency),
                "{source}: {:?}",
                flow.diagnostics()
            );

            assert_goal_state_diagnostic_kind(
                flow.diagnostics(),
                DiagnosticKind::CheckingEscapingStorageDependency,
            );
        }
    }

    #[test]
    fn returned_values_preserve_caller_storage_and_owned_results() {
        for source in [
            r#"
                module app;

                struct Value<T>
                {
                    values: [T; 2];
                }

                union Failure
                {
                    Unavailable;
                }

                func good(pos produce: func(pos input: &bool) -> Result<Value<bool>, Failure>)
                    -> Result<Value<bool>, Failure>
                {
                    let local: bool = true;

                    return produce(&local);
                }
            "#,
            r#"
                module app;

                func good() -> &string
                {
                    return &"product lifetime";
                }
            "#,
            r#"
                module app;

                func same(pos value: &string) -> &string
                {
                    return value;
                }

                func good() -> &string
                {
                    return same(&"product lifetime");
                }
            "#,
            r#"
                module app;

                func good(pos value: &bool) -> &bool
                {
                    return value;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func good(pos value: &Holder) -> &bool
                {
                    let alias = same(value);

                    return &alias.value;
                }
            "#,
            r#"
                module app;

                func project<T>(pos value: &box T) -> &T
                {
                    return match value
                    {
                        case box(inner)
                        {
                            yield &inner;
                        }
                    };
                }

                func good(pos value: &box bool) -> &bool
                {
                    return project(value);
                }
            "#,
            r#"
                module app;

                func good(pos value: &bool) -> (&bool)?
                {
                    let mut result: (&bool)? = none;

                    result = value;
                    return result;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: &bool;

                    consume func take() -> &bool
                    {
                        return self.value;
                    }
                }

                func good(pos value: &bool) -> &bool
                {
                    let holder = Holder
                    {
                        value = value
                    };

                    return holder.take();
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: &T, pos stop: bool) -> &T
                {
                    if stop
                    {
                        return value;
                    }

                    return same<T>(value, true);
                }

                func good(pos value: &bool) -> &bool
                {
                    return same(value, false);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func good(pos value: &Holder) -> &bool
                {
                    let alias = value;

                    return &alias.value;
                }
            "#,
            r#"
                module app;

                func good() -> bool
                {
                    let value: bool = true;
                    let alias = &value;

                    return true;
                }
            "#,
            r#"
                module app;

                func good() -> bool
                {
                    let value: bool = true;

                    return read(&value);
                }

                func read(pos value: &bool) -> bool
                {
                    return true;
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func good() -> Holder
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return value;
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: &T) -> &T
                {
                    return value;
                }

                func good(pos value: &bool) -> &bool
                {
                    return same(value);
                }
            "#,
            r#"
                module app;

                func same<T>(pos value: T) -> T
                {
                    return value;
                }

                struct Holder
                {
                    value: bool;
                }

                func good() -> Holder
                {
                    let value = Holder
                    {
                        value = true
                    };

                    return same(value);
                }
            "#,
            r#"
                module app;

                func use_value(pos value: &mut bool)
                {
                }

                func good() -> bool
                {
                    let mut count: i32 = 0;

                    while count < 2
                    {
                        let mut value: bool = true;

                        use_value(&mut value);
                        count += 1;
                    }

                    return true;
                }
            "#,
            r#"
                module app;

                func choose(pos value: &bool, other: &bool = value) -> &bool
                {
                    return other;
                }

                func good(pos value: &bool) -> &bool
                {
                    return choose(value);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: &bool;
                }

                func wrap(pos transient: &bool, pos anchor: &bool) -> Holder
                {
                    return Holder
                    {
                        value = anchor
                    };
                }

                func choose(pos transient: &bool, pos anchor: &bool, value: Holder = wrap(transient, anchor)) -> Holder
                {
                    return value;
                }

                func good(pos anchor: &bool) -> Holder
                {
                    let transient: bool = true;

                    return choose(&transient, anchor);
                }
            "#,
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func project(pos value: &Holder) -> &bool
                {
                    return &value.value;
                }

                struct Owner
                {
                    source: &Holder;

                    consume func take() -> &bool
                    {
                        return project(self.source);
                    }
                }

                func good(pos value: &Holder) -> &bool
                {
                    let owner = Owner
                    {
                        source = value
                    };

                    return owner.take();
                }
            "#,
            r#"
                module app;

                func first(pos values: &[bool; 2]) -> &bool
                {
                    return &values[0];
                }

                func good(pos values: &[bool; 2]) -> &bool
                {
                    return first(values);
                }
            "#,
            r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func good(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return first(
                        Pair
                        {
                            first = anchor,
                            second = &local
                        }
                    );
                }
            "#,
            r#"
                module app;

                struct Pair
                {
                    first: &bool;
                    second: &bool;
                }

                func first(pos value: Pair) -> &bool
                {
                    return value.first;
                }

                func wrap(pos first_value: &bool, pos second_value: &bool) -> &bool
                {
                    let pair = Pair
                    {
                        first = first_value,
                        second = second_value
                    };

                    return first(pair);
                }

                func good(pos anchor: &bool) -> &bool
                {
                    let local: bool = true;

                    return wrap(anchor, &local);
                }
            "#,
        ] {
            let compilation = compilation(source);

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_function_body_key(&compilation, "good");

            let flow = compilation
                .storage_flow(key)
                .expect("storage flow must publish");

            assert!(
                !flow.diagnostics().has_errors(),
                "{source}: {:?}",
                flow.diagnostics()
            );
        }
    }

    #[test]
    fn returned_borrow_aliases_lower_inside_nested_scopes() {
        let compilation = compilation(
            r#"
                module app;

                struct Holder
                {
                    value: bool;
                }

                func same(pos value: &Holder) -> &Holder
                {
                    return value;
                }

                func inspect(pos value: &Holder) -> bool
                {
                    let alias = same(value);
                    let second = alias;
                    let mut count: i32 = 0;

                    while count < 2
                    {
                        if second.value
                        {
                            count += 1;
                        }
                        else
                        {
                            return false;
                        }
                    }

                    return true;
                }
            "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "inspect");

        let lowered = compilation
            .lowered_unit(key)
            .expect("stored borrow aliases must lower");

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn chained_nullable_receiver_evaluates_its_producer_once() {
        let compilation = compilation(
            r#"
                module app;
                            struct Counter<T> {
                                mut calls: i32;
                                value: T;
                                mut func take() -> T? with(T: Copyable) {
                                    self.calls += 1;
                                    return self.value;
                                }
                                mut func remove() -> bool with(T: Copyable) {
                                    return self.take().is_present();
                                }
                            }
                            func inspect(pos counter: &mut Counter<i32>) -> bool {
                                return counter.take().is_present();
                            }
            "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = crate::test_support::source_type_callable_member_body_key(&compilation, "remove");
        let lowered = compilation.lowered_unit(key).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();
        let calls: Vec<_> = mir.operations().iter().filter(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Direct(_)))).collect();
        assert_eq!(calls.len(), 1, "{calls:#?}");
    }

    #[test]
    fn returned_slice_receiver_uses_the_call_result_representation() {
        let compilation = compilation(
            r#"
                module app;

                struct Buffer
                {
                    values: [bool; 2];

                    func as_slice() -> &[bool]
                    {
                        return &self.values[..];
                    }
                }

                func inspect() -> usize
                {
                    let buffer = Buffer
                    {
                        values = [true, false]
                    };

                    return buffer.as_slice().length();
                }
            "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "inspect");

        let lowered = compilation
            .lowered_unit(key)
            .expect("chained returned slice must lower");

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();
        let values = compilation.semantic_value_store().unwrap();

        let length = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                bray_ir::MirOperationKind::Memory(memory)
                    if memory.kind()
                        == bray_bound_tree::CheckedMemoryOperationKind::SequenceLength =>
                {
                    Some(memory)
                }
                _ => None,
            })
            .expect("slice length operation must exist");

        let [bray_ir::MirOperand::Value(receiver)] = length.operands() else {
            panic!("slice receiver must be evaluated");
        };

        let borrow = mir
            .operations()
            .iter()
            .find(|operation| operation.result() == Some(*receiver))
            .unwrap();

        let bray_ir::MirOperationKind::Borrow { place, .. } = borrow.kind() else {
            panic!("slice receiver must be reborrowed");
        };

        assert!(
            matches!(
                values.type_data(place.ty()).unwrap().as_ref(),
                TypeData::Slice(_)
            ),
            "receiver must borrow the slice result, not the anchor array: {place:?}"
        );
    }

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
            dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
                key.clone()
            ))
        );

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::CheckedPatterns(
                key.clone()
            ))
        );
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
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        let values = match compilation.literal_values(key.clone()) {
            Ok(values) => values,
            Err(error) => panic!("literal values must publish: {error:?}"),
        };

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
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

        assert_eq!(
            compilation.state.body_semantics.is_published(&key),
            Ok(false)
        );

        let first = match compilation.liveness(key.clone()) {
            Ok(analysis) => analysis,
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
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated liveness analysis must publish: {error:?}"),
        };

        assert_eq!(first.snapshot_address(), second.snapshot_address());

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published liveness must retain dependencies"),
            Err(error) => panic!("liveness dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));
        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key)));
    }

    #[test]
    fn dependency_contracts_reuse_the_body_semantic_publication() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main(value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.body_semantics.is_published(&key),
            Ok(false)
        );

        let first = match compilation.dependency_contracts(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("dependency contracts must publish: {error:?}"),
        };

        let second = match compilation.dependency_contracts(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated dependency contracts must publish: {error:?}"),
        };

        let liveness = compilation
            .liveness(key.clone())
            .unwrap_or_else(|error| panic!("liveness must publish: {error:?}"));

        let refinements = compilation
            .refinements(key.clone())
            .unwrap_or_else(|error| panic!("refinements must publish: {error:?}"));

        let storage_flow = compilation
            .storage_flow(key.clone())
            .unwrap_or_else(|error| panic!("storage flow must publish: {error:?}"));

        let asynchronous = compilation
            .async_analysis(key.clone())
            .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

        assert_eq!(first.snapshot_address(), second.snapshot_address());
        assert_eq!(first.snapshot_address(), liveness.snapshot_address());
        assert_eq!(first.snapshot_address(), refinements.snapshot_address());
        assert_eq!(first.snapshot_address(), storage_flow.snapshot_address());
        assert_eq!(first.snapshot_address(), asynchronous.snapshot_address());

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published dependency contracts must retain dependencies"),
            Err(error) => panic!("dependency contract dependencies must be readable: {error:?}"),
        };

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
                key.clone()
            ))
        );

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));
        assert!(dependencies.contains(&crate::fact::CompilationFactKey::MemoryOperations(key)));
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
            compilation.state.body_semantics.is_published(&key),
            Ok(false)
        );

        let analysis = match compilation.storage_flow(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            analysis.value().operations().iter().any(|operation| {
                operation.status() == bray_bound_tree::StorageOperationStatus::ConflictingBorrow
            }),
            "{analysis:?}"
        );

        let diagnostic = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingConflictingBorrow)
            .next()
            .unwrap_or_else(|| panic!("overlapping borrows must produce a diagnostic"));

        bray_testing::assert_goal_state_diagnostic(diagnostic);

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingConflictingBorrow,
        );

        let repeated = match compilation.storage_flow(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated storage-flow checking must publish: {error:?}"),
        };

        assert_eq!(analysis.snapshot_address(), repeated.snapshot_address());

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
        {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("published storage flow must retain dependencies"),
            Err(error) => panic!("storage-flow dependencies must be readable: {error:?}"),
        };

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
                key.clone()
            ))
        );

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::MemoryOperations(key)));
    }

    #[test]
    fn raw_pointer_use_does_not_extend_the_source_borrow() {
        assert_standard_memory_body_has_no_conflicting_borrow(concat!(
            "module std.test;\n",
            "func main()\n",
            "{\n",
            "    let mut value: i32 = 1;\n",
            "    let pointer: RawPointer<i32> = std.memory.address_of<i32>(&value);\n",
            "    std.memory.is_null<i32>(pointer);\n",
            "    value = 2;\n",
            "}\n",
        ));
    }

    #[test]
    fn raw_pointer_assignment_preserves_addressed_storage_initialization() {
        assert_standard_memory_body_has_no_diagnostic(
            concat!(
                "trusted module std.test;\n",
                "trusted func main(pos value: &usize) -> usize uses(raw_memory)\n",
                "{\n",
                "    let pointer: RawPointer<usize> = std.memory.address_of<usize>(value);\n",
                "\n",
                "    return trusted std.memory.read<usize>(pointer);\n",
                "}\n",
            ),
            DiagnosticKind::CheckingUninitializedRawStorage,
        );
    }

    #[test]
    fn mutable_slice_element_can_be_reborrowed_for_a_raw_pointer() {
        assert_standard_memory_body_has_no_conflicting_borrow(concat!(
            "module std.test;\n",
            "func main(pos bytes: & mut [u8]) -> RawPointer<u8>\n",
            "{\n",
            "    if bytes.is_empty()\n",
            "    {\n",
            "        return std.memory.null<u8>();\n",
            "    }\n",
            "\n",
            "    return std.memory.address_of_mut<u8>(& mut bytes[0]);\n",
            "}\n",
        ));
    }

    fn assert_standard_memory_body_has_no_conflicting_borrow(source: &str) {
        assert_standard_memory_body_has_no_diagnostic(
            source,
            DiagnosticKind::CheckingConflictingBorrow,
        );
    }

    fn assert_standard_memory_body_has_no_diagnostic(source: &str, diagnostic: DiagnosticKind) {
        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library identity must be valid"));

        let request = CompilationRequest::new(
            package,
            vec![
                source_input(
                    include_str!("../../../../../standard-library/std/src/std.bray"),
                    0,
                ),
                source_input(
                    include_str!("../../../../../standard-library/std/src/memory.bray"),
                    1,
                ),
                source_input(source, 2),
            ],
        )
        .with_standard_library_source_authority();

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("standard library compilation must load: {error:?}"));

        let key = source_function_body_key(&compilation, "main");

        let analysis = compilation
            .storage_flow(key)
            .unwrap_or_else(|error| panic!("storage-flow checking must publish: {error:?}"));

        assert!(
            analysis.diagnostics().by_kind(diagnostic).next().is_none(),
            "{analysis:#?}"
        );
    }

    #[test]
    fn async_analysis_reports_awaits_in_synchronous_callables() {
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

        let analysis = match compilation.async_analysis(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("async analysis must publish: {error:?}"),
        };

        assert_eq!(analysis.value().suspensions().len(), 1);

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
        );

        assert!(
            analysis
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
    fn async_analysis_respects_anonymous_callable_execution_modes() {
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

            let analysis = compilation
                .async_analysis(anonymous.clone())
                .unwrap_or_else(|error| panic!("anonymous async analysis must publish: {error:?}"));

            let has_diagnostic = analysis
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

        let analysis = compilation
            .async_analysis(key)
            .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

        let [suspension] = analysis.value().suspensions() else {
            panic!("the direct await must publish one suspension point");
        };

        assert!(suspension.deferred_calls().is_empty());
        assert!(!suspension.is_recovered(), "{suspension:?}");
    }

    #[test]
    fn ordinary_valid_awaits_retain_their_inferred_dependency_contract() {
        let compilation = compilation(concat!(
            "module app;\n",
            "async func main(pending: Future<i32>) -> i32\n",
            "{\n",
            "    await pending\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let analysis = compilation
            .async_analysis(key.clone())
            .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

        let dependencies = compilation
            .dependency_contracts(key)
            .unwrap_or_else(|error| panic!("dependency contracts must publish: {error:?}"));

        let [suspension] = analysis.value().suspensions() else {
            panic!("the direct await must publish one suspension point");
        };

        let contract = suspension
            .dependency_contract()
            .unwrap_or_else(|| panic!("a valid await must select its inferred contract"));

        assert!(dependencies.value().contract(contract).is_some());
        assert!(!suspension.is_recovered());
    }

    #[test]
    fn async_analysis_follows_deferred_calls_through_local_future_bindings() {
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
            Ok(analysis) => analysis,
            Err(error) => panic!("liveness analysis must publish: {error:?}"),
        };

        let analysis = match compilation.async_analysis(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("async analysis must publish: {error:?}"),
        };

        let storage = match compilation.storage_plan(key.clone()) {
            Ok(storage) => storage,
            Err(error) => panic!("storage plan must publish: {error:?}"),
        };

        let [suspension] = analysis.value().suspensions() else {
            panic!("the direct await must publish one suspension point");
        };

        assert_eq!(suspension.deferred_calls().len(), 1);
        assert!(!suspension.is_recovered());
        assert!(!analysis.value().frame_dependencies().is_empty());

        assert_eq!(
            suspension.retained_subjects(),
            analysis.value().frame_dependencies()
        );

        assert!(
            liveness
                .value()
                .live_across_suspensions()
                .iter()
                .all(|entry| analysis
                    .value()
                    .frame_dependencies()
                    .contains(&entry.subject()))
        );

        assert!(
            analysis
                .value()
                .scope_exits()
                .iter()
                .all(|exit| exit.cancellation_broadcast() == exit.lifecycle_resolution())
        );

        assert!(
            analysis
                .value()
                .scope_exits()
                .iter()
                .any(|exit| !exit.cancellation_broadcast().is_empty())
        );

        let cleanup_roles = analysis
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

        let repeated = match compilation.async_analysis(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated async analysis must publish: {error:?}"),
        };

        assert_eq!(analysis.snapshot_address(), repeated.snapshot_address());
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(analysis.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }));

        assert!(
            analysis
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingMissingMutationAuthority)
                .next()
                .is_some()
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingMissingMutationAuthority,
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(analysis.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }));

        assert!(
            analysis
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            analysis.value().operations().iter().all(|operation| {
                !matches!(
                    operation.status(),
                    bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
                        | bray_bound_tree::StorageOperationStatus::ConflictingBorrow
                )
            }),
            "{analysis:?}"
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(analysis.value().operations().iter().any(|operation| {
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            analysis
                .value()
                .operations()
                .iter()
                .any(|operation| operation.status()
                    == bray_bound_tree::StorageOperationStatus::Moved),
            "{analysis:?}"
        );

        let diagnostic = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
            .next()
            .unwrap_or_else(|| panic!("use after move must produce a diagnostic"));

        bray_testing::assert_goal_state_diagnostic(diagnostic);

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingUseOfMovedStorage,
        );

        assert!(
            analysis
                .value()
                .exits()
                .iter()
                .any(|exit| !exit.moved().is_empty())
        );
    }

    #[test]
    fn storage_flow_rejects_moving_a_value_through_a_shared_borrow() {
        let compilation = custom_index_storage_compilation(
            r#"func exercise(pos values: Values)
{
    let moved: Item = values[0];
    moved;
}
"#,
        );

        let flow = compilation
            .storage_flow(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("borrowed move storage flow must publish: {error:?}"));

        assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingMissingStorageOwnership,
        );

        let diagnostic = flow
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingMissingStorageOwnership)
            .next()
            .unwrap_or_else(|| panic!("custom-index move must retain its diagnostic"));

        let access = diagnostic
            .args()
            .iter()
            .find_map(|arg| match (arg.name(), arg.value()) {
                (DiagnosticArgName::StorageAccess, DiagnosticArgValue::StorageAccess(access)) => {
                    Some(access)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("custom-index move must retain its exact access"));

        assert_eq!(access.root(), DiagnosticStorageRoot::BorrowedStorage);
    }

    #[test]
    fn storage_flow_reports_every_branch_move_origin() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    value: i32;\n",
            "}\n",
            "func main(pos condition: bool, pos resource: Resource)\n",
            "{\n",
            "    if condition\n",
            "    {\n",
            "        let first: Resource = resource;\n",
            "        first;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        let second: Resource = resource;\n",
            "        second;\n",
            "    }\n",
            "    resource;\n",
            "}\n",
        ));

        let diagnostic = compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
            .next()
            .unwrap_or_else(|| panic!("branch moves must produce a use-after-move diagnostic"));

        assert_eq!(diagnostic.related_locations().len(), 2, "{diagnostic:#?}");
        bray_testing::assert_goal_state_diagnostic(diagnostic);
    }

    #[test]
    fn storage_flow_diagnostics_retain_nested_projection_paths() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource { value: i32; }\n",
            "struct Container { inner: Resource; }\n",
            "func main(pos container: Container)\n",
            "{\n",
            "    let moved: Resource = container.inner;\n",
            "    container.inner;\n",
            "    moved;\n",
            "}\n",
        ));

        let flow = compilation
            .storage_flow(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("nested storage flow must publish: {error:?}"));

        let diagnostic = flow
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
            .next()
            .unwrap_or_else(|| panic!("nested use after move must be diagnosed"));

        let access = diagnostic
            .args()
            .iter()
            .find_map(|arg| match (arg.name(), arg.value()) {
                (DiagnosticArgName::StorageAccess, DiagnosticArgValue::StorageAccess(access)) => {
                    Some(access)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("nested use must retain its exact storage access"));

        assert!(access.projections().iter().any(|projection| {
            matches!(projection, DiagnosticStorageProjection::ProductField(name) if name == "inner")
        }));

        assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingUseOfMovedStorage,
        );
    }

    #[test]
    fn storage_flow_reports_every_conflicting_borrow_origin() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let mut value: i32 = 1;\n",
            "    let first: &i32 = &value;\n",
            "    let second: &i32 = &value;\n",
            "    let exclusive: & mut i32 = & mut value;\n",
            "    first;\n",
            "    second;\n",
            "    exclusive;\n",
            "}\n",
        ));

        let diagnostic = compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::CheckingConflictingBorrow)
            .next()
            .unwrap_or_else(|| panic!("overlapping borrows must produce a diagnostic"));

        assert_eq!(diagnostic.related_locations().len(), 2, "{diagnostic:#?}");
        bray_testing::assert_goal_state_diagnostic(diagnostic);
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            analysis.value().operations().iter().all(|operation| {
                !matches!(
                    operation.status(),
                    bray_bound_tree::StorageOperationStatus::Uninitialized
                        | bray_bound_tree::StorageOperationStatus::Moved
                )
            }),
            "{analysis:?}"
        );
    }

    #[test]
    fn replacement_plans_capture_old_state_after_rhs_transfers() {
        use bray_bound_tree::{AsyncStorageCleanupRequirement, StorageReplacementState};

        for (body, expected, partial) in [
            (
                "let mut value: Guard = Guard {}; value = Guard {};",
                StorageReplacementState::Present,
                false,
            ),
            (
                "let mut value: Guard = Guard {}; let taken = value; value = Guard {};",
                StorageReplacementState::Absent,
                false,
            ),
            (
                "let mut value: Guard = Guard {}; value = value;",
                StorageReplacementState::Absent,
                false,
            ),
            (
                "let mut value: Guard = Guard {}; if flag { take(value); } value = Guard {};",
                StorageReplacementState::Conditional,
                false,
            ),
            (
                "let mut value: Pair = Pair { left = Guard {}, right = Guard {} }; let taken = value.left; value = Pair { left = Guard {}, right = Guard {} };",
                StorageReplacementState::Conditional,
                true,
            ),
        ] {
            let source = format!(
                "module app; struct Guard {{ destruct() {{}} }} struct Pair {{ left: Guard; right: Guard; }} func take(pos value: Guard) {{}} func probe(pos flag: bool) {{ {body} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_function_body_key(&compilation, "probe");
            let flow = compilation.storage_flow(key.clone()).unwrap();

            let [decision] = flow.value().replacements() else {
                panic!("one assignment must publish one old-state decision: {flow:?}");
            };

            assert_eq!(decision.state(), expected, "{source}");
            let analysis = compilation.async_analysis(key.clone()).unwrap();

            let [plan] = analysis.value().replacements() else {
                panic!("one assignment must publish one cleanup plan: {analysis:?}");
            };

            assert_eq!(plan.expression(), decision.expression());
            assert_eq!(plan.access(), decision.access());
            assert_eq!(plan.parts().is_some(), partial, "{source}");

            assert_eq!(
                plan.cleanup() == AsyncStorageCleanupRequirement::None,
                expected == StorageReplacementState::Absent,
                "{source}"
            );

            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                lowered
                    .value()
                    .as_ref()
                    .and_then(|unit| unit.mir())
                    .is_some(),
                "{source}: {lowered:?}"
            );
        }
    }

    #[test]
    fn replacement_verification_rejects_missing_and_contradictory_cleanup() {
        use bray_bound_tree::{
            AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause,
            StorageReplacementDecision, StorageReplacementPlan, StorageReplacementState,
        };

        use bray_lowering::{LoweringPlanFailureCause, LoweringPlanKind, VerifiedLoweringPlans};

        let compilation = compilation(
            "module app; struct Guard { destruct() {} } func probe() { let mut value: Guard = Guard {}; value = Guard {}; }",
        );

        let key = source_function_body_key(&compilation, "probe");
        let bound = compilation.bound_unit(key.clone()).unwrap();
        let storage = compilation.storage_plan(key.clone()).unwrap();
        let liveness = compilation.liveness(key.clone()).unwrap();
        let flow = compilation.storage_flow(key.clone()).unwrap();
        let dependencies = compilation.dependency_contracts(key.clone()).unwrap();
        let selections = compilation.semantic_selections(key.clone()).unwrap();
        let analysis = compilation.async_analysis(key).unwrap();

        let verify = |flow: &bray_bound_tree::StorageFlow,
                      analysis: &bray_bound_tree::CheckedAsync| {
            VerifiedLoweringPlans::try_new(
                bound.value(),
                storage.value(),
                liveness.value(),
                flow,
                dependencies.value(),
                selections.value(),
                compilation.available_compiler_known_symbols(),
                analysis,
            )
            .map(|_| ())
            .err()
        };

        assert_eq!(verify(flow.value(), analysis.value()), None);
        let missing_flow = flow.value().clone().with_replacements([]).unwrap();
        let missing_plan = analysis.value().clone().with_replacements([]).unwrap();

        for failure in [
            verify(&missing_flow, analysis.value()),
            verify(flow.value(), &missing_plan),
        ] {
            let failure = failure.unwrap();
            assert_eq!(failure.kind(), LoweringPlanKind::Replacement);
            assert_eq!(failure.cause(), LoweringPlanFailureCause::Missing);
        }

        let plan = &analysis.value().replacements()[0];

        let contradictory = StorageReplacementDecision::new(
            plan.expression(),
            plan.access(),
            StorageReplacementState::Present,
            [plan.access()],
            false,
        );

        let contradictory = flow
            .value()
            .clone()
            .with_replacements([contradictory])
            .unwrap();

        let failure = verify(&contradictory, analysis.value()).unwrap();

        assert_eq!(failure.kind(), LoweringPlanKind::Replacement);
        assert_eq!(failure.cause(), LoweringPlanFailureCause::Contradictory);

        for (cleanup, cause) in [
            (
                AsyncStorageCleanupRequirement::None,
                LoweringPlanFailureCause::Contradictory,
            ),
            (
                AsyncStorageCleanupRequirement::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                ),
                LoweringPlanFailureCause::StorageRecovery(
                    AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                ),
            ),
        ] {
            let malformed =
                StorageReplacementPlan::new(plan.expression(), plan.access(), cleanup, None);

            let malformed = analysis
                .value()
                .clone()
                .with_replacements([malformed])
                .unwrap();

            let failure = verify(flow.value(), &malformed).unwrap();
            assert_eq!(failure.kind(), LoweringPlanKind::Replacement);
            assert_eq!(failure.cause(), cause);
        }
    }

    #[test]
    fn replacement_native_fixture_lowers_checked_cleanup_paths() {
        let compilation = compilation(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/value-replacement.bray"
        )));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in [
            "ordinary_scope_cleanup",
            "heap_replacement",
            "pending_return",
            "abandoned_return",
            "return_across_catch",
            "exited_catch_does_not_handle_outer_cleanup",
            "owned_cancellation",
            "borrowed_task",
            "borrowed_cancellation",
            "main",
        ] {
            let lowered = compilation.lowered_unit(source_function_body_key(&compilation, name));
            assert!(lowered.is_ok(), "{name}: {lowered:?}");
            assert!(lowered.unwrap().value().is_some(), "{name}");
        }
    }

    #[test]
    fn replacement_lowering_covers_borrowed_projected_and_nullable_destinations() {
        for body in [
            "slot.value = Guard {};",
            "let mut value: Guard? = Guard {}; value = none;",
            "let mut values: [Guard; 2] = [Guard {}, Guard {}]; values[index()] = Guard {};",
        ] {
            let source = format!(
                "module app; struct Guard {{ destruct() {{}} }} struct Slot {{ mut value: Guard; }} func index() -> usize {{ return 1; }} func probe(pos slot: &mut Slot) {{ {body} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_function_body_key(&compilation, "probe");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                lowered
                    .value()
                    .as_ref()
                    .and_then(|unit| unit.mir())
                    .is_some(),
                "{source}: {lowered:?}"
            );
        }
    }

    #[test]
    fn earlier_operands_survive_later_short_circuit_and_conditional_blocks() {
        for body in [
            "return (value == first) == (first || second);",
            "return compare(value == first, first && second);",
            "let values: [bool; 2] = [value == first, first || second]; return values[0];",
            "return (value == first) == (if first { yield second; } else { yield value; });",
        ] {
            let source = format!(
                "module app; func compare(pos left: bool, pos right: bool) -> bool {{ return left == right; }} func probe(pos value: bool, pos first: bool, pos second: bool) -> bool {{ {body} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_function_body_key(&compilation, "probe");

            let lowered = compilation
                .lowered_unit(key)
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));

            assert!(
                lowered
                    .value()
                    .as_ref()
                    .and_then(|unit| unit.mir())
                    .is_some(),
                "{source}: {lowered:?}"
            );
        }
    }

    #[test]
    fn partial_cleanup_ignores_repaired_moves_at_retained_inner_exits() {
        for repair in ["outer.inner.first = replacement;", ""] {
            let source = format!(
                "module app; struct Guard {{ destruct() {{}} }} struct Inner {{ mut first: Guard; second: Guard; }} struct Outer {{ mut inner: Inner; other: Guard; }} func take(pos value: Guard) {{}} func probe(pos mut outer: Outer, pos replacement: Guard, pos flag: bool) {{ {{ let taken: Guard = outer.inner.first; }} {repair} if flag {{ take(outer.other); }} }}",
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_function_body_key(&compilation, "probe");
            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(
                lowered
                    .value()
                    .as_ref()
                    .and_then(|unit| unit.mir())
                    .is_some(),
                "{source}: {lowered:?}"
            );
        }
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

        let analysis = match compilation.storage_flow(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("storage-flow checking must publish: {error:?}"),
        };

        assert!(
            analysis
                .value()
                .operations()
                .iter()
                .any(|operation| operation.purpose() == StorageAccessPurpose::Copy),
            "{analysis:?}"
        );

        assert!(
            analysis
                .value()
                .operations()
                .iter()
                .all(|operation| operation.status()
                    != bray_bound_tree::StorageOperationStatus::Moved),
            "{analysis:?}"
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
            "        yield value;\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        yield value;\n",
            "    };\n",
            "    loop\n",
            "    {\n",
            "        if condition\n",
            "        {\n",
            "            break;\n",
            "        }\n",
            "        value;\n",
            "    }\n",
            "    return selected;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let analysis = match compilation.liveness(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("cyclic liveness analysis must converge: {error:?}"),
        };

        assert!(!analysis.value().last_uses().is_empty(), "{analysis:#?}");
    }

    #[test]
    fn refinements_are_lazy_cached_and_retain_branch_conditions() {
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
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.body_semantics.is_published(&key),
            Ok(false)
        );

        let first = match compilation.refinements(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("refinement analysis must publish: {error:?}"),
        };

        assert!(
            first.value().occurrences().iter().any(|occurrence| {
                occurrence.refinements().iter().any(|refinement| {
                    matches!(
                        refinement.kind(),
                        RefinementKind::Condition { value: true, .. }
                    )
                })
            }),
            "{first:?}"
        );

        let second = match compilation.refinements(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated refinement analysis must publish: {error:?}"),
        };

        assert_eq!(first.snapshot_address(), second.snapshot_address());

        let dependencies = match compilation
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
        {
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
            "    }\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let analysis = match compilation.liveness(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("scope liveness analysis must publish: {error:?}"),
        };

        assert!(
            analysis
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
    fn implementation_self_parameter_storage_uses_the_checked_body_type() {
        for borrow in ["&", "&mut "] {
            let source = format!(
                "module app; struct Holder {{ value: i32; }} trait Forward {{ static func forward(pos value: {borrow}Self) -> {borrow}Self; }} impl Holder(Forward) {{ static func forward(pos value: {borrow}Self) -> {borrow}Self {{ return value; }} }}"
            );

            let compilation = compilation(&source);

            assert!(!compilation.check_diagnostics().has_errors(), "{source}");

            let key = source_trait_callable_fulfillment_body_key(&compilation, "forward");

            let plan = compilation.storage_plan(key.clone()).unwrap();
            let values = compilation.semantic_value_store().unwrap();
            let mut parameters = 0;

            for (identity, storage) in plan.value().identity_entries() {
                if !matches!(storage, bray_bound_tree::StorageIdentity::Parameter(_)) {
                    continue;
                }

                parameters += 1;

                let ty = plan.value().storage_type(identity).unwrap();
                let data = values.type_data(ty).unwrap();

                let TypeData::Borrow { target, .. } = data.as_ref() else {
                    panic!("parameter must remain borrowed: {data:?}");
                };

                assert!(matches!(
                    values.type_data(*target).unwrap().as_ref(),
                    TypeData::Named { .. }
                ));

                let lowered = compilation.lowered_unit(key.clone()).unwrap();
                let mir = lowered.value().as_ref().unwrap().mir().unwrap();

                assert_eq!(
                    mir.storages()
                        .iter()
                        .find(|storage| matches!(
                            storage.kind(),
                            bray_ir::MirStorageKind::Parameter(0)
                        ))
                        .unwrap()
                        .ty(),
                    ty
                );
            }

            assert_eq!(parameters, 1);
        }
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

        let analysis = match compilation.declared_value_type_templates(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("declared value types must publish: {error:?}"),
        };

        assert!(has_value_kind(
            analysis.value(),
            SymbolKind::GenericConstParameter
        ));

        assert!(has_value_kind(
            analysis.value(),
            SymbolKind::CallableParameter
        ));

        assert!(has_value_kind(analysis.value(), SymbolKind::LocalConstant));

        assert!(
            analysis
                .value()
                .evidence()
                .iter()
                .any(|entry| matches!(entry.term(), DeclaredValueTypeTerm::Pattern(_)))
        );

        assert!(matches!(
            analysis.value().callable_result(),
            Some(TypeExpressionTemplate::Array { .. })
        ));

        assert!(has_constraint_kind(
            analysis.value(),
            DeclaredValueTypeConstraintKind::Initializer
        ));

        assert!(has_constraint_kind(
            analysis.value(),
            DeclaredValueTypeConstraintKind::PatternBinding
        ));

        assert!(has_constraint_kind(
            analysis.value(),
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

        let nested_templates = match compilation.declared_value_type_templates(nested.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("anonymous callable value types must publish: {error:?}"),
        };

        assert!(has_value_kind(
            nested_templates.value(),
            SymbolKind::AnonymousCallableParameter
        ));

        assert!(has_value_kind(
            nested_templates.value(),
            SymbolKind::GenericConstParameter
        ));

        assert!(matches!(
            nested_templates.value().callable_result(),
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

        let constant = types_for_kind(&compilation, &keys, BoundUnitKind::ConstantTemplate);
        assert!(has_value_kind(constant.value(), SymbolKind::Constant));

        assert!(has_constraint_kind(
            constant.value(),
            DeclaredValueTypeConstraintKind::Initializer
        ));

        let predicate = types_for_kind(&compilation, &keys, BoundUnitKind::PredicateDefinition);

        assert!(has_value_kind(
            predicate.value(),
            SymbolKind::PredicateParameter
        ));

        let receiver = keys
            .iter()
            .filter(|key| key.kind() == BoundUnitKind::CallableBody)
            .filter_map(|key| compilation.declared_value_type_templates(key.clone()).ok())
            .find(|analysis| has_value_kind(analysis.value(), SymbolKind::ReceiverParameter))
            .unwrap_or_else(|| panic!("type callable body must publish receiver evidence"));

        assert!(receiver.value().callable_result().is_some());

        let contract = types_for_kind(&compilation, &keys, BoundUnitKind::ContractClause);

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
                let analysis = match compilation.declared_value_type_templates(key.clone()) {
                    Ok(analysis) => analysis,
                    Err(error) => panic!("runtime default types must publish: {error:?}"),
                };

                let initializer = analysis
                    .value()
                    .constraints()
                    .iter()
                    .find(|constraint| {
                        constraint.kind()
                            == bray_bound_tree::DeclaredValueTypeConstraintKind::Initializer
                    })
                    .expect("runtime default must identify its initialized parameter");

                let evidence = analysis
                    .value()
                    .evidence()
                    .iter()
                    .find(|evidence| evidence.term() == initializer.right())
                    .expect(
                        "runtime default must publish the initialized parameter's declared type",
                    );

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
            Ok(analysis) => analysis,
            Err(error) => panic!("recovered declared value types must publish: {error:?}"),
        };

        let second = match compilation.declared_value_type_templates(key) {
            Ok(analysis) => analysis,
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
    fn expression_typing_and_call_selection_converge_into_cached_results() {
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
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
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

        let selection = only_call_selection(selections.value());

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

        let literals = compilation
            .literal_values(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("literal values must publish: {error:?}"));

        assert_eq!(types.snapshot_address(), repeated_types.snapshot_address());

        assert_eq!(
            selections.snapshot_address(),
            repeated_selections.snapshot_address()
        );

        assert_eq!(types.snapshot_address(), selections.snapshot_address());
        assert_eq!(types.snapshot_address(), literals.snapshot_address());
    }

    #[test]
    fn contextual_numeric_operations_do_not_retain_provisional_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func padding(pos width: usize, pos length: usize) -> usize\n",
            "{\n",
            "    let remaining: usize = width - length;\n",
            "\n",
            "    return remaining;\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
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

        let selection = only_call_selection(selections.value());

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

        let flow = match compilation.storage_flow(source_callable_body_key(&compilation)) {
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
    fn callback_state_requires_the_exported_entry_context_parameter() {
        let compilation = standard_callback_compilation(
            r#"trusted module std.test;

@symbol(name = "valid_callback")
@abi(c)
trusted func valid_callback(pos context: RawPointer<i32>) -> bool uses(raw_memory)
{
    let state = trusted std.ffi.callback_state<i32>(context);
    return state == 0;
}

@abi(c)
trusted func unexported_callback(pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state;
}

@symbol(name = "misplaced_context")
@abi(c)
trusted func misplaced_context(pos value: i32, pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state + value;
}

@symbol(name = "bray_abi_context")
trusted func bray_abi_context(pos context: RawPointer<i32>) -> i32 uses(raw_memory)
{
    let state: &i32 = trusted std.ffi.callback_state<i32>(context);
    return state;
}
"#,
        );

        let valid = compilation
            .memory_operations(source_function_body_key(&compilation, "valid_callback"))
            .unwrap_or_else(|error| {
                panic!("valid callback memory analysis must publish: {error:?}")
            });

        assert!(valid.diagnostics().is_empty(), "{valid:#?}");

        assert!(matches!(
            valid.value().operations()[0].kind(),
            CheckedMemoryOperationKind::CallbackState { .. }
        ));

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "valid_callback"))
            .unwrap_or_else(|error| panic!("valid callback must lower: {error:#?}"));

        assert!(lowered.diagnostics().is_empty(), "{lowered:#?}");

        for name in [
            "unexported_callback",
            "misplaced_context",
            "bray_abi_context",
        ] {
            let invalid = compilation
                .memory_operations(source_function_body_key(&compilation, name))
                .unwrap_or_else(|error| {
                    panic!("invalid callback memory analysis must publish: {error:?}")
                });

            assert!(
                invalid
                    .diagnostics()
                    .by_kind(DiagnosticKind::CheckingInvalidCallbackStateContext)
                    .next()
                    .is_some(),
                "{name}: {invalid:#?}"
            );

            assert_goal_state_diagnostic_kind(
                invalid.diagnostics(),
                DiagnosticKind::CheckingInvalidCallbackStateContext,
            );
        }
    }

    fn standard_callback_compilation(source: &str) -> Compilation {
        let package = PackageIdentity::try_new("std")
            .unwrap_or_else(|| panic!("standard library identity must be valid"));

        let request = CompilationRequest::new(
            package,
            vec![
                source_input(
                    include_str!("../../../../../standard-library/std/src/std.bray"),
                    0,
                ),
                source_input(
                    include_str!("../../../../../standard-library/std/src/memory.bray"),
                    1,
                ),
                source_input(
                    include_str!("../../../../../standard-library/std/src/ffi/callback.bray"),
                    2,
                ),
                source_input(source, 3),
            ],
        )
        .with_standard_library_source_authority();

        Compilation::load(request)
            .unwrap_or_else(|error| panic!("standard callback compilation must load: {error:?}"))
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

        assert_eq!(operation.kind(), CheckedMemoryOperationKind::RawAllocate);

        let flow = compilation
            .storage_flow(source_callable_body_key(&compilation))
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

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        );
    }

    #[test]
    fn invalid_target_control_contracts_publish_structured_diagnostics() {
        let compilation = compilation(concat!(
            "trusted module app;\n",
            "trusted func main() -> i32\n",
            "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
            "{\n",
            "    return trusted core.target.assembly<((i32, i32), ), (i32, )>(\n",
            "        template = \"\",\n",
            "        constraints = \"=reg,reg\",\n",
            "        clobbers = \"\",\n",
            "        features = \"\",\n",
            "        options = 0,\n",
            "        inputs = ((1, 2),),\n",
            "    ).0;\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("target-control memory operations must publish: {error:?}")
            });

        assert!(operations.value().operations().is_empty());

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingInvalidTargetControlContract,
        );
    }

    #[test]
    fn invalid_atomic_orders_publish_structured_diagnostics_before_lowering() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<u32>(1);\n",
            "    let value = core.atomic.load<u32, 2>(&storage);\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

        assert_eq!(operations.value().operations().len(), 1);

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingInvalidAtomicMemoryOrder,
        );
    }

    #[test]
    fn open_generic_atomic_operations_defer_representation_validation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func initialize<T>(pos value: T) -> core.atomic.Atomic<T>\n",
            "{\n",
            "    return core.atomic.initialize<T>(value);\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_function_body_key(&compilation, "initialize"))
            .unwrap_or_else(|error| {
                panic!("open generic atomic operation must publish: {error:?}")
            });

        assert!(
            operations.diagnostics().is_empty(),
            "{:#?}",
            operations.diagnostics()
        );

        assert_eq!(operations.value().operations().len(), 1);

        assert!(matches!(
            operations.value().operations()[0].kind(),
            CheckedMemoryOperationKind::AtomicInitialize { .. }
        ));
    }

    #[test]
    fn unavailable_atomic_representations_fail_before_lowering() {
        let compilation = compilation_with_target_operations(
            concat!(
                "module app;\n",
                "func main()\n",
                "{\n",
                "    let storage = core.atomic.initialize<u128>(1);\n",
                "}\n",
            ),
            true,
            true,
        );

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

        assert!(operations.value().operations().is_empty());

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        );
    }

    #[test]
    fn non_atomic_scalar_representations_fail_before_lowering() {
        let compilation = compilation_with_target_operations(
            concat!(
                "module app;\n",
                "func main()\n",
                "{\n",
                "    let storage = core.atomic.initialize<r32>(1.0);\n",
                "}\n",
            ),
            true,
            true,
        );

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("atomic memory operations must publish: {error:?}"));

        assert!(operations.value().operations().is_empty());

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        );
    }

    #[test]
    fn transparent_integer_representations_support_atomic_fetch_operations() {
        let compilation = compilation_with_target_operations(
            concat!(
                "module app;\n",
                "@copy\n",
                "@layout(transparent)\n",
                "struct Counter\n",
                "{\n",
                "    value: u32;\n",
                "}\n",
                "func main()\n",
                "{\n",
                "    let storage = core.atomic.initialize<Counter>(Counter { value = 1 });\n",
                "    let previous = core.atomic.fetch_add<Counter, 0>(\n",
                "        &storage, Counter { value = 2 });\n",
                "}\n",
            ),
            true,
            true,
        );

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("transparent atomic operations must publish: {error:?}")
            });

        assert!(
            operations.diagnostics().is_empty(),
            "{:#?}",
            operations.diagnostics()
        );

        assert_eq!(operations.value().operations().len(), 2);

        assert!(matches!(
            operations.value().operations()[1].kind(),
            CheckedMemoryOperationKind::AtomicFetch {
                kind: bray_bound_tree::AtomicFetchKind::Add,
                ..
            }
        ));
    }

    #[test]
    fn target_sized_integer_fetch_uses_integer_atomic_representation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<usize>(1);\n",
            "    let previous = core.atomic.fetch_add<usize, 0>(&storage, 2);\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("target-sized atomic operations must publish: {error:?}")
            });

        assert!(
            operations.diagnostics().is_empty(),
            "{:#?}",
            operations.diagnostics()
        );

        assert_eq!(operations.value().operations().len(), 2);

        assert!(matches!(
            operations.value().operations()[1].kind(),
            CheckedMemoryOperationKind::AtomicFetch {
                kind: bray_bound_tree::AtomicFetchKind::Add,
                ..
            }
        ));
    }

    #[test]
    fn plain_storage_representations_support_non_fetch_atomic_operations() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@copy\n",
            "@layout(stable)\n",
            "struct Pair\n",
            "{\n",
            "    low: u16;\n",
            "    high: u16;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<Pair>(Pair { low = 1, high = 2 });\n",
            "    let value = core.atomic.load<Pair, 1>(&storage);\n",
            "    let prior = core.atomic.exchange<Pair, 3>(\n",
            "        &storage, Pair { low = 3, high = 4 });\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("plain-storage atomic operations must publish: {error:?}")
            });

        assert!(
            operations.diagnostics().is_empty(),
            "{:#?}",
            operations.diagnostics()
        );

        assert_eq!(operations.value().operations().len(), 3);
    }

    #[test]
    fn padded_plain_storage_atomic_representations_fail_before_lowering() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@copy\n",
            "@layout(stable)\n",
            "struct Padded\n",
            "{\n",
            "    first: u8;\n",
            "    second: u32;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<Padded>(\n",
            "        Padded { first = 1, second = 2 });\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("padded atomic storage must publish: {error:?}"));

        assert!(operations.value().operations().is_empty());

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        );
    }

    #[test]
    fn unsupported_atomic_plain_storage_sizes_fail_before_lowering() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@copy\n",
            "@layout(stable)\n",
            "struct Triple\n",
            "{\n",
            "    first: u8;\n",
            "    second: u8;\n",
            "    third: u8;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let storage = core.atomic.initialize<Triple>(\n",
            "        Triple { first = 1, second = 2, third = 3 });\n",
            "}\n",
        ));

        let operations = compilation
            .memory_operations(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("unsupported atomic storage must publish: {error:?}"));

        assert!(operations.value().operations().is_empty());

        assert_goal_state_diagnostic_kind(
            operations.diagnostics(),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
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
                DiagnosticKind::CheckingMissingTrustedMemoryGuarantees,
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
                .storage_flow(source_callable_body_key(&compilation))
                .unwrap_or_else(|error| {
                    panic!("invalid memory storage flow must publish: {error:?}")
                });

            assert!(
                flow.diagnostics().by_kind(expected).next().is_some(),
                "{expected:?} must be reported: {:?}",
                flow.diagnostics()
            );

            match expected {
                DiagnosticKind::CheckingMissingTrustedMemoryGuarantees => {
                    assert_goal_state_diagnostic_kind(
                        flow.diagnostics(),
                        DiagnosticKind::CheckingMissingTrustedMemoryGuarantees,
                    );
                }
                DiagnosticKind::CheckingUninitializedRawStorage => {
                    assert_goal_state_diagnostic_kind(
                        flow.diagnostics(),
                        DiagnosticKind::CheckingUninitializedRawStorage,
                    );
                }
                DiagnosticKind::CheckingMemoryOperationAfterDeallocation => {
                    assert_goal_state_diagnostic_kind(
                        flow.diagnostics(),
                        DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
                    );
                }
                DiagnosticKind::CheckingDeallocationWithOutstandingObligations => {
                    assert_goal_state_diagnostic_kind(
                        flow.diagnostics(),
                        DiagnosticKind::CheckingDeallocationWithOutstandingObligations,
                    );
                }
                _ => unreachable!("memory-obligation table contains only exact memory kinds"),
            }
        }
    }

    #[test]
    fn invalidated_memory_reports_every_branch_deallocation_origin() {
        let compilation = compilation_with_target_operations(
            concat!(
                "trusted module app;\n",
                "trusted func main(pos condition: bool) uses(manual_alloc, raw_memory, unchecked_init)\n",
                "{\n",
                "    let allocated = trusted core.memory.allocate(bytes = 0, align = 1);\n",
                "    let forwarded = allocated;\n",
                "    let pointer = forwarded;\n",
                "    if condition\n",
                "    {\n",
                "        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
                "    }\n",
                "    else\n",
                "    {\n",
                "        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
                "    }\n",
                "    trusted core.memory.write<u8>(pointer, 1);\n",
                "}\n",
            ),
            true,
            true,
        );

        let flow = compilation
            .storage_flow(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("branch memory flow must publish: {error:?}"));

        let diagnostic = flow
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingMemoryOperationAfterDeallocation)
            .next()
            .unwrap_or_else(|| panic!("post-branch write must report invalidated storage"));

        let origins = diagnostic
            .related_locations()
            .iter()
            .filter(|related| related.kind() == DiagnosticRelatedLocationKind::DeallocationOrigin)
            .collect::<Vec<_>>();

        let allocations = diagnostic
            .related_locations()
            .iter()
            .filter(|related| related.kind() == DiagnosticRelatedLocationKind::AllocationOrigin)
            .collect::<Vec<_>>();

        assert_eq!(origins.len(), 2, "{diagnostic:#?}");
        assert_eq!(allocations.len(), 1, "{diagnostic:#?}");
        assert_ne!(origins[0].span(), origins[1].span());

        assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
        );
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
    fn fixed_array_generators_report_divergent_yield_cardinality() {
        let compilation =
            array_generator_compilation(concat!("        yield item;\n", "        yield item;\n",));

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
        );
    }

    #[test]
    fn fixed_array_generators_require_a_statically_known_source_count() {
        let compilation = array_generator_compilation("        yield item;\n");

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
        );
    }

    #[test]
    fn fixed_array_generators_use_literal_range_cardinality() {
        let matching = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let generated: [i32; 4] = [each item in 0..4\n",
            "    {\n",
            "        yield item;\n",
            "    }];\n",
            "}\n",
        ));

        assert!(
            matching.check_diagnostics().is_empty(),
            "{:#?}",
            matching.check_diagnostics()
        );

        let mismatched = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let generated: [i32; 3] = [each item in 0..4\n",
            "    {\n",
            "        yield item;\n",
            "    }];\n",
            "}\n",
        ));

        assert_goal_state_diagnostic_kind(
            mismatched.check_diagnostics(),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
        );
    }

    fn array_generator_compilation(body: &str) -> Compilation {
        let mut source = String::from(concat!(
            "module app;\n",
            "struct Items\n",
            "{\n",
            "}\n",
            "struct ItemsCursor\n",
            "{\n",
            "}\n",
            "impl &Items(Iterable)\n",
            "{\n",
            "    type Element = bool;\n",
            "    type Cursor = ItemsCursor;\n",
            "    consume func iterate() -> ItemsCursor\n",
            "    {\n",
            "    }\n",
            "}\n",
            "impl ItemsCursor(Iterator)\n",
            "{\n",
            "    type Element = bool;\n",
            "    mut func next() -> bool?\n",
            "    {\n",
            "    }\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let items: Items = Items {};\n",
            "    let generated: [bool; 2] = [each item in items\n",
            "    {\n",
        ));

        source.push_str(body);

        source.push_str(concat!("    }];\n", "}\n",));

        compilation(&source)
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
            "    let present: Maybe = Some(value = 1);\n",
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

        assert_goal_state_diagnostic_kind(
            selections.diagnostics(),
            DiagnosticKind::CheckingIncompatibleCandidate,
        );

        let diagnostic = selections
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
            .next()
            .unwrap_or_else(|| panic!("incompatible construction diagnostic must exist"));

        let rejections = selection_rejections(diagnostic);

        assert_eq!(rejections.rejections().len(), 1);
        assert_eq!(rejections.omitted_count(), 0);

        assert!(matches!(
            rejections.rejections()[0].reason(),
            DiagnosticSelectionRejectionReason::ConstructionInput(
                DiagnosticConstructionInputRejection::UnknownName { provided, accepted }
            ) if provided == "y" && accepted.as_ref() == [String::from("x")]
        ));
    }

    #[test]
    fn mutable_receiver_authority_reaches_mutable_projected_fields() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Writer\n",
            "{\n",
            "    mut func write(pos value: i32);\n",
            "}\n",
            "struct Wrapper<Sink>\n",
            "{\n",
            "    mut sink: Sink;\n",
            "}\n",
            "impl Wrapper<Sink>\n",
            "{\n",
            "    mut func forward()\n",
            "        with(Sink: Writer)\n",
            "    {\n",
            "        self.sink.write(1);\n",
            "    }\n",
            "}\n",
        ));

        let key = source_type_callable_member_body_key(&compilation, "forward");

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("method selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        assert!(selections.value().entries().iter().any(|entry| {
            matches!(
                entry.selection(),
                SemanticSelection::Call(call) if call.resolution().trait_dispatch().is_some()
            )
        }));
    }

    #[test]
    fn trait_default_bodies_dispatch_calls_through_the_implementing_witness() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Counter\n",
            "{\n",
            "    func count() -> i32;\n",
            "    func doubled() -> i32\n",
            "    {\n",
            "        return self.count() + self.count();\n",
            "    }\n",
            "}\n",
        ));

        let key =
            crate::test_support::source_trait_callable_member_body_key(&compilation, "doubled");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("trait default selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let calls = selections
            .value()
            .entries()
            .iter()
            .filter(|entry| {
                matches!(
                    entry.selection(),
                    SemanticSelection::Call(call) if call.resolution().trait_dispatch().is_some()
                )
            })
            .count();

        assert_eq!(calls, 2);
    }

    #[test]
    fn omitted_trait_members_select_the_trait_owned_default_body() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Counter\n",
            "{\n",
            "    func count() -> i32;\n",
            "    func doubled() -> i32\n",
            "    {\n",
            "        return self.count() + self.count();\n",
            "    }\n",
            "}\n",
            "struct Value\n",
            "{\n",
            "    count: i32;\n",
            "}\n",
            "impl ValueCounter = Value(Counter)\n",
            "{\n",
            "    func count() -> i32\n",
            "    {\n",
            "        return self.count;\n",
            "    }\n",
            "}\n",
            "using ValueCounter;\n",
            "func read(pos value: &Value) -> i32\n",
            "{\n",
            "    return value.doubled();\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "read");

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("trait default call selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let selected = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                SemanticSelection::Call(call) => Some(call.resolution()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("trait default call must be selected"));

        assert!(matches!(
            selected.target(),
            BoundCallableTarget::Declaration(instance)
                if matches!(
                    instance.definition().symbol(),
                    bray_symbols::AnySymbolId::TraitCallableMember(_)
                )
        ));

        assert_eq!(selected.implementation_witnesses().len(), 1);

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("trait default call must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn selected_trait_defaults_normalize_associated_types_from_the_witness() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Sequence\n",
            "{\n",
            "    type Element;\n",
            "    func compare(pos value: &Element) -> bool;\n",
            "    func contains(pos value: &Element) -> bool\n",
            "    {\n",
            "        return self.compare(value);\n",
            "    }\n",
            "}\n",
            "struct Numbers {}\n",
            "impl NumbersSequence = Numbers(Sequence)\n",
            "{\n",
            "    type Element = i32;\n",
            "    func compare(pos value: &i32) -> bool\n",
            "    {\n",
            "        return value == 1;\n",
            "    }\n",
            "}\n",
            "using NumbersSequence;\n",
            "func read(pos numbers: &Numbers, pos value: &i32) -> bool\n",
            "{\n",
            "    return numbers.contains(value);\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "read");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("trait default selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "associated types in the selected default signature must resolve: {:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn participating_trait_methods_bind_direct_generic_arguments() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Mapper\n",
            "{\n",
            "    func default_map<T>(pos value: T) -> T\n",
            "    {\n",
            "        return value;\n",
            "    }\n",
            "    func explicit_map<T>(pos value: T) -> T;\n",
            "}\n",
            "struct Value {}\n",
            "impl ValueMapper = Value(Mapper)\n",
            "{\n",
            "    func explicit_map<T>(pos value: T) -> T\n",
            "    {\n",
            "        return value;\n",
            "    }\n",
            "}\n",
            "using ValueMapper;\n",
            "func read(pos value: &Value) -> i32\n",
            "{\n",
            "    return value.default_map<i32>(1) + value.explicit_map<i32>(2);\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "read");

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("generic trait calls must select: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("generic trait calls must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn participating_trait_methods_report_each_ambiguous_implementation() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait First\n",
            "{\n",
            "    func inspect() -> i32\n",
            "    {\n",
            "        return 1;\n",
            "    }\n",
            "}\n",
            "trait Second\n",
            "{\n",
            "    func inspect() -> i32\n",
            "    {\n",
            "        return 2;\n",
            "    }\n",
            "}\n",
            "struct Value {}\n",
            "impl ValueFirst = Value(First) {}\n",
            "impl ValueSecond = Value(Second) {}\n",
            "func read(pos value: &Value) -> i32\n",
            "{\n",
            "    return value.inspect();\n",
            "}\n",
        ));

        let selections = compilation
            .semantic_selections(source_function_body_key(&compilation, "read"))
            .unwrap_or_else(|error| panic!("ambiguous trait method must recover: {error:?}"));

        let diagnostics = selections.diagnostics();
        let mut ambiguous = diagnostics.by_kind(DiagnosticKind::CheckingAmbiguousCandidate);

        let Some(diagnostic) = ambiguous.next() else {
            panic!("one ambiguous trait method diagnostic expected: {diagnostics:?}");
        };

        assert!(ambiguous.next().is_none(), "{diagnostics:?}");

        let Some(DiagnosticArgValue::SelectionCandidates(candidates)) = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::SelectionCandidates)
            .map(DiagnosticArg::value)
        else {
            panic!("ambiguous trait method must retain its implementations: {diagnostic:?}");
        };

        assert_eq!(candidates.omitted_count(), 0);
        assert_eq!(candidates.candidates().len(), 2);
        assert_eq!(diagnostic.related_locations().len(), 2);

        assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingAmbiguousCandidate);
    }

    #[test]
    fn mutable_borrow_receivers_reborrow_for_mutable_methods() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Writer\n",
            "{\n",
            "    mut func write(pos value: i32);\n",
            "}\n",
            "func forward<Sink>(pos sink: &mut Sink)\n",
            "    with(Sink: Writer)\n",
            "{\n",
            "    sink.write(1);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("borrowed receiver selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("borrowed receiver call must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn generic_calls_infer_arguments_from_trait_fulfillment_receivers() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Reader\n",
            "{\n",
            "    mut func read(pos destination: &mut [i32]) -> i32;\n",
            "}\n",
            "struct BufferedReader<Source>\n",
            "{\n",
            "    mut source: Source;\n",
            "}\n",
            "func read_buffered<Source>(\n",
            "    pos reader: &mut BufferedReader<Source>,\n",
            "    pos destination: &mut [i32],\n",
            ") -> i32\n",
            "    with(Source: Reader)\n",
            "{\n",
            "    return 0;\n",
            "}\n",
            "impl BufferedReaderSourceReader = BufferedReader<Source>(Reader)\n",
            "    with(Source: Reader)\n",
            "{\n",
            "    mut func read(pos destination: &mut [i32]) -> i32\n",
            "    {\n",
            "        return read_buffered(&mut self, destination);\n",
            "    }\n",
            "}\n",
        ));

        let key = source_trait_callable_fulfillment_body_key(&compilation, "read");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("fulfillment selection must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn active_generic_constraints_enable_constrained_implementation_candidates() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Compares<T> {}\n",
            "trait ItemKey<Key> {}\n",
            "struct Item<Key, Value> {}\n",
            "impl ItemKeyImplementation = Item<Key, Value>(ItemKey<Key>)\n",
            "    with(Key: Compares<Key>)\n",
            "{}\n",
            "func search<Stored, Key>()\n",
            "    with(Stored: ItemKey<Key>)\n",
            "{}\n",
            "func use_search<Key, Value>()\n",
            "    with(Key: Compares<Key>)\n",
            "{\n",
            "    search<Item<Key, Value>, Key>();\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "use_search");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("constrained generic call must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        let selected = selections
            .value()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                SemanticSelection::Call(call) => Some(call.resolution()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("constrained generic call must be selected"));

        assert_eq!(selected.implementation_witnesses().len(), 1);
    }

    #[test]
    fn missing_generic_constraint_evidence_remains_anchored_to_the_call() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Compares<T> {}\n",
            "trait ItemKey<Key> {}\n",
            "struct Item<Key, Value> {}\n",
            "impl ItemKeyImplementation = Item<Key, Value>(ItemKey<Key>)\n",
            "    with(Key: Compares<Key>)\n",
            "{}\n",
            "func search<Stored, Key>()\n",
            "    with(Stored: ItemKey<Key>)\n",
            "{}\n",
            "func use_search<Key, Value>()\n",
            "{\n",
            "    search<Item<Key, Value>, Key>();\n",
            "}\n",
        ));

        let selections = compilation
            .semantic_selections(source_function_body_key(&compilation, "use_search"))
            .unwrap_or_else(|error| panic!("invalid constrained call must recover: {error:?}"));

        let diagnostic = selections
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNoApplicableCandidate)
            .next()
            .unwrap_or_else(|| panic!("missing evidence must reject the call"));

        assert!(diagnostic.primary_span().is_some(), "{diagnostic:#?}");
    }

    #[test]
    fn scope_exits_destroy_plain_storage_with_declared_destructors() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Lock\n",
            "{\n",
            "    acquired: bool;\n",
            "    destruct()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "func use_lock()\n",
            "{\n",
            "    let lock: Lock = Lock\n",
            "    {\n",
            "        acquired = true,\n",
            "    };\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "use_lock");

        let analysis = compilation
            .async_analysis(key)
            .unwrap_or_else(|error| panic!("lifecycle analysis must publish: {error:?}"));

        assert!(
            analysis
                .value()
                .scope_exits()
                .iter()
                .any(|exit| { !exit.lifecycle_resolution().is_empty() }),
            "plans={:#?}",
            analysis.value().scope_exits()
        );
    }

    #[test]
    fn trust_boundaries_preserve_value_transfer_ownership() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Guard\n",
            "{\n",
            "    value: bool;\n",
            "    destruct() {}\n",
            "}\n",
            "trusted internal func make() -> Guard\n",
            "{\n",
            "    return Guard { value = true };\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let guard: Guard = trusted internal make();\n",
            "    guard;\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "main");

        let storage = compilation
            .storage_plan(key.clone())
            .unwrap_or_else(|error| panic!("storage plan must publish: {error:?}"));

        let analysis = compilation
            .async_analysis(key)
            .unwrap_or_else(|error| panic!("lifecycle analysis must publish: {error:?}"));

        let lifecycle_identities = analysis
            .value()
            .scope_exits()
            .iter()
            .flat_map(bray_bound_tree::AsyncScopeExitPlan::lifecycle_resolution)
            .filter_map(|access| storage.value().root_identity(*access))
            .filter_map(|identity| storage.value().identity(identity))
            .collect::<Vec<_>>();

        assert_eq!(
            lifecycle_identities
                .iter()
                .filter(|identity| matches!(identity, StorageIdentity::LocalOwned(_)))
                .count(),
            1
        );

        assert!(
            lifecycle_identities
                .iter()
                .all(|identity| !matches!(identity, StorageIdentity::Temporary(_)))
        );
    }

    #[test]
    fn conditional_whole_moves_lower_guarded_cleanup_and_preserve_return_values() {
        let compilation = compilation(
            r#"module app;
struct Guard
{
    value: bool;
    destruct() {}
}
func take(pos guard: Guard) {}
func conditional(pos flag: bool, pos guard: Guard) -> i32
{
    if flag { take(guard); }
    return 7;
}
func reinitialized(pos flag: bool, pos mut guard: Guard) -> i32
{
    if flag
    {
        take(guard);
        guard = Guard { value = true };
    }
    return 9;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["conditional", "reinitialized"] {
            let key = source_function_body_key(&compilation, name);
            let analysis = compilation.async_analysis(key.clone()).unwrap();
            let lowered = compilation.lowered_unit(key).unwrap();
            assert!(lowered.value().is_some(), "{name}: {lowered:?}");
            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            if name == "conditional" {
                assert!(mir.blocks().iter().any(|block| {
                    block.kind() == bray_ir::MirBlockKind::LifecycleResolution
                        && matches!(
                            block.terminator().kind(),
                            bray_ir::MirTerminatorKind::Branch {
                                condition: bray_ir::MirOperand::Copy(_),
                                ..
                            }
                        )
                }));

                assert!(
                    analysis
                        .value()
                        .scope_exits()
                        .iter()
                        .flat_map(|exit| exit.storage())
                        .any(|decision| matches!(
                            decision.disposition(),
                            bray_bound_tree::AsyncStorageExitDisposition::Cleanup {
                                guard: bray_bound_tree::AsyncCleanupGuard::Initialized,
                                ..
                            }
                        ))
                );
            }
        }
    }

    #[test]
    fn projected_moves_lower_guarded_remainder_cleanup() {
        let compilation = compilation(
            r#"module app;
struct Guard { value: i32; destruct() {} }
struct Pair { mut left: Guard; right: Guard; }
func take(pos guard: Guard) {}
func projected(pos flag: bool, pos pair: Pair) -> i32
{
    if flag { take(pair.left); }
    return 7;
}
func tupled(pos flag: bool, pos pair: (Guard, Guard)) -> i32
{
    if flag { take(pair.0); }
    return 9;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["projected", "tupled"] {
            let key = source_function_body_key(&compilation, name);
            let analysis = compilation.async_analysis(key.clone()).unwrap();

            let partitions = analysis
                .value()
                .storage_requirements()
                .iter()
                .filter_map(bray_bound_tree::AsyncStorageRequirement::parts)
                .collect::<Vec<_>>();

            assert_eq!(partitions.len(), 1, "{name}: {analysis:?}");
            assert_eq!(partitions[0].len(), 2, "{name}: {analysis:?}");

            let lowered = compilation.lowered_unit(key).unwrap();
            assert!(lowered.value().is_some(), "{name}: {lowered:?}");
            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                bray_ir::MirOperationKind::Cleanup { place, .. } if !place.projections().is_empty()
            )));
        }
    }

    #[test]
    fn box_construction_transfers_its_noncopyable_input_storage() {
        let compilation = compilation(include_str!(
            "../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
        ));

        let key = source_function_body_key(&compilation, "main");
        let bound = compilation.bound_unit(key.clone()).unwrap();
        let flow = compilation.storage_flow(key).unwrap();

        let operands = bound
            .value()
            .tree()
            .expressions()
            .find_map(|(_, expression)| match expression {
                BoundExpression::BoxConstruction(expression) => Some(expression.arguments()),
                _ => None,
            })
            .unwrap();

        let [operand, ..] = operands else {
            panic!("fixture must provide a boxed value")
        };

        assert!(
            flow.value().operations().iter().any(|operation| {
                operation.expression() == operand.expression()
                    && operation.purpose() == bray_bound_tree::StorageAccessPurpose::Move
                    && operation.status() == bray_bound_tree::StorageOperationStatus::Valid
            }),
            "box input must transfer ownership: {flow:?}"
        );
    }

    #[test]
    fn partially_moved_box_projects_through_policy_and_releases_on_both_exits() {
        let source = include_str!(
            "../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
        );

        let (declarations, _) = source.split_once("func main").unwrap();

        let compilation = compilation(declarations);
        let key = source_function_body_key(&compilation, "boxed");
        let analysis = compilation.async_analysis(key.clone()).unwrap();

        let releases = analysis
            .value()
            .storage_requirements()
            .iter()
            .flat_map(|requirement| requirement.parts().into_iter().flatten())
            .filter_map(bray_bound_tree::StorageCleanupPart::release)
            .map(|call| call.callable())
            .collect::<Vec<_>>();

        assert_eq!(releases.len(), 1, "{analysis:?}");

        let lowered = compilation.lowered_unit(key).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let releases = mir.operations().iter().filter(|operation| matches!(operation.kind(),
            bray_ir::MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::Direct(reference) if releases.contains(&reference.instance()))
        )).count();

        assert_eq!(
            releases, 5,
            "normal exit, body panic/cancellation and cleanup panic/cancellation must each release: {mir:?}"
        );

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            bray_ir::MirOperationKind::Cleanup { place, .. } if place.projections().first().is_some_and(|projection| projection.kind() == &bray_ir::MirProjectionKind::Dereference)
        )), "surviving fields must use the policy's borrowed target: {mir:?}");
    }

    #[test]
    fn box_patterns_select_borrows_independently_of_partial_cleanup() {
        let source = include_str!(
            "../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
        );

        let (declarations, _) = source.split_once("func main").unwrap();

        let compilation = compilation(declarations);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for (name, kind) in [
            ("observe_box", bray_symbols::BorrowKind::Shared),
            ("observe_borrowed_box", bray_symbols::BorrowKind::Shared),
            ("move_box_target", bray_symbols::BorrowKind::Mutable),
        ] {
            let key = source_function_body_key(&compilation, name);
            let storage = compilation.storage_plan(key.clone()).unwrap();
            let calls = storage.value().owned_borrows().collect::<Vec<_>>();

            assert_eq!(calls.len(), 2, "{name}: {storage:?}");

            let (_, _, selected) = calls
                .iter()
                .find(|(_, candidate, _)| *candidate == kind)
                .unwrap();

            let lowered = compilation.lowered_unit(key.clone()).unwrap();
            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                MirOperationKind::Call(call) if matches!(call.target(),
                    bray_ir::MirCallTarget::Direct(reference) if reference.instance() == selected.callable())
            )), "{name} must use the selected {kind:?} policy borrow: {mir:?}");

            if matches!(name, "observe_box" | "observe_borrowed_box") {
                assert!(!mir.operations().iter().any(|operation| matches!(operation.kind(),
                    MirOperationKind::Store { destination, .. } if !destination.projections().is_empty()
                )), "observed aliases must not write back to source storage: {mir:?}");

                let analysis = compilation.async_analysis(key).unwrap();

                assert!(
                    analysis
                        .value()
                        .storage_requirements()
                        .iter()
                        .all(|requirement| requirement.parts().is_none()),
                    "observation must not require partial cleanup metadata: {analysis:?}"
                );
            }
        }
    }

    #[test]
    fn guarded_part_fixture_preserves_payload_and_nullable_cleanup() {
        let compilation = compilation(include_str!(
            "../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in [
            "fields",
            "tupled",
            "payload",
            "nullable",
            "destructed",
            "indexed",
            "indexed_return",
            "nested_indexed",
            "indexed_repaired",
            "guarded",
            "guarded_alternative",
            "boxed",
            "boxed_panic",
            "observe_box",
            "observe_borrowed_box",
            "move_box_target",
            "main",
        ] {
            let key = source_function_body_key(&compilation, name);
            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(lowered.value().is_some(), "{name}: {lowered:?}");

            if matches!(name, "payload" | "nullable") {
                let mir = lowered.value().as_ref().unwrap().mir().unwrap();

                assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                    bray_ir::MirOperationKind::Cleanup { place, .. } if !place.projections().is_empty()
                )), "{name}: {mir:?}");
            }
        }
    }

    #[test]
    fn borrowed_box_patterns_do_not_grant_target_ownership() {
        let source = include_str!(
            "../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
        );

        let (declarations, _) = source.split_once("func main").unwrap();

        let source = format!(
            "{declarations}\nfunc invalid_borrow_move(pos pair: &box[PairStorage] Pair) {{ let box(value) = pair; consume_pair(value); }}"
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics
                .by_kind(DiagnosticKind::CheckingMissingStorageOwnership)
                .next()
                .is_some(),
            "{diagnostics:?}"
        );

        let key = source_function_body_key(&compilation, "invalid_borrow_move");
        assert!(compilation.lowered_unit(key).unwrap().value().is_none());
    }

    #[test]
    fn mutable_trait_fulfillment_receivers_authorize_field_assignment() {
        let compilation = compilation(concat!(
            "module app;\n",
            "trait Swappable\n",
            "{\n",
            "    mut func swap();\n",
            "}\n",
            "struct Pair\n",
            "{\n",
            "    mut left: i32;\n",
            "    mut right: i32;\n",
            "}\n",
            "impl PairSwappable = Pair(Swappable)\n",
            "{\n",
            "    mut func swap()\n",
            "    {\n",
            "        let left: i32 = self.left;\n",
            "\n",
            "        self.left = self.right;\n",
            "        self.right = left;\n",
            "    }\n",
            "}\n",
        ));

        let key = source_trait_callable_fulfillment_body_key(&compilation, "swap");

        let flow = compilation
            .storage_flow(key)
            .unwrap_or_else(|error| panic!("fulfillment storage flow must publish: {error:?}"));

        assert!(
            flow.value().operations().iter().all(|operation| {
                operation.status()
                    != bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
            }),
            "{flow:?}"
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

    func slice(pos start: i32?, pos end: i32?) -> &i32
    {
        return &self.value;
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

        let custom_roots = storage
            .value()
            .accesses()
            .iter()
            .filter(|access| access.projections().is_empty())
            .filter_map(|access| match access.root() {
                StorageAccessRoot::BorrowedStorage {
                    storage: identity, ..
                } => storage.value().identity(identity),
                _ => None,
            })
            .filter(|identity| matches!(identity, StorageIdentity::CustomIndexBorrow(_)))
            .count();

        assert!(custom_roots >= 3, "{storage:?}");
    }

    #[test]
    fn live_shared_custom_index_borrow_conflicts_with_mutable_indexing() {
        let compilation = custom_index_storage_compilation(
            r#"func exercise(pos input: Values)
{
    let mut values: Values = input;
    let selected: &Item = &values[0];
    values[0] = Item { value = 1 };
    selected;
}
"#,
        );

        assert!(
            compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::CheckingConflictingBorrow)
                .next()
                .is_some()
        );
    }

    #[test]
    fn live_mutable_custom_index_borrow_conflicts_with_shared_indexing() {
        let compilation = custom_index_storage_compilation(
            r#"func exercise(pos input: Values)
{
    let mut values: Values = input;
    let selected: &mut Item = &mut values[0];
    let observed: i32 = values[0].value;
    selected;
}
"#,
        );

        let diagnostics = compilation.check_diagnostics();

        let access = diagnostics
            .by_kind(DiagnosticKind::CheckingConflictingBorrow)
            .filter_map(|diagnostic| {
                diagnostic
                    .args()
                    .iter()
                    .find_map(|arg| match (arg.name(), arg.value()) {
                        (
                            DiagnosticArgName::StorageAccess,
                            DiagnosticArgValue::StorageAccess(access),
                        ) => Some(access),
                        _ => None,
                    })
            })
            .find(|access| {
                access.projections().iter().any(|projection| {
                    matches!(
                        projection,
                        DiagnosticStorageProjection::ProductField(name) if name == "value"
                    )
                })
            })
            .unwrap_or_else(|| panic!("custom-index conflict must retain its exact field access"));

        assert_eq!(access.root(), DiagnosticStorageRoot::BorrowedStorage);

        assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingConflictingBorrow);
    }

    #[test]
    fn custom_index_result_contract_retains_the_receiver_borrow_lifetime() {
        let compilation = custom_index_storage_compilation(
            r#"func select(pos values: Values)
{
    let selected: &Item = &values[0];
    selected;
}
"#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);

        let storage = compilation
            .storage_plan(key.clone())
            .unwrap_or_else(|error| panic!("custom index storage must publish: {error:?}"));

        let capability = storage
            .value()
            .accesses()
            .iter()
            .find_map(|access| match access.root() {
                StorageAccessRoot::BorrowedStorage { capability, .. } => Some(capability),
                _ => None,
            })
            .unwrap_or_else(|| panic!("custom index borrow capability must be retained"));

        let unit = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("custom index unit must publish: {error:?}"));

        let expression = unit
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| match expression {
                BoundExpression::Structured(structured)
                    if structured.kind()
                        == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex =>
                {
                    Some(id)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("custom index expression must be bound"));

        let contracts = compilation
            .dependency_contracts(key)
            .unwrap_or_else(|error| panic!("custom index contracts must publish: {error:?}"));

        let contract = contracts
            .value()
            .expression(expression)
            .and_then(|contract| contracts.value().contract(contract))
            .unwrap_or_else(|| panic!("custom index result contract must be available"));

        assert!(
            contract.requirements().iter().any(|requirement| matches!(
                requirement,
                BoundDependencyRequirement::Direct {
                    subject: BoundDependencySubject::BorrowCapability(actual),
                    ..
                } if *actual == capability
            )),
            "{contract:?}"
        );
    }

    fn custom_index_storage_compilation(body: &str) -> Compilation {
        compilation(&format!(
            r#"module app;

struct Item
{{
    mut value: i32;
}}

struct Values
{{
    mut first: Item;
    mut second: Item;
}}

impl Values(ElementIndex<i32>)
{{
    type Output = Item;

    func index(pos selector: &i32) -> &Item
    {{
        return &self.first;
    }}
}}

impl Values(MutableElementIndex<i32>)
{{
    type Output = Item;

    mut func index(pos selector: &i32) -> &mut Item
    {{
        return &mut self.second;
    }}
}}

{body}"#
        ))
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
    fn unavailable_custom_operators_report_diagnostics_before_lowering() {
        let compilation = compilation(
            r#"module app;

struct Value {}

func compare(pos left: Value, pos right: Value) -> bool
{
    return left != right;
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
            panic!("invalid custom operator must remain checkable: {error:?}")
        });

        assert!(lowered.value().is_none());

        assert_goal_state_diagnostic_kind(
            lowered.diagnostics(),
            DiagnosticKind::CheckingNoApplicableCandidate,
        );
    }

    #[test]
    fn structures_without_primary_constructors_are_not_callable() {
        let compilation = compilation(
            r#"module app;

struct Value
{
    number: i32;
}

func create() -> Value
{
    return Value(1);
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("invalid structure call must be selectable: {error:?}"));

        assert_goal_state_diagnostic_kind(
            selections.diagnostics(),
            DiagnosticKind::CheckingNoApplicableCandidate,
        );

        let lowered = compilation.lowered_unit(key).unwrap_or_else(|error| {
            panic!("invalid structure call must remain checkable: {error:?}")
        });

        assert!(lowered.value().is_none());

        assert_goal_state_diagnostic_kind(
            lowered.diagnostics(),
            DiagnosticKind::CheckingNoApplicableCandidate,
        );
    }

    #[test]
    fn built_in_operators_remain_available_without_trait_candidates() {
        let compilation = compilation(
            r#"module app;

func compare(pos left: i32, pos right: i32) -> bool
{
    return left != right;
}
"#,
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("built-in operator must lower: {error:?}"));

        assert!(lowered.value().is_some());

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn slices_and_fixed_arrays_select_and_lower_sequence_operations() {
        let compilation = compilation(
            r#"module app;

func inspect(pos slice: &[u8], pos array: [u8; 4]) -> (usize, bool, usize, bool)
{
    return (slice.length(), slice.is_empty(), array.length(), array.is_empty());
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("sequence operations must be selectable: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:#?}",
            selections.diagnostics()
        );

        let hooks = selections
            .value()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::Call(call) => call.implementation_hook(),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            hooks,
            [
                ImplementationHook::SequenceLength,
                ImplementationHook::SequenceIsEmpty,
                ImplementationHook::SequenceLength,
                ImplementationHook::SequenceIsEmpty,
            ]
        );

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("sequence operations must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let Some(mir) = lowered.value().as_ref().and_then(|unit| unit.mir()) else {
            panic!("sequence operations must produce MIR");
        };

        assert_eq!(
            mir.operations()
                .iter()
                .filter(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Memory(memory)
                        if memory.kind() == CheckedMemoryOperationKind::SequenceLength
                ))
                .count(),
            4
        );

        assert_eq!(
            mir.operations()
                .iter()
                .filter(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Binary {
                        operator: MirBinaryOperator::Equal,
                        ..
                    }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn nullable_values_select_and_lower_state_queries() {
        let compilation = compilation(
            r#"module app;

func inspect<T>(pos value: T?, pos borrowed: &(T?)) -> (bool, bool, bool, bool)
{
    return (
        value.is_present(),
        value.is_absent(),
        borrowed.is_present(),
        borrowed.is_absent(),
    );
}
"#,
        );

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key.clone())
            .unwrap_or_else(|error| panic!("nullable queries must be selectable: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "{:#?}",
            selections.diagnostics()
        );

        let hooks = selections
            .value()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                SemanticSelection::Call(call) => call.implementation_hook(),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            hooks,
            [
                ImplementationHook::NullableIsPresent,
                ImplementationHook::NullableIsAbsent,
                ImplementationHook::NullableIsPresent,
                ImplementationHook::NullableIsAbsent,
            ]
        );

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("nullable queries must lower: {error:?}"));

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );

        let Some(mir) = lowered.value().as_ref().and_then(|unit| unit.mir()) else {
            panic!("nullable queries must produce MIR");
        };

        let queries = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::NullableQuery(query) => {
                    Some((query.kind(), query.operand_type(), query.nullable_type()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(queries.len(), 4);
        assert_eq!(queries[0].0, MirNullableQueryKind::IsPresent);
        assert_eq!(queries[1].0, MirNullableQueryKind::IsAbsent);
        assert_eq!(queries[2].0, MirNullableQueryKind::IsPresent);
        assert_eq!(queries[3].0, MirNullableQueryKind::IsAbsent);

        assert!(
            queries
                .iter()
                .all(|(_, operand, nullable)| operand != nullable)
        );
    }

    #[test]
    fn nullable_state_queries_are_not_available_on_non_nullable_values() {
        let compilation = compilation(
            r#"module app;

func inspect(pos value: i32) -> bool
{
    return value.is_present();
}
"#,
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("invalid nullable query must remain checkable: {error:?}")
            });

        assert!(lowered.value().is_none());
        assert!(lowered.diagnostics().has_errors());
    }

    #[test]
    fn concrete_nullable_state_queries_lower_for_native_code_generation() {
        let compilation = compilation(
            r#"module app;

func main() -> bool
{
    let value: i32? = 1;

    return value.is_present();
}
"#,
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("concrete nullable query must lower: {error:?}"));

        assert!(lowered.value().is_some(), "{:#?}", lowered.diagnostics());

        assert!(
            lowered.diagnostics().is_empty(),
            "{:#?}",
            lowered.diagnostics()
        );
    }

    #[test]
    fn mutable_custom_indexing_requires_the_mutable_protocol() {
        let source = r#"module app;

struct Value
{
    mut element: i32;
}

impl Value(ElementIndex<i32>)
{
    type Output = i32;

    func index(pos selector: &i32) -> &i32
    {
        return &self.element;
    }
}

func mutate(pos input: Value)
{
    let mut value: Value = input;
    value[0] = 1;
}
"#;

        let compilation = compilation(source);

        let diagnostics = compilation.check_diagnostics();

        let kinds = diagnostics.iter().map(Diagnostic::kind).collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [DiagnosticKind::CheckingMutableIndexContractRequired],
            "{diagnostics:#?}"
        );

        let diagnostic = bray_testing::single_diagnostic(diagnostics);

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::referenced_name(
                CompilerKnownOperationRole::MutableElementIndex.as_str()
            )]
        );

        let key = source_function_body_key(&compilation, "mutate");

        let unit = compilation
            .bound_unit(key)
            .unwrap_or_else(|error| panic!("mutate unit must publish: {error:?}"));

        let anchor = unit
            .value()
            .tree()
            .expressions()
            .find_map(|(_, expression)| match expression {
                BoundExpression::Structured(structured)
                    if structured.kind()
                        == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex =>
                {
                    Some(structured.origin().source_anchor().syntax())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("mutable index expression must be bound"));

        assert_eq!(
            diagnostic.primary_span(),
            Some(SourceSpan::new(anchor.source_id(), anchor.full_range()))
        );

        assert_eq!(
            DiagnosticRenderer::english().render(diagnostic).message(),
            "mutable indexing requires an implementation of 'MutableElementIndex'"
        );

        assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingMutableIndexContractRequired,
        );
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
    fn narrowing_integer_conversions_reach_the_user_as_conversion_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func narrow(pos value: u64) -> u8\n",
            "{\n",
            "    return value as u8;\n",
            "}\n",
        ));

        let diagnostic = compilation
            .check_diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingNoApplicableCandidate)
            .unwrap_or_else(|| panic!("narrowing conversion diagnostic must be published"));

        assert_eq!(
            DiagnosticRenderer::english().render(diagnostic).message(),
            "no applicable conversion candidate"
        );

        assert!(diagnostic.primary_span().is_some());
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

        let selection = only_call_selection(selections.value());

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
    fn type_owned_overloads_resolve_named_constructors_and_methods() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Buffer\n",
            "{\n",
            "    value: i32;\n",
            "    internal construct empty() -> Self\n",
            "    {\n",
            "        return {value = 0};\n",
            "    }\n",
            "    internal construct with_value(pos value: i32) -> Self\n",
            "    {\n",
            "        return {value = value};\n",
            "    }\n",
            "    overload new = {empty, with_value}\n",
            "    internal func read_bool(pos marker: bool) -> i32\n",
            "    {\n",
            "        return self.value;\n",
            "    }\n",
            "    internal func read_i32(pos marker: i32) -> i32\n",
            "    {\n",
            "        return self.value + marker;\n",
            "    }\n",
            "    overload read = {read_bool, read_i32}\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let shared: Buffer = Buffer.new();\n",
            "    let _: i32 = shared.read(true);\n",
            "    let buffer: Buffer = Buffer.new(42);\n",
            "    let _: i32 = buffer.read(1);\n",
            "}\n",
        ));

        let key = source_function_body_key(&compilation, "main");

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| {
                panic!("type-owned overload selection must publish: {error:?}")
            });

        assert!(
            selections.diagnostics().is_empty(),
            "type-owned overloads must be diagnostic-free: {:?}",
            selections.diagnostics()
        );
    }

    #[test]
    fn equally_applicable_source_overloads_report_every_candidate() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let result = choose(1);\n",
            "}\n",
            "func first(pos value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "func second(pos value: i32) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "overload choose = {first, second}\n",
        ));

        let selections = compilation
            .semantic_selections(source_function_body_key(&compilation, "main"))
            .unwrap_or_else(|error| panic!("ambiguous selection must recover: {error:?}"));

        let diagnostics = selections.diagnostics();
        let mut ambiguous = diagnostics.by_kind(DiagnosticKind::CheckingAmbiguousCandidate);

        let Some(diagnostic) = ambiguous.next() else {
            panic!("one ambiguous selection expected: {diagnostics:?}");
        };

        assert!(ambiguous.next().is_none(), "{diagnostics:?}");

        let Some(DiagnosticArgValue::SelectionCandidates(candidates)) = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::SelectionCandidates)
            .map(DiagnosticArg::value)
        else {
            panic!("ambiguous selection must retain its candidates: {diagnostic:?}");
        };

        assert_eq!(candidates.omitted_count(), 0);
        assert_eq!(candidates.candidates().len(), 2);

        let names = candidates
            .candidates()
            .iter()
            .map(|candidate| match candidate.identity() {
                DiagnosticSelectionCandidateIdentity::NamedDeclaration { name, .. } => {
                    name.as_str()
                }
                other => panic!("source function candidate must retain its name: {other:?}"),
            })
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second"]);

        for candidate in candidates.candidates() {
            assert_eq!(
                candidate.signature(),
                &DiagnosticSelectionCandidateSignature::Callable {
                    parameter_types: Box::new([DiagnosticType::I32]),
                    result_type: DiagnosticType::I32,
                }
            );
        }

        assert_eq!(diagnostic.related_locations().len(), 2);

        assert!(diagnostic.related_locations().iter().all(|location| {
            location.kind() == DiagnosticRelatedLocationKind::SelectionCandidate
        }));

        assert!(
            diagnostic
                .related_locations()
                .windows(2)
                .all(|pair| pair[0].span() < pair[1].span())
        );

        assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingAmbiguousCandidate);
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

        let selection = only_call_selection(selections.value());

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

        let selection = only_call_selection(selections.value());

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
    fn generic_instance_methods_combine_receiver_and_method_arguments() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "impl Wrapper<T>\n",
            "{\n",
            "    func choose<U>(pos value: U) -> U\n",
            "    {\n",
            "        return value;\n",
            "    }\n",
            "}\n",
            "func main(pos wrapper: Wrapper<i32>) -> bool\n",
            "{\n",
            "    let explicit: bool = wrapper.choose<bool>(true);\n",
            "    let inferred: i32 = wrapper.choose(7);\n",
            "\n",
            "    return explicit;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("generic method selections must publish: {error:?}"));

        assert!(
            selections.diagnostics().is_empty(),
            "generic method selection must be diagnostic-free: {:?}",
            selections.diagnostics()
        );

        assert_eq!(
            selections
                .value()
                .entries()
                .iter()
                .filter(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
                .count(),
            2
        );
    }

    #[test]
    fn tuple_projections_preserve_receiver_capability_for_method_calls() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "impl Wrapper<T>\n",
            "{\n",
            "    func choose<U>(pos value: U) -> U\n",
            "    {\n",
            "        return value;\n",
            "    }\n",
            "}\n",
            "func main(pos values: (Wrapper<i32>, bool)) -> i32\n",
            "{\n",
            "    return values.0.choose(7);\n",
            "}\n",
        ));

        let selections = compilation
            .semantic_selections(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| {
                panic!("tuple-projected method selection must publish: {error:?}")
            });

        assert!(
            selections.diagnostics().is_empty(),
            "tuple-projected method selection must be diagnostic-free: {:?}",
            selections.diagnostics()
        );

        assert!(
            selections
                .value()
                .entries()
                .iter()
                .any(|entry| { matches!(entry.selection(), SemanticSelection::Call(_)) })
        );
    }

    #[test]
    fn generic_instance_method_diagnostics_count_only_method_arguments() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Wrapper<T>\n",
            "{\n",
            "    value: T;\n",
            "}\n",
            "impl Wrapper<T>\n",
            "{\n",
            "    func choose<U>(pos value: U) -> U\n",
            "    {\n",
            "        return value;\n",
            "    }\n",
            "}\n",
            "func main(pos wrapper: Wrapper<i32>)\n",
            "{\n",
            "    let value = wrapper.choose<bool, i32>(true);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("generic method diagnostics must publish: {error:?}"));

        let diagnostic = selections
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
            .next()
            .unwrap_or_else(|| panic!("generic argument count diagnostic must exist"));

        let rejections = selection_rejections(diagnostic);

        assert!(rejections.rejections().iter().any(|rejection| {
            matches!(
                rejection.reason(),
                DiagnosticSelectionRejectionReason::GenericArgumentCount {
                    provided: 2,
                    maximum: 1,
                }
            )
        }));
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

        let selection = only_call_selection(selections.value());

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

        let selection = only_call_selection(selections.value());

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

        let selection = only_call_selection(selections.value());

        let SemanticSelection::Call(call) = selection.selection() else {
            panic!("direct lambda invocation must publish a call selection");
        };

        assert!(matches!(call.target(), BoundCallableTarget::Anonymous(_)));
    }

    fn types_for_kind(
        compilation: &Compilation,
        keys: &[bray_bound_tree::BoundUnitKey],
        kind: BoundUnitKind,
    ) -> Arc<bray_diagnostics::DiagnosticResult<DeclaredValueTypeTemplates>> {
        let key = keys
            .iter()
            .find(|key| key.kind() == kind)
            .unwrap_or_else(|| panic!("test source must publish a {kind:?} unit"));

        match compilation.declared_value_type_templates(key.clone()) {
            Ok(analysis) => analysis,
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
        analysis: &DeclaredValueTypeTemplates,
        kind: DeclaredValueTypeConstraintKind,
    ) -> bool {
        analysis
            .constraints()
            .iter()
            .any(|entry| entry.kind() == kind)
    }

    fn has_value_kind(analysis: &DeclaredValueTypeTemplates, kind: SymbolKind) -> bool {
        analysis.evidence().iter().any(|entry| match entry.term() {
            DeclaredValueTypeTerm::Value(BoundReferenceTarget::Local(symbol)) => {
                symbol.kind() == kind
            }
            DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(symbol)) => {
                symbol.kind() == kind
            }
            DeclaredValueTypeTerm::Expression(_)
            | DeclaredValueTypeTerm::Pattern(_)
            | DeclaredValueTypeTerm::BoxStoragePolicy(_) => false,
        })
    }

    #[test]
    fn repeated_and_concurrent_requests_share_production_semantics() {
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
            panic!("declared value type result must accept a test observer: {error:?}");
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
                Ok(Ok(analysis)) => analysis,
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
            panic!("expression semantic result must accept a test observer: {error:?}");
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
            panic!("control-flow analysis must accept a test observer: {error:?}");
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
                .all(|result| Arc::ptr_eq(&checked[0], result))
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
            panic!("bound unit must accept a test observer: {error:?}");
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
            panic!("control-flow analysis must accept a test observer: {error:?}");
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
            panic!("expression semantic result must accept a test observer: {error:?}");
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
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        assert_eq!(
            compilation.state.expression_semantics.is_published(&key),
            Ok(false)
        );

        assert!(compilation.expression_types(key).is_ok());
    }

    #[test]
    fn recursive_callable_references_do_not_form_body_query_cycles() {
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
    fn unit_queries_do_not_force_nested_units() {
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
            panic!("parent control-flow analysis must be available: {error:?}");
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

        let bound = match primary.bound_unit(key.clone()) {
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
                SemanticUnitContextError::MissingOwner { unit }
            ))
            if unit == key
        ));
    }

    #[test]
    fn patterns_publish_exhaustive_boolean_match_coverage() {
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

        let analysis = match compilation.patterns(key.clone()) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        assert_eq!(
            compilation.state.checked_patterns.is_published(&key),
            Ok(true)
        );

        let repeated = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("repeated pattern analysis must be available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&analysis, &repeated));

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(coverage.unreachable_arms().is_empty());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );

        let literal_patterns = analysis
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
    fn patterns_use_constant_paths_for_boolean_coverage() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("constant-backed pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(coverage.unreachable_arms().is_empty());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );

        assert_eq!(
            analysis
                .value()
                .patterns()
                .iter()
                .filter(|pattern| matches!(pattern.test(), Some(PatternPredicate::Constant(_))))
                .count(),
            2
        );
    }

    #[test]
    fn patterns_report_subsumed_constant_alternatives() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("constant alternative analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingUnreachablePatternAlternative]
        );

        let diagnostic = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUnreachablePatternAlternative)
            .next()
            .unwrap_or_else(|| panic!("subsumed alternative must be diagnosed"));

        assert_eq!(
            diagnostic
                .related_locations()
                .iter()
                .filter(|related| {
                    related.kind() == DiagnosticRelatedLocationKind::CoveredByPattern
                })
                .count(),
            1
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingUnreachablePatternAlternative,
        );
    }

    #[test]
    fn patterns_use_constant_guard_truth_for_coverage() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("constant guard analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert_eq!(coverage.unreachable_arms(), &[0]);

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingUnreachableMatchArm]
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingUnreachableMatchArm,
        );
    }

    #[test]
    fn patterns_evaluate_closed_local_constant_patterns() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("local constant pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_reject_constant_paths_with_incompatible_types() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("incompatible constant pattern must recover: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );

        assert!(analysis.value().is_recovered());
    }

    #[test]
    fn patterns_report_non_exhaustive_matches() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingNonExhaustiveMatch]
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingNonExhaustiveMatch,
        );

        let missing = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
            .next()
            .and_then(|diagnostic| {
                diagnostic.args().iter().find_map(|arg| match arg.value() {
                    DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("non-exhaustive match must retain missing cases"));

        assert_eq!(missing, &[DiagnosticPatternMissingCase::Boolean(false)]);
    }

    #[test]
    fn patterns_report_nullable_union_and_open_domain_missing_cases() {
        let nullable = pattern_compilation(concat!(
            "    let value: i32? = none;\n",
            "    match value\n",
            "    {\n",
            "        case ?present {}\n",
            "    }\n",
        ));

        let nullable_analysis = nullable
            .patterns(source_callable_body_key(&nullable))
            .unwrap_or_else(|error| panic!("nullable coverage must publish: {error:?}"));

        let nullable_missing = nullable_analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
            .next()
            .and_then(|diagnostic| {
                diagnostic.args().iter().find_map(|arg| match arg.value() {
                    DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("nullable match must retain its missing case"));

        assert_eq!(
            nullable_missing,
            &[DiagnosticPatternMissingCase::NullableAbsent]
        );

        let union = compilation(concat!(
            "module app;\n",
            "union Choice { First; Second; }\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value { case .First {} }\n",
            "}\n",
        ));

        let union_analysis = union
            .patterns(source_callable_body_key(&union))
            .unwrap_or_else(|error| panic!("union coverage must publish: {error:?}"));

        let union_missing = union_analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
            .next()
            .and_then(|diagnostic| {
                diagnostic.args().iter().find_map(|arg| match arg.value() {
                    DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("union match must retain its missing case"));

        assert_eq!(
            union_missing,
            &[DiagnosticPatternMissingCase::UnionVariant(
                "Second".to_owned()
            )]
        );

        let open = pattern_compilation(concat!(
            "    let value: i32 = 1;\n",
            "    match value { case 1 {} }\n",
        ));

        let open_analysis = open
            .patterns(source_callable_body_key(&open))
            .unwrap_or_else(|error| panic!("open-domain coverage must publish: {error:?}"));

        let open_missing = open_analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
            .next()
            .and_then(|diagnostic| {
                diagnostic.args().iter().find_map(|arg| match arg.value() {
                    DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage.missing()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("open match must retain catch-all context"));

        assert_eq!(
            open_missing,
            &[DiagnosticPatternMissingCase::RemainingValues]
        );
    }

    #[test]
    fn patterns_bound_missing_union_cases_and_retain_every_covering_origin() {
        let bounded = compilation(concat!(
            "module app;\n",
            "union Choice { A; B; C; D; E; F; G; H; I; J; }\n",
            "func main(value: Choice)\n",
            "{\n",
            "    match value { case .A {} }\n",
            "}\n",
        ));

        let bounded_analysis = bounded
            .patterns(source_callable_body_key(&bounded))
            .unwrap_or_else(|error| panic!("bounded union coverage must publish: {error:?}"));

        let coverage = bounded_analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingNonExhaustiveMatch)
            .next()
            .and_then(|diagnostic| {
                diagnostic.args().iter().find_map(|arg| match arg.value() {
                    DiagnosticArgValue::PatternCoverage(coverage) => Some(coverage),
                    _ => None,
                })
            })
            .unwrap_or_else(|| panic!("bounded union match must retain coverage context"));

        assert_eq!(coverage.missing().len(), 8);
        assert_eq!(coverage.omitted_count(), 1);

        let covered = pattern_compilation(concat!(
            "    let value: bool = true;\n",
            "    match value\n",
            "    {\n",
            "        case true | false {}\n",
            "        case true | false {}\n",
            "    }\n",
        ));

        let covered_patterns = covered
            .patterns(source_callable_body_key(&covered))
            .unwrap_or_else(|error| panic!("covered alternatives must publish: {error:?}"));

        let diagnostic = covered_patterns
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUnreachableMatchArm)
            .next()
            .unwrap_or_else(|| panic!("covered arm must be diagnosed"));

        assert_eq!(
            diagnostic
                .related_locations()
                .iter()
                .filter(|related| {
                    related.kind() == DiagnosticRelatedLocationKind::CoveredByPattern
                })
                .count(),
            2
        );
    }

    #[test]
    fn patterns_report_unreachable_match_arms() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert_eq!(coverage.unreachable_arms(), &[1]);

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingUnreachableMatchArm]
        );

        let diagnostic = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingUnreachableMatchArm)
            .next()
            .unwrap_or_else(|| panic!("duplicate match arm must be diagnosed"));

        assert_eq!(
            diagnostic
                .related_locations()
                .iter()
                .filter(|related| {
                    related.kind() == DiagnosticRelatedLocationKind::CoveredByPattern
                })
                .count(),
            1
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingUnreachableMatchArm,
        );
    }

    #[test]
    fn patterns_report_patterns_incompatible_with_the_subject() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingIncompatiblePattern,
        );
    }

    #[test]
    fn patterns_reject_refutable_declaration_patterns() {
        let compilation = pattern_compilation("    let true: bool = true;\n");
        let key = source_callable_body_key(&compilation);

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingRefutablePattern]
        );

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingRefutablePattern,
        );
    }

    #[test]
    fn patterns_publish_exhaustive_nullable_match_coverage() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive());
        assert!(coverage.unreachable_arms().is_empty());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_compose_nested_nullable_coverage() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("nullable pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_publish_exhaustive_closed_union_coverage() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(coverage.unreachable_arms().is_empty());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn borrowed_union_subjects_use_the_referent_for_pattern_semantics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "    Second;\n",
            "}\n",
            "func main(pos value: &Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case .First {}\n",
            "        case .Second {}\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("borrowed pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(!analysis.value().is_recovered());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_resolve_bare_variants_through_the_expected_subject_type() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("bare variant pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(analysis.value().binding_types().is_empty());
        assert!(!analysis.value().is_recovered());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn single_variant_tagless_unions_preserve_exact_variant_knowledge() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@layout(c, tag = none)\n",
            "union Choice\n",
            "{\n",
            "    Only;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    match Choice.Only\n",
            "    {\n",
            "        case .Only {}\n",
            "    }\n",
            "    let preserved: Choice = Choice.Only;\n",
            "    match preserved\n",
            "    {\n",
            "        case .Only {}\n",
            "    }\n",
            "}\n",
        ));

        let analysis = compilation
            .patterns(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("tagless patterns must be available: {error:?}"));

        assert_eq!(analysis.value().matches().len(), 2);

        assert!(
            analysis
                .value()
                .matches()
                .iter()
                .all(bray_bound_tree::MatchCoverageEntry::is_exhaustive),
            "{analysis:?}"
        );

        assert!(!analysis.value().is_recovered(), "{analysis:?}");

        assert!(
            analysis.diagnostics().is_empty(),
            "{:#?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn multi_variant_tagless_union_patterns_require_active_variant_facts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@layout(c, tag = none)\n",
            "union Choice\n",
            "{\n",
            "    First;\n",
            "    Second;\n",
            "}\n",
            "func inspect(value: Choice)\n",
            "{\n",
            "    match value\n",
            "    {\n",
            "        case .First {}\n",
            "        case .Second {}\n",
            "    }\n",
            "}\n",
        ));

        let analysis = compilation
            .patterns(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("tagless patterns must be available: {error:?}"));

        assert_goal_state_diagnostic_kind(
            analysis.diagnostics(),
            DiagnosticKind::CheckingTaglessUnionPatternRequiresVariant,
        );
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
            "    return First;\n",
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("late subject pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");
        assert!(analysis.value().binding_types().is_empty());
        assert!(!analysis.value().is_recovered());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("nested pattern analysis must be available: {error:?}"),
        };

        assert!(analysis.value().binding_types().is_empty());
        assert!(!analysis.value().is_recovered());

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
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
            "    }\n",
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("late binding pattern analysis must be available: {error:?}"),
        };

        let Some(binding_type) = analysis.value().binding_type(binding) else {
            panic!("resolved binding must publish its checked type");
        };

        assert_type_representation(
            &compilation,
            binding_type.ty(),
            RepresentationRole::ScalarBool,
        );

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
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
            "    }\n",
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
    fn patterns_introduce_bare_bindings_after_pattern_name_resolution_fails() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("bare binding pattern analysis must be available: {error:?}"),
        };

        let Some(binding) = analysis
            .value()
            .binding_types()
            .iter()
            .copied()
            .find(|binding| binding.binding() == captured)
        else {
            panic!("bare binding pattern must publish the captured binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
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
            Err(error) => panic!("bare variant references must bind: {error:?}"),
        };

        assert!(bound.value().local_symbols().bindings().is_empty());

        let mut contextual_variants = 0;

        walk_bound_unit_view(bound.value().view(), bound.value().root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            if matches!(
                bound.value().view().expression(expression),
                Some(BoundExpression::UnqualifiedVariant(_))
            ) {
                contextual_variants += 1;
            }

            BoundWalkControl::Continue
        });

        assert_eq!(contextual_variants, 2);
    }

    #[test]
    fn patterns_do_not_treat_bare_variant_alternatives_as_bindings() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("bare variant alternative analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive(), "{analysis:?}");

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
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
    fn patterns_report_arms_after_a_catch_all_as_unreachable() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("pattern analysis must be available: {error:?}"),
        };

        let [coverage] = analysis.value().matches() else {
            panic!("test source must contain one match expression");
        };

        assert!(coverage.is_exhaustive());
        assert_eq!(coverage.unreachable_arms(), &[1]);
    }

    #[test]
    fn patterns_check_fixed_array_shape_before_proving_irrefutability() {
        let valid = pattern_compilation("    let [first, .., last]: [i32; 3] = [1, 2, 3];\n");
        let invalid = pattern_compilation("    let [first]: [i32; 2] = [1, 2];\n");

        let valid_key = source_callable_body_key(&valid);
        let invalid_key = source_callable_body_key(&invalid);

        let valid_patterns = match valid.patterns(valid_key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("valid array-pattern analysis must be available: {error:?}"),
        };

        let invalid_patterns = match invalid.patterns(invalid_key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("invalid array-pattern analysis must be available: {error:?}"),
        };

        assert!(
            valid_patterns.diagnostics().is_empty(),
            "{:?}",
            valid_patterns.diagnostics()
        );

        let [first, last] = valid_patterns.value().binding_types() else {
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

        assert!(!valid_patterns.value().is_recovered());

        assert_eq!(
            crate::test_support::diagnostic_kinds(invalid_patterns.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn patterns_check_product_field_coverage() {
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

        let valid_patterns = match valid.patterns(valid_key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("valid product-pattern analysis must be available: {error:?}"),
        };

        let invalid_patterns = match invalid.patterns(invalid_key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("invalid product-pattern analysis must be available: {error:?}"),
        };

        assert!(
            valid_patterns.diagnostics().is_empty(),
            "{:?}",
            valid_patterns.diagnostics()
        );

        assert_eq!(
            crate::test_support::diagnostic_kinds(invalid_patterns.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn patterns_check_and_project_generic_product_fields() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("generic product-pattern analysis must be available: {error:?}"),
        };

        let [binding] = analysis.value().binding_types() else {
            panic!("field shorthand must publish one binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert_eq!(binding.operation(), PatternOperation::Consume);

        assert!(matches!(
            binding.projection(),
            Some(PatternProjection::ProductField(_))
        ));

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_check_and_project_generic_union_payload_fields() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("generic payload-pattern analysis must be available: {error:?}"),
        };

        let [binding] = analysis.value().binding_types() else {
            panic!("payload pattern must publish one binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert!(matches!(
            binding.projection(),
            Some(PatternProjection::ActiveUnionPayloadField { .. })
        ));

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn patterns_bind_positional_generic_union_payload_fields() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Maybe<T>\n",
            "{\n",
            "    Some(pos value: T);\n",
            "    None;\n",
            "}\n",
            "func main(input: Maybe<bool>)\n",
            "{\n",
            "    match input\n",
            "    {\n",
            "        case Some(value)\n",
            "        {\n",
            "            assert(value);\n",
            "        }\n",
            "\n",
            "        case None {}\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => {
                panic!("positional payload-pattern analysis must be available: {error:?}")
            }
        };

        let [binding] = analysis.value().binding_types() else {
            panic!("positional payload pattern must publish one binding type");
        };

        assert_type_representation(&compilation, binding.ty(), RepresentationRole::ScalarBool);

        assert_ne!(binding.operation(), PatternOperation::Recovered);

        assert!(matches!(
            binding.projection(),
            Some(PatternProjection::ActiveUnionPayloadField { .. })
        ));

        assert!(
            analysis.diagnostics().is_empty(),
            "{:?}",
            analysis.diagnostics()
        );
    }

    #[test]
    fn positional_payload_bindings_support_member_calls() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Item\n",
            "{\n",
            "    func value() -> i32\n",
            "    {\n",
            "        return 1;\n",
            "    }\n",
            "}\n",
            "union Choice\n",
            "{\n",
            "    Pair(pos first: Item, pos second: Item);\n",
            "    Empty;\n",
            "}\n",
            "func main(input: Choice)\n",
            "{\n",
            "    match input\n",
            "    {\n",
            "        case Pair(first, second)\n",
            "        {\n",
            "            let value: i32 = first.value();\n",
            "        }\n",
            "\n",
            "        case Empty {}\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("member-call selections must be available: {error:?}"),
        };

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        assert!(selections.value().entries().iter().any(|entry| matches!(
            entry.selection(),
            SemanticSelection::Operation(SelectedOperation::Member(_))
        )));

        assert!(
            selections
                .value()
                .entries()
                .iter()
                .any(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
        );
    }

    #[test]
    fn operation_resolved_match_subjects_support_payload_member_calls() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Item\n",
            "{\n",
            "    func value() -> i32\n",
            "    {\n",
            "        return 1;\n",
            "    }\n",
            "}\n",
            "union Choice\n",
            "{\n",
            "    Present(pos value: Item);\n",
            "    Empty;\n",
            "}\n",
            "func main(input: Result<Choice, bool>) -> Result<unit, bool>\n",
            "{\n",
            "    match try input\n",
            "    {\n",
            "        case Present(value)\n",
            "        {\n",
            "            let number: i32 = value.value();\n",
            "        }\n",
            "\n",
            "        case Empty {}\n",
            "    }\n",
            "\n",
            "    return Ok(unit);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let selections = match compilation.semantic_selections(key) {
            Ok(selections) => selections,
            Err(error) => panic!("operation-resolved pattern selections must exist: {error:?}"),
        };

        assert!(
            selections.diagnostics().is_empty(),
            "{:?}",
            selections.diagnostics()
        );

        assert!(selections.value().entries().iter().any(|entry| matches!(
            entry.selection(),
            SemanticSelection::Operation(SelectedOperation::Member(_))
        )));

        assert!(
            selections
                .value()
                .entries()
                .iter()
                .any(|entry| matches!(entry.selection(), SemanticSelection::Call(_)))
        );
    }

    #[test]
    fn patterns_do_not_treat_unknown_named_payload_fields_as_positional() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("invalid payload-pattern analysis must recover: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn patterns_check_nested_product_patterns_against_field_types() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("nested product-pattern analysis must be available: {error:?}"),
        };

        assert_eq!(
            crate::test_support::diagnostic_kinds(analysis.diagnostics()),
            [DiagnosticKind::CheckingIncompatiblePattern]
        );
    }

    #[test]
    fn patterns_retain_consuming_match_operations() {
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

        let analysis = match compilation.patterns(key) {
            Ok(analysis) => analysis,
            Err(error) => panic!("consuming pattern analysis must be available: {error:?}"),
        };

        assert!(
            analysis
                .value()
                .patterns()
                .iter()
                .all(|pattern| pattern.operation() == PatternOperation::Consume)
        );
    }

    fn pattern_compilation(body: &str) -> Compilation {
        compilation(&format!("module app;\nfunc main()\n{{\n{body}}}\n",))
    }

    #[test]
    fn pattern_conditions_lower_with_guarded_temporary_cleanup() {
        let compilation = compilation(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/pattern-conditions.bray"
        )));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["next_value", "probe_return", "main"] {
            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, name))
                .unwrap();

            assert!(lowered.value().is_some(), "{name}: {lowered:?}");
        }
    }

    #[test]
    fn observed_call_result_patterns_evaluate_their_subject_once() {
        for body in [
            "match make() { case ?value { let (first, second) = value; return first; } case none { return 0; } }",
            "if let ?value = make() { let (first, second) = value; return first; } return 0;",
        ] {
            for subject in [
                "make()",
                "trusted make()",
                "trusted internal make()",
                "(trusted internal make())",
            ] {
                let body = body.replace("make()", subject);

                let source = format!(
                    "trusted module app; internal func make() -> (i32, bool)? {{ return (7, true); }} func main() -> i32 {{ {body} }}"
                );

                let compilation = compilation(&source);
                let diagnostics = compilation.check_diagnostics();

                assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");

                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "main"))
                    .unwrap();

                let mir = lowered.value().as_ref().unwrap().mir().unwrap();

                let calls = mir
                    .operations()
                    .iter()
                    .filter(|operation| {
                        matches!(operation.kind(), bray_ir::MirOperationKind::Call(_))
                    })
                    .count();

                assert_eq!(
                    calls, 1,
                    "the subject must be evaluated only before entering the arm: {mir:?}"
                );
            }
        }
    }

    #[test]
    fn singleton_member_moves_preserve_containing_lifecycle_obligations() {
        for lifecycle in [
            "",
            "destruct() {}",
            "finalize() {}",
            "consume enter() -> bool { return true; } exit(pos lease: bool) {}",
        ] {
            for (declaration, body) in [
                (
                    format!("struct Wrapper {{ value: Item; {lifecycle} }}"),
                    "return wrapper.value;",
                ),
                (
                    format!("union Wrapper {{ Only(value: Item); {lifecycle} }}"),
                    "match wrapper { case Only(value = item) { return item; } }",
                ),
            ] {
                let source = format!(
                    "module app; struct Item {{}} {declaration}
                    func unpack(pos wrapper: Wrapper) -> Item {{ {body} }}"
                );

                let compilation = compilation(&source);
                let key = source_function_body_key(&compilation, "unpack");
                let storage = compilation.storage_plan(key.clone()).unwrap();
                let flow = compilation.storage_flow(key.clone()).unwrap();

                if lifecycle.is_empty() {
                    assert!(
                        !flow.diagnostics().has_errors(),
                        "{source}: {:?}",
                        flow.diagnostics()
                    );
                } else {
                    assert_goal_state_diagnostic_kind(
                        flow.diagnostics(),
                        DiagnosticKind::CheckingIncompleteLifecycleStorage,
                    );
                }

                let root = storage
                    .value()
                    .identity_entries()
                    .find_map(|(id, identity)| {
                        matches!(identity, bray_bound_tree::StorageIdentity::Parameter(_))
                            .then_some(id)
                    })
                    .unwrap();

                assert!(
                    flow.value().exits().iter().any(|exit| exit
                        .moved()
                        .iter()
                        .any(|access| storage.value().root_identity(*access) == Some(root))),
                    "{source}"
                );

                assert!(
                    !flow
                        .value()
                        .exits()
                        .iter()
                        .any(|exit| exit.fully_moved().contains(&root)),
                    "{source}"
                );

                if !lifecycle.is_empty() {
                    let analysis = compilation.async_analysis(key).unwrap();

                    bray_testing::assert_goal_state_diagnostic_kind(
                        analysis.diagnostics(),
                        DiagnosticKind::CheckingIncompleteLifecycleStorage,
                    );

                    assert!(analysis.value().scope_exits().iter().flat_map(|exit| exit.storage()).any(|decision|
                        decision.identity() == root && decision.disposition() ==
                            bray_bound_tree::AsyncStorageExitDisposition::Recovered(
                                bray_bound_tree::AsyncStorageExitRecoveryCause::UnavailablePartialCleanup)),
                        "{source}: {:?}", analysis.value());
                }
            }
        }
    }

    #[test]
    fn pattern_conditions_resolve_alternative_names_before_checking_coherence() {
        for body in [
            "let input: Choice = Empty; assert(input matches Data(_) | Empty);",
            "let input: Choice = Empty; match input { case Data(_) | Empty {} }",
            "let input: Choice = Empty; if let Data(_) | Empty = input {}",
        ] {
            let source = format!(
                "module app; union Choice {{ Data(pos value: i32); Empty; }} func main() {{ {body} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );

            assert!(
                compilation
                    .lowered_unit(source_callable_body_key(&compilation))
                    .unwrap()
                    .value()
                    .is_some()
            );
        }

        let compilation = compilation(
            "module app; union Choice { Data(pos value: i32); Empty; } func main() { let input: Choice = Empty; if let Data(value) | Empty = input {} }",
        );

        assert!(
            compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::BindingIncoherentAlternativePattern)
                .next()
                .is_some(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn pattern_conditions_check_and_lower_structural_tests_and_bindings() {
        for body in [
            "let value: bool = true; assert(value matches true);",
            "const limit: i32 = 7; let value: i32 = 7; assert(value matches limit);",
            "let value: char = 'a'; assert(value matches 'a');",
            "let value: r64 = 1.5; assert(value matches 1.5);",
            "let value: string = \"ready\"; assert(value matches \"ready\");",
            "let value: i32? = none; if let ?number = value { assert(number == 1); } else { assert(value matches none); }",
            "let mut value: i32? = none; while let ?number = value { assert(number == 1); break; } else { assert(value matches none); }",
            "let value: i32? = none; if false {} else if let ?number = value { assert(number == 1); } else {}",
            "let value: i32? = 1; if let ?number = value && number > 0 { assert(number == 1); }",
            "let value: i32? = 1; if true && let ?number = value && (number > 0 || false) { assert(number == 1); }",
            "let value: i32? = 1; if let ?number = value && let next = number + 1 && next > number { assert(next == 2); }",
            "let value: i32? = 1; while true && let ?number = value && number > 0 { assert(number == 1); break; } else {}",
            "let value: i32? = 1; if false {} else if let ?number = value && number > 0 { assert(number == 1); }",
            "let value: i32? = 1; if let ?number = value && let number = number + 1 { assert(number == 2); }",
            "if let true = (false || true) && true {}",
        ] {
            let compilation = pattern_compilation(body);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_callable_body_key(&compilation);
            let lowered = compilation.lowered_unit(key).unwrap();

            assert!(lowered.value().is_some(), "{body}: {lowered:?}");
        }
    }

    #[test]
    fn pattern_conditions_observe_owned_subjects_and_retain_move_errors() {
        for body in [
            "let input: Owned = Owned { value = 1 }; assert(input matches { value = 1 }); let transferred: Owned = input; assert(transferred.value == 1);",
            "let input: Owned = Owned { value = 1 }; if let { value } = input { assert(value == 1); } let transferred: Owned = input; assert(transferred.value == 1);",
            "if let { value } = Owned { value = 1 } { assert(value == 1); }",
        ] {
            let source =
                format!("module app; struct Owned {{ value: i32; }} func main() {{ {body} }}");

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );

            assert!(
                compilation
                    .lowered_unit(source_callable_body_key(&compilation))
                    .unwrap()
                    .value()
                    .is_some()
            );
        }

        let compilation = compilation(
            "module app; struct Owned { value: i32; } func main() { let input: Owned = Owned { value = 1 }; let transferred: Owned = input; assert(input matches { value = 1 }); }",
        );

        assert!(
            compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
                .next()
                .is_some(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn pattern_conditions_reject_bindings_in_matches() {
        for pattern in ["number", "?number", "(number, _)", "[number, ..]"] {
            let initializer = match pattern {
                "?number" => "let value: i32? = none;",
                "(number, _)" => "let value: (i32, i32) = (1, 2);",
                "[number, ..]" => "let value: [i32; 2] = [1, 2];",
                _ => "let value: i32 = 1;",
            };

            let compilation =
                pattern_compilation(&format!("{initializer} assert(value matches {pattern});"));

            let analysis = compilation
                .patterns(source_callable_body_key(&compilation))
                .unwrap();

            assert!(
                analysis
                    .diagnostics()
                    .by_kind(DiagnosticKind::CheckingBindingInPatternTest)
                    .next()
                    .is_some(),
                "{pattern}: {:?}",
                analysis.diagnostics()
            );

            assert_goal_state_diagnostic_kind(
                analysis.diagnostics(),
                DiagnosticKind::CheckingBindingInPatternTest,
            );
        }
    }

    #[test]
    fn pattern_conditions_cover_shapes_and_variant_spellings_without_equatable() {
        let source = r#"module app;
struct Point { x: i32; y: i32; }
union Choice { Data(pos value: i32); Empty; }
func main(pos choice: Choice, pos point: Point, pos optional: i32?, pos pair: (i32, bool), pos array: [i32; 2], pos boxed: box i32)
{
    assert(choice matches Data(_) | Empty);
    assert(choice matches Choice.Data(_) | .Empty);
    if choice matches Empty {} else if let Data(value) = choice { assert(value matches 0 | 1); }
    assert(point matches Point { x = 0, y = _ });
    assert(point matches { x = _, .. });
    if point matches Point { x = 0, .. } {}
    if point matches { x = 0, .. } {}
    assert(optional matches ?_ | none);
    assert(pair matches (0, true));
    assert(array matches [0, ..]);
    assert(boxed matches box(_));
}
"#;

        let compilation = compilation(source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .unwrap();

        assert!(lowered.value().is_some(), "{lowered:?}");
    }

    #[test]
    fn pattern_conditions_keep_bindings_out_of_failure_and_outer_scopes() {
        for body in [
            "let value: i32? = none; if let ?number = value {} else { assert(number == 0); }",
            "let value: i32? = none; if let ?number = value {} assert(number == 0);",
            "let value: i32? = none; while let ?number = value { break; } else { assert(number == 0); }",
            "let value: i32? = none; if let ?number = value && number > 0 {} else { assert(number == 0); }",
            "let value: i32? = none; if number > 0 && let ?number = value {}",
            "let value: i32? = none; if let ?number = value && number > 0 {} else if number > 0 {}",
            "let value: i32? = none; while let ?number = value && number > 0 { break; } assert(number == 0);",
        ] {
            let compilation = pattern_compilation(body);

            assert!(
                compilation
                    .check_diagnostics()
                    .by_kind(DiagnosticKind::BindingUnresolvedName)
                    .next()
                    .is_some(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn pattern_conditions_publish_both_nullable_outcomes_and_common_alternative_refinements() {
        for condition in [
            "value matches none",
            "!(value matches none)",
            "value matches ?0 | ?1",
        ] {
            let source = format!(
                "module app; func main(pos value: i32?) {{ if {condition} {{ value; }} else {{ value; }} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let analysis = compilation
                .refinements(source_callable_body_key(&compilation))
                .unwrap();

            let present = analysis.value().occurrences().iter().any(|occurrence| {
                occurrence.refinements().iter().any(|refinement| {
                    matches!(
                        refinement.kind(),
                        RefinementKind::Pattern {
                            predicate: PatternPredicate::NullablePresent,
                            value: true,
                            ..
                        }
                    )
                })
            });

            assert!(present, "{condition}: {analysis:?}");

            if condition != "value matches ?0 | ?1" {
                assert!(
                    analysis.value().occurrences().iter().any(|occurrence| {
                        occurrence.refinements().iter().any(|refinement| {
                            matches!(
                                refinement.kind(),
                                RefinementKind::Pattern {
                                    predicate: PatternPredicate::NullableAbsent,
                                    value: true,
                                    ..
                                }
                            )
                        })
                    }),
                    "{condition}: {analysis:?}"
                );
            }
        }
    }

    #[test]
    fn pattern_conditions_refine_exact_paths_and_preserve_only_common_outcomes() {
        use PatternPredicate::{NullableAbsent, NullablePresent};

        for (condition, present, absent) in [
            (
                "value matches (?_, _)",
                Some(NullablePresent),
                Some(NullableAbsent),
            ),
            ("value matches (?0, _)", Some(NullablePresent), None),
            (
                "value matches (?0, _) | (?1, _)",
                Some(NullablePresent),
                None,
            ),
            (
                "value matches (?0, _) || value matches (?1, _)",
                Some(NullablePresent),
                None,
            ),
            (
                "!(value matches (?_, _))",
                Some(NullableAbsent),
                Some(NullablePresent),
            ),
        ] {
            let source = format!(
                "module app; func main(pos value: (i32?, i32?)) {{ if {condition} {{ 11; }} else {{ 22; }} 33; }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_callable_body_key(&compilation);
            let unit = compilation.bound_unit(key.clone()).unwrap();
            let storage = compilation.storage_plan(key.clone()).unwrap();
            let analysis = compilation.refinements(key).unwrap();

            for (marker, expected) in [("11;", present), ("22;", absent), ("33;", None)] {
                let offset = u32::try_from(source.find(marker).unwrap()).unwrap();

                let node = unit.value().tree().expressions().find_map(|(id, expression)| {
                    matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
                }).unwrap();

                let actual = analysis
                    .value()
                    .refinements_before(node.into())
                    .iter()
                    .filter_map(|refinement| match refinement.kind() {
                        RefinementKind::Pattern {
                            predicate: predicate @ (NullablePresent | NullableAbsent),
                            access,
                            value: true,
                            ..
                        } => Some((
                            predicate,
                            storage
                                .value()
                                .resolved_projections(access)
                                .unwrap()
                                .to_vec(),
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                let expected = expected
                    .into_iter()
                    .map(|predicate| {
                        (
                            predicate,
                            vec![StorageProjection::TupleElement(SymbolOrdinal::new(0))],
                        )
                    })
                    .collect::<Vec<_>>();

                assert_eq!(actual, expected, "{condition} at {marker}");
            }
        }
    }

    #[test]
    fn boolean_condition_refinements_respect_later_operand_mutation() {
        for (body, expected) in [
            (
                "if !(value matches none || { value = none; yield false; }) { 11; }",
                None,
            ),
            (
                "if !(value matches ?_ && { value = none; yield true; }) {} else { 11; }",
                None,
            ),
            (
                "if value matches ?_ && ({ value = none; yield true; }) { 11; }",
                None,
            ),
            (
                "assert(!(value matches none || { value = none; yield false; })); 11;",
                None,
            ),
            (
                "match true { case true when !(value matches none || { value = none; yield false; }) { 11; } case _ {} }",
                None,
            ),
            (
                "while !(value matches none || { value = none; yield false; }) { 11; break; }",
                None,
            ),
            (
                "let result = (value matches none || { value = none; yield false; }) || { 11; yield false; };",
                None,
            ),
            (
                "if !(value matches none || { other = none; yield false; }) { 11; }",
                Some(PatternPredicate::NullablePresent),
            ),
            (
                "if !(value matches none || false) { 11; }",
                Some(PatternPredicate::NullablePresent),
            ),
        ] {
            let source = format!(
                "module app; func main(pos mut value: i32?, pos mut other: i32?) {{ {body} }}"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            let key = source_callable_body_key(&compilation);
            let unit = compilation.bound_unit(key.clone()).unwrap();
            let analysis = compilation.refinements(key).unwrap();
            let offset = u32::try_from(source.find("11;").unwrap()).unwrap();

            let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
            }).unwrap();

            let actual = analysis
                .value()
                .refinements_before(marker.into())
                .iter()
                .filter_map(|refinement| match refinement.kind() {
                    RefinementKind::Pattern {
                        predicate:
                            predicate @ (PatternPredicate::NullablePresent
                            | PatternPredicate::NullableAbsent),
                        value: true,
                        ..
                    } => Some(predicate),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(actual, expected.into_iter().collect::<Vec<_>>(), "{body}");
        }
    }

    #[test]
    fn pattern_conditions_binding_diagnostics_point_to_the_introduced_name() {
        for pattern in ["number", "{ number }"] {
            let source = format!(
                "module app; struct Value {{ number: i32; }} func main(pos input: Value) {{ input matches {pattern}; }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            let diagnostic = diagnostics
                .by_kind(DiagnosticKind::CheckingBindingInPatternTest)
                .next()
                .unwrap();

            let span = diagnostic.primary_span().unwrap();

            assert_eq!(
                span.start().bytes(),
                u32::try_from(source.rfind("number").unwrap()).unwrap()
            );

            assert_eq!(span.range().slice_str(&source).unwrap().trim(), "number");
        }
    }

    #[test]
    fn pattern_conditions_allow_initializers_that_terminate_execution() {
        for body in [
            "if panic(\"stop\") {}",
            "if let _ = panic(\"stop\") {}",
            "while let _ = panic(\"stop\") {}",
            "panic(\"stop\") matches _;",
            "if true {} else if let _ = panic(\"stop\") {}",
            "if true && let _ = panic(\"stop\") {} else {}",
            "if let value = 1 && panic(\"stop\") { assert(value == 1); } else {}",
            "while let value = 1 && panic(\"stop\") { assert(value == 1); } else {}",
            "if panic(\"stop\") && let value = 1 { assert(value == 1); }",
            "let value: bool = !panic(\"stop\");",
            "let value: bool = true && panic(\"stop\");",
            "let value: bool = panic(\"stop\") && true;",
            "let value: bool = false || panic(\"stop\");",
            "let value: bool = panic(\"stop\") || false;",
        ] {
            let compilation = pattern_compilation(body);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );

            let lowered = compilation
                .lowered_unit(source_callable_body_key(&compilation))
                .unwrap();

            assert!(lowered.value().is_some(), "{body}: {lowered:?}");
        }
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
