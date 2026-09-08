use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeFrameAffinity, NativeFrameExit, NativeFrameProgressKind, NativeLaneRequirements,
    NativeProtectedFrame,
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

use super::result_storage::NativeResultStorage;

pub(crate) fn inactive_frame_output() -> bray_runtime_abi::NativeInactiveFrame {
    extern "C" fn uninitialized_frame(
        _: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        panic!("cleanup callback did not initialize its inactive frame")
    }

    bray_runtime_abi::NativeInactiveFrame::new(0, uninitialized_frame)
}

pub(super) struct NativeFrame {
    descriptor: ProtectedFrameDescriptor,
    abi: NativeProtectedFrame,
    terminal: Arc<NativeTerminalState>,
}

pub(super) struct NativeTerminalState {
    payload: Mutex<Option<NativeTerminalPayload>>,
    cleanup_incidents: Mutex<Vec<Box<dyn std::any::Any + Send>>>,
}

pub(super) enum NativeTerminalPayload {
    Completion(NativeResultStorage),
    Opaque(usize),
}

impl NativeTerminalPayload {
    fn completion(size: usize, alignment: usize) -> Option<Self> {
        NativeResultStorage::new(size, alignment)
            .ok()
            .map(Self::Completion)
    }

    pub(super) fn handle(&self) -> usize {
        match self {
            Self::Completion(storage) => storage.address(),
            Self::Opaque(address) => *address,
        }
    }
}

impl NativeFrame {
    pub(super) fn checked_descriptor(
        abi: &NativeProtectedFrame,
    ) -> Option<ProtectedFrameDescriptor> {
        let alignment = NonZeroUsize::new(abi.alignment())?;

        let completion_alignment = NonZeroUsize::new(abi.completion_alignment())?;

        let layout = ProtectedFrameLayout::try_new(abi.size(), alignment).ok()?;

        let completion_layout =
            ProtectedFrameLayout::try_new(abi.completion_size(), completion_alignment).ok()?;

        let states = states(abi)?;
        let operations = operations(abi.identity())?;
        let version = RuntimeAbiVersion::CURRENT;

        ProtectedFrameDescriptor::try_new(
            ProtectedAsyncFrameId::new(abi.identity()),
            version,
            ProtectedFrameAbiVersions::uniform(version),
            layout,
            completion_layout,
            operations,
            states,
        )
        .ok()
    }

    pub(super) fn new(abi: NativeProtectedFrame, descriptor: ProtectedFrameDescriptor) -> Self {
        Self {
            descriptor,
            abi,
            terminal: Arc::new(NativeTerminalState::new()),
        }
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
}

impl NativeTerminalState {
    pub(super) const fn new() -> Self {
        Self {
            payload: Mutex::new(None),
            cleanup_incidents: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn record_cleanup_incident(&self, payload: Box<dyn std::any::Any + Send>) {
        self.cleanup_incidents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(payload);
    }
    pub(super) fn take_panic_payload(&self) -> Option<usize> {
        let mut retained = self
            .payload
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(NativeTerminalPayload::Opaque(payload)) = retained.as_ref() else {
            return None;
        };

        if *payload <= 1 {
            return None;
        }

        retained.take().map(|payload| payload.handle())
    }

    pub(super) fn take_cleanup_incidents(&self) -> Vec<Box<dyn std::any::Any + Send>> {
        std::mem::take(
            &mut *self
                .cleanup_incidents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    pub(super) fn attach_cleanup_incidents(&self, mut primary: usize) -> usize {
        let mut incidents = self.take_cleanup_incidents();

        incidents.reverse();

        incidents.retain_mut(|payload| {
            let Some(incident) = payload.downcast_mut::<crate::incident::OwnedCleanupIncident>()
            else {
                return true;
            };

            let Some(combined) = incident.attach_to_report(primary) else {
                return true;
            };

            primary = combined;

            false
        });

        incidents.reverse();

        let mut retained = self
            .cleanup_incidents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        incidents.append(&mut retained);
        *retained = incidents;

        primary
    }

    pub(super) fn resolve_cleanup_outcome(
        &self,
        outcome: bray_runtime_abi::NativeRunOutcome,
    ) -> bray_runtime_abi::NativeRunOutcome {
        use bray_runtime_abi::{NativeRunOutcome, NativeRunState};

        let primary = if outcome.state() == NativeRunState::PANICKED {
            Some(outcome.payload())
        } else {
            let mut incidents = self
                .cleanup_incidents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            // Encounter order selects the first panic. Attachment retains reverse cleanup order.
            let primary = incidents
                .iter_mut()
                .enumerate()
                .find_map(|(index, payload)| {
                    payload
                        .downcast_mut::<crate::incident::OwnedCleanupIncident>()?
                        .take_panic_report()
                        .map(|report| (index, report))
                });

            primary.map(|(index, report)| {
                // Taking the report leaves this wrapper empty, so removal invokes no callback.
                incidents.remove(index);

                report
            })
        };

        match primary {
            Some(primary) => NativeRunOutcome::new(
                NativeRunState::PANICKED,
                self.attach_cleanup_incidents(primary),
            ),
            None => outcome,
        }
    }
}

impl ProtectedFrame for NativeFrame {
    type Output = usize;

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    fn resume(self: Pin<&mut Self>, context: FrameContext) -> FrameProgress<Self::Output> {
        let progress = if context.cancellation_requested() {
            self.abi.cancel()(self.abi.context())
        } else {
            self.abi.resume()(self.abi.context())
        };

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

        if kind == NativeFrameProgressKind::TASK_EVENT
            || kind == NativeFrameProgressKind::TASK_COMPLETION
        {
            if progress.payload() == 0 {
                return FrameProgress::RuntimeFailure;
            }

            let state = ProtectedFrameStateId::new(progress.state());

            let suspension = if kind == NativeFrameProgressKind::TASK_EVENT {
                FrameSuspension::task_event(state, progress.payload())
            } else {
                FrameSuspension::task_completion(state, progress.payload())
            };

            return FrameProgress::Suspended(suspension);
        }

        if kind == NativeFrameProgressKind::COMPLETED {
            let Some(payload) = NativeTerminalPayload::completion(
                self.abi.completion_size(),
                self.abi.completion_alignment(),
            ) else {
                return FrameProgress::RuntimeFailure;
            };

            if let Err(incident) = catch_unwind(AssertUnwindSafe(|| {
                self.abi.move_completion()(self.abi.context(), payload.handle());
            })) {
                self.terminal.record_cleanup_incident(incident);

                return FrameProgress::RuntimeFailure;
            }

            let handle = payload.handle();
            self.record_terminal_payload(payload);

            return FrameProgress::Completed(handle);
        }

        if kind == NativeFrameProgressKind::CANCELLED {
            return FrameProgress::Cancelled;
        }

        if kind == NativeFrameProgressKind::PANICKED {
            self.record_terminal_payload(NativeTerminalPayload::Opaque(progress.payload()));

            return FrameProgress::Panicked(RuntimePanic::new(progress.payload()));
        }

        FrameProgress::RuntimeFailure
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            self.abi.broadcast_tasks()(self.abi.context());
        })) {
            self.terminal.record_cleanup_incident(payload);
        }
    }

    fn resolve_lifecycle(self: Pin<&mut Self>, exit: FrameExit) {
        let exit = match exit {
            FrameExit::Completed => NativeFrameExit::COMPLETED,
            FrameExit::Cancelled => NativeFrameExit::CANCELLED,
            FrameExit::Panicked => NativeFrameExit::PANICKED,
            FrameExit::RuntimeFailure => NativeFrameExit::RUNTIME_FAILURE,
        };

        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            (self.abi.resolve_lifecycle())(self.abi.context(), exit);
        })) {
            self.terminal.record_cleanup_incident(payload);
        }
    }
}

impl Drop for NativeFrame {
    fn drop(&mut self) {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
            self.abi.destroy()(self.abi.context());
        })) {
            self.terminal.record_cleanup_incident(payload);
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
    destroy_on_drop: bool,
}

impl NativeFrameTransfer {
    pub(super) const fn new(frame: NativeProtectedFrame) -> Self {
        Self {
            frame: Some(frame),
            destroy_on_drop: true,
        }
    }

    pub(super) const fn borrowed(frame: NativeProtectedFrame) -> Self {
        Self {
            frame: Some(frame),
            destroy_on_drop: false,
        }
    }

    pub(super) fn frame(&self) -> &NativeProtectedFrame {
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
        if !self.destroy_on_drop {
            return;
        }

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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupIncident, NativePanicReportCallbacks, NativeRunOutcome,
        NativeRunState, NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
    };

    use super::{NativeTerminalPayload, NativeTerminalState};
    use crate::incident::OwnedCleanupIncident;
    use crate::{RunOutcome, RuntimePanic};

    #[test]
    fn terminal_panics_own_cleanup_errors_and_suppressed_reports_until_destruction() {
        static ERRORS: Mutex<Vec<NativeCleanupIncident>> = Mutex::new(Vec::new());
        static APPENDS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
        static DESTROYED: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn construct(incident: &NativeCleanupIncident) -> usize {
            ERRORS.lock().unwrap().push(*incident);

            100 + incident.payload()
        }

        extern "C" fn suppress(primary: usize, secondary: usize) -> usize {
            APPENDS.lock().unwrap().push((primary, secondary));

            primary
        }

        extern "C-unwind" fn report(_: usize) -> NativeRuntimeStatus {
            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn destroy_error(payload: usize) -> NativeBrayCallOutcome {
            DESTROYED.fetch_add(payload, Ordering::Relaxed);

            NativeBrayCallOutcome::completed()
        }

        extern "C-unwind" fn destroy_report(payload: usize) -> NativeRuntimeStatus {
            assert_eq!(payload, 64);

            let errors = std::mem::take(&mut *ERRORS.lock().unwrap());

            for error in errors {
                assert!((error.destroy())(error.payload()).is_completed());
            }

            NativeRuntimeStatus::SUCCESS
        }

        let callbacks =
            NativePanicReportCallbacks::new(report, destroy_report, construct, suppress);

        extern "C-unwind" fn report_error(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
            report(incident.payload())
        }

        let terminal = NativeTerminalState {
            payload: Mutex::new(Some(NativeTerminalPayload::Opaque(64))),
            cleanup_incidents: Mutex::new(Vec::new()),
        };

        for payload in [7, 11] {
            let incident = NativeCleanupIncident::new(
                payload,
                NativeTypeIdentity::new([9; 32]),
                NativeSourceAnchor::unavailable(),
                report_error,
                destroy_error,
                callbacks,
            );

            terminal
                .record_cleanup_incident(Box::new(OwnedCleanupIncident::native(incident).unwrap()));
        }

        let mut panic = RuntimePanic::new(64_usize);

        panic.push_suppressed(Box::new(
            OwnedCleanupIncident::boundary(NativeBrayCallOutcome::panicked(16).unwrap(), callbacks)
                .unwrap(),
        ));

        let outcome =
            super::super::state::task_outcome(RunOutcome::Panicked(panic), &terminal).unwrap();

        assert_eq!(outcome, NativeRunOutcome::new(NativeRunState::PANICKED, 64));
        assert_eq!(*APPENDS.lock().unwrap(), [(64, 16), (64, 111), (64, 107)]);
        assert!(terminal.take_cleanup_incidents().is_empty());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);

        drop(terminal);

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);

        assert_eq!(
            (callbacks.destroy())(outcome.payload()),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 18);
    }

    #[test]
    fn a_completion_allocation_is_never_used_as_a_panic_report() {
        let completion = NativeTerminalPayload::completion(8, 8).unwrap();
        let address = completion.handle();

        let terminal = NativeTerminalState {
            payload: Mutex::new(Some(completion)),
            cleanup_incidents: Mutex::new(Vec::new()),
        };

        let outcome = super::super::state::task_outcome(
            RunOutcome::Panicked(RuntimePanic::new("runtime cleanup failure")),
            &terminal,
        );

        assert!(outcome.is_err());

        assert_eq!(
            terminal.payload.lock().unwrap().as_ref().unwrap().handle(),
            address
        );
    }

    #[test]
    fn non_panic_cleanup_errors_keep_the_run_outcome_and_transfer_to_host_reporting() {
        static REPORTED: AtomicUsize = AtomicUsize::new(0);
        static DESTROYED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn report(payload: usize) -> NativeRuntimeStatus {
            REPORTED.fetch_add(payload, Ordering::Relaxed);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn report_error(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
            report(incident.payload())
        }

        extern "C-unwind" fn destroy(payload: usize) -> NativeBrayCallOutcome {
            DESTROYED.fetch_add(payload, Ordering::Relaxed);

            NativeBrayCallOutcome::completed()
        }

        for (run, expected) in [
            (RunOutcome::Cancelled, NativeRunState::CANCELLED),
            (RunOutcome::Completed(0), NativeRunState::COMPLETED),
        ] {
            let terminal = NativeTerminalState {
                payload: Mutex::new(None),
                cleanup_incidents: Mutex::new(Vec::new()),
            };

            let incident = NativeCleanupIncident::new(
                5,
                NativeTypeIdentity::new([7; 32]),
                NativeSourceAnchor::unavailable(),
                report_error,
                destroy,
                crate::test_support::panic_callbacks(report, report),
            );

            terminal
                .record_cleanup_incident(Box::new(OwnedCleanupIncident::native(incident).unwrap()));

            assert_eq!(
                super::super::state::task_outcome(run, &terminal).unwrap(),
                NativeRunOutcome::new(expected, 0)
            );

            let pending = terminal.take_cleanup_incidents();
            let sink = crate::CleanupReportSink::new();

            let origin = crate::CleanupIncidentOrigin::new(
                bray_runtime_model::ProtectedAsyncFrameId::new([7; 32]),
                bray_runtime_model::ProtectedFrameStateId::new(0),
            );

            assert_eq!(pending.len(), 1);

            for incident in pending {
                sink.transfer_erased(
                    crate::CleanupIncidentProducer::SynchronousRoot,
                    origin,
                    incident,
                );
            }

            sink.drain(|incident| assert!(!incident.report_native_payload()));
        }

        assert_eq!(REPORTED.load(Ordering::Relaxed), 10);
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 10);
    }
}
