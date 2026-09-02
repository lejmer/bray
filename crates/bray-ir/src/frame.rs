use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_runtime_interface::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameAffinity, RuntimeAbiVersion,
};
use bray_symbols::TypeId;

use crate::{MirBlockId, MirFrameStateId, MirStorageId};

/// How one inactive future identifies its protected frame representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirFrameReference {
    /// The exact protected frame representation is statically known.
    Known(ProtectedAsyncFrameId),
    /// The future value carries an existential frame descriptor.
    Erased,
}

/// Checked lane, affinity, and storage state for one protected-frame state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameState {
    state: MirFrameStateId,
    entry: MirBlockId,
    affinity: ProtectedFrameAffinity,
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
    initialized_storages: Arc<[MirStorageId]>,
}

impl MirFrameState {
    /// Creates state-indexed frame state.
    pub fn new(
        state: MirFrameStateId,
        entry: MirBlockId,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        initialized_storages: impl IntoIterator<Item = MirStorageId>,
    ) -> Self {
        Self {
            state,
            entry,
            affinity: ProtectedFrameAffinity::Movable,
            lane_requirements: sorted_unique_shared_slice(lane_requirements),
            initialized_storages: sorted_unique_shared_slice(initialized_storages),
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

    /// Retains the exact thread-affinity required by this frame state.
    pub const fn with_affinity(mut self, affinity: ProtectedFrameAffinity) -> Self {
        self.affinity = affinity;

        self
    }

    /// Returns the checked thread-affinity for this frame state.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.affinity
    }

    /// Returns checked execution-lane requirements in canonical order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }

    /// Returns storages known to be initialized when this state is entered.
    pub fn initialized_storages(&self) -> &[MirStorageId] {
        &self.initialized_storages
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
        })
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

        let state = MirFrameState::new(MirFrameStateId::new(0), entry, [], []);

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, frame_abi, result_type, [state.clone(), state]),
            Err(MirFrameDescriptorBuildError::DuplicateStateOrEntry)
        );

        let non_contiguous = MirFrameState::new(MirFrameStateId::new(1), entry, [], []);

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, frame_abi, result_type, [non_contiguous]),
            Err(MirFrameDescriptorBuildError::NonContiguousState)
        );
    }
}
