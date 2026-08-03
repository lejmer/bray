use std::collections::BTreeSet;
use std::env;
use std::io::{self, Read, Write};
use std::sync::{Condvar, Mutex, OnceLock};

use bray_runtime_interface::NativePlatformStatus;

const CONTEXT_HEADER_BYTES: usize = 96;
const STANDARD_INPUT_HANDLE: u64 = 1;
const STANDARD_OUTPUT_HANDLE: u64 = 2;
const STANDARD_ERROR_HANDLE: u64 = 3;

macro_rules! native_platform_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the native platform provider requires a stable exported ABI and checked raw buffer access"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_measure_v1(
        required: *mut u64,
    ) -> NativePlatformStatus {
        if required.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let Some(length) = u64::try_from(process_context().len()).ok() else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { required.write(length) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_copy_v1(
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
    ) -> NativePlatformStatus {
        if written_or_required.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let context = process_context();
        let Some(required) = u64::try_from(context.len()).ok() else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { written_or_required.write(required) };

        if capacity < required {
            return NativePlatformStatus::INSUFFICIENT_BUFFER;
        }

        if required != 0 && destination.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe { std::ptr::copy_nonoverlapping(context.as_ptr(), destination, context.len()) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_read_v1(
        handle: u64,
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = transfer_arguments(destination, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if handle != STANDARD_INPUT_HANDLE {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let initialized = publish_transfer(transferred, 0);

        if initialized != NativePlatformStatus::SUCCESS || length == 0 {
            return initialized;
        }

        let destination = unsafe { std::slice::from_raw_parts_mut(destination, length) };

        match io::stdin().lock().read(destination) {
            Ok(count) => publish_transfer(transferred, count),
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_write_v1(
        handle: u64,
        source: *const u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = transfer_arguments(source, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let initialized = publish_transfer(transferred, 0);

        if initialized != NativePlatformStatus::SUCCESS || length == 0 {
            return initialized;
        }

        let source = unsafe { std::slice::from_raw_parts(source, length) };

        let result = match handle {
            STANDARD_OUTPUT_HANDLE => io::stdout().lock().write(source),
            STANDARD_ERROR_HANDLE => io::stderr().lock().write(source),
            _ => return NativePlatformStatus::INVALID_INPUT,
        };

        match result {
            Ok(count) => publish_transfer(transferred, count),
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_flush_v1(handle: u64) -> NativePlatformStatus {
        let result = match handle {
            STANDARD_OUTPUT_HANDLE => io::stdout().lock().flush(),
            STANDARD_ERROR_HANDLE => io::stderr().lock().flush(),
            _ => return NativePlatformStatus::INVALID_INPUT,
        };

        match result {
            Ok(()) => NativePlatformStatus::SUCCESS,
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_lock_v1(handle: u64) -> NativePlatformStatus {
        lock_stream(handle)
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_unlock_v1(handle: u64) -> NativePlatformStatus {
        unlock_stream(handle)
    }
}

fn transfer_arguments<T>(pointer: *const T, length: u64, transferred: *mut u64) -> Option<usize> {
    if transferred.is_null() {
        return None;
    }

    let length = usize::try_from(length).ok()?;

    if length != 0 && pointer.is_null() {
        return None;
    }

    Some(length)
}

#[expect(
    unsafe_code,
    reason = "the checked native transfer boundary publishes the initialized byte count"
)]
fn publish_transfer(transferred: *mut u64, count: usize) -> NativePlatformStatus {
    let Some(count) = u64::try_from(count).ok() else {
        return NativePlatformStatus::EXHAUSTED;
    };

    unsafe { transferred.write(count) };

    NativePlatformStatus::SUCCESS
}

fn platform_io_error(error: &io::Error) -> NativePlatformStatus {
    let category = match error.kind() {
        io::ErrorKind::Unsupported => 1,
        io::ErrorKind::PermissionDenied => 2,
        io::ErrorKind::NotFound => 3,
        io::ErrorKind::AlreadyExists => 4,
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => 5,
        io::ErrorKind::Interrupted => 6,
        io::ErrorKind::OutOfMemory => 7,
        io::ErrorKind::BrokenPipe | io::ErrorKind::UnexpectedEof => 8,
        io::ErrorKind::TimedOut => 9,
        _ => 12,
    };

    NativePlatformStatus::new(category, error.raw_os_error().map_or(0, i64::from))
}

fn stream_locks() -> &'static (Mutex<BTreeSet<u64>>, Condvar) {
    static LOCKS: OnceLock<(Mutex<BTreeSet<u64>>, Condvar)> = OnceLock::new();

    LOCKS.get_or_init(|| (Mutex::new(BTreeSet::new()), Condvar::new()))
}

fn lock_stream(handle: u64) -> NativePlatformStatus {
    if !is_standard_stream(handle) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let (locks, available) = stream_locks();

    let Ok(mut held) = locks.lock() else {
        return NativePlatformStatus::OTHER;
    };

    while held.contains(&handle) {
        let Ok(next) = available.wait(held) else {
            return NativePlatformStatus::OTHER;
        };

        held = next;
    }

    held.insert(handle);

    NativePlatformStatus::SUCCESS
}

fn unlock_stream(handle: u64) -> NativePlatformStatus {
    if !is_standard_stream(handle) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let (locks, available) = stream_locks();

    let Ok(mut held) = locks.lock() else {
        return NativePlatformStatus::OTHER;
    };

    if !held.remove(&handle) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    available.notify_all();

    NativePlatformStatus::SUCCESS
}

const fn is_standard_stream(handle: u64) -> bool {
    matches!(
        handle,
        STANDARD_INPUT_HANDLE | STANDARD_OUTPUT_HANDLE | STANDARD_ERROR_HANDLE
    )
}

fn process_context() -> &'static [u8] {
    static CONTEXT: OnceLock<Box<[u8]>> = OnceLock::new();

    CONTEXT.get_or_init(build_process_context)
}

fn build_process_context() -> Box<[u8]> {
    let working_directory = env::current_dir()
        .ok()
        .map(|path| native_text(path.as_os_str()))
        .unwrap_or_default();

    let arguments = env::args_os()
        .map(|argument| native_text(&argument))
        .collect::<Vec<_>>();

    let environment = env::vars_os()
        .map(|(key, value)| (native_text(&key), native_text(&value)))
        .collect::<Vec<_>>();

    let argument_table = CONTEXT_HEADER_BYTES;
    let environment_table = argument_table + arguments.len() * 16;
    let payload_start = environment_table + environment.len() * 32;
    let mut block = vec![0; payload_start];

    let working_directory_range = push_payload(&mut block, &working_directory);

    let argument_ranges = arguments
        .iter()
        .map(|argument| push_payload(&mut block, argument))
        .collect::<Vec<_>>();

    let environment_ranges = environment
        .iter()
        .map(|(key, value)| {
            (
                push_payload(&mut block, key),
                push_payload(&mut block, value),
            )
        })
        .collect::<Vec<_>>();

    let block_length = length_u64(block.len());

    write_u16(&mut block, 0, 1);
    write_u16(&mut block, 2, 0);
    block[4] = native_text_width();
    block[5] = environment_comparison();
    write_u64(&mut block, 8, u64::from(std::process::id()));
    write_u64(&mut block, 16, STANDARD_INPUT_HANDLE);
    write_u64(&mut block, 24, STANDARD_OUTPUT_HANDLE);
    write_u64(&mut block, 32, STANDARD_ERROR_HANDLE);
    write_range(&mut block, 40, working_directory_range);
    write_u64(&mut block, 56, length_u64(arguments.len()));
    write_u64(&mut block, 64, length_u64(argument_table));
    write_u64(&mut block, 72, length_u64(environment.len()));
    write_u64(&mut block, 80, length_u64(environment_table));
    write_u64(&mut block, 88, block_length);

    for (index, range) in argument_ranges.into_iter().enumerate() {
        write_range(&mut block, argument_table + index * 16, range);
    }

    for (index, (key, value)) in environment_ranges.into_iter().enumerate() {
        let offset = environment_table + index * 32;

        write_range(&mut block, offset, key);
        write_range(&mut block, offset + 16, value);
    }

    block.into_boxed_slice()
}

fn push_payload(block: &mut Vec<u8>, payload: &[u8]) -> (u64, u64) {
    let offset = length_u64(block.len());

    block.extend_from_slice(payload);

    (offset, length_u64(payload.len()))
}

fn write_range(block: &mut [u8], offset: usize, range: (u64, u64)) {
    write_u64(block, offset, range.0);
    write_u64(block, offset + 8, range.1);
}

fn write_u16(block: &mut [u8], offset: usize, value: u16) {
    block[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(block: &mut [u8], offset: usize, value: u64) {
    block[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn length_u64(value: usize) -> u64 {
    match u64::try_from(value) {
        Ok(value) => value,
        Err(_) => u64::MAX,
    }
}

#[cfg(windows)]
fn native_text(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(not(windows))]
fn native_text(value: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes().to_vec()
}

const fn native_text_width() -> u8 {
    if cfg!(windows) { 16 } else { 8 }
}

const fn environment_comparison() -> u8 {
    if cfg!(windows) { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use bray_runtime_interface::NativePlatformStatus;

    use super::{
        CONTEXT_HEADER_BYTES, STANDARD_OUTPUT_HANDLE, lock_stream, process_context, unlock_stream,
    };

    #[test]
    fn native_platform_status_has_the_specified_layout() {
        assert_eq!(size_of::<NativePlatformStatus>(), 16);
    }

    #[test]
    fn process_context_publishes_the_complete_header() {
        let context = process_context();
        let mut total = [0; 8];

        total.copy_from_slice(&context[88..96]);

        assert!(context.len() >= CONTEXT_HEADER_BYTES);
        assert_eq!(u64::from_le_bytes(total), context.len() as u64);
        assert_eq!(context[16], 1);
        assert_eq!(context[24], 2);
        assert_eq!(context[32], 3);
    }

    #[test]
    fn stream_lock_serializes_owners_until_release() {
        assert_eq!(
            lock_stream(STANDARD_OUTPUT_HANDLE),
            NativePlatformStatus::SUCCESS
        );

        let (started, wait_started) = mpsc::channel();

        let (acquired, wait_acquired) = mpsc::channel();

        let waiter = thread::spawn(move || {
            started
                .send(())
                .unwrap_or_else(|_| panic!("test start receiver must remain"));

            let status = lock_stream(STANDARD_OUTPUT_HANDLE);

            acquired
                .send(status)
                .unwrap_or_else(|_| panic!("test result receiver must remain"));

            let _ = unlock_stream(STANDARD_OUTPUT_HANDLE);
        });

        wait_started
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|_| panic!("waiter must start"));

        assert!(
            wait_acquired
                .recv_timeout(Duration::from_millis(30))
                .is_err()
        );

        assert_eq!(
            unlock_stream(STANDARD_OUTPUT_HANDLE),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            wait_acquired
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| panic!("waiter must acquire after release")),
            NativePlatformStatus::SUCCESS
        );

        waiter
            .join()
            .unwrap_or_else(|_| panic!("waiter must finish"));
    }
}
