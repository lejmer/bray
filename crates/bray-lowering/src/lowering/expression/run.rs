use bray_bound_tree::{AsyncSuspensionKind, BoundExpressionId, SelectedArgument, SelectedCall};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirEdge, MirFrameState, MirOperationKind,
    MirSourceAnchor, MirSuspensionKind, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

macro_rules! define_runtime_call_roles {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        #[deny(unreachable_patterns)]
        pub(super) const fn runtime_call_role(hook: Option<ImplementationHook>) -> Option<RuntimeAbiRole> {
            match hook {
                $($(Some(ImplementationHook::$hook) => Some(RuntimeAbiRole::$role),)?)+
                _ => None,
            }
        }
    };
}
bray_runtime_interface::runtime_role_catalog!(define_runtime_call_roles);

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
            Some(hook @ (ImplementationHook::TaskYield | ImplementationHook::TaskEventWait)) => {
                if self.input.unit_kind().protected_frame().is_none() {
                    panic!("lowering contract violation: AwaitOutsideProtectedFrame {value:?}", value = expression);
                }

                let mut current = current;

                let payload = if hook == ImplementationHook::TaskEventWait {
                    let [
                        SelectedArgument::Explicit {
                            expression: event,
                            conversion,
                            ..
                        },
                    ] = selection.arguments()
                    else {
                        panic!("lowering contract violation: MissingSemanticSelection {value:?}", value = expression);
                    };

                    let lowered = self.lower_expression(*event, current)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(Some(lowered));
                    };

                    current = continuation;

                    let event = lowered
                        .value
                        .unwrap_or_else(|| panic!("lowering contract violation: MissingOperationResult {value:?}", value = *event));

                    let (continuation, event) = self.convert_operand(
                        expression,
                        current,
                        Self::retained_source(&source),
                        event,
                        conversion,
                    )?;

                    current = continuation;

                    Some(event)
                } else {
                    None
                };

                let resume = self
                    .builder
                    .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

                let cancellation = self.suspension_cleanup_edge(&source, expression.into())?;
                let state = self.next_frame_state()?;

                let suspension = self
                    .input
                    .suspension(expression)
                    .filter(|suspension| suspension.kind() == AsyncSuspensionKind::Yield)
                    .cloned()
                    .unwrap_or_else(|| panic!("lowering contract violation: MissingSuspensionPoint {value:?}", value = expression));

                self.set_terminator(
                    current,
                    Self::retained_source(&source),
                    MirTerminatorKind::Suspend {
                        kind: if hook == ImplementationHook::TaskYield {
                            MirSuspensionKind::Yield
                        } else {
                            MirSuspensionKind::TaskEvent
                        },
                        payload,
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

                let ty = self.expression_type(expression);
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
