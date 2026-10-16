use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeLaneRequirements,
    NativeProtectedFrame, NativeRuntimeStatus,
};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAffinity, ProtectedFrameLayout,
    ProtectedFrameStateDescriptor, ProtectedFrameStateId,
};

use crate::{
    FrameContext, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame, RuntimePanic,
};

pub(super) struct NativeFrame {
    abi: NativeProtectedFrame,
    terminal: Arc<NativeTerminalState>,
    completion: Option<crate::backing::Backing>,
    pub(in crate::native) outgoing: crate::outgoing::OutgoingRecords,
}

pub(super) struct NativeTerminalState {
    payload: Mutex<Option<crate::backing::Backing>>,
    cleanup_incidents: Mutex<[Option<RuntimePanic>; 4]>,
}

impl NativeFrame {
    pub(super) fn try_new(abi: NativeProtectedFrame) -> Result<Self, NativeRuntimeStatus> {
        let mut transfer = NativeFrameTransfer::new(abi);
        let abi = transfer.frame();

        let alignment =
            NonZeroUsize::new(abi.alignment()).ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let completion_alignment = NonZeroUsize::new(abi.completion_alignment())
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        ProtectedFrameLayout::try_new(abi.size(), alignment)
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        ProtectedFrameLayout::try_new(abi.completion_size(), completion_alignment)
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        if abi.state_count() == 0 || state(abi, ProtectedFrameStateId::new(0)).is_none() {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        let completion =
            crate::backing::Backing::admit(abi.completion_size(), abi.completion_alignment())?;

        Ok(Self {
            completion: Some(completion),
            abi: transfer.take(),
            outgoing: crate::outgoing::OutgoingRecords::default(),
            terminal: Arc::new(NativeTerminalState {
                payload: Mutex::new(None),
                cleanup_incidents: Mutex::new(std::array::from_fn(|_| None)),
            }),
        })
    }

    pub(super) fn terminal_state(&self) -> Arc<NativeTerminalState> {
        self.terminal.clone()
    }

    fn record_terminal_payload(&self, payload: crate::backing::Backing) {
        *self
            .terminal
            .payload
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(payload);
    }

    fn record_cleanup_incident(&mut self, payload: Box<dyn std::any::Any + Send>) {
        let report = RuntimePanic::from_payload(payload, &mut self.outgoing);

        self.record_cleanup_report(report);
    }

    fn retain_callback_failure(
        &mut self,
        mut progress: NativeFrameProgress,
        caught: std::thread::Result<()>,
    ) {
        let mut report = (progress.kind() == NativeFrameProgressKind::PANICKED)
            .then(|| RuntimePanic::from_native(progress.take_report()));

        if let Err(payload) = caught {
            RuntimePanic::record(&mut report, payload, &mut self.outgoing);
        }

        if let Some(report) = report {
            self.record_cleanup_report(report);
        }
    }

    fn record_cleanup_report(&mut self, mut report: RuntimePanic) {
        report.reserve_from(&mut self.outgoing);
        self.terminal.record_cleanup_report(report);
    }
}

impl NativeTerminalState {
    pub(super) fn record_cleanup_report(&self, report: RuntimePanic) {
        let mut incidents = self
            .cleanup_incidents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(destination) = incidents.iter_mut().find(|incident| incident.is_none()) else {
            unreachable!("each of the four native cleanup callbacks runs at most once");
        };

        *destination = Some(report);
    }

    pub(super) fn take_cleanup_incidents(&self) -> [Option<RuntimePanic>; 4] {
        std::mem::replace(
            &mut *self
                .cleanup_incidents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            std::array::from_fn(|_| None),
        )
    }
}

impl ProtectedFrame for NativeFrame {
    type Output = usize;

    fn frame(&self) -> ProtectedAsyncFrameId {
        ProtectedAsyncFrameId::new(self.abi.identity())
    }

    fn execution_state(
        &self,
        ordinal: ProtectedFrameStateId,
    ) -> Option<crate::FrameExecutionState> {
        state(&self.abi, ordinal).map(|state| crate::FrameExecutionState::new(self.frame(), state))
    }

    fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output> {
        let frame = self.get_mut();

        let callback = if context.cancellation_requested() {
            frame.abi.cancel()
        } else {
            frame.abi.resume()
        };

        let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);

        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            callback(&mut progress, frame.abi.context())
        })) {
            let panic = RuntimePanic::from_payload(payload, &mut frame.outgoing);

            return FrameProgress::Panicked(
                if progress.kind() == NativeFrameProgressKind::PANICKED {
                    let mut report = RuntimePanic::from_native(progress.take_report());

                    report.append(panic, &mut frame.outgoing);

                    report
                } else {
                    panic
                },
            );
        }

        let kind = progress.kind();

        if kind == NativeFrameProgressKind::SUSPENDED {
            return FrameProgress::Suspended(FrameSuspension::new(ProtectedFrameStateId::new(
                progress.state(),
            )));
        }

        if kind == NativeFrameProgressKind::YIELDED {
            return FrameProgress::Suspended(FrameSuspension::yielding(
                ProtectedFrameStateId::new(progress.state()),
            ));
        }

        if kind == NativeFrameProgressKind::TASK_EVENT {
            if progress.payload() == 0 {
                return FrameProgress::RuntimeFailure;
            }

            return FrameProgress::Suspended(FrameSuspension::task_event(
                ProtectedFrameStateId::new(progress.state()),
                progress.payload(),
            ));
        }

        if kind == NativeFrameProgressKind::COMPLETED {
            let payload = frame.completion.take().expect(
                "completion backing is admitted before frame execution and transferred once",
            );

            if let Err(incident) = catch_unwind(AssertUnwindSafe(|| {
                frame.abi.move_completion()(frame.abi.context(), payload.address());
            })) {
                frame.record_cleanup_incident(incident);

                return FrameProgress::RuntimeFailure;
            }

            let handle = payload.address();

            frame.record_terminal_payload(payload);

            return FrameProgress::Completed(handle);
        }

        if kind == NativeFrameProgressKind::CANCELLED {
            return FrameProgress::Cancelled;
        }

        if kind == NativeFrameProgressKind::PANICKED {
            return FrameProgress::Panicked(RuntimePanic::from_native(progress.take_report()));
        }

        FrameProgress::RuntimeFailure
    }

    fn broadcast_tasks(mut self: Pin<&mut Self>) {
        let mut progress =
            NativeFrameProgress::new(bray_runtime_abi::NativeFrameProgressKind::COMPLETED, 0, 0);

        let caught = catch_unwind(AssertUnwindSafe(|| {
            self.abi.broadcast_tasks()(&mut progress, self.abi.context());
        }));

        assert!(
            matches!(
                progress.kind(),
                NativeFrameProgressKind::COMPLETED
                    | NativeFrameProgressKind::CANCELLED
                    | NativeFrameProgressKind::PANICKED
            ),
            "frame broadcast must return a non-suspending cleanup outcome"
        );

        self.retain_callback_failure(progress, caught);
    }

    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
        let exit = match exit {
            FrameExit::Completed => NativeFrameExit::COMPLETED,
            FrameExit::Cancelled => NativeFrameExit::CANCELLED,
            FrameExit::Panicked => NativeFrameExit::PANICKED,
            FrameExit::RuntimeFailure => NativeFrameExit::RUNTIME_FAILURE,
        };

        let frame = self.get_mut();
        let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);

        let caught = catch_unwind(AssertUnwindSafe(|| {
            (frame.abi.resolve_lifecycle())(&mut progress, frame.abi.context(), exit);
        }));

        frame.retain_callback_failure(progress, caught);
    }
}

impl Drop for NativeFrame {
    fn drop(&mut self) {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            self.abi.destroy()(self.abi.context());
        })) {
            self.record_cleanup_incident(payload);
        }
    }
}

fn state(
    abi: &NativeProtectedFrame,
    state: ProtectedFrameStateId,
) -> Option<ProtectedFrameStateDescriptor> {
    if state.raw() >= abi.state_count() {
        return None;
    }

    let native = abi.state()(abi.context(), state.raw());
    let affinity = ProtectedFrameAffinity::from_code(native.affinity().code())?;
    let requirements = native.lane_requirements().bits();

    let known = NativeLaneRequirements::BLOCKING.bits()
        | NativeLaneRequirements::COMPUTE.bits()
        | NativeLaneRequirements::MAIN_THREAD.bits();

    if requirements & !known != 0 {
        return None;
    }

    let lanes = ExecutionLaneRequirement::ALL
        .into_iter()
        .filter(|requirement| {
            let flag = match requirement {
                ExecutionLaneRequirement::Blocking => NativeLaneRequirements::BLOCKING,
                ExecutionLaneRequirement::Compute => NativeLaneRequirements::COMPUTE,
                ExecutionLaneRequirement::MainThread => NativeLaneRequirements::MAIN_THREAD,
            };

            requirements & flag.bits() != 0
        });

    Some(ProtectedFrameStateDescriptor::new(
        state,
        lanes,
        [],
        [],
        affinity,
    ))
}

pub(super) struct NativeFrameTransfer {
    frame: Option<NativeProtectedFrame>,
}

impl NativeFrameTransfer {
    pub(super) const fn new(frame: NativeProtectedFrame) -> Self {
        Self { frame: Some(frame) }
    }

    fn frame(&self) -> &NativeProtectedFrame {
        let Some(frame) = &self.frame else {
            unreachable!("frame transfer must remain owned");
        };

        frame
    }

    pub(super) fn take(&mut self) -> NativeProtectedFrame {
        let Some(frame) = self.frame.take() else {
            unreachable!("frame transfer must remain owned");
        };

        frame
    }
}

impl Drop for NativeFrameTransfer {
    fn drop(&mut self) {
        let Some(frame) = self.frame.take() else {
            return;
        };

        frame.destroy()(frame.context());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use bray_runtime_abi::{NativeFrameAffinity, NativeFrameState, NativeProtectedFrame};
    use bray_runtime_model::ProtectedFrameStateId;

    use crate::ProtectedFrame;

    use super::{NativeFrame, NativeFrameExit, NativeFrameProgress, NativeLaneRequirements};

    #[test]
    fn activation_queries_only_requested_compiler_states() {
        thread_local! { static QUERIES: Cell<usize> = const { Cell::new(0) }; }

        extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
            QUERIES.set(QUERIES.get() + 1);

            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
        }

        extern "C-unwind" fn resume(_: &mut NativeFrameProgress, _: usize) {
            unreachable!();
        }

        extern "C-unwind" fn action(_: usize) {}
        extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, _: usize) {}
        extern "C-unwind" fn resolve(_: &mut NativeFrameProgress, _: usize, _: NativeFrameExit) {}

        extern "C-unwind" fn completion(_: usize, _: usize) {
            unreachable!();
        }

        QUERIES.set(0);

        let abi = NativeProtectedFrame::new(
            0, [7; 32], 128, 8, 8, 0, 1, state, resume, resume, broadcast, resolve, completion,
            action,
        );

        let frame = NativeFrame::try_new(abi).expect("valid frame must be admitted");

        assert_eq!(QUERIES.get(), 1);

        assert_eq!(
            frame
                .execution_state(ProtectedFrameStateId::new(127))
                .unwrap()
                .state(),
            ProtectedFrameStateId::new(127)
        );

        assert_eq!(QUERIES.get(), 2);

        assert!(
            frame
                .execution_state(ProtectedFrameStateId::new(128))
                .is_none()
        );

        assert_eq!(QUERIES.get(), 2);
    }
}
