use std::env;
use std::sync::OnceLock;

use bray_platform_abi_support::{
    MemoryRegion, disjoint, native_platform_export, startup_working_directory,
};
use bray_runtime_abi::{NativePlatformStatus, NativePlatformText};

const CONTEXT_HEADER_BYTES: usize = 72;
const CONTEXT_ABI_MAJOR: u16 = 2;
const CONTEXT_ABI_MINOR: u16 = 0;

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

    write_u16(&mut block, 0, CONTEXT_ABI_MAJOR);
    write_u16(&mut block, 2, CONTEXT_ABI_MINOR);
    block[4] = native_text_width();
    block[5] = environment_comparison();
    write_u64(&mut block, 8, u64::from(std::process::id()));
    write_range(&mut block, 16, working_directory_range);
    write_u64(&mut block, 32, length_u64(arguments.len()));
    write_u64(&mut block, 40, length_u64(argument_table));
    write_u64(&mut block, 48, length_u64(environment.len()));
    write_u64(&mut block, 56, length_u64(environment_table));
    write_u64(&mut block, 64, block_length);

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
    use bray_runtime_abi::{NativePlatformStatus, NativePlatformText};

    use super::{
        CONTEXT_ABI_MAJOR, CONTEXT_ABI_MINOR, CONTEXT_HEADER_BYTES,
        bray_platform_context_environment_key_equals, native_text, process_context,
    };

    #[test]
    fn native_platform_status_has_the_specified_layout() {
        assert_eq!(size_of::<NativePlatformStatus>(), 16);
    }

    #[test]
    fn process_context_publishes_the_complete_header() {
        let context = process_context()
            .unwrap_or_else(|status| panic!("process context must be available: {status:?}"));

        let mut total = [0; 8];

        total.copy_from_slice(&context[64..72]);

        assert!(context.len() >= CONTEXT_HEADER_BYTES);
        assert_eq!(u16::from_le_bytes([context[0], context[1]]), CONTEXT_ABI_MAJOR);
        assert_eq!(u16::from_le_bytes([context[2], context[3]]), CONTEXT_ABI_MINOR);
        assert_eq!(u64::from_le_bytes(total), context.len() as u64);
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

}
