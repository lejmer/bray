use crate::NativeFrameStateCallback;

/// Returns one immutable frame layout from a compiler-generated cleanup reservation bundle.
/// The defining native product retains the metadata for the lifetime of every admitted owner.
pub type NativeFrameMetadataProvider = extern "C" fn(usize) -> Option<&'static NativeFrameMetadata>;

/// Returns immutable pre-entry metadata for one compiler-generated frame.
/// The defining native product retains it for the lifetime of every admitted owner.
pub type NativeFrameMetadataCallback = extern "C" fn() -> Option<&'static NativeFrameMetadata>;

/// Immutable layout and scheduling metadata, available before a frame owns its captures.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeFrameMetadata {
    identity: [u8; 32],
    state_count: u32,
    size: usize,
    alignment: usize,
    completion_size: usize,
    completion_alignment: usize,
    state: NativeFrameStateCallback,
}

impl NativeFrameMetadata {
    /// Returns the completion layout required by the selected ownership entry.
    pub const fn for_entry(mut self, entry: crate::NativeFrameEntry) -> Self {
        if matches!(
            entry,
            crate::NativeFrameEntry::CaptureQuiescence
                | crate::NativeFrameEntry::CaptureDestruction
        ) {
            self.completion_size = 0;
            self.completion_alignment = 1;
        }

        self
    }

    /// Describes a generated frame without constructing or borrowing its owned context.
    pub const fn new(
        identity: [u8; 32],
        state_count: u32,
        size: usize,
        alignment: usize,
        completion_size: usize,
        completion_alignment: usize,
        state: NativeFrameStateCallback,
    ) -> Self {
        Self {
            identity,
            state_count,
            size,
            alignment,
            completion_size,
            completion_alignment,
            state,
        }
    }

    /// Returns the stable frame representation identity.
    pub const fn identity(&self) -> [u8; 32] {
        self.identity
    }

    /// Returns the number of resumable frame states.
    pub const fn state_count(&self) -> u32 {
        self.state_count
    }

    /// Returns the frame storage size.
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Returns the frame storage alignment.
    pub const fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns the completion storage size.
    pub const fn completion_size(&self) -> usize {
        self.completion_size
    }

    /// Returns the completion storage alignment.
    pub const fn completion_alignment(&self) -> usize {
        self.completion_alignment
    }

    /// Returns the context-independent state-description callback.
    pub const fn state(&self) -> NativeFrameStateCallback {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::NativeFrameMetadata;

    #[test]
    fn optional_metadata_providers_use_one_native_function_pointer() {
        assert_eq!(
            std::mem::size_of::<Option<super::NativeFrameMetadataProvider>>(),
            std::mem::size_of::<usize>()
        );

        assert_eq!(
            std::mem::align_of::<Option<super::NativeFrameMetadataProvider>>(),
            std::mem::align_of::<usize>()
        );
    }

    #[test]
    fn frame_metadata_has_the_native_abi_layout() {
        assert_abi_layout!(NativeFrameMetadata, size: 80, align: 8, fields: {
            identity: 0,
            state_count: 32,
            size: 40,
            alignment: 48,
            completion_size: 56,
            completion_alignment: 64,
            state: 72,
        });
    }
}
