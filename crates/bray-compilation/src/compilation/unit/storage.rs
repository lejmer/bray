use std::sync::Arc;

use bray_bound_tree::{BoundUnitKey, CheckedBodySemantics, StoragePlan};
use bray_checker::{BodySemanticChecker, DefaultBodySemanticChecker};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::{checker_unit_view, plan_storage, semantic_unit_context_for};
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

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let result = plan_storage(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    declared.result().value(),
                    expressions.result().value().types(),
                    patterns.result().value(),
                    expressions.result().value().selections(),
                )?;

                let (plan, plan_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    declared.result().diagnostics(),
                    expressions.result().diagnostics(),
                    patterns.result().diagnostics(),
                    &plan_diagnostics,
                ]);

                Ok((DiagnosticResult::new(plan, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn body_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedBodySemantics>>, FactQueryError> {
        self.unit_query(
            &self.state.body_semantics,
            CompilationFactKey::BodySemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let control_flow =
                    self.control_flow_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit = checker_unit_view(bound.result().value(), &semantic_context, &context)?;

                let guarantees = self.execution_guarantee_input(
                    unit,
                    expressions.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                    cancellation,
                )?;

                let result = checker_result(DefaultBodySemanticChecker.check_body_semantics(
                    unit,
                    control_flow.result().value(),
                    expressions.result().value(),
                    patterns.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                    guarantees.value().as_ref(),
                ))?;

                let (semantics, semantic_diagnostics) = result.into_parts();

                Ok((
                    DiagnosticResult::new(
                        semantics,
                        semantic_diagnostics.merged(guarantees.diagnostics()),
                    ),
                    Box::new([]),
                ))
            },
        )
    }
}
