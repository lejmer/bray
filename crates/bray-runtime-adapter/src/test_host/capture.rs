use std::cell::RefCell;
use std::io::{self, Write};
use std::sync::OnceLock;

use bray_platform::{
    RunOutputOperation, RunOutputOperationError, RunOutputStream,
    begin_current_run_output_operation,
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
    with_standard_stream_operation(stream, StandardStreamOperation::flush)
        .unwrap_or(NativePlatformStatus::INVALID_INPUT)
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

native_adapter! {
    pub extern "C" fn bray_platform_standard_input_lock() -> NativePlatformStatus {
        STANDARD_INPUT_OPERATION.with(|operation| {
            let mut operation = operation.borrow_mut();

            if operation.is_some() {
                return NativePlatformStatus::INVALID_INPUT;
            }

            *operation = Some(INHERITED_STANDARD_INPUT.get_or_init(io::stdin).lock());

            NativePlatformStatus::SUCCESS
        })
    }
}

native_adapter! {
    pub extern "C" fn bray_platform_standard_input_unlock() -> NativePlatformStatus {
        STANDARD_INPUT_OPERATION.with(|operation| {
            operation
                .borrow_mut()
                .take()
                .map(|_| NativePlatformStatus::SUCCESS)
                .unwrap_or(NativePlatformStatus::INVALID_INPUT)
        })
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

    if !STANDARD_INPUT_OPERATION.with(|operation| operation.borrow().is_some()) {
        return NativePlatformStatus::INVALID_INPUT;
    }

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
    ($lock:ident, $unlock:ident, $stream:expr) => {
        native_adapter! {
            pub extern "C" fn $lock() -> NativePlatformStatus {
                begin_standard_stream_operation($stream)
            }
        }

        native_adapter! {
            pub extern "C" fn $unlock() -> NativePlatformStatus {
                end_standard_stream_operation($stream)
            }
        }
    };
}

enum StandardStreamOperation {
    InheritedOutput(io::StdoutLock<'static>),
    InheritedError(io::StderrLock<'static>),
    Redirected(RunOutputOperation),
}

impl StandardStreamOperation {
    const fn stream(&self) -> RunOutputStream {
        match self {
            Self::InheritedOutput(_) => RunOutputStream::StandardOutput,
            Self::InheritedError(_) => RunOutputStream::StandardError,
            Self::Redirected(operation) => operation.stream(),
        }
    }

    fn write(&mut self, source: &[u8]) -> Result<usize, NativePlatformStatus> {
        match self {
            Self::InheritedOutput(output) => inherited_io_result(output.write(source)),
            Self::InheritedError(error) => inherited_io_result(error.write(source)),
            Self::Redirected(operation) => Ok(operation.write(source)),
        }
    }

    fn flush(&mut self) -> NativePlatformStatus {
        let result = match self {
            Self::InheritedOutput(output) => output.flush(),
            Self::InheritedError(error) => error.flush(),
            Self::Redirected(operation) => {
                operation.flush();

                return NativePlatformStatus::SUCCESS;
            }
        };

        inherited_io_result(result)
            .map(|()| NativePlatformStatus::SUCCESS)
            .unwrap_or_else(|status| status)
    }
}

static INHERITED_STANDARD_OUTPUT: OnceLock<io::Stdout> = OnceLock::new();
static INHERITED_STANDARD_ERROR: OnceLock<io::Stderr> = OnceLock::new();
static INHERITED_STANDARD_INPUT: OnceLock<io::Stdin> = OnceLock::new();

thread_local! {
    static STANDARD_INPUT_OPERATION: RefCell<Option<io::StdinLock<'static>>> =
        const { RefCell::new(None) };
    static STANDARD_OUTPUT_OPERATION: RefCell<Option<StandardStreamOperation>> =
        const { RefCell::new(None) };
    static STANDARD_ERROR_OPERATION: RefCell<Option<StandardStreamOperation>> =
        const { RefCell::new(None) };
}

fn with_operation_slot<T>(
    stream: RunOutputStream,
    callback: impl FnOnce(&RefCell<Option<StandardStreamOperation>>) -> T,
) -> T {
    match stream {
        RunOutputStream::StandardOutput => STANDARD_OUTPUT_OPERATION.with(callback),
        RunOutputStream::StandardError => STANDARD_ERROR_OPERATION.with(callback),
    }
}

fn begin_standard_stream_operation(stream: RunOutputStream) -> NativePlatformStatus {
    with_operation_slot(stream, |operation| {
        let mut operation = operation.borrow_mut();

        if operation.is_some() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        match begin_current_run_output_operation(stream) {
            Ok(Some(redirected)) => {
                *operation = Some(StandardStreamOperation::Redirected(redirected));

                return NativePlatformStatus::SUCCESS;
            }
            Ok(None) => {}
            Err(RunOutputOperationError::Reentrant) => {
                return NativePlatformStatus::INVALID_INPUT;
            }
        }

        *operation = Some(match stream {
            RunOutputStream::StandardOutput => StandardStreamOperation::InheritedOutput(
                INHERITED_STANDARD_OUTPUT.get_or_init(io::stdout).lock(),
            ),
            RunOutputStream::StandardError => StandardStreamOperation::InheritedError(
                INHERITED_STANDARD_ERROR.get_or_init(io::stderr).lock(),
            ),
        });

        NativePlatformStatus::SUCCESS
    })
}

fn end_standard_stream_operation(stream: RunOutputStream) -> NativePlatformStatus {
    with_operation_slot(stream, |operation| {
        let Some(operation) = operation.borrow_mut().take() else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        match operation {
            StandardStreamOperation::Redirected(redirected) if redirected.stream() == stream => {
                NativePlatformStatus::SUCCESS
            }
            StandardStreamOperation::InheritedOutput(_)
                if stream == RunOutputStream::StandardOutput =>
            {
                NativePlatformStatus::SUCCESS
            }
            StandardStreamOperation::InheritedError(_)
                if stream == RunOutputStream::StandardError =>
            {
                NativePlatformStatus::SUCCESS
            }
            _ => NativePlatformStatus::INVALID_INPUT,
        }
    })
}

fn with_standard_stream_operation<T>(
    stream: RunOutputStream,
    callback: impl FnOnce(&mut StandardStreamOperation) -> T,
) -> Option<T> {
    with_operation_slot(stream, |operation| {
        let mut operation = operation.borrow_mut();
        let operation = operation.as_mut()?;

        (operation.stream() == stream).then(|| callback(operation))
    })
}

captured_standard_stream_lock!(
    bray_platform_standard_output_lock,
    bray_platform_standard_output_unlock,
    RunOutputStream::StandardOutput
);

captured_standard_stream_lock!(
    bray_platform_standard_error_lock,
    bray_platform_standard_error_unlock,
    RunOutputStream::StandardError
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

    let Some(result) = with_standard_stream_operation(stream, |operation| operation.write(source))
    else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    let written = match result {
        Ok(written) => written,
        Err(status) => return status,
    };

    unsafe { publish_transfer_count(transferred, written) }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Barrier};

    use bray_platform::{RunOutputContext, RunOutputStream, with_run_output_context};
    use bray_runtime_abi::NativePlatformStatus;

    use super::{
        bray_platform_standard_input_lock, bray_platform_standard_input_read,
        bray_platform_standard_input_unlock, bray_platform_standard_output_flush,
        bray_platform_standard_output_lock, bray_platform_standard_output_unlock,
        bray_platform_standard_output_write,
    };

    #[test]
    fn test_host_standard_input_is_reserved_for_the_control_protocol() {
        let mut destination = [0_u8; 1];
        let mut transferred = 9;

        assert_eq!(
            bray_platform_standard_input_read(destination.as_mut_ptr(), 1, &raw mut transferred,),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(
            bray_platform_standard_input_lock(),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            bray_platform_standard_input_read(destination.as_mut_ptr(), 1, &raw mut transferred,),
            NativePlatformStatus::UNSUPPORTED
        );

        assert_eq!(
            bray_platform_standard_input_unlock(),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(transferred, 0);
    }

    #[test]
    fn inherited_stream_errors_preserve_portable_and_native_details() {
        let broken =
            super::inherited_io_result::<()>(Err(io::Error::from(io::ErrorKind::BrokenPipe)))
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
            bray_platform_standard_output_write(std::ptr::null(), 0, &raw mut transferred,),
            NativePlatformStatus::INVALID_INPUT
        );

        let (write, flush) = with_run_output_context(output.clone(), || {
            assert_eq!(
                bray_platform_standard_output_lock(),
                NativePlatformStatus::SUCCESS
            );

            let write =
                bray_platform_standard_output_write(b"abcde".as_ptr(), 5, &raw mut transferred);

            let flush = bray_platform_standard_output_flush();

            assert_eq!(
                bray_platform_standard_output_unlock(),
                NativePlatformStatus::SUCCESS
            );

            (write, flush)
        });

        let captured = output
            .captured_stream(RunOutputStream::StandardOutput)
            .unwrap_or_else(|| panic!("selected capture must expose standard output"));

        assert_eq!(write, NativePlatformStatus::SUCCESS);
        assert_eq!(flush, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, 5);
        assert_eq!(captured.bytes(), b"abc");
        assert_eq!(captured.discarded_byte_count(), 2);
    }

    #[test]
    fn selected_capture_adapter_enforces_standard_stream_ownership() {
        let output = RunOutputContext::discarded();

        with_run_output_context(output, || {
            let mut transferred = 9;

            assert_eq!(
                bray_platform_standard_output_flush(),
                NativePlatformStatus::INVALID_INPUT
            );

            assert_eq!(
                bray_platform_standard_output_lock(),
                NativePlatformStatus::SUCCESS
            );

            assert_eq!(
                bray_platform_standard_output_lock(),
                NativePlatformStatus::INVALID_INPUT
            );

            assert_eq!(
                bray_platform_standard_output_write(std::ptr::null(), 1, &raw mut transferred,),
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

            assert_eq!(
                bray_platform_standard_output_lock(),
                NativePlatformStatus::SUCCESS
            );

            assert_eq!(
                bray_platform_standard_output_unlock(),
                NativePlatformStatus::SUCCESS
            );
        });
    }

    #[test]
    fn inherited_output_reuses_its_operation_guard() {
        let output = RunOutputContext::inherited();
        let mut transferred = 9;

        with_run_output_context(output, || {
            assert_eq!(
                bray_platform_standard_output_lock(),
                NativePlatformStatus::SUCCESS
            );

            assert_eq!(
                bray_platform_standard_output_write(std::ptr::null(), 0, &raw mut transferred,),
                NativePlatformStatus::SUCCESS
            );

            assert_eq!(
                bray_platform_standard_output_flush(),
                NativePlatformStatus::SUCCESS
            );

            assert_eq!(
                bray_platform_standard_output_unlock(),
                NativePlatformStatus::SUCCESS
            );
        });

        assert_eq!(transferred, 0);
    }

    #[test]
    fn captured_stream_operations_are_atomic_across_threads() {
        const WRITE_COUNT: usize = 64;

        let output = RunOutputContext::captured(WRITE_COUNT * 2, WRITE_COUNT * 2);
        let barrier = Arc::new(Barrier::new(2));
        let mut threads = Vec::new();

        for byte in [b'a', b'b'] {
            let output = output.clone();
            let barrier = Arc::clone(&barrier);

            threads.push(std::thread::spawn(move || {
                with_run_output_context(output, || {
                    barrier.wait();

                    assert_eq!(
                        bray_platform_standard_output_lock(),
                        NativePlatformStatus::SUCCESS
                    );

                    for _ in 0..WRITE_COUNT {
                        let mut transferred = 0;

                        assert_eq!(
                            bray_platform_standard_output_write(
                                &raw const byte,
                                1,
                                &raw mut transferred,
                            ),
                            NativePlatformStatus::SUCCESS
                        );

                        assert_eq!(transferred, 1);
                    }

                    assert_eq!(
                        bray_platform_standard_output_unlock(),
                        NativePlatformStatus::SUCCESS
                    );
                });
            }));
        }

        for thread in threads {
            thread
                .join()
                .unwrap_or_else(|_| panic!("captured stream writer must complete"));
        }

        let captured = output
            .captured_stream(RunOutputStream::StandardOutput)
            .unwrap_or_else(|| panic!("selected capture must expose standard output"));

        let first = vec![b'a'; WRITE_COUNT]
            .into_iter()
            .chain(vec![b'b'; WRITE_COUNT])
            .collect::<Vec<_>>();

        let second = vec![b'b'; WRITE_COUNT]
            .into_iter()
            .chain(vec![b'a'; WRITE_COUNT])
            .collect::<Vec<_>>();

        assert!(captured.bytes() == first || captured.bytes() == second);
    }
}
