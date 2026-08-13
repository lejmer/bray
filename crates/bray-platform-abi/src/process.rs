use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bray_platform::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter, NativeProcessCommand,
    NativeStdio, PlatformError, PlatformErrorKind,
};
use bray_platform_abi_support::{
    MemoryRegion, destination_slice, disjoint, mutually_disjoint, native_platform_export,
    platform_io_error, publish_transfer_count, source_slice, validate_transfer,
    PROCESS_HANDLE_TAG,
};
use bray_runtime_abi::{
    NativePlatformChildRequest, NativePlatformEnvironmentList, NativePlatformExitStatus,
    NativePlatformSpanList, NativePlatformStatus, NativePlatformText,
};

enum ProcessHandle {
    Child(ChildState),
    Reader(NativePipeReader),
    Writer(NativePipeWriter),
    Closed,
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
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(PROCESS_HANDLE_TAG);

    let mut ids = Vec::with_capacity(values.len());

    let mut handles = handles()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    for _ in 0..values.len() {
        let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);

        if id & PROCESS_HANDLE_TAG == 0 || handles.contains_key(&id) {
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
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&id)
        .cloned()
        .ok_or(NativePlatformStatus::INVALID_INPUT)
}

fn remove_handle(id: u64) -> Result<ProcessHandle, NativePlatformStatus> {
    let mut handles = handles()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(handle) = handles.remove(&id) else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    let mut handle = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    Ok(std::mem::replace(&mut *handle, ProcessHandle::Closed))
}

fn with_child<T>(
    id: u64,
    operation: impl FnOnce(&mut ChildState) -> Result<T, NativePlatformStatus>,
) -> Result<T, NativePlatformStatus> {
    let handle = handle(id)?;

    let mut handle = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

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

        if !mutually_disjoint(&input_regions, &output_regions) {
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

        for output in outputs {
            unsafe { output.write(0) };
        }

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

        let exit = loop {
            let observed = match with_child(child, |child| {
                if let Some(status) = child.terminal {
                    return Ok(Some(status));
                }

                let status = child.child.try_wait().map_err(platform_error)?;

                if let Some(status) = status {
                    child.terminal = Some(status);
                }

                Ok(status)
            }) {
                Ok(status) => status,
                Err(status) => return status,
            };

            if let Some(status) = observed {
                break status;
            }

            std::thread::sleep(Duration::from_millis(1));
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
    pub extern "C" fn bray_platform_child_dispose(child_handle: u64) -> NativePlatformStatus {
        if let Err(status) = with_child(child_handle, |_| Ok(())) {
            return status;
        }

        let handle = match remove_handle(child_handle) {
            Ok(handle) => handle,
            Err(status) => return status,
        };

        let ProcessHandle::Child(mut child) = handle else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        dispose_child(&mut child)
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

fn is_process_stream(id: u64) -> bool {
    handle(id).is_ok_and(|handle| {
        let handle = handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        matches!(*handle, ProcessHandle::Reader(_) | ProcessHandle::Writer(_))
    })
}

fn read_process_stream(
    id: u64,
    destination: &mut [u8],
) -> Result<usize, NativePlatformStatus> {
    let handle = handle(id)?;

    let mut handle = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let ProcessHandle::Reader(reader) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    reader
        .read(destination)
        .map_err(|error| platform_io_error(&error))
}

fn write_process_stream(id: u64, source: &[u8]) -> Result<usize, NativePlatformStatus> {
    let handle = handle(id)?;

    let mut handle = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let ProcessHandle::Writer(writer) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    writer
        .write(source)
        .map_err(|error| platform_io_error(&error))
}

fn flush_process_stream(id: u64) -> Result<(), NativePlatformStatus> {
    let handle = handle(id)?;

    let mut handle = handle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let ProcessHandle::Writer(writer) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    writer.flush().map_err(|error| platform_io_error(&error))
}

fn close_process_stream(id: u64) -> NativePlatformStatus {
    if !is_process_stream(id) {
        return NativePlatformStatus::INVALID_INPUT;
    }

    match remove_handle(id) {
        Ok(ProcessHandle::Reader(_) | ProcessHandle::Writer(_)) => NativePlatformStatus::SUCCESS,
        Ok(ProcessHandle::Child(_) | ProcessHandle::Closed) => NativePlatformStatus::INVALID_INPUT,
        Err(status) => status,
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_process_pipe_read(
        handle: u64,
        destination: *mut u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = validate_transfer(destination, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let initialized = unsafe { publish_transfer_count(transferred, 0) };

        if initialized != NativePlatformStatus::SUCCESS {
            return initialized;
        }

        let destination = unsafe { destination_slice(destination, length) };

        match read_process_stream(handle, destination) {
            Ok(count) => unsafe { publish_transfer_count(transferred, count) },
            Err(status) => status,
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_process_pipe_write(
        handle: u64,
        source: *const u8,
        length: u64,
        transferred: *mut u64,
    ) -> NativePlatformStatus {
        let Some(length) = validate_transfer(source, length, transferred) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let initialized = unsafe { publish_transfer_count(transferred, 0) };

        if initialized != NativePlatformStatus::SUCCESS {
            return initialized;
        }

        let source = unsafe { source_slice(source, length) };

        match write_process_stream(handle, source) {
            Ok(count) => unsafe { publish_transfer_count(transferred, count) },
            Err(status) => status,
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_process_pipe_flush(handle: u64) -> NativePlatformStatus {
        match flush_process_stream(handle) {
            Ok(()) => NativePlatformStatus::SUCCESS,
            Err(status) => status,
        }
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_process_pipe_close(handle: u64) -> NativePlatformStatus {
        close_process_stream(handle)
    }
}

fn dispose_process_handles(values: Vec<ProcessHandle>) {
    for value in values {
        if let ProcessHandle::Child(mut child) = value {
            let _ = dispose_child(&mut child);
        }
    }
}

fn dispose_child(child: &mut ChildState) -> NativePlatformStatus {
    if child.terminal.is_some() {
        return NativePlatformStatus::SUCCESS;
    }

    let mut first_error = match child.child.try_wait() {
        Ok(Some(status)) => {
            child.terminal = Some(status);

            return NativePlatformStatus::SUCCESS;
        }
        Ok(None) => None,
        Err(error) => Some(platform_error(error)),
    };

    if let Err(error) = child.child.terminate() {
        first_error.get_or_insert_with(|| platform_error(error));
    }

    match child.child.wait() {
        Ok(status) => child.terminal = Some(status),
        Err(error) => {
            first_error.get_or_insert_with(|| platform_error(error));
        }
    }

    first_error.unwrap_or(NativePlatformStatus::SUCCESS)
}

fn push_optional_handle(
    values: &mut Vec<ProcessHandle>,
    value: Option<ProcessHandle>,
) -> Option<usize> {
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

    if count == 0 {
        return Ok(Vec::new());
    }

    let region =
        MemoryRegion::read(list.entries(), count).ok_or(NativePlatformStatus::INVALID_INPUT)?;

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

    if count == 0 {
        return Ok(Vec::new());
    }

    let region =
        MemoryRegion::read(list.entries(), count).ok_or(NativePlatformStatus::INVALID_INPUT)?;

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
    let length =
        usize::try_from(value.length()).map_err(|_| NativePlatformStatus::INVALID_INPUT)?;

    let region =
        MemoryRegion::read(value.address(), length).ok_or(NativePlatformStatus::INVALID_INPUT)?;

    regions.push(region);

    let bytes = unsafe { source_slice(value.address(), length) };

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
        PlatformErrorKind::ThreadIdentityExhausted
        | PlatformErrorKind::EventGenerationExhausted => NativePlatformStatus::EXHAUSTED,
        PlatformErrorKind::Unsupported => NativePlatformStatus::UNSUPPORTED,
        PlatformErrorKind::SynchronizationPoisoned
        | PlatformErrorKind::RuntimeThreadAlreadyInitialized => NativePlatformStatus::OTHER,
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};

    use bray_runtime_abi::{
        NativePlatformChildRequest, NativePlatformEnvironmentEntry, NativePlatformEnvironmentList,
        NativePlatformExitStatus, NativePlatformSpanList, NativePlatformStatus, NativePlatformText,
    };
    use bray_platform_abi_support::PROCESS_HANDLE_TAG;

    use super::{
        bray_platform_child_dispose, bray_platform_child_reap, bray_platform_child_spawn,
        bray_platform_child_wait,
        bray_platform_process_pipe_close, bray_platform_process_pipe_read,
        bray_platform_process_pipe_write,
    };

    #[test]
    fn empty_process_pipe_transfers_still_validate_the_pipe_owner() {
        let mut transferred = 1;

        assert_eq!(
            bray_platform_process_pipe_read(
                1,
                std::ptr::null_mut(),
                0,
                &raw mut transferred,
            ),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(transferred, 0);
        transferred = 1;

        assert_eq!(
            bray_platform_process_pipe_write(1, std::ptr::null(), 0, &raw mut transferred),
            NativePlatformStatus::INVALID_INPUT
        );

        assert_eq!(transferred, 0);
    }

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
            bray_platform_child_spawn(request, &mut child, &mut input, &mut output, &mut error,),
            NativePlatformStatus::SUCCESS,
        );

        assert_ne!(child, 0);
        assert_eq!(input, 0);
        assert_ne!(output, 0);
        assert_eq!(error, 0);
        assert_ne!(child & PROCESS_HANDLE_TAG, 0);
        assert_ne!(output & PROCESS_HANDLE_TAG, 0);

        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 64];

        loop {
            let mut transferred = 0;

            assert_eq!(
                bray_platform_process_pipe_read(
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
            bray_platform_process_pipe_close(output),
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

    #[test]
    fn child_spawn_allows_read_only_input_aliasing() {
        let (program, arguments) = aliased_input_command();

        let program = NativeText::new(program);
        let argument = NativeText::new(arguments.0);
        let trailing_argument = NativeText::new(arguments.1);

        let argument_spans = [
            argument.span(),
            trailing_argument.span(),
            trailing_argument.span(),
        ];

        let request = NativePlatformChildRequest::new(
            program.span(),
            NativePlatformText::new(std::ptr::null(), 0),
            NativePlatformSpanList::new(argument_spans.as_ptr(), argument_spans.len() as u64),
            NativePlatformEnvironmentList::new(std::ptr::null(), 0),
            0,
            0,
            0,
        );

        let mut child = 0;
        let mut input = 0;
        let mut output = 0;
        let mut error = 0;

        assert_eq!(
            bray_platform_child_spawn(request, &mut child, &mut input, &mut output, &mut error,),
            NativePlatformStatus::SUCCESS,
        );

        assert_ne!(child, 0);

        assert_eq!(
            bray_platform_child_dispose(child),
            NativePlatformStatus::SUCCESS,
        );
    }

    #[test]
    fn child_spawn_rejects_overlapping_outputs_before_mutation() {
        let (program, _) = output_command();

        let program = NativeText::new(program);

        let request = NativePlatformChildRequest::new(
            program.span(),
            NativePlatformText::new(std::ptr::null(), 0),
            NativePlatformSpanList::new(std::ptr::null(), 0),
            NativePlatformEnvironmentList::new(std::ptr::null(), 0),
            0,
            0,
            0,
        );

        let mut output = 7_u64;
        let output_pointer = &mut output as *mut u64;

        assert_eq!(
            bray_platform_child_spawn(
                request,
                output_pointer,
                output_pointer,
                output_pointer,
                output_pointer,
            ),
            NativePlatformStatus::INVALID_INPUT,
        );

        assert_eq!(output, 7);
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

    #[cfg(windows)]
    fn aliased_input_command() -> (OsString, (OsString, OsString)) {
        let program = std::env::var_os("COMSPEC")
            .unwrap_or_else(|| OsString::from(r"C:\Windows\System32\cmd.exe"));

        (program, (OsString::from("/C"), OsString::from("exit")))
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

    #[cfg(unix)]
    fn aliased_input_command() -> (OsString, (OsString, OsString)) {
        (
            OsString::from("/bin/true"),
            (OsString::from("first"), OsString::from("repeated")),
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
