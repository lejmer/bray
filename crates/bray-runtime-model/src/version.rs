use crate::RuntimeAbiVersion;

/// Independently versioned operation in one protected-frame descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProtectedFrameAbiOperation {
    /// Enter or resume one protected frame.
    Resume,
    /// Broadcast cancellation to owned unresolved tasks.
    TaskBroadcast,
    /// Resolve retained lifecycle state.
    LifecycleResolution,
    /// Move an initialized completion value out of the frame.
    CompletionMove,
    /// Infallibly destroy terminal frame storage.
    Destruction,
}

impl ProtectedFrameAbiOperation {
    /// Every protected-frame operation in deterministic ABI order.
    pub const ALL: [Self; 5] = [
        Self::Resume,
        Self::TaskBroadcast,
        Self::LifecycleResolution,
        Self::CompletionMove,
        Self::Destruction,
    ];
}

/// ABI versions of the independently evolvable protected-frame operations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectedFrameAbiVersions {
    resume: RuntimeAbiVersion,
    task_broadcast: RuntimeAbiVersion,
    lifecycle_resolution: RuntimeAbiVersion,
    completion_move: RuntimeAbiVersion,
    destruction: RuntimeAbiVersion,
}

impl ProtectedFrameAbiVersions {
    /// Creates the complete protected-frame operation version set.
    pub const fn new(
        resume: RuntimeAbiVersion,
        task_broadcast: RuntimeAbiVersion,
        lifecycle_resolution: RuntimeAbiVersion,
        completion_move: RuntimeAbiVersion,
        destruction: RuntimeAbiVersion,
    ) -> Self {
        Self {
            resume,
            task_broadcast,
            lifecycle_resolution,
            completion_move,
            destruction,
        }
    }

    /// Creates a version set when every operation uses one ABI version.
    pub const fn uniform(version: RuntimeAbiVersion) -> Self {
        Self::new(version, version, version, version, version)
    }

    /// Returns the version of one protected-frame operation.
    pub const fn operation(self, operation: ProtectedFrameAbiOperation) -> RuntimeAbiVersion {
        match operation {
            ProtectedFrameAbiOperation::Resume => self.resume,
            ProtectedFrameAbiOperation::TaskBroadcast => self.task_broadcast,
            ProtectedFrameAbiOperation::LifecycleResolution => self.lifecycle_resolution,
            ProtectedFrameAbiOperation::CompletionMove => self.completion_move,
            ProtectedFrameAbiOperation::Destruction => self.destruction,
        }
    }

    /// Returns the first operation whose required version this set cannot satisfy.
    pub fn first_incompatible(self, required: Self) -> Option<ProtectedFrameAbiOperation> {
        ProtectedFrameAbiOperation::ALL
            .into_iter()
            .find(|operation| {
                !self
                    .operation(*operation)
                    .supports(required.operation(*operation))
            })
    }

    /// Combines two compatible version requirements at their strongest minor versions.
    pub fn merge(self, other: Self) -> Result<Self, ProtectedFrameAbiOperation> {
        Ok(Self::new(
            merge_version(
                self.resume,
                other.resume,
                ProtectedFrameAbiOperation::Resume,
            )?,
            merge_version(
                self.task_broadcast,
                other.task_broadcast,
                ProtectedFrameAbiOperation::TaskBroadcast,
            )?,
            merge_version(
                self.lifecycle_resolution,
                other.lifecycle_resolution,
                ProtectedFrameAbiOperation::LifecycleResolution,
            )?,
            merge_version(
                self.completion_move,
                other.completion_move,
                ProtectedFrameAbiOperation::CompletionMove,
            )?,
            merge_version(
                self.destruction,
                other.destruction,
                ProtectedFrameAbiOperation::Destruction,
            )?,
        ))
    }
}

fn merge_version(
    left: RuntimeAbiVersion,
    right: RuntimeAbiVersion,
    operation: ProtectedFrameAbiOperation,
) -> Result<RuntimeAbiVersion, ProtectedFrameAbiOperation> {
    if left.major() != right.major() {
        return Err(operation);
    }

    Ok(RuntimeAbiVersion::new(
        left.major(),
        left.minor().max(right.minor()),
    ))
}
