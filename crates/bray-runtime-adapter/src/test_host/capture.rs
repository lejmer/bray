use std::io::{self, Write};
use std::sync::{Condvar, Mutex};
use std::thread::ThreadId;

use bray_platform::{
    RunOutputStream, flush_current_run_output, write_current_run_output,
};
use bray_platform_abi_support::{
    platform_io_error, publish_transfer_count, source_slice, validate_transfer,
};
use bray_runtime_abi::NativePlatformStatus;

macro_rules! captured_standard_stream {
    ($write:ident, $flush:ident, $stream:expr) => {
        native_adapter! {
            pub extern "C" fn $write(
                source: *const u8,
                length: u64,
                transferred: *mut u64,
            ) -> NativePlatformStatus {
                write_standard_stream($stream, source, length, transferred)
            }
        }

        native_adapter! {
            pub extern "C" fn $flush() -> NativePlatformStatus {
                flush_standard_stream($stream)
            }
        }
    };
}

fn flush_standard_stream(stream: RunOutputStream) -> NativePlatformStatus {
    if flush_current_run_output(stream).is_some() {
        return NativePlatformStatus::SUCCESS;
    }

    let result = match stream {
        RunOutputStream::StandardOutput => io::stdout().lock().flush(),
        RunOutputStream::StandardError => io::stderr().lock().flush(),
    };

    match inherited_io_result(result) {
        Ok(()) => NativePlatformStatus::SUCCESS,
        Err(status) => status,
    }
}

fn inherited_io_result<T>(result: io::Result<T>) -> Result<T, NativePlatformStatus> {
    result.map_err(|error| platform_io_error(&error))
}

native_adapter! {
    pub extern "C" fn bray_platform_standard_input_read(
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        reject_standard_input(destination, length, transferred)
    }
}

#[expect(
    unsafe_code,
    reason = "the validated test-host input boundary initializes caller-owned count storage"
)]
fn reject_standard_input(
    destination: *mut u8,
    length: u64,
    transferred: *mut u64,
) -> NativePlatformStatus {
    let Some(_) = validate_transfer(destination, length, transferred) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    unsafe { publish_transfer_count(transferred, 0) };

    NativePlatformStatus::UNSUPPORTED
}

captured_standard_stream!(
    bray_platform_standard_output_write,
    bray_platform_standard_output_flush,
    RunOutputStream::StandardOutput
);

captured_standard_stream!(
    bray_platform_standard_error_write,
    bray_platform_standard_error_flush,
    RunOutputStream::StandardError
);

macro_rules! captured_standard_stream_lock {
    ($lock:ident, $unlock:ident, $state:ident) => {
        native_adapter! {
            pub extern "C" fn $lock() -> NativePlatformStatus {
                $state.lock()
            }
        }

        native_adapter! {
            pub extern "C" fn $unlock() -> NativePlatformStatus {
                $state.unlock()
            }
        }
    };
}

struct CapturedStandardStreamLock {
    owner: Mutex<Option<ThreadId>>,
    available: Condvar,
}

impl CapturedStandardStreamLock {
    const fn new() -> Self {
        Self {
            owner: Mutex::new(None),
            available: Condvar::new(),
        }
    }

    fn lock(&self) -> NativePlatformStatus {
        let owner = std::thread::current().id();

        let Ok(mut held_by) = self.owner.lock() else {
            return NativePlatformStatus::OTHER;
        };

        while let Some(current) = *held_by {
            if current == owner {
                return NativePlatformStatus::INVALID_INPUT;
            }

            let Ok(next) = self.available.wait(held_by) else {
                return NativePlatformStatus::OTHER;
            };

            held_by = next;
        }

        *held_by = Some(owner);

        NativePlatformStatus::SUCCESS
    }

    fn unlock(&self) -> NativePlatformStatus {
        let owner = std::thread::current().id();

        let Ok(mut held_by) = self.owner.lock() else {
            return NativePlatformStatus::OTHER;
        };

        if held_by.as_ref() != Some(&owner) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        *held_by = None;
        self.available.notify_one();

        NativePlatformStatus::SUCCESS
    }
}

static STANDARD_OUTPUT_LOCK: CapturedStandardStreamLock = CapturedStandardStreamLock::new();
static STANDARD_ERROR_LOCK: CapturedStandardStreamLock = CapturedStandardStreamLock::new();

captured_standard_stream_lock!(
    bray_platform_standard_output_lock,
    bray_platform_standard_output_unlock,
    STANDARD_OUTPUT_LOCK
);

captured_standard_stream_lock!(
    bray_platform_standard_error_lock,
    bray_platform_standard_error_unlock,
    STANDARD_ERROR_LOCK
);

#[expect(
    unsafe_code,
    reason = "the validated test-host transfer boundary borrows caller-owned output bytes"
)]
fn write_standard_stream(
    stream: RunOutputStream,
    source: *const u8,
    length: u64,
    transferred: *mut u64,
) -> NativePlatformStatus {
    let Some(length) = validate_transfer(source, length, transferred) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    unsafe { publish_transfer_count(transferred, 0) };

    let source = unsafe { source_slice(source, length) };

    if write_current_run_output(stream, source).is_none() {
        let result = match stream {
            RunOutputStream::StandardOutput => io::stdout().lock().write(source),
            RunOutputStream::StandardError => io::stderr().lock().write(source),
        };

        let written = match inherited_io_result(result) {
            Ok(written) => written,
            Err(status) => return status,
        };

        return unsafe { publish_transfer_count(transferred, written) };
    }

    unsafe { publish_transfer_count(transferred, length) }
}

#[cfg(test)]
mod tests {
    use std::io;

    use bray_platform::{RunOutputContext, RunOutputStream, with_run_output_context};
    use bray_runtime_abi::NativePlatformStatus;

    use super::{
        bray_platform_standard_input_read,
        bray_platform_standard_output_lock, bray_platform_standard_output_unlock,
        bray_platform_standard_output_write,
    };

    #[test]
    fn test_host_standard_input_is_reserved_for_the_control_protocol() {
        let mut destination = [0_u8; 1];
        let mut transferred = 9;

        assert_eq!(
            bray_platform_standard_input_read(
                destination.as_mut_ptr(),
                1,
                &raw mut transferred,
            ),
            NativePlatformStatus::UNSUPPORTED
        );

        assert_eq!(transferred, 0);
    }

    #[test]
    fn inherited_stream_errors_preserve_portable_and_native_details() {
        let broken = super::inherited_io_result::<()>(Err(io::Error::from(
            io::ErrorKind::BrokenPipe,
        )))
        .expect_err("broken stream must fail");

        let native = super::inherited_io_result::<()>(Err(io::Error::from_raw_os_error(12_345)))
            .expect_err("native error must fail");

        assert_eq!(broken, NativePlatformStatus::BROKEN_STREAM);
        assert_eq!(native.native_code(), 12_345);
    }

    #[test]
    fn selected_capture_adapter_routes_output_through_bounded_run_context() {
        let output = RunOutputContext::captured(3, 3);
        let mut transferred = 0;

        assert_eq!(
            bray_platform_standard_output_write(
                std::ptr::null(),
                0,
                &raw mut transferred,
            ),
            NativePlatformStatus::SUCCESS
        );

        let status = with_run_output_context(output.clone(), || {
            bray_platform_standard_output_write(b"abcde".as_ptr(), 5, &raw mut transferred)
        });

        let captured = output
            .captured_stream(RunOutputStream::StandardOutput)
            .unwrap_or_else(|| panic!("selected capture must expose standard output"));

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, 5);
        assert_eq!(captured.bytes(), b"abc");
        assert_eq!(captured.discarded_byte_count(), 2);
    }

    #[test]
    fn selected_capture_adapter_enforces_standard_stream_ownership() {
        assert_eq!(
            bray_platform_standard_output_lock(),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            bray_platform_standard_output_lock(),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(
            bray_platform_standard_output_unlock(),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            bray_platform_standard_output_unlock(),
            NativePlatformStatus::INVALID_INPUT
        );
    }
}
