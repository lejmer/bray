use std::env;
use std::sync::OnceLock;

use bray_platform_abi_support::{
    MemoryRegion, disjoint, native_platform_export, startup_working_directory,
};
use bray_runtime_abi::{NativePlatformStatus, NativePlatformText};

const CONTEXT_HEADER_BYTES: usize = 72;
const CONTEXT_ABI_MAJOR: u16 = 1;
const CONTEXT_ABI_MINOR: u16 = 0;

struct ProcessContextOwner {
    identity: OnceLock<u64>,
    native_text_width: u8,
    block: OnceLock<Result<Box<[u8]>, NativePlatformStatus>>,
}

impl ProcessContextOwner {
    const fn new() -> Self {
        Self {
            identity: OnceLock::new(),
            native_text_width: native_text_width(),
            block: OnceLock::new(),
        }
    }

    fn identity(&self) -> u64 {
        *self
            .identity
            .get_or_init(|| u64::from(std::process::id()))
    }

    fn block(&self) -> Result<&[u8], NativePlatformStatus> {
        match self.block.get_or_init(build_process_context) {
            Ok(context) => Ok(context),
            Err(status) => Err(*status),
        }
    }

    const fn native_text_width(&self) -> u8 {
        self.native_text_width
    }
}

static PROCESS_CONTEXT: ProcessContextOwner = ProcessContextOwner::new();

native_platform_export! {
    pub extern "C" fn bray_platform_context_identity(identity: *mut u64) -> NativePlatformStatus {
        publish_value(identity, Ok(PROCESS_CONTEXT.identity()))
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_native_text_width(
        width: *mut u32,
    ) -> NativePlatformStatus {
        publish_value(width, Ok(u32::from(PROCESS_CONTEXT.native_text_width())))
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_working_directory(
        address: *mut *const std::ffi::c_void,
        length: *mut u64,
    ) -> NativePlatformStatus {
        publish_context_text(address, length, || {
            process_context().map(|context| context_range(context, 16))
        })
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_argument_count(
        count: *mut u64,
    ) -> NativePlatformStatus {
        publish_value(count, process_context().map(|context| read_u64(context, 32)))
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_argument(
        index: u64,
        address: *mut *const std::ffi::c_void,
        length: *mut u64,
    ) -> NativePlatformStatus {
        publish_context_text(address, length, || {
            process_context()
                .and_then(|context| context_indexed_range(context, index, 32, 40, 16))
        })
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_environment_count(
        count: *mut u64,
    ) -> NativePlatformStatus {
        publish_value(count, process_context().map(|context| read_u64(context, 48)))
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_context_environment_entry(
        index: u64,
        key_address: *mut *const std::ffi::c_void,
        key_length: *mut u64,
        value_address: *mut *const std::ffi::c_void,
        value_length: *mut u64,
    ) -> NativePlatformStatus {
        publish_context_text_pair(
            key_address,
            key_length,
            value_address,
            value_length,
            || {
                let context = process_context()?;
                let entry = context_indexed_offset(context, index, 48, 56, 32)?;

                Ok((context_range(context, entry), context_range(context, entry + 16)))
            },
        )
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
    PROCESS_CONTEXT.block()
}

#[expect(
    unsafe_code,
    reason = "validated native ABI output storage receives one initialized scalar"
)]
fn publish_value<T>(
    destination: *mut T,
    value: Result<T, NativePlatformStatus>,
) -> NativePlatformStatus {
    if MemoryRegion::write(destination).is_none() {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let value = match value {
        Ok(value) => value,
        Err(status) => return status,
    };

    unsafe { destination.write(value) };

    NativePlatformStatus::SUCCESS
}

#[expect(
    unsafe_code,
    reason = "validated native ABI output storage receives one product-lifetime text view"
)]
fn publish_context_text(
    address: *mut *const std::ffi::c_void,
    length: *mut u64,
    text: impl FnOnce() -> Result<&'static [u8], NativePlatformStatus>,
) -> NativePlatformStatus {
    let Some(address_region) = MemoryRegion::write(address) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    let Some(length_region) = MemoryRegion::write(length) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    if address_region.overlaps(length_region) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let text = match text() {
        Ok(text) => text,
        Err(status) => return status,
    };

    let Some(text_length) = u64::try_from(text.len()).ok() else {
        return NativePlatformStatus::EXHAUSTED;
    };

    unsafe {
        address.write(text.as_ptr().cast());
        length.write(text_length);
    }

    NativePlatformStatus::SUCCESS
}

#[expect(
    unsafe_code,
    reason = "validated disjoint native ABI output storage receives product-lifetime text views"
)]
fn publish_context_text_pair(
    key_address: *mut *const std::ffi::c_void,
    key_length: *mut u64,
    value_address: *mut *const std::ffi::c_void,
    value_length: *mut u64,
    text: impl FnOnce() -> Result<(&'static [u8], &'static [u8]), NativePlatformStatus>,
) -> NativePlatformStatus {
    let Some(key_address_region) = MemoryRegion::write(key_address) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    let Some(key_length_region) = MemoryRegion::write(key_length) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    let Some(value_address_region) = MemoryRegion::write(value_address) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    let Some(value_length_region) = MemoryRegion::write(value_length) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    if !disjoint(&[
        key_address_region,
        key_length_region,
        value_address_region,
        value_length_region,
    ]) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    let (key, value) = match text() {
        Ok(text) => text,
        Err(status) => return status,
    };

    let Some(key_length_value) = u64::try_from(key.len()).ok() else {
        return NativePlatformStatus::EXHAUSTED;
    };

    let Some(value_length_value) = u64::try_from(value.len()).ok() else {
        return NativePlatformStatus::EXHAUSTED;
    };

    unsafe {
        key_address.write(key.as_ptr().cast());
        key_length.write(key_length_value);
        value_address.write(value.as_ptr().cast());
        value_length.write(value_length_value);
    }

    NativePlatformStatus::SUCCESS
}

fn context_indexed_range(
    context: &'static [u8],
    index: u64,
    count_offset: usize,
    table_offset: usize,
    entry_size: usize,
) -> Result<&'static [u8], NativePlatformStatus> {
    let entry = context_indexed_offset(context, index, count_offset, table_offset, entry_size)?;

    Ok(context_range(context, entry))
}

fn context_indexed_offset(
    context: &[u8],
    index: u64,
    count_offset: usize,
    table_offset: usize,
    entry_size: usize,
) -> Result<usize, NativePlatformStatus> {
    if index >= read_u64(context, count_offset) {
        return Err(NativePlatformStatus::NOT_FOUND);
    }

    let index = usize::try_from(index).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let table = usize::try_from(read_u64(context, table_offset))
        .map_err(|_| NativePlatformStatus::EXHAUSTED)?;

    Ok(table + index * entry_size)
}

fn context_range(context: &'static [u8], descriptor_offset: usize) -> &'static [u8] {
    let offset = read_u64(context, descriptor_offset) as usize;
    let length = read_u64(context, descriptor_offset + 8) as usize;

    &context[offset..offset + length]
}

fn read_u64(context: &[u8], offset: usize) -> u64 {
    let mut bytes = [0; 8];

    bytes.copy_from_slice(&context[offset..offset + 8]);

    u64::from_le_bytes(bytes)
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
    block[4] = PROCESS_CONTEXT.native_text_width();
    block[5] = environment_comparison();
    write_u64(&mut block, 8, PROCESS_CONTEXT.identity());
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
        PROCESS_CONTEXT,
        bray_platform_context_argument, bray_platform_context_argument_count,
        bray_platform_context_environment_key_equals, bray_platform_context_identity,
        bray_platform_context_native_text_width, bray_platform_context_working_directory,
        native_text, process_context,
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
        let mut identity = [0; 8];

        total.copy_from_slice(&context[64..72]);
        identity.copy_from_slice(&context[8..16]);

        assert_eq!((CONTEXT_ABI_MAJOR, CONTEXT_ABI_MINOR), (1, 0));
        assert!(context.len() >= CONTEXT_HEADER_BYTES);
        assert_eq!(u16::from_le_bytes([context[0], context[1]]), CONTEXT_ABI_MAJOR);
        assert_eq!(u16::from_le_bytes([context[2], context[3]]), CONTEXT_ABI_MINOR);
        assert_eq!(context[4], PROCESS_CONTEXT.native_text_width());
        assert_eq!(u64::from_le_bytes(identity), PROCESS_CONTEXT.identity());
        assert_eq!(u64::from_le_bytes(total), context.len() as u64);
    }

    #[test]
    fn scalar_context_roles_publish_immutable_values() {
        let mut first_identity = 0;
        let mut second_identity = 0;
        let mut width = 0;
        let mut argument_count = 0;

        let first_status = bray_platform_context_identity(&raw mut first_identity);
        let second_status = bray_platform_context_identity(&raw mut second_identity);
        let width_status = bray_platform_context_native_text_width(&raw mut width);
        let count_status = bray_platform_context_argument_count(&raw mut argument_count);

        assert_eq!(first_status, NativePlatformStatus::SUCCESS);
        assert_eq!(second_status, NativePlatformStatus::SUCCESS);
        assert_eq!(width_status, NativePlatformStatus::SUCCESS);
        assert_eq!(count_status, NativePlatformStatus::SUCCESS);
        assert_eq!(first_identity, second_identity);
        assert_eq!(first_identity, u64::from(std::process::id()));
        assert_eq!(width, if cfg!(windows) { 16 } else { 8 });
        assert!(argument_count > 0);
    }

    #[test]
    fn borrowed_context_text_remains_stable_for_the_product_lifetime() {
        let mut first_address = std::ptr::null();
        let mut first_length = 0;
        let mut second_address = std::ptr::null();
        let mut second_length = 0;

        let first_status = bray_platform_context_working_directory(
            &raw mut first_address,
            &raw mut first_length,
        );

        let second_status = bray_platform_context_working_directory(
            &raw mut second_address,
            &raw mut second_length,
        );

        assert_eq!(first_status, NativePlatformStatus::SUCCESS);
        assert_eq!(second_status, NativePlatformStatus::SUCCESS);
        assert_eq!((first_address, first_length), (second_address, second_length));
        assert!(!first_address.is_null());
        assert!(first_length > 0);

        let first_status = bray_platform_context_argument(
            0,
            &raw mut first_address,
            &raw mut first_length,
        );

        let second_status = bray_platform_context_argument(
            0,
            &raw mut second_address,
            &raw mut second_length,
        );

        assert_eq!(first_status, NativePlatformStatus::SUCCESS);
        assert_eq!(second_status, NativePlatformStatus::SUCCESS);
        assert_eq!((first_address, first_length), (second_address, second_length));
        assert!(!first_address.is_null());
        assert!(first_length > 0);
    }

    #[test]
    fn borrowed_context_text_rejects_overlapping_outputs() {
        let mut output = 0_u64;
        let output_pointer = &raw mut output;

        let status = bray_platform_context_working_directory(
            output_pointer.cast(),
            output_pointer,
        );

        assert_eq!(status, NativePlatformStatus::INVALID_INPUT);
        assert_eq!(output, 0);
    }

    #[test]
    fn indexed_context_roles_reject_absent_entries() {
        let mut count = 0;
        let mut address = std::ptr::null();
        let mut length = 0;

        assert_eq!(
            bray_platform_context_argument_count(&raw mut count),
            NativePlatformStatus::SUCCESS
        );

        let status = bray_platform_context_argument(
            count,
            &raw mut address,
            &raw mut length,
        );

        assert_eq!(status, NativePlatformStatus::NOT_FOUND);
        assert!(address.is_null());
        assert_eq!(length, 0);
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
