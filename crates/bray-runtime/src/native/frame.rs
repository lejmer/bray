use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
    NativeLaneRequirements, NativeProtectedFrame,
};
use bray_runtime_model::{
    BinarySymbolName, ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, ProtectedFrameDescriptor, ProtectedFrameLayout,
    ProtectedFrameOperations, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    RuntimeAbiVersion,
};

use crate::{
    FrameContext, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame, RuntimePanic,
};

pub(super) struct NativeFrame {
    descriptor: ProtectedFrameDescriptor,
    abi: NativeProtectedFrame,
    terminal: Arc<NativeTerminalState>,
    pub(in crate::native) outgoing: crate::outgoing::OutgoingRecords,
}

pub(super) struct NativeTerminalState {
    payload: Mutex<Option<NativeTerminalPayload>>,
    cleanup_incidents: Mutex<[Option<RuntimePanic>; 4]>,
}

pub(super) struct NativeTerminalPayload {
    address: usize,
    _storage: Box<[u128]>,
}

impl NativeTerminalPayload {
    fn completion(size: usize, alignment: usize) -> Option<Self> {
        if alignment > align_of::<u128>() {
            return None;
        }

        let word_count = size.div_ceil(size_of::<u128>()).max(1);
        let storage = vec![0_u128; word_count].into_boxed_slice();
        let address = storage.as_ptr() as usize;

        Some(Self {
            address,
            _storage: storage,
        })
    }

    pub(super) const fn handle(&self) -> usize {
        self.address
    }
}

impl NativeFrame {
    pub(super) fn try_new(abi: NativeProtectedFrame) -> Option<Self> {
        let mut transfer = NativeFrameTransfer::new(abi);
        let abi = transfer.frame();

        let alignment = NonZeroUsize::new(abi.alignment())?;

        let completion_alignment = NonZeroUsize::new(abi.completion_alignment())?;

        let layout = ProtectedFrameLayout::try_new(abi.size(), alignment).ok()?;

        let completion_layout =
            ProtectedFrameLayout::try_new(abi.completion_size(), completion_alignment).ok()?;

        let states = states(abi)?;
        let operations = operations(abi.identity())?;
        let version = RuntimeAbiVersion::new(1, 0);

        let descriptor = ProtectedFrameDescriptor::try_new(
            ProtectedAsyncFrameId::new(abi.identity()),
            version,
            ProtectedFrameAbiVersions::uniform(version),
            layout,
            completion_layout,
            operations,
            states,
        )
        .ok()?;

        Some(Self {
            descriptor,
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

    fn record_terminal_payload(&self, payload: NativeTerminalPayload) {
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

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
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
            let Some(payload) = NativeTerminalPayload::completion(
                frame.abi.completion_size(),
                frame.abi.completion_alignment(),
            ) else {
                return FrameProgress::RuntimeFailure;
            };

            if let Err(incident) = catch_unwind(AssertUnwindSafe(|| {
                frame.abi.move_completion()(frame.abi.context(), payload.handle());
            })) {
                frame.record_cleanup_incident(incident);

                return FrameProgress::RuntimeFailure;
            }

            let handle = payload.handle();
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
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            self.abi.broadcast_tasks()(self.abi.context());
        })) {
            self.record_cleanup_incident(payload);
        }
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

        let mut report = (progress.kind() == NativeFrameProgressKind::PANICKED)
            .then(|| RuntimePanic::from_native(progress.take_report()));

        if let Err(payload) = caught {
            RuntimePanic::record(&mut report, payload, &mut frame.outgoing);
        }

        if let Some(report) = report {
            frame.record_cleanup_report(report);
        }
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

fn states(abi: &NativeProtectedFrame) -> Option<Vec<ProtectedFrameStateDescriptor>> {
    (0..abi.state_count())
        .map(|state| {
            let native = abi.state()(abi.context(), state);
            let affinity = affinity(native.affinity())?;
            let requirements = lane_requirements(native.lane_requirements())?;

            Some(ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(state),
                requirements,
                [],
                [],
                affinity,
            ))
        })
        .collect()
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

fn affinity(native: NativeFrameAffinity) -> Option<ProtectedFrameAffinity> {
    ProtectedFrameAffinity::from_code(native.code())
}

fn lane_requirements(native: NativeLaneRequirements) -> Option<Vec<ExecutionLaneRequirement>> {
    let known = NativeLaneRequirements::BLOCKING.bits()
        | NativeLaneRequirements::COMPUTE.bits()
        | NativeLaneRequirements::MAIN_THREAD.bits();

    if native.bits() & !known != 0 {
        return None;
    }

    let mut requirements = Vec::new();

    if native.bits() & NativeLaneRequirements::BLOCKING.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::Blocking);
    }

    if native.bits() & NativeLaneRequirements::COMPUTE.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::Compute);
    }

    if native.bits() & NativeLaneRequirements::MAIN_THREAD.bits() != 0 {
        requirements.push(ExecutionLaneRequirement::MainThread);
    }

    Some(requirements)
}

fn operations(identity: [u8; 32]) -> Option<ProtectedFrameOperations> {
    let prefix = frame_symbol_prefix(identity);

    Some(ProtectedFrameOperations::new(
        operation_symbol(&prefix, "move_before_start")?,
        operation_symbol(&prefix, "state_description")?,
        operation_symbol(&prefix, "resume")?,
        operation_symbol(&prefix, "cancellation_entry")?,
        operation_symbol(&prefix, "task_broadcast")?,
        operation_symbol(&prefix, "lifecycle_resolution")?,
        operation_symbol(&prefix, "completion_move")?,
        operation_symbol(&prefix, "destruction")?,
    ))
}

fn frame_symbol_prefix(identity: [u8; 32]) -> String {
    let mut prefix = String::from("bray_frame_");

    for byte in identity {
        use std::fmt::Write as _;

        let _ = write!(prefix, "{byte:02x}");
    }

    prefix
}

fn operation_symbol(prefix: &str, operation: &str) -> Option<BinarySymbolName> {
    BinarySymbolName::try_new(format!("{prefix}_{operation}"))
}
