use std::pin::Pin;

use bray_runtime_abi::{NativeRunOutcome, NativeRuntimeStatus};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedFrameAffinity, ProtectedFrameDescriptor,
    ProtectedFrameStateId,
};
use triomphe::Arc;

use crate::{FrameExecutionState, ProtectedFrame};

use super::super::frame::{NativeFrame, NativeTerminalState};

pub(in crate::native) struct NativeActivation {
    pub(super) frame: Option<Pin<Box<NativeFrame>>>,
    pub(super) descriptor: ProtectedFrameDescriptor,
    pub(super) state: ProtectedFrameStateId,
    pub(super) parent: Option<Box<Self>>,
    pub(super) child: Option<Box<Self>>,
    pub(super) terminal: Arc<NativeTerminalState>,
    pub(super) outcome: Option<NativeRunOutcome>,
    pub(super) cancellation_requested: bool,
    retained_lanes: [bool; ExecutionLaneRequirement::ALL.len()],
    retained_affinity: ProtectedFrameAffinity,
    origin: Option<bray_platform::RuntimeThreadId>,
    retained_origin: Option<bray_platform::RuntimeThreadId>,
}

impl NativeActivation {
    pub(in crate::native) fn new(
        frame: Pin<Box<NativeFrame>>,
        terminal: Arc<NativeTerminalState>,
    ) -> Self {
        // The immutable descriptor outlives the callback frame while its result is retained.
        let descriptor = frame.descriptor().clone();

        Self {
            frame: Some(frame),
            descriptor,
            state: ProtectedFrameStateId::new(0),
            parent: None,
            child: None,
            terminal,
            outcome: None,
            cancellation_requested: false,
            retained_lanes: [false; ExecutionLaneRequirement::ALL.len()],
            retained_affinity: ProtectedFrameAffinity::Movable,
            origin: bray_platform::current_runtime_thread().map(|thread| thread.id()),
            retained_origin: None,
        }
    }

    pub(super) fn retain_parent_execution(&mut self, execution: &FrameExecutionState) {
        for (retained, requirement) in self
            .retained_lanes
            .iter_mut()
            .zip(ExecutionLaneRequirement::ALL)
        {
            *retained = execution
                .descriptor()
                .lane_requirements()
                .contains(&requirement);
        }

        self.retained_affinity = execution.descriptor().affinity();
        self.retained_origin = execution.origin();
    }

    pub(super) fn entry_execution(
        descriptor: &ProtectedFrameDescriptor,
        parent: &FrameExecutionState,
    ) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        let lanes = ExecutionLaneRequirement::ALL.map(|requirement| {
            parent
                .descriptor()
                .lane_requirements()
                .contains(&requirement)
        });

        Self::compose_execution(
            descriptor,
            ProtectedFrameStateId::new(0),
            lanes,
            parent.descriptor().affinity(),
            bray_platform::current_runtime_thread().map(|thread| thread.id()),
            parent.origin(),
        )
    }

    pub(super) fn execution_state(&self) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        Self::compose_execution(
            &self.descriptor,
            self.state,
            self.retained_lanes,
            self.retained_affinity,
            self.origin,
            self.retained_origin,
        )
    }

    fn compose_execution(
        descriptor: &ProtectedFrameDescriptor,
        state: ProtectedFrameStateId,
        retained_lanes: [bool; ExecutionLaneRequirement::ALL.len()],
        retained_affinity: ProtectedFrameAffinity,
        origin: Option<bray_platform::RuntimeThreadId>,
        retained_origin: Option<bray_platform::RuntimeThreadId>,
    ) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        let local = descriptor
            .state(state)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        if local.affinity() == ProtectedFrameAffinity::OriginThread
            && retained_affinity == ProtectedFrameAffinity::OriginThread
            && origin
                .zip(retained_origin)
                .is_some_and(|(own, parent)| own != parent)
        {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        let affinities = [local.affinity(), retained_affinity];

        let affinity = if affinities.contains(&ProtectedFrameAffinity::OriginThread) {
            ProtectedFrameAffinity::OriginThread
        } else if affinities.contains(&ProtectedFrameAffinity::MainThread) {
            ProtectedFrameAffinity::MainThread
        } else {
            ProtectedFrameAffinity::Movable
        };

        let lanes = ExecutionLaneRequirement::ALL
            .into_iter()
            .zip(retained_lanes)
            .filter_map(|(requirement, retained)| {
                (retained
                    || local.lane_requirements().contains(&requirement)
                    || (requirement == ExecutionLaneRequirement::MainThread
                        && affinities.contains(&ProtectedFrameAffinity::MainThread)))
                .then_some(requirement)
            });

        // Retained constraints have fixed size. Local storage and dependency tables stay shared.
        let state = local.clone().with_execution_requirements(lanes, affinity);

        let execution = FrameExecutionState::new(descriptor.frame(), state);

        let origin = if retained_affinity == ProtectedFrameAffinity::OriginThread {
            retained_origin.or(origin)
        } else {
            origin
        };

        Ok(
            match origin.filter(|_| affinity == ProtectedFrameAffinity::OriginThread) {
                Some(origin) => execution.with_origin(origin),
                None => execution,
            },
        )
    }
}
