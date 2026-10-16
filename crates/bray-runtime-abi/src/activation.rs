use crate::{
    NativeCleanupStorageShape, NativeFrameActionCallback, NativeFrameCancellationCallback,
    NativeFrameCompletionMoveCallback, NativeFrameResolveCallback, NativeFrameResumeCallback,
    NativeFrameStateCallback,
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
    pub move_completion: NativeFrameCompletionMoveCallback,
    /// Releases provider-owned context after its value obligations have ended.
    pub destroy: NativeFrameActionCallback,
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::NativeActivationDescriptor;

    #[test]
    fn activation_descriptor_matches_the_producer_layout() {
        assert_abi_layout!(NativeActivationDescriptor, size: 144, align: 8, fields: {
            storage: 0, header: 48, completion: 56, completion_bytes: 64,
            completion_align: 72, states: 80, state: 88, resume: 96, cancel: 104,
            broadcast: 112, resolve: 120, move_completion: 128, destroy: 136,
        });
    }
}
