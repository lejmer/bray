use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_runtime_interface::{
    ExecutableHostContract, ExecutionLaneRequirement, ProtectedAsyncFrameId, RuntimeAbiVersion,
};
use bray_symbols::{DependencyContractTemplateId, TypeId};

use crate::MirBlockId;
use crate::MirFrameStateId;

/// Checked execution and affinity facts for one protected-frame state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameStateFacts {
    state: MirFrameStateId,
    entry: MirBlockId,
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
    affinity: Option<DependencyContractTemplateId>,
}

impl MirFrameStateFacts {
    /// Creates state-indexed execution facts.
    pub fn new(
        state: MirFrameStateId,
        entry: MirBlockId,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        affinity: Option<DependencyContractTemplateId>,
    ) -> Self {
        Self {
            state,
            entry,
            lane_requirements: sorted_unique_shared_slice(lane_requirements),
            affinity,
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

    /// Returns checked execution-lane requirements in canonical order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }

    /// Returns the checked state-local affinity contract, when constrained.
    pub const fn affinity(&self) -> Option<DependencyContractTemplateId> {
        self.affinity
    }
}

/// Hidden descriptor required to execute one protected async frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    result_type: TypeId,
    states: Arc<[MirFrameStateFacts]>,
}

impl MirFrameDescriptor {
    /// Creates a descriptor when state identities and entry blocks are unique.
    pub fn try_new(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        result_type: TypeId,
        states: impl IntoIterator<Item = MirFrameStateFacts>,
    ) -> Result<Self, MirFrameDescriptorBuildError> {
        let states: Vec<_> = states.into_iter().collect();

        if states.is_empty() {
            return Err(MirFrameDescriptorBuildError::MissingState);
        }

        let mut state_ids = BTreeSet::new();
        let mut entry_blocks = BTreeSet::new();

        for state in &states {
            if !state_ids.insert(state.state()) || !entry_blocks.insert(state.entry()) {
                return Err(MirFrameDescriptorBuildError::DuplicateStateOrEntry);
            }
        }

        Ok(Self {
            frame,
            abi_version,
            result_type,
            states: shared_slice(states),
        })
    }

    /// Returns the stable protected-frame identity.
    pub const fn frame(&self) -> ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the selected private execution ABI version.
    pub const fn abi_version(&self) -> RuntimeAbiVersion {
        self.abi_version
    }

    /// Returns the frame's completed result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns resumable states in descriptor order.
    pub fn states(&self) -> &[MirFrameStateFacts] {
        &self.states
    }
}

/// A contract violation that prevents protected-frame descriptor construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MirFrameDescriptorBuildError {
    /// The descriptor has no resumable state.
    MissingState,
    /// Two states use the same state identity or entry block.
    DuplicateStateOrEntry,
}

/// Execution representation owned by one MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirUnitExecution {
    /// Ordinary synchronous control flow with no protected async frame.
    Synchronous,
    /// Compiler-protected inactive and resumable async-frame representation.
    ProtectedAsyncFrame(ProtectedAsyncFrameId),
    /// Compiler-generated native host stub for one executable or test root.
    ExecutableHost(ExecutableHostContract),
}

impl MirUnitExecution {
    /// Returns the protected-frame identity when this unit owns one.
    pub const fn protected_frame(&self) -> Option<ProtectedAsyncFrameId> {
        match self {
            Self::ProtectedAsyncFrame(frame) => Some(*frame),
            Self::Synchronous | Self::ExecutableHost(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{ProtectedAsyncFrameId, RuntimeAbiVersion};
    use bray_testing::test_bound_unit;

    use super::{
        MirFrameDescriptor, MirFrameDescriptorBuildError, MirFrameStateFacts, MirFrameStateId,
    };
    use crate::{
        MirBlockKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder, MirUnitExecution,
    };

    #[test]
    fn frame_descriptors_require_unique_nonempty_state_tables() {
        let frame = ProtectedAsyncFrameId::new([3; 32]);
        let bound = test_bound_unit(3);
        let source = MirSourceAnchor::from(bound.key().source());
        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitExecution::ProtectedAsyncFrame(frame),
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

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, result_type, []),
            Err(MirFrameDescriptorBuildError::MissingState)
        );

        let state = MirFrameStateFacts::new(MirFrameStateId::new(0), entry, [], None);

        assert_eq!(
            MirFrameDescriptor::try_new(frame, abi, result_type, [state.clone(), state]),
            Err(MirFrameDescriptorBuildError::DuplicateStateOrEntry)
        );
    }
}
