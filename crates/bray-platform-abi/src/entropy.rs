use bray_platform::SystemEntropy;
use bray_platform_abi_support::{
    destination_slice, native_platform_export, publish_transfer_count, validate_transfer,
};
use bray_runtime_abi::NativePlatformStatus;

native_platform_export! {
    pub extern "C" fn bray_platform_entropy_fill(
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = validate_transfer(destination, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let initialized = unsafe { publish_transfer_count(transferred, 0) };

        if initialized != NativePlatformStatus::SUCCESS {
            return initialized;
        }

        let destination = unsafe { destination_slice(destination, length) };

        if fill_system_entropy(destination).is_err() {
            return NativePlatformStatus::OTHER;
        }

        unsafe { publish_transfer_count(transferred, length) }
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
