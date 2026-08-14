use std::sync::Arc;

use bray_bound_tree::{BoundUnitKey, CheckedMemoryOperations};
use bray_checker::{
    CheckerInfrastructureError, CheckerUnitView, DefaultMemoryOperationChecker,
    MemoryOperationChecker,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::semantic_unit_context_for;
use crate::compilation::checker::checker_result;
use crate::compilation::facts::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

impl Compilation {
    pub(in crate::compilation) fn memory_operations_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedMemoryOperations>>, FactQueryError> {
        self.unit_fact(
            &self.state.memory_operations,
            CompilationFactKey::MemoryOperations(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                // Literal and selection queries retain independent immutable unit identities.
                let literals = self.literal_values_with_cancellation(key.clone(), cancellation)?;

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

                let result = checker_result(
                    DefaultMemoryOperationChecker
                        .check_memory_operations(
                            unit,
                            selections.result().value(),
                            literals.result().value(),
                        ),
                )?;

                let (operations, operation_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    selections.result().diagnostics(),
                    literals.result().diagnostics(),
                    &operation_diagnostics,
                ]);

                Ok((DiagnosticResult::new(operations, diagnostics), Box::new([])))
            },
        )
    }
}
