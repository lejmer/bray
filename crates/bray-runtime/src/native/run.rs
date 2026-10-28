use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{
    NativeActivationExecution, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativeReportRecords, NativeRuntimeStatus,
};
use bray_runtime_model::{ProtectedAsyncFrameId, ProtectedFrameStateId};

use crate::outgoing::OutgoingRecords;
use crate::report_provider as provider;
use crate::{
    FrameContext, FrameExecutionState, FrameExit, FrameProgress, FrameSuspension, ProtectedFrame,
    RuntimePanic,
};

use super::activation::{bray_runtime_activation_callback, execution, readiness};
use super::state::{current_native_task, with_runtime};

// Bound uninterrupted driver work without adding source cancellation points.
const DISPATCH_TRANSITIONS: usize = 256;

pub(super) struct NativeRun {
    identity: ProtectedAsyncFrameId,
    root: AtomicUsize,
    dispatching: AtomicBool,
    completion: Mutex<Option<crate::CancellationShield>>,
    execution: Mutex<FrameExecutionState>,
    callback: Mutex<Option<(usize, bool)>>,
}

pub(super) struct CallbackScope<'a> {
    run: &'a NativeRun,
    previous: Option<(usize, bool)>,
}

impl Drop for CallbackScope<'_> {
    fn drop(&mut self) {
        *self
            .run
            .callback
            .lock()
            .expect("callback scope is available") = self.previous;
    }
}

struct DispatchClaim<'a>(&'a AtomicBool);

impl Drop for DispatchClaim<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl NativeRun {
    pub(super) fn inspect(
        token: &NativeInactiveFrame,
        origin: u64,
    ) -> Result<NativeActivationExecution, NativeRuntimeStatus> {
        let mut snapshot = NativeActivationExecution::default();
        let status = provider::bray_runtime_activation_inspection(token, 0, 0, 0, &mut snapshot);

        if status != 0 {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        snapshot.origin = origin;

        Ok(snapshot)
    }

    pub(super) fn new(initial: &NativeActivationExecution) -> Arc<Self> {
        Arc::new(Self {
            identity: ProtectedAsyncFrameId::new(initial.identity),
            root: AtomicUsize::new(0),
            dispatching: AtomicBool::new(false),
            completion: Mutex::new(None),
            execution: Mutex::new(execution(initial)),
            callback: Mutex::new(None),
        })
    }

    pub(super) fn activate(&self, token: &NativeInactiveFrame, origin: u64) -> NativeRuntimeStatus {
        assert_eq!(
            self.root.load(Ordering::Acquire),
            0,
            "activation transfers an inactive owner once"
        );

        let root = provider::bray_runtime_activation_entry(token, origin);

        if root == 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        self.root.store(root, Ordering::Release);

        NativeRuntimeStatus::SUCCESS
    }

    fn claim(&self) -> DispatchClaim<'_> {
        assert!(
            self.dispatching
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok(),
            "same-run dispatch cannot reenter its active owner"
        );

        DispatchClaim(&self.dispatching)
    }

    pub(super) fn publish(&self, snapshot: &NativeActivationExecution) {
        assert!(
            self.dispatching.load(Ordering::Acquire),
            "only the dispatch owner publishes execution metadata"
        );

        *self
            .execution
            .lock()
            .expect("execution snapshot is available") = execution(snapshot);
    }

    pub(super) fn enter_callback(
        &self,
        snapshot: &NativeActivationExecution,
        resumable: bool,
    ) -> CallbackScope<'_> {
        let mut callback = self.callback.lock().expect("callback scope is available");

        if resumable {
            assert!(
                self.dispatching.load(Ordering::Acquire),
                "resumption retains the exclusive run claim"
            );

            assert!(
                callback.is_none(),
                "source callbacks cannot recursively dispatch this run"
            );
        }

        // Synchronous actions can nest, but mask the parent's permission to compose.
        let previous = callback.replace((snapshot.address, resumable));

        CallbackScope {
            run: self,
            previous,
        }
    }

    pub(super) fn compose(
        &self,
        token: &NativeInactiveFrame,
        state: u32,
        destination: usize,
        composition: bray_runtime_abi::NativeFrameComposition,
    ) -> NativeRuntimeStatus {
        let Some((parent, resumable)) = *self.callback.lock().expect("callback scope is available")
        else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if !resumable {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        let origin = crate::bootstrap::current_runtime_thread()
            .expect("active callback has an attached thread")
            .id()
            .raw();

        let address = provider::bray_runtime_activation_composition(
            std::ptr::with_exposed_provenance_mut(parent),
            token,
            state,
            origin,
            destination,
            composition.code(),
            super::activation::admission,
        );

        if address == 0 {
            NativeRuntimeStatus::INVALID_ARGUMENT
        } else {
            NativeRuntimeStatus::SUCCESS
        }
    }

    pub(super) fn execution_at(
        &self,
        state: ProtectedFrameStateId,
    ) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        let execution = self.snapshot();

        if execution.state() != state {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        Ok(execution)
    }

    pub(super) fn snapshot(&self) -> FrameExecutionState {
        self.execution
            .lock()
            .expect("execution snapshot is available")
            .clone()
    }

    pub(super) fn is_finished(&self) -> bool {
        self.completion
            .lock()
            .expect("terminal cancellation shield is available")
            .is_some()
    }

    fn refresh(&self, cancellation: bool) {
        let mut snapshot = NativeActivationExecution::default();

        provider::bray_runtime_activation_run_snapshot(
            std::ptr::with_exposed_provenance_mut(self.root.load(Ordering::Acquire)),
            u8::from(cancellation),
            &mut snapshot,
        );

        self.publish(&snapshot);
    }

    fn transfer_incidents(&self, incidents: NativeReportRecords, remaining: NativeReportRecords) {
        let _remaining =
            OutgoingRecords::from_parts(remaining.head, remaining.tail, remaining.count);

        if incidents.count == 0 {
            return;
        }

        let records = OutgoingRecords::from_parts(incidents.head, incidents.tail, incidents.count);
        let snapshot = self.snapshot();
        let origin = crate::CleanupIncidentOrigin::new(snapshot.frame(), snapshot.state());
        let task = current_native_task().expect("dispatch retains the native task");

        with_runtime(|runtime| {
            let identity = runtime
                .retained_task(task, true)
                .expect("dispatch retains its registered task")
                .task
                .id();

            runtime.cleanup_reports.transfer_records(
                crate::CleanupIncidentProducer::Task(identity),
                origin,
                records,
            );
        })
        .expect("dispatch retains its runtime");
    }

    pub(super) fn retire(
        &self,
        task: &super::state::StartedTask,
        lane: crate::ExecutionLane,
    ) -> NativeRuntimeStatus {
        let _claim = self.claim();

        assert!(
            self.is_finished(),
            "retirement follows terminal observation"
        );

        let root = self.root.load(Ordering::Acquire);

        if root == 0 {
            return NativeRuntimeStatus::SUCCESS;
        }

        let context = crate::TaskExecutionContext::new(
            task.task.id(),
            self.snapshot().state(),
            task.task.cancellation_context().clone(),
            task.task.output_context(),
            lane,
            task.registration.wake_handle(),
        );

        crate::context::with_task_execution_context(context, || {
            let mut progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
            let mut records = NativeReportRecords::default();
            let mut incidents = NativeReportRecords::default();

            let status = provider::bray_runtime_activation_dispatch(
                std::ptr::with_exposed_provenance_mut(root),
                bray_runtime_activation_callback,
                readiness,
                5,
                0,
                &mut progress,
                &mut records,
                &mut incidents,
            );

            if status == NativeRuntimeStatus::PENDING.code() {
                return NativeRuntimeStatus::PENDING;
            }

            assert_eq!(
                status, 0,
                "retirement retains its admitted entry capabilities"
            );

            self.root.store(0, Ordering::Release);
            self.transfer_incidents(incidents, records);

            if progress.kind() == NativeFrameProgressKind::PANICKED {
                let report = RuntimePanic::from_native(progress.take_report());

                with_runtime(|runtime| {
                    task.task.transfer_cleanup_incident(
                        &runtime.cleanup_reports,
                        task.task.execution_origin(),
                        report,
                    )
                })
                .expect("retirement retains its report sink");
            }

            NativeRuntimeStatus::SUCCESS
        })
    }

    fn drive(&self) -> FrameProgress<usize> {
        let _claim = self.claim();
        let root = self.root.load(Ordering::Acquire);

        assert_ne!(root, 0, "dispatch requires committed activation");
        assert!(!self.is_finished(), "terminal run cannot resume");

        for _ in 0..DISPATCH_TRANSITIONS {
            // A callback can enter or leave cleanup shielding without yielding.
            let cancellation = crate::context::current_run_cancellation_observable();

            let mut progress =
                NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);

            let mut records = NativeReportRecords::default();
            let mut incidents = NativeReportRecords::default();

            let status = provider::bray_runtime_activation_step(
                std::ptr::with_exposed_provenance_mut(root),
                bray_runtime_activation_callback,
                readiness,
                u8::from(cancellation),
                0,
                &mut progress,
                &mut records,
                &mut incidents,
            );

            self.transfer_incidents(incidents, records);

            match status {
                0 => continue,
                1 => {
                    self.refresh(crate::context::current_run_cancellation_observable());

                    return suspension(progress);
                }
                2 => {
                    self.refresh(false);

                    let context = crate::current_task_execution_context()
                        .expect("terminal dispatch retains its task context");

                    *self
                        .completion
                        .lock()
                        .expect("terminal cancellation shield is available") =
                        Some(context.cancellation().shield());

                    return match progress.kind() {
                        NativeFrameProgressKind::COMPLETED => {
                            FrameProgress::Completed(progress.payload())
                        }
                        NativeFrameProgressKind::CANCELLED => FrameProgress::Cancelled,
                        NativeFrameProgressKind::PANICKED => FrameProgress::Panicked(
                            RuntimePanic::from_native(progress.take_report()),
                        ),
                        NativeFrameProgressKind::RUNTIME_FAILURE => FrameProgress::RuntimeFailure,
                        kind => panic!("terminal driver returned {kind:?}"),
                    };
                }
                3 => {
                    assert_eq!(
                        progress.payload(),
                        usize::try_from(NativeRuntimeStatus::PENDING.code())
                            .expect("pending runtime status fits the native progress payload"),
                        "admitted run retains capabilities required by its callbacks"
                    );

                    return FrameProgress::Suspended(FrameSuspension::yielding(
                        self.snapshot().state(),
                    ));
                }
                status => panic!("unknown Bray dispatcher outcome {status}"),
            }
        }

        self.refresh(crate::context::current_run_cancellation_observable());

        FrameProgress::Suspended(FrameSuspension::yielding(self.snapshot().state()))
    }
}

fn suspension(progress: NativeFrameProgress) -> FrameProgress<usize> {
    let state = ProtectedFrameStateId::new(progress.state());

    FrameProgress::Suspended(match progress.kind() {
        NativeFrameProgressKind::SUSPENDED => FrameSuspension::new(state),
        NativeFrameProgressKind::YIELDED => FrameSuspension::yielding(state),
        NativeFrameProgressKind::TASK_EVENT => {
            FrameSuspension::task_event(state, progress.payload())
        }
        kind => panic!("suspended driver returned {kind:?}"),
    })
}

impl ProtectedFrame for Arc<NativeRun> {
    type Output = usize;

    fn frame(&self) -> ProtectedAsyncFrameId {
        self.identity
    }
    fn execution_state(&self, _: ProtectedFrameStateId) -> Option<FrameExecutionState> {
        Some(self.snapshot())
    }
    fn resume(self: Pin<&mut Self>, _context: FrameContext) -> FrameProgress<Self::Output> {
        self.drive()
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {
        assert!(
            self.root.load(Ordering::Acquire) == 0 || self.is_finished(),
            "the iterative driver resolves the active chain before terminal publication"
        );
    }

    fn resolve_lifecycle(self: Pin<&mut Self>, _: FrameExit) {
        assert!(
            self.root.load(Ordering::Acquire) == 0 || self.is_finished(),
            "the iterative driver resolves the active chain before terminal publication"
        );
    }
}
