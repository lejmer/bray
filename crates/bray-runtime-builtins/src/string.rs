use std::mem::{align_of, size_of};
use std::panic::panic_any;
use std::sync::atomic::AtomicUsize;

use crate::allocation::NativeMemoryAllocationFailure;

#[derive(Debug)]
struct NativeStringSliceBoundsFailure;

native_export! {
    pub extern "C" fn bray_runtime_string_scalar_count_v1(
        data: *const u8,
        length: usize,
    ) -> usize {
        unsafe { native_utf8(data, length) }.chars().count()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_string_equals_v1(
        left_data: *const u8,
        left_length: usize,
        right_data: *const u8,
        right_length: usize,
    ) -> u8 {
        u8::from(
            (unsafe { native_bytes(left_data, left_length) })
                == (unsafe { native_bytes(right_data, right_length) }),
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_string_scalar_at_v1(
        data: *const u8,
        length: usize,
        index: usize,
        scalar: *mut u32,
    ) -> u8 {
        let Some(value) = unsafe { native_utf8(data, length) }.chars().nth(index) else {
            return 0;
        };

        #[expect(
            unsafe_code,
            reason = "the private native ABI writes to a compiler-provided scalar result"
        )]
        unsafe {
            scalar.write(u32::from(value));
        }

        1
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_string_scalar_slice_v1(
        data: *const u8,
        length: usize,
        start: usize,
        end: usize,
        result_data: *mut *const u8,
        result_length: *mut usize,
        result_owner: *mut *mut u8,
    ) {
        let text = unsafe { native_utf8(data, length) };

        let (Some(start), Some(end)) =
            (scalar_byte_offset(text, start), scalar_byte_offset(text, end))
        else {
            panic_any(NativeStringSliceBoundsFailure);
        };

        if start > end {
            panic_any(NativeStringSliceBoundsFailure);
        }

        write_owned_utf8(
            &text.as_bytes()[start..end],
            result_data,
            result_length,
            result_owner,
        );
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_string_from_utf8_v1(
        data: *const u8,
        length: usize,
        result_data: *mut *const u8,
        result_length: *mut usize,
        result_owner: *mut *mut u8,
    ) -> u8 {
        let bytes = unsafe { native_bytes(data, length) };

        if std::str::from_utf8(bytes).is_err() {
            return 0;
        }

        write_owned_utf8(bytes, result_data, result_length, result_owner);

        1
    }
}

#[expect(
    unsafe_code,
    reason = "private native ABI operations borrow caller-provided byte ranges"
)]
unsafe fn native_bytes<'bytes>(data: *const u8, length: usize) -> &'bytes [u8] {
    if length == 0 {
        return &[];
    }

    assert!(!data.is_null());

    unsafe { std::slice::from_raw_parts(data, length) }
}

#[expect(
    unsafe_code,
    reason = "private string operations receive compiler-proven valid UTF-8"
)]
unsafe fn native_utf8<'text>(data: *const u8, length: usize) -> &'text str {
    unsafe { std::str::from_utf8_unchecked(native_bytes(data, length)) }
}

fn scalar_byte_offset(text: &str, index: usize) -> Option<usize> {
    if index == text.chars().count() {
        return Some(text.len());
    }

    text.char_indices().nth(index).map(|(offset, _)| offset)
}

fn write_owned_utf8(
    bytes: &[u8],
    result_data: *mut *const u8,
    result_length: *mut usize,
    result_owner: *mut *mut u8,
) {
    let header = size_of::<AtomicUsize>();

    let total = header
        .checked_add(bytes.len())
        .unwrap_or_else(|| panic_any(NativeMemoryAllocationFailure));

    let owner = crate::allocation::allocate(total, align_of::<AtomicUsize>());

    #[expect(
        unsafe_code,
        reason = "private native ABI operations initialize compiler-owned string storage"
    )]
    unsafe {
        owner.cast::<AtomicUsize>().write(AtomicUsize::new(1));

        let data = owner.add(header);

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());

        result_data.write(data);
        result_length.write(bytes.len());
        result_owner.write(owner);
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};
    use std::panic::catch_unwind;
    use std::ptr;
    use std::sync::atomic::AtomicUsize;

    use super::{
        bray_runtime_string_equals_v1, bray_runtime_string_from_utf8_v1,
        bray_runtime_string_scalar_at_v1, bray_runtime_string_scalar_count_v1,
        bray_runtime_string_scalar_slice_v1,
    };

    #[test]
    fn utf8_operations_use_scalar_indices_and_preserve_bytes() {
        let text = "Aé🙂";

        assert_eq!(
            bray_runtime_string_scalar_count_v1(text.as_ptr(), text.len()),
            3
        );

        assert_eq!(
            bray_runtime_string_equals_v1(text.as_ptr(), text.len(), "Aé🙂".as_ptr(), "Aé🙂".len()),
            1
        );

        assert_eq!(
            bray_runtime_string_equals_v1(text.as_ptr(), text.len(), b"other".as_ptr(), 5),
            0
        );

        let mut scalar = 0;

        assert_eq!(
            bray_runtime_string_scalar_at_v1(text.as_ptr(), text.len(), 1, &raw mut scalar),
            1
        );

        assert_eq!(scalar, u32::from('é'));

        assert_eq!(
            bray_runtime_string_scalar_at_v1(text.as_ptr(), text.len(), 3, &raw mut scalar),
            0
        );
    }

    #[test]
    fn scalar_slicing_returns_owned_valid_utf8() {
        let text = "Aé🙂Z";
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        bray_runtime_string_scalar_slice_v1(
            text.as_ptr(),
            text.len(),
            1,
            3,
            &raw mut data,
            &raw mut length,
            &raw mut owner,
        );

        #[expect(
            unsafe_code,
            reason = "the test verifies bytes returned through the private native ABI"
        )]
        let result = unsafe { std::slice::from_raw_parts(data, length) };

        assert_eq!(result, "é🙂".as_bytes());

        release_owned_text(owner, length);

        assert!(
            catch_unwind(|| {
                let mut data = ptr::null();
                let mut length = 0;
                let mut owner = ptr::null_mut();

                bray_runtime_string_scalar_slice_v1(
                    text.as_ptr(),
                    text.len(),
                    3,
                    2,
                    &raw mut data,
                    &raw mut length,
                    &raw mut owner,
                );
            })
            .is_err()
        );
    }

    #[test]
    fn utf8_conversion_rejects_invalid_bytes_without_allocating() {
        let valid = "Grüße".as_bytes();
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        assert_eq!(
            bray_runtime_string_from_utf8_v1(
                valid.as_ptr(),
                valid.len(),
                &raw mut data,
                &raw mut length,
                &raw mut owner,
            ),
            1
        );

        #[expect(
            unsafe_code,
            reason = "the test verifies bytes returned through the private native ABI"
        )]
        let result = unsafe { std::slice::from_raw_parts(data, length) };

        assert_eq!(result, valid);

        release_owned_text(owner, length);

        let invalid = [0xf0, 0x28, 0x8c, 0x28];
        let mut data = ptr::null();
        let mut length = 0;
        let mut owner = ptr::null_mut();

        assert_eq!(
            bray_runtime_string_from_utf8_v1(
                invalid.as_ptr(),
                invalid.len(),
                &raw mut data,
                &raw mut length,
                &raw mut owner,
            ),
            0
        );

        assert!(data.is_null());
        assert_eq!(length, 0);
        assert!(owner.is_null());
    }

    fn release_owned_text(owner: *mut u8, length: usize) {
        crate::allocation::deallocate(
            owner,
            size_of::<AtomicUsize>() + length,
            align_of::<AtomicUsize>(),
        );
    }
}
