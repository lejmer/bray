use std::pin::Pin;
use std::sync::{Mutex, MutexGuard};

use bray_runtime_abi::NativeRuntimeStatus;
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId};
use triomphe::Arc;

use crate::{FrameContext, FrameExecutionState, FrameExit, FrameProgress, ProtectedFrame};

use super::super::frame::{NativeFrameTransfer, NativeTerminalState};
use super::NativeActivation;

pub(in crate::native) struct NativeRun {
    pub(super) descriptor: ProtectedFrameDescriptor,
    pub(super) terminal: Arc<NativeTerminalState>,
    state: Mutex<NativeRunState>,
}

#[derive(Default)]
pub(super) struct NativeRunState {
    pub(super) current: Option<Box<NativeActivation>>,
    pub(super) sequence: Option<Box<super::NativeStaticSequence>>,
}

impl NativeRun {
    pub(in crate::native) fn new(
        descriptor: ProtectedFrameDescriptor,
        terminal: Arc<NativeTerminalState>,
    ) -> Self {
        Self {
            descriptor,
            terminal,
            state: Mutex::new(NativeRunState::default()),
        }
    }

    pub(in crate::native) fn install_root(&self, activation: Box<NativeActivation>) {
        let mut state = self.lock_state();
        let current = &mut state.current;

        // Admission installs one root before the scheduler can dispatch its task.
        assert!(current.is_none(), "native run root must install once");
        *current = Some(activation);
    }

    pub(super) fn lock_state(&self) -> MutexGuard<'_, NativeRunState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn execution(&self) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        let state = self.lock_state();

        if let Some(current) = &state.current {
            return current.execution_state();
        }

        if state.sequence.is_some() {
            let callback = self
                .descriptor
                .state(ProtectedFrameStateId::new(0))
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

            return Ok(FrameExecutionState::new(
                self.descriptor.frame(),
                callback.clone(),
            ));
        }

        Err(NativeRuntimeStatus::INVALID_ARGUMENT)
    }

    pub(in crate::native) fn compose(&self, transfer: NativeFrameTransfer) -> NativeRuntimeStatus {
        let claim = match super::super::frames::claim(
            transfer.frame().context(),
            transfer.entry(),
            transfer.frame().metadata(),
        ) {
            Ok(claim) => claim,
            Err(status) => return status,
        };

        if let Some(mut claim) = claim {
            // Validation borrows the shared contract while installation consumes its rollback claim.
            let descriptor = claim.reservation().descriptor().clone();

            return self.install_child(transfer, &descriptor, |frame| {
                claim.install_activation(frame)
            });
        }

        let reservation =
            match super::NativeActivationReservation::prepare(transfer.frame().metadata()) {
                Ok(reservation) => reservation,
                Err(status) => return status,
            };

        // A fresh child owns only activation storage, with no task identity or cancellation context.
        let descriptor = reservation.descriptor().clone();

        self.install_child(transfer, &descriptor, |frame| reservation.install(frame))
    }

    fn install_child(
        &self,
        mut transfer: NativeFrameTransfer,
        descriptor: &ProtectedFrameDescriptor,
        install: impl FnOnce(bray_runtime_abi::NativeProtectedFrame) -> Box<NativeActivation>,
    ) -> NativeRuntimeStatus {
        let execution = match self.execution() {
            Ok(execution) => execution,
            Err(status) => return status,
        };

        // The child must be executable in the caller's present authority before ownership moves.
        let entry = match NativeActivation::entry_execution(descriptor, &execution) {
            Ok(entry) => entry,
            Err(status) => return status,
        };

        let current_lane = match crate::context::current_task_execution_lane() {
            Some(lane) => lane,
            None => return NativeRuntimeStatus::INVALID_ARGUMENT,
        };

        let lane = match super::driver::execution_lane(&entry) {
            Ok(lane) => lane,
            Err(status) => return status,
        };

        if !super::driver::lane_allows(current_lane, lane) {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        // Metadata callbacks above may reenter. Recheck the slot before consuming the claim.
        {
            let state = self.lock_state();
            let current = &state.current;

            if current
                .as_ref()
                .is_none_or(|current| current.frame.is_some() || current.child.is_some())
            {
                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }
        }

        let mut child = install(transfer.take());
        child.retain_parent_execution(&execution);

        self.lock_state()
            .current
            .as_mut()
            .expect("dispatch retains the active parent")
            .child = Some(child);

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn request_child_cancellation(&self) -> NativeRuntimeStatus {
        let mut state = self.lock_state();
        let current = &mut state.current;

        let Some(child) = current.as_mut().and_then(|current| current.child.as_mut()) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        // This generated edge is a checked delivery point, including inside parent cleanup shields.
        // Consume it once when entering the child rather than changing the run's persistent request.
        child.cancellation_requested = true;

        NativeRuntimeStatus::SUCCESS
    }
}

impl ProtectedFrame for Arc<NativeRun> {
    type Output = usize;

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    fn execution_state(&self, _: ProtectedFrameStateId) -> Option<FrameExecutionState> {
        self.execution().ok()
    }

    fn resume(self: Pin<&mut Self>, _context: FrameContext) -> FrameProgress<usize> {
        self.drive()
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {}

    fn resolve_lifecycle(self: Pin<&mut Self>, _exit: FrameExit) {
        let current = self.lock_state().current.take();
        NativeRun::release_chain(current, &self.terminal);
    }
}

impl Drop for NativeRun {
    fn drop(&mut self) {
        let current = self
            .state
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current
            .take();

        NativeRun::release_chain(current, &self.terminal);
    }
}
