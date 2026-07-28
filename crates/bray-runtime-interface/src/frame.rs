use std::collections::BTreeSet;
use std::alloc::Layout;
use std::num::NonZeroUsize;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    RuntimeAbiVersion,
};

/// Descriptor-local identity of one resumable protected-frame state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectedFrameStateId(u32);

impl ProtectedFrameStateId {
    /// Creates a descriptor-local state identity.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the descriptor-local ordinal.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Size and alignment of one concrete protected-frame representation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameLayout {
    size: usize,
    alignment: NonZeroUsize,
}

impl ProtectedFrameLayout {
    /// Creates a frame layout when its alignment is a power of two.
    pub const fn try_new(
        size: usize,
        alignment: NonZeroUsize,
    ) -> Result<Self, ProtectedFrameLayoutBuildError> {
        if !alignment.get().is_power_of_two() {
            return Err(ProtectedFrameLayoutBuildError::InvalidAlignment);
        }

        if Layout::from_size_align(size, alignment.get()).is_err() {
            return Err(ProtectedFrameLayoutBuildError::SizeOverflow);
        }

        Ok(Self { size, alignment })
    }

    /// Returns the concrete frame size in bytes.
    pub const fn size(self) -> usize {
        self.size
    }

    /// Returns the concrete frame alignment in bytes.
    pub const fn alignment(self) -> NonZeroUsize {
        self.alignment
    }
}

/// A malformed protected-frame storage layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectedFrameLayoutBuildError {
    /// The alignment is not a power of two.
    InvalidAlignment,
    /// The aligned size is not representable by the target allocator.
    SizeOverflow,
}

/// Runtime-visible requirements of one resumable protected-frame state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameStateDescriptor {
    state: ProtectedFrameStateId,
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
}

impl ProtectedFrameStateDescriptor {
    /// Creates one state descriptor with canonical lane requirements.
    pub fn new(
        state: ProtectedFrameStateId,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
    ) -> Self {
        Self {
            state,
            lane_requirements: sorted_unique_shared_slice(lane_requirements),
        }
    }

    /// Returns the descriptor-local state identity.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.state
    }

    /// Returns the execution-lane requirements active in this state.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }
}

/// Complete runtime contract for one compiler-generated protected frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    layout: ProtectedFrameLayout,
    states: Arc<[ProtectedFrameStateDescriptor]>,
}

impl ProtectedFrameDescriptor {
    /// Creates a descriptor with a nonempty contiguous state table.
    pub fn try_new(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        layout: ProtectedFrameLayout,
        states: impl IntoIterator<Item = ProtectedFrameStateDescriptor>,
    ) -> Result<Self, ProtectedFrameDescriptorBuildError> {
        let states: Vec<_> = states.into_iter().collect();

        if states.is_empty() {
            return Err(ProtectedFrameDescriptorBuildError::MissingState);
        }

        let mut state_ids = BTreeSet::new();

        for (ordinal, state) in states.iter().enumerate() {
            if !state_ids.insert(state.state()) {
                return Err(ProtectedFrameDescriptorBuildError::DuplicateState);
            }

            let Ok(ordinal) = u32::try_from(ordinal) else {
                return Err(
                    ProtectedFrameDescriptorBuildError::IdentityCapacityExceeded,
                );
            };

            if state.state().raw() != ordinal {
                return Err(ProtectedFrameDescriptorBuildError::NonContiguousState);
            }
        }

        Ok(Self {
            frame,
            abi_version,
            frame_abi,
            layout,
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

    /// Returns the independently versioned protected-frame operations.
    pub const fn frame_abi(&self) -> ProtectedFrameAbiVersions {
        self.frame_abi
    }

    /// Returns the concrete frame storage layout.
    pub const fn layout(&self) -> ProtectedFrameLayout {
        self.layout
    }

    /// Returns the ordered resumable state table.
    pub fn states(&self) -> &[ProtectedFrameStateDescriptor] {
        &self.states
    }

    /// Returns one resumable state when its identity belongs to this frame.
    pub fn state(
        &self,
        state: ProtectedFrameStateId,
    ) -> Option<&ProtectedFrameStateDescriptor> {
        let index = usize::try_from(state.raw()).ok()?;

        self.states.get(index).filter(|entry| entry.state() == state)
    }
}

/// A malformed protected-frame descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectedFrameDescriptorBuildError {
    /// The descriptor has no resumable state.
    MissingState,
    /// Ordered state identities are not contiguous from zero.
    NonContiguousState,
    /// The descriptor repeats one state identity.
    DuplicateState,
    /// The state table exceeds its compact identity representation.
    IdentityCapacityExceeded,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::{
        ProtectedFrameDescriptor, ProtectedFrameDescriptorBuildError,
        ProtectedFrameLayout, ProtectedFrameLayoutBuildError,
        ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    };
    use crate::{
        ExecutionLaneRequirement, ProtectedAsyncFrameId,
        ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };

    #[test]
    fn frame_layouts_require_power_of_two_alignment() {
        let Some(valid_alignment) = NonZeroUsize::new(8) else {
            panic!("test alignment must be nonzero");
        };

        let Some(invalid_alignment) = NonZeroUsize::new(3) else {
            panic!("test alignment must be nonzero");
        };

        assert_eq!(
            ProtectedFrameLayout::try_new(64, valid_alignment),
            Ok(ProtectedFrameLayout {
                size: 64,
                alignment: valid_alignment,
            })
        );

        assert_eq!(
            ProtectedFrameLayout::try_new(64, invalid_alignment),
            Err(ProtectedFrameLayoutBuildError::InvalidAlignment)
        );

        assert_eq!(
            ProtectedFrameLayout::try_new(usize::MAX, valid_alignment),
            Err(ProtectedFrameLayoutBuildError::SizeOverflow)
        );
    }

    #[test]
    fn frame_descriptors_require_nonempty_contiguous_states() {
        let frame = ProtectedAsyncFrameId::new([3; 32]);
        let abi = RuntimeAbiVersion::new(1, 0);
        let frame_abi = ProtectedFrameAbiVersions::uniform(abi);

        let Some(alignment) = NonZeroUsize::new(8) else {
            panic!("test alignment must be nonzero");
        };

        let Ok(layout) = ProtectedFrameLayout::try_new(64, alignment) else {
            panic!("test frame layout must be valid");
        };

        let state = ProtectedFrameStateDescriptor::new(
            ProtectedFrameStateId::new(0),
            [ExecutionLaneRequirement::Compute],
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(frame, abi, frame_abi, layout, []),
            Err(ProtectedFrameDescriptorBuildError::MissingState)
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                [state.clone(), state]
            ),
            Err(ProtectedFrameDescriptorBuildError::DuplicateState)
        );

        let non_contiguous = ProtectedFrameStateDescriptor::new(
            ProtectedFrameStateId::new(1),
            [],
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                [non_contiguous]
            ),
            Err(ProtectedFrameDescriptorBuildError::NonContiguousState)
        );
    }
}
