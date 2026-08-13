use super::{MemoryRegion, disjoint};
use bray_runtime_abi::NativePlatformStatus;

/// Validates one native byte-transfer region and its disjoint count output.
pub fn validate_transfer<T>(
    pointer: *const T,
    length: u64,
    transferred: *mut u64,
) -> Option<usize> {
    let length = usize::try_from(length).ok()?;
    let data = MemoryRegion::read(pointer, length)?;
    let transferred = MemoryRegion::write(transferred)?;

    disjoint(&[data, transferred]).then_some(length)
}

/// Borrows a source byte region after [`validate_transfer`] accepted it.
///
/// # Safety
///
/// `pointer` and `length` must describe the readable region accepted by the same transfer
/// boundary's validation, and the returned borrow must not outlive that region.
#[expect(
    unsafe_code,
    reason = "checked native transfer boundaries borrow caller-owned source bytes"
)]
pub unsafe fn source_slice<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
    if length == 0 {
        return &[];
    }

    unsafe { std::slice::from_raw_parts(pointer, length) }
}

/// Borrows a destination byte region after [`validate_transfer`] accepted it.
///
/// # Safety
///
/// `pointer` and `length` must describe the writable region accepted by the same transfer
/// boundary's validation, and the returned borrow must not outlive that region.
#[expect(
    unsafe_code,
    reason = "checked native transfer boundaries borrow caller-owned destination bytes"
)]
pub unsafe fn destination_slice<'a>(pointer: *mut u8, length: usize) -> &'a mut [u8] {
    if length == 0 {
        return &mut [];
    }

    unsafe { std::slice::from_raw_parts_mut(pointer, length) }
}

/// Publishes a transfer count after [`validate_transfer`] accepted the count storage.
///
/// # Safety
///
/// `transferred` must be the writable count storage accepted by the same transfer boundary's
/// validation.
#[expect(
    unsafe_code,
    reason = "checked native transfer boundaries publish initialized byte counts"
)]
pub unsafe fn publish_transfer_count(
    transferred: *mut u64,
    count: usize,
) -> NativePlatformStatus {
    let Ok(count) = u64::try_from(count) else {
        return NativePlatformStatus::EXHAUSTED;
    };

    unsafe { transferred.write(count) };

    NativePlatformStatus::SUCCESS
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::NativePlatformStatus;

    use super::{destination_slice, publish_transfer_count, source_slice, validate_transfer};

    #[test]
    fn transfers_require_disjoint_aligned_count_storage() {
        let mut bytes = [0_u64; 2];

        assert_eq!(
            validate_transfer(bytes.as_ptr(), 1, &raw mut bytes[1]),
            Some(1)
        );

        assert!(validate_transfer(bytes.as_ptr(), 1, &raw mut bytes[0]).is_none());
        assert!(validate_transfer(std::ptr::null::<u8>(), 0, &raw mut bytes[0]).is_some());
    }

    #[test]
    #[expect(
        unsafe_code,
        reason = "the test exercises the documented post-validation transfer boundary"
    )]
    fn validated_transfers_borrow_zero_length_regions_and_publish_counts() {
        let mut transferred = 0;

        let source = unsafe { source_slice(std::ptr::null(), 0) };
        let destination = unsafe { destination_slice(std::ptr::null_mut(), 0) };
        let status = unsafe { publish_transfer_count(&raw mut transferred, 7) };

        assert!(source.is_empty());
        assert!(destination.is_empty());
        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, 7);
    }
}
