use bray_platform::SystemEntropy;
use bray_platform_abi_support::native_platform_export;
use bray_runtime_abi::NativePlatformStatus;

use super::region::{MemoryRegion, disjoint};

native_platform_export! {
    pub extern "C" fn bray_platform_entropy_fill(
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Ok(length) = usize::try_from(length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(destination_region) = MemoryRegion::read(destination, length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(transferred_region) = MemoryRegion::write(transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[destination_region, transferred_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe { transferred.write(0) };

        let destination = if length == 0 {
            &mut []
        } else {
            unsafe { std::slice::from_raw_parts_mut(destination, length) }
        };

        if fill_system_entropy(destination).is_err() {
            return NativePlatformStatus::OTHER;
        }

        let Ok(transferred_count) = u64::try_from(length) else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { transferred.write(transferred_count) };

        NativePlatformStatus::SUCCESS
    }
}

fn fill_system_entropy(destination: &mut [u8]) -> Result<(), ()> {
    SystemEntropy.fill(destination).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::NativePlatformStatus;

    use super::bray_platform_entropy_fill;

    #[test]
    fn native_entropy_initializes_the_complete_destination() {
        let mut bytes = [0_u8; 32];
        let mut transferred = u64::MAX;

        let status = bray_platform_entropy_fill(
            bytes.as_mut_ptr(),
            bytes.len() as u64,
            &raw mut transferred,
        );

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, bytes.len() as u64);
    }

    #[test]
    fn native_entropy_accepts_an_empty_null_destination() {
        let mut transferred = u64::MAX;

        let status = bray_platform_entropy_fill(std::ptr::null_mut(), 0, &raw mut transferred);

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, 0);
    }
}
