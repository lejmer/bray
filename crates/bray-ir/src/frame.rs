use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_runtime_interface::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, RuntimeAbiVersion,
};
use bray_symbols::TypeId;

use crate::{MirBlockId, MirFrameStateId, MirStorageId};

/// The storage ownership boundary used to create one inactive frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirFrameStorageSource {
    /// Secure fresh storage before accepting ordinary capture ownership.
    Fresh,
    /// Activate capacity secured before the corresponding cleanup obligation was established.
    CleanupCapacity,
}

impl MirFrameStorageSource {
    /// Returns the stable inspection name of this storage source.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::CleanupCapacity => "cleanup_capacity",
        }
    }

    /// Returns the runtime storage operation implementing this ownership boundary.
    pub const fn runtime_role(self) -> bray_runtime_interface::RuntimeAbiRole {
        match self {
            Self::Fresh => bray_runtime_interface::RuntimeAbiRole::FrameStorageAdmission,
            Self::CleanupCapacity => bray_runtime_interface::RuntimeAbiRole::FrameStorageActivation,
        }
    }
}

/// How one inactive future identifies its protected frame representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirFrameReference {
    /// The exact protected frame representation is statically known.
    Known(ProtectedAsyncFrameId),
    /// The future value carries an existential frame descriptor.
    Erased,
}

pub use bray_runtime_interface::NativeFrameEntry as MirFrameEntry;

/// Checked execution requirements and storage retained across a protected-frame suspension.
/// Retained storage can contain conditionally initialized values governed by ownership guards.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameExecutionState {
    affinity: ProtectedFrameAffinity,
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
    retained_storages: Arc<[MirStorageId]>,
}

impl MirFrameExecutionState {
    /// Creates movable execution state with normalized lane and storage requirements.
    pub fn new(
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        retained_storages: impl IntoIterator<Item = MirStorageId>,
    ) -> Self {
        Self {
            affinity: ProtectedFrameAffinity::Movable,
            lane_requirements: sorted_unique_shared_slice(lane_requirements),
            retained_storages: sorted_unique_shared_slice(retained_storages),
        }
    }

    /// Retains the exact thread affinity required by this execution state.
    pub const fn with_affinity(mut self, affinity: ProtectedFrameAffinity) -> Self {
        self.affinity = affinity;

        self
    }

    /// Returns the checked thread affinity.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.affinity
    }

    /// Returns checked execution-lane requirements in sorted order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }

    /// Returns storage retained across suspension, including guarded conditional values.
    pub fn retained_storages(&self) -> &[MirStorageId] {
        &self.retained_storages
    }
}

/// One protected-frame entry and its checked execution state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameState {
    state: MirFrameStateId,
    entry: MirBlockId,
    execution: MirFrameExecutionState,
}

impl MirFrameState {
    /// Creates state-indexed frame state.
    pub const fn new(
        state: MirFrameStateId,
        entry: MirBlockId,
        execution: MirFrameExecutionState,
    ) -> Self {
        Self {
            state,
            entry,
            execution,
        }
    }

    /// Returns the descriptor-local frame state.
    pub const fn state(&self) -> MirFrameStateId {
        self.state
    }

    /// Returns the block entered when this state resumes.
    pub const fn entry(&self) -> MirBlockId {
        self.entry
    }

    /// Returns the checked execution and retention requirements at this entry.
    pub const fn execution(&self) -> &MirFrameExecutionState {
        &self.execution
    }
}

/// Hidden descriptor required to execute one protected async frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    result_type: TypeId,
    states: Arc<[MirFrameState]>,
    inactive_cleanup: Option<MirBlockId>,
    capture_abandonment: Option<(MirBlockId, MirBlockId)>,
}

impl MirFrameDescriptor {
    /// Creates a descriptor from a nonempty state table with unique entries and
    /// contiguous descriptor-local state identities in order.
    pub fn try_new(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        result_type: TypeId,
        states: impl IntoIterator<Item = MirFrameState>,
    ) -> Result<Self, MirFrameDescriptorBuildError> {
        let states: Vec<_> = states.into_iter().collect();

        if states.is_empty() {
            return Err(MirFrameDescriptorBuildError::MissingState);
        }

        let mut state_ids = BTreeSet::new();
        let mut entry_blocks = BTreeSet::new();

        for (ordinal, state) in states.iter().enumerate() {
            if !state_ids.insert(state.state()) || !entry_blocks.insert(state.entry()) {
                return Err(MirFrameDescriptorBuildError::DuplicateStateOrEntry);
            }

            let Some(ordinal) = crate::id::compact_slot(ordinal) else {
                return Err(MirFrameDescriptorBuildError::IdentityCapacityExceeded);
            };

            if state.state().raw() != ordinal {
                return Err(MirFrameDescriptorBuildError::NonContiguousState);
            }
        }

        Ok(Self {
            frame,
            abi_version,
            frame_abi,
            result_type,
            states: shared_slice(states),
            inactive_cleanup: None,
            capture_abandonment: None,
        })
    }

    /// Selects cleanup entered when the frame is cancelled before its body starts.
    pub const fn with_inactive_cleanup(mut self, entry: MirBlockId) -> Self {
        self.inactive_cleanup = Some(entry);

        self
    }

    /// Returns the inactive capture-cleanup entry, distinct from ordinary body execution.
    pub const fn inactive_cleanup(&self) -> Option<MirBlockId> {
        self.inactive_cleanup
    }

    /// Selects the initial entries for borrowed quiescence and consuming capture destruction.
    pub const fn with_capture_abandonment(
        mut self,
        quiescence: MirBlockId,
        destruction: MirBlockId,
    ) -> Self {
        self.capture_abandonment = Some((quiescence, destruction));

        self
    }

    /// Returns the distinct initial entries for capture quiescence and destruction.
    pub const fn capture_abandonment(&self) -> Option<(MirBlockId, MirBlockId)> {
        self.capture_abandonment
    }

    /// Returns the stable protected-frame identity.
    pub const fn frame(&self) -> ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the selected private runtime ABI version.
    pub const fn abi_version(&self) -> RuntimeAbiVersion {
        self.abi_version
    }

    /// Returns the independently versioned protected-frame operation contract.
    pub const fn frame_abi(&self) -> ProtectedFrameAbiVersions {
        self.frame_abi
    }

    /// Returns the frame's completed result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns resumable states in descriptor order.
    pub fn states(&self) -> &[MirFrameState] {
        &self.states
    }
}

/// A contract violation that prevents protected-frame descriptor construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirFrameDescriptorBuildError {
    /// The descriptor has no resumable state.
    MissingState,
    /// The descriptor's ordered states do not use contiguous descriptor-local ordinals.
    NonContiguousState,
    /// Two states use the same state identity or entry block.
    DuplicateStateOrEntry,
    /// The state table exceeded its compact identity representation.
    IdentityCapacityExceeded,
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };
    use bray_testing::test_bound_unit;

    use super::{MirFrameDescriptor, MirFrameDescriptorBuildError, MirFrameState, MirFrameStateId};
    use crate::{MirBlockKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder, MirUnitKind};

    #[test]
    fn inactive_cleanup_entries_are_validated_separately_from_body_entries() {
        for kind in [MirBlockKind::CleanupBroadcast, MirBlockKind::Ordinary] {
            let frame = ProtectedAsyncFrameId::new([7; 32]);
            let bound = test_bound_unit(7);
            let source = MirSourceAnchor::from(bound.key().source());

            let target = crate::test_support::test_target();
            let abi = target.runtime_abi();

            let mut builder = MirUnitBuilder::for_bound(
                bound.identity(),
                MirUnitKind::ProtectedAsyncFrame(frame),
                target,
            );

            let body = builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap();

            let cleanup = builder.push_block(source.clone(), kind).unwrap();

            builder
                .set_terminator(body, source.clone(), MirTerminatorKind::Return(None))
                .unwrap();

            let finished = builder
                .push_block(source.clone(), MirBlockKind::LifecycleResolution)
                .unwrap();

            builder
                .set_terminator(finished, source.clone(), MirTerminatorKind::Return(None))
                .unwrap();

            builder
                .set_terminator(
                    cleanup,
                    source,
                    MirTerminatorKind::ContinueCleanup(crate::MirCleanupEdge::new(
                        crate::MirCleanupPhase::LifecycleResolution,
                        crate::MirEdge::new(finished, []),
                    )),
                )
                .unwrap();

            let descriptor = MirFrameDescriptor::try_new(
                frame,
                abi,
                ProtectedFrameAbiVersions::uniform(abi),
                crate::test_support::test_type(),
                [MirFrameState::new(
                    MirFrameStateId::new(0),
                    body,
                    crate::MirFrameExecutionState::new([], []),
                )],
            )
            .unwrap()
            .with_inactive_cleanup(cleanup);

            assert_eq!(descriptor.inactive_cleanup(), Some(cleanup));

            builder.set_frame_descriptor(descriptor).unwrap();

            let result = builder.finish(body);

            if kind == MirBlockKind::CleanupBroadcast {
                assert!(result.is_ok(), "{result:?}");
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    crate::MirUnitBuildError::InvalidFrameStateEntry(cleanup)
                );
            }
        }
    }

    #[test]
    fn frame_descriptors_require_unique_nonempty_state_tables() {
        let frame = ProtectedAsyncFrameId::new([3; 32]);
        let bound = test_bound_unit(3);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            crate::test_support::test_target(),
        );

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test MIR block must be valid");
        };

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test MIR terminator must be valid");
        };

        let result_type = crate::test_support::test_type();
        let abi = RuntimeAbiVersion::new(1, 0);
        let frame_abi = ProtectedFrameAbiVersions::uniform(abi);

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, frame_abi, result_type, []),
            Err(MirFrameDescriptorBuildError::MissingState)
        );

        let state = MirFrameState::new(
            MirFrameStateId::new(0),
            entry,
            crate::MirFrameExecutionState::new([], []),
        );

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, frame_abi, result_type, [state.clone(), state]),
            Err(MirFrameDescriptorBuildError::DuplicateStateOrEntry)
        );

        let non_contiguous = MirFrameState::new(
            MirFrameStateId::new(1),
            entry,
            crate::MirFrameExecutionState::new([], []),
        );

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, frame_abi, result_type, [non_contiguous]),
            Err(MirFrameDescriptorBuildError::NonContiguousState)
        );
    }
}
