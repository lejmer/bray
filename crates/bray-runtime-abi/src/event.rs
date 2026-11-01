/// Retained storage for one Bray-owned event. Copying its address grants no access.
#[repr(C)]
#[derive(Debug, Default)]
pub struct NativeEventReference {
    address: usize,
}

/// Retained native context and ABI callback transferred to one event registration.
#[repr(C)]
#[derive(Debug)]
pub struct NativeEventCallback {
    /// Context kept alive until the release callback returns.
    pub context: usize,
    /// Notify on operation one, or release the descriptor on operation zero.
    pub call: extern "C" fn(usize, u32) -> crate::NativeRuntimeStatus,
}

#[cfg(test)]
mod tests {
    use super::{NativeEventCallback, NativeEventReference};

    #[test]
    fn event_owners_match_bray_layouts() {
        assert_abi_layout!(NativeEventReference, size: 8, align: 8, fields: { address: 0 });
        assert_abi_layout!(NativeEventCallback, size: 16, align: 8, fields: { context: 0, call: 8 });
    }
}
