use bray_bound_tree::{AsyncSuspensionKind, BoundExpressionId, SelectedCall};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirEdge, MirFrameState, MirOperationKind,
    MirSourceAnchor, MirSuspensionKind, MirTerminatorKind,
};
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
            Some(ImplementationHook::CurrentRunCancellationPropagation) => {
                self.finish_cancellation(current, &source, expression.into())?;

                Ok(Some(LoweredExpression::terminated(source)))
            }
            Some(ImplementationHook::TaskYield) => {
                if self.input.unit_kind().protected_frame().is_none() {
                    return Err(LoweringError::AwaitOutsideProtectedFrame(expression));
                }

                let resume = self
                    .builder
                    .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

                let cancellation = self.suspension_cleanup_edge(&source, expression.into())?;
                let state = self.next_frame_state()?;

                let suspension = self
                    .input
                    .async_analysis()
                    .suspensions()
                    .iter()
                    .find(|suspension| {
                        suspension.expression() == expression
                            && suspension.kind() == AsyncSuspensionKind::Yield
                    })
                    .ok_or(LoweringError::MissingSuspensionPoint(expression))?;

                self.builder.set_terminator(
                    current,
                    Self::retained_source(&source),
                    MirTerminatorKind::Suspend {
                        kind: MirSuspensionKind::Yield,
                        resume_state: state,
                        resume: MirEdge::new(resume, []),
                        cancellation,
                        registration: self
                            .runtime_reference(RuntimeAbiRole::SuspensionRegistration),
                        wake: self.runtime_reference(RuntimeAbiRole::Wake),
                    },
                )?;

                let initialized_storages =
                    self.retained_storages(suspension.retained_subjects())?;

                self.frame_states.push(
                    MirFrameState::new(
                        state,
                        resume,
                        self.execution_lane_requirements(),
                        initialized_storages,
                    )
                    .with_affinity(self.frame_affinity()),
                );

                let ty = self.expression_type(expression)?;
                let value = self.unit_operand(ty);

                Ok(Some(LoweredExpression::continuing(
                    resume,
                    Some(value),
                    source,
                )))
            }
            _ => Ok(None),
        }
    }
}
