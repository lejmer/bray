use std::mem::MaybeUninit;

use bray_runtime_abi::{NativeFrameMetadata, NativeProtectedFrame, NativeRuntimeStatus};
use bray_runtime_model::ProtectedFrameDescriptor;
use triomphe::Arc;

use super::super::frame::{NativeFrame, NativeTerminalState};
use super::super::storage::NativeStorage;
use super::NativeActivation;

pub(in crate::native) struct NativeActivationReservation {
    descriptor: ProtectedFrameDescriptor,
    terminal: Arc<NativeTerminalState>,
    frame_storage: Box<MaybeUninit<NativeFrame>>,
    activation_storage: Box<MaybeUninit<NativeActivation>>,
    completion: NativeStorage,
}

impl NativeActivationReservation {
    pub(in crate::native) fn prepare(
        metadata: &NativeFrameMetadata,
    ) -> Result<Self, NativeRuntimeStatus> {
        let descriptor = NativeFrame::checked_descriptor(metadata)?;

        // Secure the terminal destination before accepting ownership of the inactive frame.
        let completion =
            NativeStorage::new(metadata.completion_size(), metadata.completion_alignment())?;

        let frame_storage = crate::allocation::reserve_storage::<NativeFrame>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let terminal = NativeTerminalState::reserve()?;

        let activation_storage = crate::allocation::reserve_storage::<NativeActivation>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        Ok(Self {
            descriptor,
            terminal,
            frame_storage,
            activation_storage,
            completion,
        })
    }

    pub(in crate::native) fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    pub(in crate::native) fn terminal(&self) -> &Arc<NativeTerminalState> {
        &self.terminal
    }

    pub(in crate::native) fn install(self, abi: NativeProtectedFrame) -> Box<NativeActivation> {
        // The frame and activation share the admitted terminal payload owner.
        let frame = NativeFrame::new(
            abi,
            self.descriptor,
            self.completion,
            Arc::clone(&self.terminal),
        );

        let frame = Box::into_pin(Box::write(self.frame_storage, frame));

        Box::write(
            self.activation_storage,
            NativeActivation::new(frame, self.terminal),
        )
    }
}
