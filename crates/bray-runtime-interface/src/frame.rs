use std::alloc::Layout;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

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

/// Compiler-emitted operation available for one protected frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProtectedFrameOperation {
    /// Moves an inactive frame into runtime-owned storage.
    MoveBeforeStart,
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
    pub const ALL: [Self; 7] = [
        Self::MoveBeforeStart,
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
            Self::MoveBeforeStart => "move_before_start",
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
    move_before_start: BinarySymbolName,
    resume: BinarySymbolName,
    cancellation_entry: BinarySymbolName,
    task_broadcast: BinarySymbolName,
    lifecycle_resolution: BinarySymbolName,
    completion_move: BinarySymbolName,
    destruction: BinarySymbolName,
}

impl ProtectedFrameOperations {
    /// Creates the complete protected-frame operation table.
    pub const fn new(
        move_before_start: BinarySymbolName,
        resume: BinarySymbolName,
        cancellation_entry: BinarySymbolName,
        task_broadcast: BinarySymbolName,
        lifecycle_resolution: BinarySymbolName,
        completion_move: BinarySymbolName,
        destruction: BinarySymbolName,
    ) -> Self {
        Self {
            move_before_start,
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
            ProtectedFrameOperation::MoveBeforeStart => &self.move_before_start,
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
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
    initialized_storage: Arc<[ProtectedFrameStorageId]>,
    dependencies: Arc<[ProtectedFrameDependencyId]>,
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
            lane_requirements: sorted_unique_shared_slice(lane_requirements),
            initialized_storage: sorted_unique_shared_slice(initialized_storage),
            dependencies: sorted_unique_shared_slice(dependencies),
            affinity,
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

    /// Returns storage known to be initialized while suspended in this state.
    pub fn initialized_storage(&self) -> &[ProtectedFrameStorageId] {
        &self.initialized_storage
    }

    /// Returns unresolved task dependencies owned in this state.
    pub fn dependencies(&self) -> &[ProtectedFrameDependencyId] {
        &self.dependencies
    }

    /// Returns the thread-affinity contract active in this state.
    pub const fn affinity(&self) -> ProtectedFrameAffinity {
        self.affinity
    }
}

/// Complete runtime contract for one compiler-generated protected frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedFrameDescriptor {
    frame: ProtectedAsyncFrameId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    layout: ProtectedFrameLayout,
    completion_layout: ProtectedFrameLayout,
    operations: ProtectedFrameOperations,
    states: Arc<[ProtectedFrameStateDescriptor]>,
}

impl ProtectedFrameDescriptor {
    /// Creates a descriptor with a nonempty contiguous state table.
    pub fn try_new(
        frame: ProtectedAsyncFrameId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        layout: ProtectedFrameLayout,
        completion_layout: ProtectedFrameLayout,
        operations: ProtectedFrameOperations,
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
                return Err(ProtectedFrameDescriptorBuildError::IdentityCapacityExceeded);
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
            completion_layout,
            operations,
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

    /// Returns the completion-payload storage layout.
    pub const fn completion_layout(&self) -> ProtectedFrameLayout {
        self.completion_layout
    }

    /// Returns the compiler-emitted operation table.
    pub const fn operations(&self) -> &ProtectedFrameOperations {
        &self.operations
    }

    /// Returns the ordered resumable state table.
    pub fn states(&self) -> &[ProtectedFrameStateDescriptor] {
        &self.states
    }

    /// Returns one resumable state when its identity belongs to this frame.
    pub fn state(&self, state: ProtectedFrameStateId) -> Option<&ProtectedFrameStateDescriptor> {
        let index = usize::try_from(state.raw()).ok()?;

        self.states
            .get(index)
            .filter(|entry| entry.state() == state)
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
        ProtectedFrameAffinity, ProtectedFrameDescriptor, ProtectedFrameDescriptorBuildError,
        ProtectedFrameLayout, ProtectedFrameLayoutBuildError, ProtectedFrameOperation,
        ProtectedFrameOperations, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    };
    use crate::{
        BinarySymbolName, ExecutionLaneRequirement, ProtectedAsyncFrameId,
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
            [],
            [],
            ProtectedFrameAffinity::Movable,
        );

        let operations = test_operations();

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                layout,
                operations.clone(),
                []
            ),
            Err(ProtectedFrameDescriptorBuildError::MissingState)
        );

        assert_eq!(
            ProtectedFrameDescriptor::try_new(
                frame,
                abi,
                frame_abi,
                layout,
                layout,
                operations.clone(),
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
                operations,
                [non_contiguous]
            ),
            Err(ProtectedFrameDescriptorBuildError::NonContiguousState)
        );
    }

    #[test]
    fn frame_descriptors_expose_emitted_operation_symbols() {
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
            symbol("__bray_test_move"),
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
