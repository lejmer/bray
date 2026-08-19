use std::time::Duration;

use bray_platform::{MonotonicClock, WallClock};
use bray_platform_abi_support::{MemoryRegion, native_platform_export};
use bray_runtime_abi::{NATIVE_MONOTONIC_TICKS_UNAVAILABLE, NativePlatformStatus};
#[cfg(windows)]
use windows_sys::Win32::System::WindowsProgramming::QueryInterruptTimePrecise;

native_platform_export! {
    pub extern "C" fn bray_platform_clock_monotonic_now() -> u64 {
        #[cfg(windows)]
        {
            let mut ticks = 0;

            unsafe {
                QueryInterruptTimePrecise(&raw mut ticks);
            }

            ticks
                .checked_mul(100)
                .unwrap_or(NATIVE_MONOTONIC_TICKS_UNAVAILABLE)
        }

        #[cfg(unix)]
        {
            let mut observed = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };

            let status = unsafe {
                libc::clock_gettime(libc::CLOCK_MONOTONIC, &raw mut observed)
            };

            if status != 0 {
                return NATIVE_MONOTONIC_TICKS_UNAVAILABLE;
            }

            let Ok(seconds) = u64::try_from(observed.tv_sec) else {
                return NATIVE_MONOTONIC_TICKS_UNAVAILABLE;
            };

            let Ok(nanoseconds) = u64::try_from(observed.tv_nsec) else {
                return NATIVE_MONOTONIC_TICKS_UNAVAILABLE;
            };

            if nanoseconds >= 1_000_000_000 {
                return NATIVE_MONOTONIC_TICKS_UNAVAILABLE;
            }

            seconds
                .checked_mul(1_000_000_000)
                .and_then(|whole| whole.checked_add(nanoseconds))
                .filter(|ticks| *ticks != NATIVE_MONOTONIC_TICKS_UNAVAILABLE)
                .unwrap_or(NATIVE_MONOTONIC_TICKS_UNAVAILABLE)
        }

        #[cfg(not(any(unix, windows)))]
        {
            MonotonicClock
                .ticks()
                .unwrap_or(NATIVE_MONOTONIC_TICKS_UNAVAILABLE)
        }
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
    use bray_runtime_abi::{NATIVE_MONOTONIC_TICKS_UNAVAILABLE, NativePlatformStatus};

    use super::{
        bray_platform_clock_monotonic_now, bray_platform_clock_sleep, bray_platform_clock_wall_now,
    };

    #[test]
    fn native_clock_boundaries_publish_valid_readings() {
        let first = bray_platform_clock_monotonic_now();
        let second = bray_platform_clock_monotonic_now();

        assert_ne!(first, NATIVE_MONOTONIC_TICKS_UNAVAILABLE);
        assert!(second >= first);

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
    fn native_clock_boundaries_reject_invalid_sleep_durations() {
        assert_eq!(
            bray_platform_clock_sleep(0, 1_000_000_000),
            NativePlatformStatus::INVALID_INPUT
        );
    }
}
