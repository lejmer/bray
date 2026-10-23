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

/// Ownership disposition when a child joins its enclosing run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirFrameComposition {
    /// Transfer the completion into the parent's admitted destination, then retire the child.
    Completion(MirStorageId),
    /// Resolve inactive captures and retire the child without executing its body.
    Abandon,
    /// Resolve child work but retain capture storage for later synchronous destruction.
    Quiesce,
}

impl MirFrameComposition {
    /// Returns the admitted result destination, if this composition produces a value.
    pub const fn completion(self) -> Option<MirStorageId> {
        match self {
            Self::Completion(storage) => Some(storage),
            Self::Abandon | Self::Quiesce => None,
        }
    }
}

/// Checked retention shared by a source continuation and its structural cleanup expansion.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameRetention {
    broadcast: MirBlockId,
    affinity: ProtectedFrameAffinity,
    completion_cleanup: bool,
    lane_requirements: [&'static [ExecutionLaneRequirement]; 3],
    initialized_storages: Arc<[MirStorageId]>,
}

impl MirFrameRetention {
    /// Retains the checked broadcast entry, execution requirements and live storage.
    pub fn new(
        broadcast: MirBlockId,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        initialized_storages: impl IntoIterator<Item = MirStorageId>,
    ) -> Self {
        Self {
            broadcast,
            affinity: ProtectedFrameAffinity::Movable,
            completion_cleanup: false,
            lane_requirements: [ExecutionLaneRequirement::set(lane_requirements), &[], &[]],
            initialized_storages: sorted_unique_shared_slice(initialized_storages),
        }
    }

    /// Returns the checked entry that only broadcasts initialized task obligations.
    pub const fn broadcast(&self) -> MirBlockId {
        self.broadcast
    }

    /// Retains the exact thread-affinity required by this frame state.
    pub const fn with_affinity(mut self, affinity: ProtectedFrameAffinity) -> Self {
        self.affinity = affinity;

        self
    }

    /// Marks entries belonging to completed-result disposal, including its continuations.
    pub const fn with_completion_cleanup(mut self, completion_cleanup: bool) -> Self {
        self.completion_cleanup = completion_cleanup;

        self
    }

    /// Returns whether this state executes completed-result disposal.
    pub const fn is_completion_cleanup(&self) -> bool {
        self.completion_cleanup
    }

    /// Returns the checked thread-affinity for this frame state.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.affinity
    }

    /// Returns checked execution-lane requirements in canonical order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        self.lane_requirements[0]
    }

    /// Returns requirements for resume, cancellation or broadcast, in that order.
    /// Abandonment enters cancellation. Moving a result and releasing empty backing
    /// execute no source lifecycle operation.
    pub fn operation_requirements(&self) -> &[&'static [ExecutionLaneRequirement]; 3] {
        &self.lane_requirements
    }

    /// Attaches requirements established for the concrete callback entries.
    pub fn with_operation_requirements(
        mut self,
        requirements: [Vec<ExecutionLaneRequirement>; 3],
    ) -> Self {
        self.lane_requirements = requirements.map(ExecutionLaneRequirement::set);

        self
    }

    pub(crate) fn remap_local_ids(
        &mut self,
        mappings: &impl crate::unit::local_id_remap::MirLocalIdMapping,
    ) {
        self.broadcast = mappings.block(self.broadcast);

        for storage in Arc::make_mut(&mut self.initialized_storages) {
            *storage = mappings.storage(*storage);
        }
    }

    /// Returns storages known to be initialized when this state is entered.
    pub fn initialized_storages(&self) -> &[MirStorageId] {
        &self.initialized_storages
    }
}

/// One executable continuation and the checked state it retains.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameState {
    state: MirFrameStateId,
    entry: MirBlockId,
    retention: MirFrameRetention,
}

impl MirFrameState {
    /// Associates a continuation with its established retention contract.
    pub const fn new(
        state: MirFrameStateId,
        entry: MirBlockId,
        retention: MirFrameRetention,
    ) -> Self {
        Self {
            state,
            entry,
            retention,
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

    /// Returns the established state retained across suspension.
    pub const fn retention(&self) -> &MirFrameRetention {
        &self.retention
    }
}

/// Hidden descriptor required to execute one protected async frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    result_type: TypeId,
    completion_state: Option<MirFrameStateId>,
    capture_cleanup: Option<(MirFrameStateId, MirFrameStateId)>,
    states: Arc<[MirFrameState]>,
}

impl MirFrameDescriptor {
    pub(crate) fn set_execution_requirements(
        &mut self,
        requirements: &[[&'static [ExecutionLaneRequirement]; 3]],
    ) {
        assert_eq!(
            self.states.len(),
            requirements.len(),
            "realized callback requirements cover every frame state"
        );

        for (state, requirements) in Arc::make_mut(&mut self.states).iter_mut().zip(requirements) {
            state.retention.lane_requirements = *requirements;
        }
    }

    pub(crate) fn remap_local_ids(
        &mut self,
        mappings: &impl crate::unit::local_id_remap::MirLocalIdMapping,
    ) {
        for state in Arc::make_mut(&mut self.states) {
            state.entry = mappings.block(state.entry);
            state.retention.remap_local_ids(mappings);
        }
    }

    /// Creates a descriptor from a nonempty state table with unique entries and
    /// contiguous descriptor-local state identities in order.
    pub fn new(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        result_type: TypeId,
        completion_state: Option<MirFrameStateId>,
        states: impl IntoIterator<Item = MirFrameState>,
    ) -> Result<Self, crate::MirCapacityError> {
        let states: Vec<_> = states.into_iter().collect();

        assert!(
            !states.is_empty(),
            "protected frame descriptors need a state"
        );

        let mut state_ids = BTreeSet::new();
        let mut entry_blocks = BTreeSet::new();

        for (ordinal, state) in states.iter().enumerate() {
            assert!(
                state_ids.insert(state.state()),
                "protected frame descriptor repeats a state identity"
            );

            assert!(
                entry_blocks.insert(state.entry()),
                "protected frame descriptor repeats a state entry"
            );

            let Some(ordinal) = crate::id::compact_slot(ordinal) else {
                return Err(crate::MirCapacityError::IdentityCapacityExceeded);
            };

            assert_eq!(
                state.state().raw(),
                ordinal,
                "protected frame descriptor state identities must be contiguous"
            );
        }

        assert!(
            completion_state.is_none_or(|completion| state_ids.contains(&completion)),
            "completed-result cleanup must identify a described frame state"
        );

        Ok(Self {
            frame,
            abi_version,
            frame_abi,
            result_type,
            completion_state,
            capture_cleanup: None,
            states: shared_slice(states),
        })
    }

    /// Records quiescence and subsequent synchronous destruction of inactive captures.
    /// Both entries retain the original capture storage. Quiescence never enters the body.
    pub fn with_capture_cleanup(
        mut self,
        entries: Option<(MirFrameStateId, MirFrameStateId)>,
    ) -> Self {
        if let Some((quiescence, destruction)) = entries {
            assert_ne!(
                quiescence, destruction,
                "capture quiescence and destruction are distinct entries"
            );

            for entry in [quiescence, destruction] {
                assert!(
                    self.states.iter().any(|state| state.state() == entry),
                    "capture cleanup identifies a described frame state"
                );
            }
        }

        self.capture_cleanup = entries;

        self
    }

    /// Returns the quiescence entry followed by the retained-capture destruction entry.
    pub const fn capture_cleanup(&self) -> Option<(MirFrameStateId, MirFrameStateId)> {
        self.capture_cleanup
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

    /// Returns the entry that owns cleanup of a completed, unconsumed result.
    /// Trivial result types need no result-cleanup entry.
    pub const fn completion_state(&self) -> Option<MirFrameStateId> {
        self.completion_state
    }

    /// Returns resumable states in descriptor order.
    pub fn states(&self) -> &[MirFrameState] {
        &self.states
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };
    use bray_testing::test_bound_unit;

    use super::{MirFrameDescriptor, MirFrameState, MirFrameStateId};
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

        builder.set_terminator(
            entry,
            source,
            MirTerminatorKind::Return {
                value: None,
                report: None,
            },
        );

        let result_type = crate::test_support::test_type();
        let abi = RuntimeAbiVersion::new(1, 0);
        let frame_abi = ProtectedFrameAbiVersions::uniform(abi);

        assert!(
            std::panic::catch_unwind(|| {
                MirFrameDescriptor::new(frame, abi, frame_abi, result_type, None, [])
            })
            .is_err()
        );

        let state = MirFrameState::new(
            MirFrameStateId::new(0),
            entry,
            crate::MirFrameRetention::new(entry, [], []),
        );

        assert!(
            std::panic::catch_unwind(|| {
                MirFrameDescriptor::new(
                    frame,
                    abi,
                    frame_abi,
                    result_type,
                    None,
                    [state.clone(), state],
                )
            })
            .is_err()
        );

        let non_contiguous = MirFrameState::new(
            MirFrameStateId::new(1),
            entry,
            crate::MirFrameRetention::new(entry, [], []),
        );

        assert!(
            std::panic::catch_unwind(|| {
                MirFrameDescriptor::new(frame, abi, frame_abi, result_type, None, [non_contiguous])
            })
            .is_err()
        );
    }
}
