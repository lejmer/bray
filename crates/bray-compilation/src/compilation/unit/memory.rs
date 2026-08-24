use std::sync::Arc;

use bray_bound_tree::{BoundUnitKey, CheckedMemoryOperations};
use bray_checker::{DefaultMemoryOperationChecker, MemoryOperationChecker};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::{checker_unit_view, semantic_unit_context_for};
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn memory_operations_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedMemoryOperations>>, FactQueryError> {
        self.unit_query(
            &self.state.memory_operations,
            CompilationFactKey::MemoryOperations(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let unit = checker_unit_view(bound.result().value(), &semantic_context, &context)?;

                let result =
                    checker_result(DefaultMemoryOperationChecker.check_memory_operations(
                        unit,
                        expressions.result().value().selections(),
                        expressions.result().value().literals(),
                    ))?;

                let (operations, operation_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    expressions.result().diagnostics(),
                    &operation_diagnostics,
                ]);

                Ok((DiagnosticResult::new(operations, diagnostics), Box::new([])))
            },
        )
    }
}
