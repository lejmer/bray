use std::time::Duration;

use bray_platform::{MonotonicClock, WallClock};
#[cfg(unix)]
use bray_platform_abi_support::platform_io_error;
use bray_platform_abi_support::{MemoryRegion, native_platform_export};
use bray_runtime_abi::NativePlatformStatus;
#[cfg(windows)]
use windows_sys::Win32::System::WindowsProgramming::QueryInterruptTimePrecise;

native_platform_export! {
    pub extern "C" fn bray_platform_clock_monotonic_now(
        ticks_out: *mut u64,
    ) -> NativePlatformStatus {
        let Some(_) = MemoryRegion::write(ticks_out) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let observation: Result<u64, NativePlatformStatus> = (|| {
            #[cfg(windows)]
            {
                let mut ticks = 0;

                unsafe {
                    QueryInterruptTimePrecise(&raw mut ticks);
                }

                ticks.checked_mul(100).ok_or(NativePlatformStatus::OTHER)
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
                    return Err(platform_io_error(&std::io::Error::last_os_error()));
                }

                let Ok(seconds) = u64::try_from(observed.tv_sec) else {
                    return Err(NativePlatformStatus::OTHER);
                };

                let Ok(nanoseconds) = u64::try_from(observed.tv_nsec) else {
                    return Err(NativePlatformStatus::OTHER);
                };

                if nanoseconds >= 1_000_000_000 {
                    return Err(NativePlatformStatus::OTHER);
                }

                seconds
                    .checked_mul(1_000_000_000)
                    .and_then(|whole| whole.checked_add(nanoseconds))
                    .ok_or(NativePlatformStatus::OTHER)
            }

            #[cfg(not(any(unix, windows)))]
            {
                MonotonicClock.ticks().ok_or(NativePlatformStatus::OTHER)
            }
        })();

        let ticks = match observation {
            Ok(ticks) => ticks,
            Err(status) => return status,
        };

        unsafe {
            ticks_out.write(ticks);
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
        let mut first = 0;
        let mut second = 0;

        assert_eq!(
            bray_platform_clock_monotonic_now(&raw mut first),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            bray_platform_clock_monotonic_now(&raw mut second),
            NativePlatformStatus::SUCCESS
        );

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
            bray_platform_clock_monotonic_now(std::ptr::null_mut()),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(
            bray_platform_clock_sleep(0, 1_000_000_000),
            NativePlatformStatus::INVALID_INPUT
        );
    }
}
