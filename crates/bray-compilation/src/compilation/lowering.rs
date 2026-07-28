use std::sync::Arc;

use bray_bound_tree::BoundUnitKey;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::{MirTargetFacts, MirUnit, MirUnitKind};
use bray_lowering::{LoweringInput, lower_unit};

use super::Compilation;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact, QueryPriority,
};

type MirUnitComputation = (
    DiagnosticResult<Option<MirUnit>>,
    Box<[bray_binder::BinderDependency]>,
);

impl Compilation {
    // TODO(BRA-157): Publish generated executable-host MIR through this fact surface.

    /// Returns validated MIR and dependency diagnostics for one checked semantic unit.
    pub fn mir_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<Option<MirUnit>>>, FactQueryError> {
        let published = self.mir_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns validated MIR for a cancellable prioritized request.
    pub fn mir_unit_with_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<DiagnosticResult<Option<MirUnit>>>, FactQueryError> {
        let published =
            self.mir_unit_with_cancellation_and_priority(key, cancellation, priority)?;

        Ok(Arc::clone(published.result()))
    }

    fn mir_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<Option<MirUnit>>>, FactQueryError> {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.mir_unit_with_cancellation_and_priority(key, cancellation, priority)
    }

    fn mir_unit_with_cancellation_and_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<PublishedUnitFact<Option<MirUnit>>>, FactQueryError> {
        self.unit_fact_with_priority(
            &self.state.mir_units,
            CompilationFactKey::MirUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| self.compute_mir_unit(&key, cancellation),
        )
    }

    fn compute_mir_unit(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<MirUnitComputation, FactQueryError> {
        let unit = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let control_flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;

        let expression_types =
            self.expression_types_with_cancellation(key.clone(), cancellation)?;

        let patterns = self.pattern_facts_with_cancellation(key.clone(), cancellation)?;

        let selections =
            self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

        let literals = self.literal_values_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let liveness = self.liveness_with_cancellation(key.clone(), cancellation)?;
        let refinements = self.refinement_facts_with_cancellation(key.clone(), cancellation)?;

        let storage_flow =
            self.storage_flow_facts_with_cancellation(key.clone(), cancellation)?;

        let dependencies =
            self.dependency_contracts_with_cancellation(key.clone(), cancellation)?;

        let async_facts = self.async_facts_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let diagnostics = DiagnosticBag::merged_all([
            unit.result().diagnostics(),
            control_flow.result().diagnostics(),
            expression_types.result().diagnostics(),
            patterns.result().diagnostics(),
            selections.result().diagnostics(),
            literals.result().diagnostics(),
            storage.result().diagnostics(),
            liveness.result().diagnostics(),
            refinements.result().diagnostics(),
            storage_flow.result().diagnostics(),
            dependencies.result().diagnostics(),
            async_facts.result().diagnostics(),
            behavior.result().diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok((DiagnosticResult::new(None, diagnostics), Box::new([])));
        }

        cancellation.check()?;

        let selected_target = self.selected_target().target();

        let target = MirTargetFacts::new(
            // MIR owns the immutable target profile independently of compilation state.
            selected_target.profile().clone(),
            selected_target.runtime_abi(),
        );

        // TODO(BRA-157): Select protected async-frame MIR kinds from checked async facts.
        let input = LoweringInput::try_new(
            unit.result().value(),
            control_flow.result().value(),
            expression_types.result().value(),
            patterns.result().value(),
            selections.result().value(),
            literals.result().value(),
            storage.result().value(),
            liveness.result().value(),
            refinements.result().value(),
            storage_flow.result().value(),
            dependencies.result().value(),
            async_facts.result().value(),
            behavior.result().value(),
            self.semantic_value_store()?,
            self.available_compiler_known_symbols(),
            MirUnitKind::Synchronous,
            target,
        )
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mir = lower_unit(input).map_err(|_| FactQueryError::InfrastructureFailure)?;

        cancellation.check()?;

        Ok((
            DiagnosticResult::new(Some(mir), diagnostics),
            Box::new([]),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Compilation;
    use crate::test_support::{compilation, source_callable_body_key};
    use crate::{CancellationToken, FactQueryError, QueryPriority};

    #[test]
    fn mir_units_are_lowered_lazily_and_published_once() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);

        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(false));

        let first = compilation
            .mir_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must be available: {error:?}"));

        let second = compilation
            .mir_unit(key.clone())
            .unwrap_or_else(|error| panic!("MIR must remain available: {error:?}"));

        assert!(first.diagnostics().is_empty());
        assert!(first.value().is_some());
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(true));
    }

    #[test]
    fn cancelled_mir_requests_publish_no_partial_unit() {
        let compilation = lowering_compilation();
        let key = source_callable_body_key(&compilation);
        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let result =
            compilation.mir_unit_with_priority(key.clone(), &cancellation, QueryPriority::Normal);

        assert_eq!(result, Err(FactQueryError::Cancelled));
        assert_eq!(compilation.state.mir_units.is_published(&key), Ok(false));

        assert!(compilation.mir_unit(key).is_ok());
    }

    fn lowering_compilation() -> Compilation {
        compilation(concat!(
            "module app;\n",
            "func main() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ))
    }
}
