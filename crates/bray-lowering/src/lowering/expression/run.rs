use bray_bound_tree::{BoundExpressionId, SelectedCall};
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirAsyncOperation, MirBlockId, MirOperationKind, MirSourceAnchor};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_run_call(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        selection: &SelectedCall,
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        match selection.implementation_hook() {
            Some(ImplementationHook::CurrentRunCancellationObservation) => {
                let value = self.push_value_operation(
                    expression,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Async(MirAsyncOperation::ObserveCurrentRunCancellation {
                        runtime: self
                            .runtime_reference(RuntimeAbiRole::CurrentRunCancellationObservation),
                    }),
                )?;

                Ok(Some(LoweredExpression::continuing(
                    current,
                    Some(value),
                    source,
                )))
            }
            Some(ImplementationHook::CurrentRunCancellationEntry) => {
                self.finish_cancellation(current, &source)?;

                Ok(Some(LoweredExpression::terminated(source)))
            }
            _ => Ok(None),
        }
    }
}
