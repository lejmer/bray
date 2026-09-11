use std::alloc::Layout;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::{
    BinarySymbolName, ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
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

/// Descriptor-local identity of storage retained by one protected frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectedFrameStorageId(u32);

impl ProtectedFrameStorageId {
    /// Creates a descriptor-local storage identity.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the descriptor-local ordinal.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Descriptor-local identity of a task dependency retained by one frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectedFrameDependencyId(u32);

impl ProtectedFrameDependencyId {
    /// Creates a descriptor-local dependency identity.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the descriptor-local ordinal.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Thread-affinity contract active while a protected frame is suspended.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProtectedFrameAffinity {
    /// The suspended frame may resume on any compatible worker.
    Movable,
    /// The suspended frame must resume on the thread that started it.
    OriginThread,
    /// The suspended frame must resume on the process main thread.
    MainThread,
}

impl ProtectedFrameAffinity {
    /// Returns the stable native and executable-format code.
    pub const fn code(self) -> u32 {
        match self {
            Self::Movable => 0,
            Self::OriginThread => 1,
            Self::MainThread => 2,
        }
    }

    /// Resolves one stable native or executable-format code.
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Movable),
            1 => Some(Self::OriginThread),
            2 => Some(Self::MainThread),
            _ => None,
        }
    }
}

/// Compiler-emitted operation available for one protected frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProtectedFrameOperation {
    /// Returns immutable admission metadata without constructing a frame.
    MetadataDescription,
    /// Moves an inactive frame into runtime-owned storage.
    MoveBeforeStart,
    /// Describes one resumable state to the runtime.
    StateDescription,
    /// Enters or resumes the frame.
    Resume,
    /// Enters cancellation cleanup.
    CancellationEntry,
    /// Broadcasts cancellation to unresolved owned tasks.
    TaskBroadcast,
    /// Resolves retained lifecycle state.
    LifecycleResolution,
    /// Moves the completion payload to its observer.
    CompletionMove,
    /// Destroys retained frame storage.
    Destruction,
}

impl ProtectedFrameOperation {
    /// Every compiler-emitted protected-frame operation in stable order.
    pub const ALL: [Self; 9] = [
        Self::MetadataDescription,
        Self::MoveBeforeStart,
        Self::StateDescription,
        Self::Resume,
        Self::CancellationEntry,
        Self::TaskBroadcast,
        Self::LifecycleResolution,
        Self::CompletionMove,
        Self::Destruction,
    ];

    /// Returns this compiler-generated operation's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetadataDescription => "metadata_description",
            Self::MoveBeforeStart => "move_before_start",
            Self::StateDescription => "state_description",
            Self::Resume => "resume",
            Self::CancellationEntry => "cancellation_entry",
            Self::TaskBroadcast => "task_broadcast",
            Self::LifecycleResolution => "lifecycle_resolution",
            Self::CompletionMove => "completion_move",
            Self::Destruction => "destruction",
        }
    }
}

/// Binary symbol table for compiler-emitted protected-frame operations.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameOperations {
    metadata_description: BinarySymbolName,
    move_before_start: BinarySymbolName,
    state_description: BinarySymbolName,
    resume: BinarySymbolName,
    cancellation_entry: BinarySymbolName,
    task_broadcast: BinarySymbolName,
    lifecycle_resolution: BinarySymbolName,
    completion_move: BinarySymbolName,
    destruction: BinarySymbolName,
}

impl ProtectedFrameOperations {
    /// Creates the complete protected-frame operation table.
    #[expect(
        clippy::too_many_arguments,
        reason = "each protected-frame operation has one required typed symbol"
    )]
    pub const fn new(
        metadata_description: BinarySymbolName,
        move_before_start: BinarySymbolName,
        state_description: BinarySymbolName,
        resume: BinarySymbolName,
        cancellation_entry: BinarySymbolName,
        task_broadcast: BinarySymbolName,
        lifecycle_resolution: BinarySymbolName,
        completion_move: BinarySymbolName,
        destruction: BinarySymbolName,
    ) -> Self {
        Self {
            metadata_description,
            move_before_start,
            state_description,
            resume,
            cancellation_entry,
            task_broadcast,
            lifecycle_resolution,
            completion_move,
            destruction,
        }
    }

    /// Returns the binary symbol implementing one protected-frame operation.
    pub const fn symbol(&self, operation: ProtectedFrameOperation) -> &BinarySymbolName {
        match operation {
            ProtectedFrameOperation::MetadataDescription => &self.metadata_description,
            ProtectedFrameOperation::MoveBeforeStart => &self.move_before_start,
            ProtectedFrameOperation::StateDescription => &self.state_description,
            ProtectedFrameOperation::Resume => &self.resume,
            ProtectedFrameOperation::CancellationEntry => &self.cancellation_entry,
            ProtectedFrameOperation::TaskBroadcast => &self.task_broadcast,
            ProtectedFrameOperation::LifecycleResolution => &self.lifecycle_resolution,
            ProtectedFrameOperation::CompletionMove => &self.completion_move,
            ProtectedFrameOperation::Destruction => &self.destruction,
        }
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
    lane_requirements:
        arrayvec::ArrayVec<ExecutionLaneRequirement, { ExecutionLaneRequirement::ALL.len() }>,
    initialized_storage: Option<Arc<[ProtectedFrameStorageId]>>,
    dependencies: Option<Arc<[ProtectedFrameDependencyId]>>,
    affinity: ProtectedFrameAffinity,
}

impl ProtectedFrameStateDescriptor {
    /// Creates one state descriptor with canonical retained-state metadata.
    pub fn new(
        state: ProtectedFrameStateId,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        initialized_storage: impl IntoIterator<Item = ProtectedFrameStorageId>,
        dependencies: impl IntoIterator<Item = ProtectedFrameDependencyId>,
        affinity: ProtectedFrameAffinity,
    ) -> Self {
        Self {
            state,
            lane_requirements: arrayvec::ArrayVec::new(),
            initialized_storage: sorted_unique_slice(initialized_storage),
            dependencies: sorted_unique_slice(dependencies),
            affinity,
        }
        .with_execution_requirements(lane_requirements, affinity)
    }

    /// Replaces execution requirements while retaining this frame's local storage identities.
    /// Composed execution uses this to include requirements of suspended parent activations.
    pub fn with_execution_requirements(
        mut self,
        lane_requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        affinity: ProtectedFrameAffinity,
    ) -> Self {
        self.lane_requirements.clear();

        for requirement in lane_requirements {
            if !self.lane_requirements.contains(&requirement) {
                self.lane_requirements.push(requirement);
            }
        }

        self.lane_requirements.sort_unstable();
        self.affinity = affinity;

        self
    }

    /// Returns the descriptor-local state identity.
    pub const fn state(&self) -> ProtectedFrameStateId {
        self.state
    }

    /// Returns the execution-lane requirements active in this state.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }

    /// Returns storage known to be initialized while suspended in this state.
    pub fn initialized_storage(&self) -> &[ProtectedFrameStorageId] {
        self.initialized_storage.as_deref().unwrap_or(&[])
    }

    /// Returns unresolved task dependencies owned in this state.
    pub fn dependencies(&self) -> &[ProtectedFrameDependencyId] {
        self.dependencies.as_deref().unwrap_or(&[])
    }

    /// Returns the thread-affinity contract active in this state.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.affinity
    }
}

/// Layout and scheduling contract for one compiler-generated protected frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    layout: ProtectedFrameLayout,
    completion_layout: ProtectedFrameLayout,
    states: triomphe::ThinArc<(), ProtectedFrameStateDescriptor>,
}

impl ProtectedFrameDescriptor {
    /// Creates a descriptor with a nonempty contiguous state table.
    /// Returns an allocation failure if storage for the shared table is unavailable.
    pub fn try_new<S>(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        layout: ProtectedFrameLayout,
        completion_layout: ProtectedFrameLayout,
        states: S,
    ) -> Result<Self, ProtectedFrameDescriptorBuildError>
    where
        S: IntoIterator<Item = ProtectedFrameStateDescriptor>,
        S::IntoIter: ExactSizeIterator,
    {
        let states = states.into_iter();

        if states.len() == 0 {
            return Err(ProtectedFrameDescriptorBuildError::MissingState);
        }

        let states = triomphe::ThinArc::try_from_header_and_iter((), states)
            .map_err(|_| ProtectedFrameDescriptorBuildError::AllocationFailed)?;

        for (ordinal, state) in states.slice.iter().enumerate() {
            let Ok(ordinal) = u32::try_from(ordinal) else {
                return Err(ProtectedFrameDescriptorBuildError::IdentityCapacityExceeded);
            };

            // Every preceding identity already equals its ordinal.
            if state.state().raw() < ordinal {
                return Err(ProtectedFrameDescriptorBuildError::DuplicateState);
            }

            if state.state().raw() != ordinal {
                return Err(ProtectedFrameDescriptorBuildError::NonContiguousState);
            }
        }

        Ok(Self {
            frame,
            abi_version,
            frame_abi,
            layout,
            completion_layout,
            states,
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

    /// Returns the completion-payload storage layout.
    pub const fn completion_layout(&self) -> ProtectedFrameLayout {
        self.completion_layout
    }

    /// Returns the ordered resumable state table.
    pub fn states(&self) -> &[ProtectedFrameStateDescriptor] {
        &self.states.slice
    }

    /// Returns one resumable state when its identity belongs to this frame.
    pub fn state(&self, state: ProtectedFrameStateId) -> Option<&ProtectedFrameStateDescriptor> {
        let index = usize::try_from(state.raw()).ok()?;

        self.states
            .slice
            .get(index)
            .filter(|entry| entry.state() == state)
    }
}

fn sorted_unique_slice<T: Ord>(values: impl IntoIterator<Item = T>) -> Option<Arc<[T]>> {
    let values = values.into_iter().collect::<BTreeSet<_>>();

    (!values.is_empty()).then(|| values.into_iter().collect())
}

/// A descriptor construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectedFrameDescriptorBuildError {
    /// Storage for the shared state table could not be allocated.
    AllocationFailed,
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
        ProtectedFrameAffinity, ProtectedFrameDescriptor, ProtectedFrameDescriptorBuildError,
        ProtectedFrameLayout, ProtectedFrameLayoutBuildError, ProtectedFrameOperation,
        ProtectedFrameOperations, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    };
    use crate::{
        BinarySymbolName, ExecutionLaneRequirement, ProtectedAsyncFrameId,
        ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };

    #[test]
    fn state_descriptors_keep_lanes_inline_and_empty_metadata_unallocated() {
        let descriptor = ProtectedFrameStateDescriptor::new(
            ProtectedFrameStateId::new(0),
            ExecutionLaneRequirement::ALL
                .into_iter()
                .rev()
                .chain(ExecutionLaneRequirement::ALL),
            [],
            [],
            ProtectedFrameAffinity::Movable,
        );

        assert_eq!(
            descriptor.lane_requirements(),
            &ExecutionLaneRequirement::ALL
        );

        assert!(descriptor.initialized_storage.is_none());
        assert!(descriptor.dependencies.is_none());
        assert!(descriptor.initialized_storage().is_empty());
        assert!(descriptor.dependencies().is_empty());
    }

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
            [],
            [],
            ProtectedFrameAffinity::Movable,
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(frame, abi, frame_abi, layout, layout, []),
            Err(ProtectedFrameDescriptorBuildError::MissingState)
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                layout,
                [state.clone(), state]
            ),
            Err(ProtectedFrameDescriptorBuildError::DuplicateState)
        );

        let non_contiguous = ProtectedFrameStateDescriptor::new(
            ProtectedFrameStateId::new(1),
            [],
            [],
            [],
            ProtectedFrameAffinity::Movable,
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                layout,
                [non_contiguous]
            ),
            Err(ProtectedFrameDescriptorBuildError::NonContiguousState)
        );
    }

    #[test]
    fn frame_state_lookup_matches_contiguous_identities_and_rejects_late_repeats() {
        let frame = ProtectedAsyncFrameId::new([9; 32]);
        let abi = RuntimeAbiVersion::CURRENT;
        let frame_abi = ProtectedFrameAbiVersions::uniform(abi);
        let layout = ProtectedFrameLayout::try_new(32, NonZeroUsize::new(8).unwrap()).unwrap();

        let state = |ordinal| {
            ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(ordinal),
                [],
                [],
                [],
                ProtectedFrameAffinity::Movable,
            )
        };

        let descriptor = ProtectedFrameDescriptor::try_new(
            frame,
            abi,
            frame_abi,
            layout,
            layout,
            (0..4).map(state),
        )
        .unwrap();

        for ordinal in 0..4 {
            assert_eq!(
                descriptor.state(ProtectedFrameStateId::new(ordinal)),
                Some(&state(ordinal))
            );
        }

        assert!(descriptor.state(ProtectedFrameStateId::new(4)).is_none());

        assert!(
            descriptor
                .state(ProtectedFrameStateId::new(u32::MAX))
                .is_none()
        );

        for (last, error) in [
            (0, ProtectedFrameDescriptorBuildError::DuplicateState),
            (2, ProtectedFrameDescriptorBuildError::DuplicateState),
            (4, ProtectedFrameDescriptorBuildError::NonContiguousState),
        ] {
            assert_eq!(
                ProtectedFrameDescriptor::try_new(
                    frame,
                    abi,
                    frame_abi,
                    layout,
                    layout,
                    [0, 1, 2, last].map(state),
                ),
                Err(error),
            );
        }
    }

    #[test]
    fn frame_operation_tables_expose_emitted_symbols() {
        let operations = test_operations();

        assert_eq!(ProtectedFrameOperation::Resume.as_str(), "resume");

        assert_eq!(
            operations.symbol(ProtectedFrameOperation::Resume).as_str(),
            "__bray_test_resume"
        );

        assert_eq!(
            operations
                .symbol(ProtectedFrameOperation::LifecycleResolution)
                .as_str(),
            "__bray_test_resolve_lifecycle"
        );
    }

    fn test_operations() -> ProtectedFrameOperations {
        ProtectedFrameOperations::new(
            symbol("__bray_test_metadata"),
            symbol("__bray_test_move"),
            symbol("__bray_test_state"),
            symbol("__bray_test_resume"),
            symbol("__bray_test_cancel"),
            symbol("__bray_test_broadcast"),
            symbol("__bray_test_resolve_lifecycle"),
            symbol("__bray_test_move_completion"),
            symbol("__bray_test_destroy"),
        )
    }

    fn symbol(name: &'static str) -> BinarySymbolName {
        BinarySymbolName::try_new(name)
            .unwrap_or_else(|| panic!("test operation symbol must be nonempty"))
    }
}
