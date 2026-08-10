use std::time::Duration;

use bray_platform::{MonotonicClock, WallClock};
use bray_platform_abi_support::native_platform_export;
use bray_runtime_abi::NativePlatformStatus;

use super::region::{MemoryRegion, disjoint};

native_platform_export! {
    pub extern "C" fn bray_platform_clock_monotonic_now(
        ticks: *mut u64,
        frequency: *mut u64,
        clock_identity: *mut u64,
    ) -> NativePlatformStatus {
        let Some(ticks_region) = MemoryRegion::write(ticks) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(frequency_region) = MemoryRegion::write(frequency) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(identity_region) = MemoryRegion::write(clock_identity) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[ticks_region, frequency_region, identity_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let Some(reading) = MonotonicClock.reading() else {
            return NativePlatformStatus::OTHER;
        };

        unsafe {
            ticks.write(reading.ticks());
            frequency.write(reading.frequency());
            clock_identity.write(reading.clock_identity());
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_clock_wall_now(
        seconds: *mut i64,
        nanoseconds: *mut u32,
    ) -> NativePlatformStatus {
        let Some(seconds_region) = MemoryRegion::write(seconds) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(nanoseconds_region) = MemoryRegion::write(nanoseconds) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if seconds_region.overlaps(nanoseconds_region) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let Some(timestamp) = WallClock.now() else {
            return NativePlatformStatus::OTHER;
        };

        unsafe {
            seconds.write(timestamp.seconds());
            nanoseconds.write(timestamp.nanoseconds());
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_clock_sleep(
        seconds: u64,
        nanoseconds: u32,
    ) -> NativePlatformStatus {
        if nanoseconds >= 1_000_000_000 {
            return NativePlatformStatus::INVALID_INPUT;
        }

        MonotonicClock.sleep(Duration::new(seconds, nanoseconds));

        NativePlatformStatus::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::NativePlatformStatus;

    use super::{
        bray_platform_clock_monotonic_now, bray_platform_clock_sleep, bray_platform_clock_wall_now,
    };

    #[test]
    fn native_clock_boundaries_publish_valid_readings() {
        let mut ticks = 0;
        let mut frequency = 0;
        let mut identity = 0;

        let monotonic = bray_platform_clock_monotonic_now(
            &raw mut ticks,
            &raw mut frequency,
            &raw mut identity,
        );

        assert_eq!(monotonic, NativePlatformStatus::SUCCESS);
        assert_ne!(frequency, 0);
        assert_ne!(identity, 0);

        let mut seconds = 0;
        let mut nanoseconds = 0;
        let wall = bray_platform_clock_wall_now(&raw mut seconds, &raw mut nanoseconds);

        assert_eq!(wall, NativePlatformStatus::SUCCESS);
        assert!(nanoseconds < 1_000_000_000);

        assert_eq!(
            bray_platform_clock_sleep(0, 0),
            NativePlatformStatus::SUCCESS
        );
    }

    #[test]
    fn native_clock_boundaries_reject_overlapping_or_invalid_outputs() {
        let mut shared = 0_u64;

        assert_eq!(
            bray_platform_clock_monotonic_now(&raw mut shared, &raw mut shared, &raw mut shared,),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(
            bray_platform_clock_sleep(0, 1_000_000_000),
            NativePlatformStatus::INVALID_INPUT
        );
    }
}
