use crate::{
    NativeCleanupStorageShape, NativeFrameActionCallback, NativeFrameCancellationCallback,
    NativeFrameCompletionMoveCallback, NativeFrameExit, NativeFrameProgress,
    NativeFrameResolveCallback, NativeFrameResumeCallback, NativeFrameState,
    NativeFrameStateCallback, NativeReportRecords,
};

/// Immutable concrete frame description retained by its generated or native provider.
/// The descriptor describes bytes and operations. It does not grant ownership of a lease.
/// Retaining callbacks keep the frame live, including through reentry. Destruction
/// follows result, report and child retirement and may release frame storage before
/// returning. The dispatcher copies the descriptor before that consuming callback,
/// so providers may store it in their frame. Callback code remains live through return.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeActivationDescriptor {
    /// Exact resident storage identity, size and alignment.
    pub storage: NativeCleanupStorageShape,
    /// Byte offset of the Bray activation header within the admitted region.
    pub header: usize,
    /// Byte offset of the completion destination within the admitted region.
    pub completion: usize,
    /// Size of the completion value.
    pub completion_bytes: usize,
    /// Required alignment of the completion value.
    pub completion_align: usize,
    /// Number of source continuation states, including inactive state zero.
    pub states: u32,
    /// Returns the immutable requirements for one continuation state.
    pub state: NativeFrameStateCallback,
    /// Enters the current source continuation.
    pub resume: NativeFrameResumeCallback,
    /// Delivers cancellation at the checked source entry.
    pub cancel: NativeFrameCancellationCallback,
    /// Broadcasts initialized child obligations without resolving them.
    pub broadcast: NativeFrameResumeCallback,
    /// Advances mandatory lifecycle resolution, which may suspend.
    pub resolve: NativeFrameResolveCallback,
    /// Transfers the completed value to the caller's admitted destination.
    /// Unwinding before transfer must leave the input owned by the frame. Once
    /// transfer commits, the callback must return normally and cannot suspend.
    /// Returned dependencies follow the destination's result contract.
    pub move_completion: NativeFrameCompletionMoveCallback,
    /// Releases provider-owned context after its value obligations have ended.
    pub destroy: NativeFrameActionCallback,
}

/// Current checked execution requirements for one exclusively owned activation.
/// Parent requirements remain effective while the parent is suspended on this frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeActivationExecution {
    /// Address of the exact exclusively dispatched activation header.
    pub address: usize,
    /// Identity of the immutable concrete frame description.
    pub identity: [u8; 32],
    /// Current continuation ordinal.
    pub state: u32,
    /// Combined local and retained-parent execution requirements.
    pub requirements: NativeFrameState,
    /// Origin thread required by an origin-affine frame.
    pub origin: u64,
}

impl Default for NativeActivationExecution {
    fn default() -> Self {
        Self {
            address: 0,
            identity: [0; 32],
            state: 0,
            requirements: NativeFrameState::new(
                crate::NativeFrameAffinity::MOVABLE,
                crate::NativeLaneRequirements::NONE,
            ),
            origin: 0,
        }
    }
}

/// The selected producer operation and its operation-specific input.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeActivationOperation {
    /// Resume the current continuation.
    Resume,
    /// Enter checked cancellation.
    Cancel,
    /// Broadcast cancellation to initialized children.
    Broadcast,
    /// Advance mandatory cleanup for this exit.
    Resolve(NativeFrameExit),
    /// Move a successful completion into its admitted destination.
    Move(usize),
    /// Release the resolved frame context.
    Destroy,
}

impl NativeActivationOperation {
    /// Returns whether the callback can publish a retained continuation.
    pub fn is_resumable(self) -> bool {
        match self {
            Self::Resume | Self::Cancel => true,
            Self::Resolve(exit) => {
                exit != NativeFrameExit::RETAINED_DESTRUCTION
                    && exit != NativeFrameExit::SYNCHRONOUS_DISPOSAL
            }
            Self::Broadcast | Self::Move(_) | Self::Destroy => false,
        }
    }
}

/// One admitted producer call with a retained descriptor and exclusive outputs.
///
/// The caller retains the execution snapshot, descriptor provider and both
/// output slots through return. Destruction may consume frame storage, while
/// this call and its outputs remain live independently of that storage.
#[repr(C)]
#[derive(Debug)]
pub struct NativeActivationInvocation<'a> {
    /// Exact execution snapshot checked for this call.
    pub execution: &'a NativeActivationExecution,
    /// Immutable descriptor retained independently of consuming callbacks.
    pub descriptor: &'a NativeActivationDescriptor,
    /// Producer-owned frame context.
    pub context: usize,
    /// Checked operation and its input.
    pub operation: NativeActivationOperation,
    /// Initialized progress slot with an empty incoming report.
    pub progress: &'a mut NativeFrameProgress,
    /// Exclusively owned native callback report allowance.
    pub records: &'a mut NativeReportRecords,
    /// Lane admission result for this invocation, set before producer entry.
    pub admission: u32,
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::{
        NativeActivationDescriptor, NativeActivationExecution, NativeActivationInvocation,
        NativeActivationOperation,
    };

    #[test]
    fn activation_invocation_matches_the_dispatcher_layout() {
        assert_abi_layout!(NativeActivationOperation, size: 16, align: 8, fields: {});

        assert_abi_layout!(NativeActivationInvocation<'_>, size: 64, align: 8, fields: {
            execution: 0, descriptor: 8, context: 16, operation: 24, progress: 40, records: 48, admission: 56,
        });
    }

    #[test]
    fn activation_execution_matches_the_dispatcher_layout() {
        assert_abi_layout!(NativeActivationExecution, size: 64, align: 8, fields: {
            address: 0, identity: 8, state: 40, requirements: 44, origin: 56,
        });
    }

    #[test]
    fn activation_descriptor_matches_the_producer_layout() {
        assert_abi_layout!(NativeActivationDescriptor, size: 144, align: 8, fields: {
            storage: 0, header: 48, completion: 56, completion_bytes: 64,
            completion_align: 72, states: 80, state: 88, resume: 96, cancel: 104,
            broadcast: 112, resolve: 120, move_completion: 128, destroy: 136,
        });
    }
}
