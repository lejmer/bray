use crate::{NativeRunOutcome, NativeRunState, NativeRuntimeStatus};

/// Moves a terminal outcome into its concrete Bray `RunResult<T>` representation.
///
/// The destination has the descriptor's size and alignment. A completed outcome borrows
/// an initialized `T` until success transfers it. Failure leaves both owners unchanged.
/// The callback retains neither address and initializes the destination only on success.
pub type NativeRunResultTransferCallback =
    extern "C" fn(destination: usize, outcome: &NativeRunOutcome) -> NativeRuntimeStatus;

/// Storage requirements and compiler-generated transfer for one Bray `RunResult<T>`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeRunResultLayout {
    size: usize,
    alignment: usize,
    transfer: NativeRunResultTransferCallback,
}

impl NativeRunResultLayout {
    /// Creates a descriptor whose callback constructs the selected concrete union.
    pub const fn new(
        size: usize,
        alignment: usize,
        transfer: NativeRunResultTransferCallback,
    ) -> Self {
        Self {
            size,
            alignment,
            transfer,
        }
    }

    /// Returns the total represented byte size.
    pub const fn size(self) -> usize {
        self.size
    }

    /// Returns the required represented alignment.
    pub const fn alignment(self) -> usize {
        self.alignment
    }

    /// Returns whether the descriptor admits a native allocation.
    pub fn is_valid(self) -> bool {
        std::alloc::Layout::from_size_align(self.size.max(1), self.alignment).is_ok()
    }

    /// Transfers one terminal outcome, preserving ownership when validation or the callback fails.
    pub fn transfer(self, destination: usize, outcome: &NativeRunOutcome) -> NativeRuntimeStatus {
        if !self.is_valid() || destination == 0 || !destination.is_multiple_of(self.alignment) {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        match outcome.state() {
            NativeRunState::COMPLETED | NativeRunState::PANICKED | NativeRunState::CANCELLED => {
                (self.transfer)(destination, outcome)
            }
            NativeRunState::PENDING => NativeRuntimeStatus::PENDING,
            _ => NativeRuntimeStatus::RUNTIME_FAILURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeRunResultLayout;
    use crate::{NativeRunOutcome, NativeRunState, NativeRuntimeStatus};

    extern "C" fn reject_transfer(_: usize, _: &NativeRunOutcome) -> NativeRuntimeStatus {
        NativeRuntimeStatus::UNKNOWN_TASK
    }

    #[test]
    fn run_result_layout_has_the_native_abi_layout() {
        assert_abi_layout!(NativeRunResultLayout, size: 24, align: 8, fields: {
            size: 0,
            alignment: 8,
            transfer: 16,
        });
    }

    #[test]
    fn transfer_validates_storage_and_preserves_the_exact_callback_failure() {
        let layout = NativeRunResultLayout::new(16, 8, reject_transfer);
        let completed = NativeRunOutcome::new(NativeRunState::COMPLETED, 8);

        assert!(layout.is_valid());
        assert!(!NativeRunResultLayout::new(16, 3, reject_transfer).is_valid());
        assert!(!NativeRunResultLayout::new(usize::MAX, 8, reject_transfer).is_valid());
        assert!(NativeRunResultLayout::new(0, 8, reject_transfer).is_valid());

        assert!(
            !NativeRunResultLayout::new(0, 1usize << (usize::BITS - 1), reject_transfer).is_valid()
        );

        assert_eq!(
            layout.transfer(0, &completed),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            layout.transfer(1, &completed),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            layout.transfer(8, &completed),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        for (state, expected) in [
            (NativeRunState::PENDING, NativeRuntimeStatus::PENDING),
            (
                NativeRunState::RUNTIME_FAILURE,
                NativeRuntimeStatus::RUNTIME_FAILURE,
            ),
        ] {
            assert_eq!(
                layout.transfer(8, &NativeRunOutcome::new(state, 0)),
                expected
            );
        }
    }
}
