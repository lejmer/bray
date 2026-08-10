use std::alloc::{Layout, alloc, dealloc};
use std::panic::panic_any;

#[derive(Debug)]
pub(crate) struct NativeMemoryAllocationFailure;

pub(crate) fn allocate(bytes: usize, alignment: usize) -> *mut u8 {
    let layout = layout(bytes, alignment);

    #[expect(
        unsafe_code,
        reason = "the native allocation ABI initializes raw storage for generated code"
    )]
    let pointer = unsafe { alloc(layout) };

    if pointer.is_null() {
        panic_any(NativeMemoryAllocationFailure);
    }

    pointer
}

pub(crate) fn deallocate(pointer: *mut u8, bytes: usize, alignment: usize) {
    let layout = layout(bytes, alignment);

    let Some(pointer) = std::ptr::NonNull::new(pointer) else {
        panic_any(NativeMemoryAllocationFailure);
    };

    #[expect(
        unsafe_code,
        reason = "the native allocation ABI releases raw storage owned by generated code"
    )]
    unsafe {
        dealloc(pointer.as_ptr(), layout);
    }
}

fn layout(bytes: usize, alignment: usize) -> Layout {
    Layout::from_size_align(bytes.max(1), alignment)
        .unwrap_or_else(|_| panic_any(NativeMemoryAllocationFailure))
}
