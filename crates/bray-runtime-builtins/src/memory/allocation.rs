native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_allocation_v1(
        bytes: usize,
        alignment: usize,
    ) -> *mut u8 {
        crate::allocation::allocate(bytes, alignment)
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_deallocation_v1(
        pointer: *mut u8,
        bytes: usize,
        alignment: usize,
    ) {
        crate::allocation::deallocate(pointer, bytes, alignment);
    }
}

#[cfg(test)]
mod tests {
    use super::{bray_runtime_memory_allocation_v1, bray_runtime_memory_deallocation_v1};

    #[test]
    fn allocation_obeys_empty_and_nonempty_layout_contracts() {
        for (bytes, alignment) in [(0, 1), (32, 16)] {
            let address = bray_runtime_memory_allocation_v1(bytes, alignment);

            assert!(!address.is_null());
            assert_eq!(address.addr() % alignment, 0);

            bray_runtime_memory_deallocation_v1(address, bytes, alignment);
        }
    }
}
