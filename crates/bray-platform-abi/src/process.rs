use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bray_platform::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter, NativeProcessCommand,
    NativeStdio, PlatformError, PlatformErrorKind,
};
use bray_runtime_interface::{
    NativePlatformChildRequest, NativePlatformEnvironmentList, NativePlatformExitStatus,
    NativePlatformSpanList, NativePlatformStatus, NativePlatformText,
};

use super::platform::platform_io_error;
use super::region::{MemoryRegion, disjoint};

const FIRST_PROCESS_HANDLE: u64 = 1 << 63;

enum ProcessHandle {
    Child(ChildState),
    Reader(NativePipeReader),
    Writer(NativePipeWriter),
}

struct ChildState {
    child: NativeChildProcess,
    terminal: Option<NativeExitStatus>,
}

fn handles() -> &'static Mutex<BTreeMap<u64, Arc<Mutex<ProcessHandle>>>> {
    static HANDLES: OnceLock<Mutex<BTreeMap<u64, Arc<Mutex<ProcessHandle>>>>> = OnceLock::new();

    HANDLES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn insert_handles(
    values: Vec<ProcessHandle>,
) -> Result<Vec<u64>, (NativePlatformStatus, Vec<ProcessHandle>)> {
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(FIRST_PROCESS_HANDLE);

    let mut ids = Vec::with_capacity(values.len());

    let Ok(mut handles) = handles().lock() else {
        return Err((NativePlatformStatus::OTHER, values));
    };

    for _ in 0..values.len() {
        let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);

        if id < FIRST_PROCESS_HANDLE || handles.contains_key(&id) {
            return Err((NativePlatformStatus::EXHAUSTED, values));
        }

        ids.push(id);
    }

    for (id, value) in ids.iter().copied().zip(values) {
        handles.insert(id, Arc::new(Mutex::new(value)));
    }

    Ok(ids)
}

fn handle(id: u64) -> Result<Arc<Mutex<ProcessHandle>>, NativePlatformStatus> {
    handles()
        .lock()
        .map_err(|_| NativePlatformStatus::OTHER)?
        .get(&id)
        .cloned()
        .ok_or(NativePlatformStatus::INVALID_INPUT)
}

fn remove_handle(id: u64) -> Result<ProcessHandle, NativePlatformStatus> {
    let mut handles = handles().lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let Some(handle) = handles.remove(&id) else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    match Arc::try_unwrap(handle) {
        Ok(handle) => handle.into_inner().map_err(|_| NativePlatformStatus::OTHER),
        Err(handle) => {
            handles.insert(id, handle);

            Err(NativePlatformStatus::INVALID_INPUT)
        }
    }
}

fn with_child<T>(
    id: u64,
    operation: impl FnOnce(&mut ChildState) -> Result<T, NativePlatformStatus>,
) -> Result<T, NativePlatformStatus> {
    let handle = handle(id)?;
    let mut handle = handle.lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let ProcessHandle::Child(child) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    operation(child)
}

native_platform_export! {
    pub extern "C" fn bray_platform_child_spawn(
        request: NativePlatformChildRequest,
        child: *mut u64,
        standard_input: *mut u64,
        standard_output: *mut u64,
        standard_error: *mut u64,
    ) -> NativePlatformStatus {
        let outputs = [child, standard_input, standard_output, standard_error];
        let Some(output_regions) = outputs
            .map(MemoryRegion::write)
            .into_iter()
            .collect::<Option<Vec<_>>>()
        else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&output_regions) || request.reserved() != 0 {
            return NativePlatformStatus::INVALID_INPUT;
        }

        for output in outputs {
            unsafe { output.write(0) };
        }

        let mut input_regions = Vec::new();

        let executable = match native_value(request.executable(), &mut input_regions) {
            Ok(value) if !value.is_empty() => value,
            Ok(_) => return NativePlatformStatus::INVALID_INPUT,
            Err(status) => return status,
        };

        let working_directory = match optional_native_value(
            request.working_directory(),
            &mut input_regions,
        ) {
            Ok(value) => value.map(PathBuf::from),
            Err(status) => return status,
        };

        let arguments = match native_values(request.arguments(), &mut input_regions) {
            Ok(values) => values,
            Err(status) => return status,
        };

        let environment = match native_environment(request.environment(), &mut input_regions) {
            Ok(values) => values,
            Err(status) => return status,
        };

        if !disjoint(
            &input_regions
                .iter()
                .copied()
                .chain(output_regions.iter().copied())
                .collect::<Vec<_>>(),
        ) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let policies = [
            request.standard_input(),
            request.standard_output(),
            request.standard_error(),
        ];

        let [Some(input_policy), Some(output_policy), Some(error_policy)] =
            policies.map(stream_policy)
        else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let mut command = match NativeProcessCommand::new(executable) {
            Ok(command) => command,
            Err(error) => return platform_error(error),
        };

        command.env_clear();

        for argument in arguments {
            command.arg(argument);
        }

        for (key, value) in environment {
            command.env(key, value);
        }

        if let Some(directory) = working_directory {
            command.current_dir(directory);
        }

        command
            .stdin(input_policy)
            .stdout(output_policy)
            .stderr(error_policy);

        let mut process = match command.spawn() {
            Ok(process) => process,
            Err(error) => return platform_error(error),
        };

        let input = process.take_stdin().map(ProcessHandle::Writer);
        let output = process.take_stdout().map(ProcessHandle::Reader);
        let error = process.take_stderr().map(ProcessHandle::Reader);

        let mut values = vec![ProcessHandle::Child(ChildState {
            child: process,
            terminal: None,
        })];

        let input_index = push_optional_handle(&mut values, input);
        let output_index = push_optional_handle(&mut values, output);
        let error_index = push_optional_handle(&mut values, error);

        let ids = match insert_handles(values) {
            Ok(ids) => ids,
            Err((status, values)) => {
                dispose_process_handles(values);

                return status;
            }
        };

        unsafe {
            child.write(ids[0]);
            standard_input.write(optional_handle_id(&ids, input_index));
            standard_output.write(optional_handle_id(&ids, output_index));
            standard_error.write(optional_handle_id(&ids, error_index));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_child_wait(
        child: u64,
        terminal: *mut u32,
        status: *mut NativePlatformExitStatus,
    ) -> NativePlatformStatus {
        let Some(terminal_region) = MemoryRegion::write(terminal) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(status_region) = MemoryRegion::write(status) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[terminal_region, status_region]) {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let exit = match with_child(child, |child| {
            if let Some(status) = child.terminal {
                return Ok(status);
            }

            let status = child.child.wait().map_err(platform_error)?;

            child.terminal = Some(status);

            Ok(status)
        }) {
            Ok(status) => status,
            Err(status) => return status,
        };

        unsafe {
            terminal.write(1);
            status.write(exit_status(exit));
        }

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_child_terminate(
        child: u64,
        mode: u32,
    ) -> NativePlatformStatus {
        if mode == 0 {
            return NativePlatformStatus::UNSUPPORTED;
        }

        if mode != 1 {
            return NativePlatformStatus::INVALID_INPUT;
        }

        match with_child(child, |child| {
            if child.terminal.is_some() {
                return Ok(());
            }

            child.child.terminate().map_err(platform_error)
        }) {
            Ok(()) => NativePlatformStatus::SUCCESS,
            Err(status) => status,
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_child_reap(
        child_handle: u64,
        status: *mut NativePlatformExitStatus,
    ) -> NativePlatformStatus {
        if MemoryRegion::write(status).is_none() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let exit = match with_child(child_handle, |child| {
            child.terminal.ok_or(NativePlatformStatus::INVALID_INPUT)
        }) {
            Ok(exit) => exit,
            Err(status) => return status,
        };

        if remove_handle(child_handle).is_err() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        unsafe { status.write(exit_status(exit)) };

        NativePlatformStatus::SUCCESS
    }
}

pub(super) const fn is_process_handle(id: u64) -> bool {
    id >= FIRST_PROCESS_HANDLE
}

pub(super) fn is_process_stream(id: u64) -> bool {
    handle(id).is_ok_and(|handle| {
        handle.lock().is_ok_and(|handle| {
            matches!(*handle, ProcessHandle::Reader(_) | ProcessHandle::Writer(_))
        })
    })
}

pub(super) fn read_process_stream(
    id: u64,
    destination: &mut [u8],
) -> Result<usize, NativePlatformStatus> {
    let handle = handle(id)?;
    let mut handle = handle.lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let ProcessHandle::Reader(reader) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    reader
        .read(destination)
        .map_err(|error| platform_io_error(&error))
}

pub(super) fn write_process_stream(
    id: u64,
    source: &[u8],
) -> Result<usize, NativePlatformStatus> {
    let handle = handle(id)?;
    let mut handle = handle.lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let ProcessHandle::Writer(writer) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    writer.write(source).map_err(|error| platform_io_error(&error))
}

pub(super) fn flush_process_stream(id: u64) -> Result<(), NativePlatformStatus> {
    let handle = handle(id)?;
    let mut handle = handle.lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let ProcessHandle::Writer(writer) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    writer.flush().map_err(|error| platform_io_error(&error))
}

pub(super) fn close_process_stream(id: u64) -> NativePlatformStatus {
    if !is_process_stream(id) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    match remove_handle(id) {
        Ok(ProcessHandle::Reader(_) | ProcessHandle::Writer(_)) => NativePlatformStatus::SUCCESS,
        Ok(ProcessHandle::Child(_)) => NativePlatformStatus::INVALID_INPUT,
        Err(status) => status,
    }
}

fn dispose_process_handles(values: Vec<ProcessHandle>) {
    for value in values {
        if let ProcessHandle::Child(mut child) = value {
            let _ = child.child.terminate();
            let _ = child.child.wait();
        }
    }
}

fn push_optional_handle(values: &mut Vec<ProcessHandle>, value: Option<ProcessHandle>) -> Option<usize> {
    let value = value?;
    let index = values.len();

    values.push(value);

    Some(index)
}

fn optional_handle_id(ids: &[u64], index: Option<usize>) -> u64 {
    index.map_or(0, |index| ids[index])
}

fn stream_policy(value: u32) -> Option<NativeStdio> {
    match value {
        0 => Some(NativeStdio::Inherit),
        1 => Some(NativeStdio::Null),
        2 => Some(NativeStdio::Piped),
        _ => None,
    }
}

#[expect(
    unsafe_code,
    reason = "the validated call-only span-list region is borrowed for native conversion"
)]
fn native_values(
    list: NativePlatformSpanList,
    regions: &mut Vec<MemoryRegion>,
) -> Result<Vec<OsString>, NativePlatformStatus> {
    let count = usize::try_from(list.count()).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let region = MemoryRegion::read(list.entries(), count)
        .ok_or(NativePlatformStatus::INVALID_INPUT)?;

    regions.push(region);

    let entries = unsafe { std::slice::from_raw_parts(list.entries(), count) };

    entries
        .iter()
        .copied()
        .map(|entry| native_value(entry, regions))
        .collect()
}

#[expect(
    unsafe_code,
    reason = "the validated call-only environment-list region is borrowed for native conversion"
)]
fn native_environment(
    list: NativePlatformEnvironmentList,
    regions: &mut Vec<MemoryRegion>,
) -> Result<Vec<(OsString, OsString)>, NativePlatformStatus> {
    let count = usize::try_from(list.count()).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let region = MemoryRegion::read(list.entries(), count)
        .ok_or(NativePlatformStatus::INVALID_INPUT)?;

    regions.push(region);

    let entries = unsafe { std::slice::from_raw_parts(list.entries(), count) };

    entries
        .iter()
        .copied()
        .map(|entry| {
            Ok((
                native_value(entry.key(), regions)?,
                native_value(entry.value(), regions)?,
            ))
        })
        .collect()
}

fn optional_native_value(
    value: NativePlatformText,
    regions: &mut Vec<MemoryRegion>,
) -> Result<Option<OsString>, NativePlatformStatus> {
    if value.length() == 0 {
        if !value.address().is_null() {
            let region = MemoryRegion::read(value.address(), 0)
                .ok_or(NativePlatformStatus::INVALID_INPUT)?;

            regions.push(region);
        }

        return Ok(None);
    }

    native_value(value, regions).map(Some)
}

#[expect(
    unsafe_code,
    reason = "the validated call-only byte region is borrowed for native conversion"
)]
fn native_value(
    value: NativePlatformText,
    regions: &mut Vec<MemoryRegion>,
) -> Result<OsString, NativePlatformStatus> {
    let length = usize::try_from(value.length()).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let region = MemoryRegion::read(value.address(), length)
        .ok_or(NativePlatformStatus::INVALID_INPUT)?;

    regions.push(region);

    let bytes = unsafe { std::slice::from_raw_parts(value.address(), length) };

    native_text_from_bytes(bytes)
}

#[cfg(windows)]
fn native_text_from_bytes(bytes: &[u8]) -> Result<OsString, NativePlatformStatus> {
    use std::os::windows::ffi::OsStringExt;

    if !bytes.len().is_multiple_of(2) {
        return Err(NativePlatformStatus::INVALID_INPUT);
    }

    let units = bytes
        .chunks_exact(2)
        .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
        .collect::<Vec<_>>();

    Ok(OsString::from_wide(&units))
}

#[cfg(unix)]
fn native_text_from_bytes(bytes: &[u8]) -> Result<OsString, NativePlatformStatus> {
    use std::os::unix::ffi::OsStringExt;

    Ok(OsString::from_vec(bytes.to_vec()))
}

fn exit_status(status: NativeExitStatus) -> NativePlatformExitStatus {
    match status.code() {
        Some(code) => NativePlatformExitStatus::code(code),
        None => NativePlatformExitStatus::target_termination(
            status.target_termination().unwrap_or_default(),
        ),
    }
}

fn platform_error(error: PlatformError) -> NativePlatformStatus {
    match error.kind() {
        PlatformErrorKind::Io(kind) => platform_io_error(&std::io::Error::from(kind)),
        PlatformErrorKind::InvalidSize | PlatformErrorKind::InvalidEventIdentity => {
            NativePlatformStatus::INVALID_INPUT
        }
        PlatformErrorKind::ThreadIdentityExhausted | PlatformErrorKind::EventGenerationExhausted => {
            NativePlatformStatus::EXHAUSTED
        }
        PlatformErrorKind::Unsupported => NativePlatformStatus::UNSUPPORTED,
        PlatformErrorKind::SynchronizationPoisoned
        | PlatformErrorKind::RuntimeThreadAlreadyInitialized => NativePlatformStatus::OTHER,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use bray_runtime_interface::{
        NativePlatformChildRequest, NativePlatformEnvironmentEntry,
        NativePlatformEnvironmentList, NativePlatformExitStatus, NativePlatformSpanList,
        NativePlatformStatus, NativePlatformText,
    };

    use super::{
        bray_platform_child_reap, bray_platform_child_spawn, bray_platform_child_wait,
    };
    use crate::platform::{bray_platform_stream_close, bray_platform_stream_read};

    struct NativeText {
        bytes: Vec<u8>,
    }

    impl NativeText {
        fn new(value: impl AsRef<OsStr>) -> Self {
            Self {
                bytes: native_bytes(value.as_ref()),
            }
        }

        fn span(&self) -> NativePlatformText {
            NativePlatformText::new(
                self.bytes.as_ptr(),
                u64::try_from(self.bytes.len())
                    .unwrap_or_else(|_| panic!("test text length must fit the native ABI")),
            )
        }
    }

    #[test]
    fn child_process_abi_preserves_environment_output_and_ownership() {
        let (program, argument_values) = output_command();

        let program = NativeText::new(program);

        let arguments = argument_values
            .into_iter()
            .map(NativeText::new)
            .collect::<Vec<_>>();

        let argument_spans = arguments.iter().map(NativeText::span).collect::<Vec<_>>();
        let environment_key = NativeText::new("BRAY_CHILD_VALUE");
        let environment_value = NativeText::new("child-value");

        let environment = [NativePlatformEnvironmentEntry::new(
            environment_key.span(),
            environment_value.span(),
        )];

        let request = NativePlatformChildRequest::new(
            program.span(),
            NativePlatformText::new(std::ptr::null(), 0),
            NativePlatformSpanList::new(
                argument_spans.as_ptr(),
                u64::try_from(argument_spans.len())
                    .unwrap_or_else(|_| panic!("test argument count must fit the native ABI")),
            ),
            NativePlatformEnvironmentList::new(environment.as_ptr(), 1),
            1,
            2,
            1,
        );

        let mut child = 0;
        let mut input = 0;
        let mut output = 0;
        let mut error = 0;

        assert_eq!(
            bray_platform_child_spawn(
                request,
                &mut child,
                &mut input,
                &mut output,
                &mut error,
            ),
            NativePlatformStatus::SUCCESS,
        );

        assert_ne!(child, 0);
        assert_eq!(input, 0);
        assert_ne!(output, 0);
        assert_eq!(error, 0);

        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 64];

        loop {
            let mut transferred = 0;

            assert_eq!(
                bray_platform_stream_read(
                    output,
                    buffer.as_mut_ptr(),
                    buffer.len() as u64,
                    &mut transferred,
                ),
                NativePlatformStatus::SUCCESS,
            );

            if transferred == 0 {
                break;
            }

            bytes.extend_from_slice(&buffer[..transferred as usize]);
        }

        assert_eq!(
            bray_platform_stream_close(output),
            NativePlatformStatus::SUCCESS,
        );

        let mut terminal = 0;
        let mut status = NativePlatformExitStatus::code(-1);

        assert_eq!(
            bray_platform_child_wait(child, &mut terminal, &mut status),
            NativePlatformStatus::SUCCESS,
        );

        assert_eq!(terminal, 1);
        assert_eq!(status, NativePlatformExitStatus::code(0));

        assert_eq!(
            bray_platform_child_reap(child, &mut status),
            NativePlatformStatus::SUCCESS,
        );

        assert_eq!(
            bray_platform_child_wait(child, &mut terminal, &mut status),
            NativePlatformStatus::INVALID_INPUT,
        );

        assert_eq!(String::from_utf8_lossy(&bytes).trim(), "child-value");
    }

    #[cfg(windows)]
    fn output_command() -> (OsString, Vec<OsString>) {
        let program = std::env::var_os("COMSPEC")
            .unwrap_or_else(|| OsString::from(r"C:\Windows\System32\cmd.exe"));

        (
            program,
            vec![
                OsString::from("/D"),
                OsString::from("/C"),
                OsString::from("echo %BRAY_CHILD_VALUE%"),
            ],
        )
    }

    #[cfg(unix)]
    fn output_command() -> (OsString, Vec<OsString>) {
        (
            OsString::from("/bin/sh"),
            vec![
                OsString::from("-c"),
                OsString::from("printf '%s' \"$BRAY_CHILD_VALUE\""),
            ],
        )
    }

    #[cfg(windows)]
    fn native_bytes(value: &OsStr) -> Vec<u8> {
        use std::os::windows::ffi::OsStrExt;

        value
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    }

    #[cfg(unix)]
    fn native_bytes(value: &OsStr) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;

        value.as_bytes().to_vec()
    }
}
