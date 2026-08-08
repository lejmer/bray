use std::env;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

use bray_platform::{RunOutputStream, flush_current_run_output, write_current_run_output};
use bray_runtime_interface::{NativePlatformStatus, NativePlatformText};

use super::filesystem::{close_file, flush_file, is_file_handle, read_file, seek_file, write_file};
use super::process::{
    close_process_stream, flush_process_stream, is_process_handle, is_process_stream,
    read_process_stream, write_process_stream,
};
use super::region::{MemoryRegion, disjoint};

const CONTEXT_HEADER_BYTES: usize = 96;
const STANDARD_INPUT_HANDLE: u64 = 1;
const STANDARD_OUTPUT_HANDLE: u64 = 2;
const STANDARD_ERROR_HANDLE: u64 = 3;

native_platform_export! {
    pub extern "C" fn bray_platform_context_measure(
        required: *mut u64,
    ) -> NativePlatformStatus {
        if MemoryRegion::write(required).is_none() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let context = match process_context() {
            Ok(context) => context,
            Err(status) => return status,
        };

        let Some(length) = u64::try_from(context.len()).ok() else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { required.write(length) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_copy(
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
    ) -> NativePlatformStatus {
        let Ok(capacity) = usize::try_from(capacity) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(destination_region) = MemoryRegion::read(destination, capacity) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(required_region) = MemoryRegion::write(written_or_required) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[destination_region, required_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let context = match process_context() {
            Ok(context) => context,
            Err(status) => return status,
        };

        let Some(required) = u64::try_from(context.len()).ok() else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { written_or_required.write(required) };

        if capacity < context.len() {
            return NativePlatformStatus::INSUFFICIENT_BUFFER;
        }

        unsafe { std::ptr::copy_nonoverlapping(context.as_ptr(), destination, context.len()) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_environment_key_equals(
        left: NativePlatformText,
        right: NativePlatformText,
        equal: *mut u32,
    ) -> NativePlatformStatus {
        let (left_address, left_length, left_region) = match native_text_region(left) {
            Ok(left) => left,
            Err(status) => return status,
        };

        let (right_address, right_length, right_region) = match native_text_region(right) {
            Ok(right) => right,
            Err(status) => return status,
        };

        let Some(equal_region) = MemoryRegion::write(equal) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if equal_region.overlaps(left_region) || equal_region.overlaps(right_region) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let left = if left_length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(left_address, left_length) }
        };

        let right = if right_length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(right_address, right_length) }
        };

        let equal_value = match environment_keys_equal(left, right) {
            Ok(equal) => u32::from(equal),
            Err(status) => return status,
        };

        unsafe { equal.write(equal_value) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_read(
        handle: u64,
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = transfer_arguments(destination, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let initialized = publish_transfer(transferred, 0);

        if initialized != NativePlatformStatus::SUCCESS || length == 0 {
            return initialized;
        }

        let destination = unsafe { std::slice::from_raw_parts_mut(destination, length) };

        if handle != STANDARD_INPUT_HANDLE {
            let result = if is_process_handle(handle) {
                read_process_stream(handle, destination)
            } else {
                read_file(handle, destination)
            };

            return match result {
                Ok(count) => publish_transfer(transferred, count),
                Err(status) => status,
            };
        }

        match io::stdin().lock().read(destination) {
            Ok(count) => publish_transfer(transferred, count),
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_write(
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

        let Some(stream) = run_output_stream(handle) else {
            let result = if is_process_handle(handle) {
                write_process_stream(handle, source)
            } else {
                write_file(handle, source)
            };

            return match result {
                Ok(count) => publish_transfer(transferred, count),
                Err(status) => status,
            };
        };

        if let Some(count) = write_current_run_output(stream, source) {
            return publish_transfer(transferred, count);
        }

        let result = match stream {
            RunOutputStream::StandardOutput => io::stdout().lock().write(source),
            RunOutputStream::StandardError => io::stderr().lock().write(source),
        };

        match result {
            Ok(count) => publish_transfer(transferred, count),
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_flush(handle: u64) -> NativePlatformStatus {
        let Some(stream) = run_output_stream(handle) else {
            let result = if is_process_handle(handle) {
                flush_process_stream(handle)
            } else {
                flush_file(handle)
            };

            return match result {
                Ok(()) => NativePlatformStatus::SUCCESS,
                Err(status) => status,
            };
        };

        if flush_current_run_output(stream).is_some() {
            return NativePlatformStatus::SUCCESS;
        }

        let result = match stream {
            RunOutputStream::StandardOutput => io::stdout().lock().flush(),
            RunOutputStream::StandardError => io::stderr().lock().flush(),
        };

        match result {
            Ok(()) => NativePlatformStatus::SUCCESS,
            Err(error) => platform_io_error(&error),
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_seek(
        handle: u64,
        offset_bits: u64,
        origin: u32,
        position: *mut u64,
    ) -> NativePlatformStatus {
        if MemoryRegion::write(position).is_none() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        match seek_file(handle, offset_bits, origin) {
            Ok(value) => {
                unsafe { position.write(value) };

                NativePlatformStatus::SUCCESS
            }
            Err(status) => status,
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_close(handle: u64) -> NativePlatformStatus {
        if is_process_handle(handle) {
            close_process_stream(handle)
        } else {
            close_file(handle)
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_lock(handle: u64) -> NativePlatformStatus {
        lock_stream(handle)
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_stream_unlock(handle: u64) -> NativePlatformStatus {
        unlock_stream(handle)
    }
}

fn transfer_arguments<T>(pointer: *const T, length: u64, transferred: *mut u64) -> Option<usize> {
    let length = usize::try_from(length).ok()?;
    let data = MemoryRegion::read(pointer, length)?;
    let transferred = MemoryRegion::write(transferred)?;

    disjoint(&[data, transferred]).then_some(length)
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

pub(super) fn platform_io_error(error: &io::Error) -> NativePlatformStatus {
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

#[derive(Default)]
struct StreamLockState {
    owners: Vec<(u64, ThreadId)>,
}

impl StreamLockState {
    fn owner(&self, handle: u64) -> Option<ThreadId> {
        self.owners
            .iter()
            .find_map(|(held, owner)| (*held == handle).then_some(*owner))
    }

    fn acquire(&mut self, handle: u64, owner: ThreadId) {
        self.owners.push((handle, owner));
    }

    fn release(&mut self, handle: u64, owner: ThreadId) -> bool {
        let Some(index) = self
            .owners
            .iter()
            .position(|(held, held_by)| *held == handle && *held_by == owner)
        else {
            return false;
        };

        self.owners.swap_remove(index);

        true
    }
}

fn stream_locks() -> &'static (Mutex<StreamLockState>, Condvar) {
    static LOCKS: OnceLock<(Mutex<StreamLockState>, Condvar)> = OnceLock::new();

    LOCKS.get_or_init(|| (Mutex::new(StreamLockState::default()), Condvar::new()))
}

fn lock_stream(handle: u64) -> NativePlatformStatus {
    if !is_standard_stream(handle) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let (locks, available) = stream_locks();

    let owner = std::thread::current().id();

    let Ok(mut held) = locks.lock() else {
        return NativePlatformStatus::OTHER;
    };

    while let Some(held_by) = held.owner(handle) {
        if held_by == owner {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let Ok(next) = available.wait(held) else {
            return NativePlatformStatus::OTHER;
        };

        held = next;
    }

    held.acquire(handle, owner);

    NativePlatformStatus::SUCCESS
}

fn unlock_stream(handle: u64) -> NativePlatformStatus {
    if !is_standard_stream(handle) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let (locks, available) = stream_locks();

    let owner = std::thread::current().id();

    let Ok(mut held) = locks.lock() else {
        return NativePlatformStatus::OTHER;
    };

    if !held.release(handle, owner) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    available.notify_all();

    NativePlatformStatus::SUCCESS
}

fn is_standard_stream(handle: u64) -> bool {
    matches!(
        handle,
        STANDARD_INPUT_HANDLE | STANDARD_OUTPUT_HANDLE | STANDARD_ERROR_HANDLE
    ) || is_file_handle(handle)
        || is_process_stream(handle)
}

fn process_context() -> Result<&'static [u8], NativePlatformStatus> {
    static CONTEXT: OnceLock<Result<Box<[u8]>, NativePlatformStatus>> = OnceLock::new();

    match CONTEXT.get_or_init(build_process_context) {
        Ok(context) => Ok(context),
        Err(status) => Err(*status),
    }
}

fn native_text_region(
    text: NativePlatformText,
) -> Result<(*const u8, usize, MemoryRegion), NativePlatformStatus> {
    let length = usize::try_from(text.length()).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let region =
        MemoryRegion::read(text.address(), length).ok_or(NativePlatformStatus::INVALID_INPUT)?;

    Ok((text.address(), length, region))
}

#[cfg(not(windows))]
fn environment_keys_equal(left: &[u8], right: &[u8]) -> Result<bool, NativePlatformStatus> {
    Ok(left == right)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "target-native environment comparison calls the Windows ordinal string ABI"
)]
fn environment_keys_equal(left: &[u8], right: &[u8]) -> Result<bool, NativePlatformStatus> {
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};

    let left = native_utf16(left)?;
    let right = native_utf16(right)?;
    let left_length = i32::try_from(left.len()).map_err(|_| NativePlatformStatus::EXHAUSTED)?;
    let right_length = i32::try_from(right.len()).map_err(|_| NativePlatformStatus::EXHAUSTED)?;

    let comparison = unsafe {
        CompareStringOrdinal(left.as_ptr(), left_length, right.as_ptr(), right_length, 1)
    };

    if comparison == 0 {
        return Err(NativePlatformStatus::OTHER);
    }

    Ok(comparison == CSTR_EQUAL)
}

#[cfg(windows)]
fn native_utf16(bytes: &[u8]) -> Result<Vec<u16>, NativePlatformStatus> {
    if !bytes.len().is_multiple_of(2) {
        return Err(NativePlatformStatus::INVALID_INPUT);
    }

    Ok(bytes
        .chunks_exact(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect())
}

/// Materializes the immutable process context used by platform-service calls.
pub fn initialize_process_context() -> Result<(), NativePlatformStatus> {
    process_context().map(|_| ())
}

fn build_process_context() -> Result<Box<[u8]>, NativePlatformStatus> {
    let working_directory = native_text(startup_working_directory()?.as_os_str());

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

    Ok(block.into_boxed_slice())
}

pub(super) fn startup_working_directory() -> Result<&'static Path, NativePlatformStatus> {
    static DIRECTORY: OnceLock<Result<PathBuf, NativePlatformStatus>> = OnceLock::new();

    match DIRECTORY.get_or_init(|| env::current_dir().map_err(|error| platform_io_error(&error))) {
        Ok(path) => Ok(path),
        Err(status) => Err(*status),
    }
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
    u64::try_from(value).unwrap_or(u64::MAX)
}

const fn run_output_stream(handle: u64) -> Option<RunOutputStream> {
    match handle {
        STANDARD_OUTPUT_HANDLE => Some(RunOutputStream::StandardOutput),
        STANDARD_ERROR_HANDLE => Some(RunOutputStream::StandardError),
        _ => None,
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
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use bray_runtime_interface::{NativePlatformStatus, NativePlatformText};

    use super::{
        CONTEXT_HEADER_BYTES, STANDARD_ERROR_HANDLE, STANDARD_OUTPUT_HANDLE,
        bray_platform_context_environment_key_equals, bray_platform_stream_write, lock_stream,
        native_text, process_context, unlock_stream,
    };
    use bray_platform::{RunOutputContext, RunOutputStream, with_run_output_context};

    #[test]
    fn native_platform_status_has_the_specified_layout() {
        assert_eq!(size_of::<NativePlatformStatus>(), 16);
    }

    #[test]
    fn process_context_publishes_the_complete_header() {
        let context = process_context()
            .unwrap_or_else(|status| panic!("process context must be available: {status:?}"));

        let mut total = [0; 8];

        total.copy_from_slice(&context[88..96]);

        assert!(context.len() >= CONTEXT_HEADER_BYTES);
        assert_eq!(u64::from_le_bytes(total), context.len() as u64);
        assert_eq!(context[16], 1);
        assert_eq!(context[24], 2);
        assert_eq!(context[32], 3);
    }

    #[test]
    fn environment_key_comparison_uses_target_rules() {
        let left = native_text(OsStr::new("Path"));
        let right = native_text(OsStr::new("PATH"));
        let left = NativePlatformText::new(left.as_ptr(), left.len() as u64);
        let right = NativePlatformText::new(right.as_ptr(), right.len() as u64);
        let mut equal = u32::MAX;

        let status = bray_platform_context_environment_key_equals(left, right, &raw mut equal);

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(equal, u32::from(cfg!(windows)));
    }

    #[test]
    fn environment_key_comparison_accepts_empty_keys() {
        let empty = NativePlatformText::new(std::ptr::null(), 0);
        let mut equal = u32::MAX;

        let status = bray_platform_context_environment_key_equals(empty, empty, &raw mut equal);

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(equal, 1);
    }

    #[test]
    fn platform_stream_writes_use_the_active_run_capture() {
        let output = RunOutputContext::captured(32, 32);
        let bytes = b"captured output";

        let byte_count = u64::try_from(bytes.len())
            .unwrap_or_else(|_| panic!("test output length must fit the native ABI"));

        let mut transferred = 0;

        let status = with_run_output_context(output.clone(), || {
            bray_platform_stream_write(
                STANDARD_OUTPUT_HANDLE,
                bytes.as_ptr(),
                byte_count,
                &mut transferred,
            )
        });

        assert_eq!(status, NativePlatformStatus::SUCCESS);
        assert_eq!(transferred, byte_count);

        assert_eq!(
            output
                .captured_stream(RunOutputStream::StandardOutput)
                .as_ref()
                .map(bray_platform::CapturedRunStream::bytes),
            Some(&bytes[..])
        );
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

    #[test]
    fn stream_lock_rejects_reentrant_acquisition() {
        assert_eq!(
            lock_stream(STANDARD_ERROR_HANDLE),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(
            lock_stream(STANDARD_ERROR_HANDLE),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(
            unlock_stream(STANDARD_ERROR_HANDLE),
            NativePlatformStatus::SUCCESS
        );
    }
}
