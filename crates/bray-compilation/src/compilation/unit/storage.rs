use std::sync::Arc;

use bray_bound_tree::{
    BoundUnitKey, CheckedDependencyContracts, CheckedRefinements, Liveness, StorageFlow,
    StoragePlan,
};
use bray_checker::{
    CheckerInfrastructureError, CheckerUnitView, DefaultDependencyContractChecker,
    DefaultStorageFlowChecker, DependencyContractChecker, StorageFlowChecker,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::{
    analyze_liveness, analyze_refinements, plan_storage, semantic_unit_context_for,
};
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn storage_plan_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<StoragePlan>>, FactQueryError> {
        self.unit_query(
            &self.state.storage_plans,
            CompilationFactKey::StoragePlan(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                let types = self.expression_types_with_cancellation(key.clone(), cancellation)?;
                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

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
                )?;

                let (plan, plan_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    declared.result().diagnostics(),
                    types.result().diagnostics(),
                    patterns.result().diagnostics(),
                    selections.result().diagnostics(),
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
    ) -> Result<Arc<PublishedUnitResult<Liveness>>, FactQueryError> {
        self.unit_query(
            &self.state.liveness,
            CompilationFactKey::Liveness(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = analyze_liveness(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    selections.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                )?;

                let (liveness, liveness_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    selections.result().diagnostics(),
                    storage.result().diagnostics(),
                    memory.result().diagnostics(),
                    &liveness_diagnostics,
                ]);

                Ok((DiagnosticResult::new(liveness, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn refinements_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedRefinements>>, FactQueryError> {
        self.unit_query(
            &self.state.refinements,
            CompilationFactKey::Refinements(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = analyze_refinements(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    patterns.result().value(),
                    selections.result().value(),
                    storage.result().value(),
                )?;

                let (refinements, refinement_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    patterns.result().diagnostics(),
                    selections.result().diagnostics(),
                    storage.result().diagnostics(),
                    &refinement_diagnostics,
                ]);

                Ok((
                    DiagnosticResult::new(refinements, diagnostics),
                    Box::new([]),
                ))
            },
        )
    }

    pub(in crate::compilation) fn storage_flow_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<StorageFlow>>, FactQueryError> {
        self.unit_query(
            &self.state.storage_flow,
            CompilationFactKey::StorageFlow(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;

                let refinements = self.refinements_with_cancellation(key.clone(), cancellation)?;

                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;

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
                    selections.result().value(),
                    storage.result().value(),
                    liveness.result().value(),
                    refinements.result().value(),
                    memory.result().value(),
                ))?;

                let (storage_flow, flow_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    selections.result().diagnostics(),
                    storage.result().diagnostics(),
                    liveness.result().diagnostics(),
                    refinements.result().diagnostics(),
                    memory.result().diagnostics(),
                    &flow_diagnostics,
                ]);

                Ok((
                    DiagnosticResult::new(storage_flow, diagnostics),
                    Box::new([]),
                ))
            },
        )
    }

    pub(in crate::compilation) fn dependency_contracts_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedDependencyContracts>>, FactQueryError> {
        self.unit_query(
            &self.state.dependency_contracts,
            CompilationFactKey::DependencyContracts(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let flow = self.storage_flow_with_cancellation(key.clone(), cancellation)?;
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
}
