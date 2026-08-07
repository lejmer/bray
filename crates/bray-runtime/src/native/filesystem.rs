use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use bray_runtime_interface::{
    NativePlatformFileMetadata, NativePlatformFileOptions, NativePlatformPath,
    NativePlatformStatus,
};

use super::platform::platform_io_error;

const FIRST_OWNED_HANDLE: u64 = 4;

macro_rules! native_platform_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the native filesystem provider requires a stable exported ABI and checked raw storage access"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}

enum NativeHandle {
    File(File),
    Directory(DirectoryTraversal),
}

struct DirectoryTraversal {
    entries: VecDeque<DirectoryEntry>,
    pending: Option<DirectoryEntry>,
}

struct DirectoryEntry {
    path: Vec<u8>,
    metadata: NativePlatformFileMetadata,
}

fn handles() -> &'static Mutex<BTreeMap<u64, Arc<Mutex<NativeHandle>>>> {
    static HANDLES: OnceLock<Mutex<BTreeMap<u64, Arc<Mutex<NativeHandle>>>>> = OnceLock::new();

    HANDLES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn insert_handle(handle: NativeHandle) -> Result<u64, NativePlatformStatus> {
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(FIRST_OWNED_HANDLE);

    let mut handles = handles().lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let mut handle = Some(handle);

    for _ in 0..128 {
        let id = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);

        if id == 0 || handles.contains_key(&id) {
            continue;
        }

        let Some(handle) = handle.take() else {
            return Err(NativePlatformStatus::OTHER);
        };

        handles.insert(id, Arc::new(Mutex::new(handle)));

        return Ok(id);
    }

    Err(NativePlatformStatus::EXHAUSTED)
}

fn handle(id: u64) -> Result<Arc<Mutex<NativeHandle>>, NativePlatformStatus> {
    handles()
        .lock()
        .map_err(|_| NativePlatformStatus::OTHER)?
        .get(&id)
        .cloned()
        .ok_or(NativePlatformStatus::INVALID_INPUT)
}

fn remove_handle(id: u64, expected: HandleKind) -> Result<NativeHandle, NativePlatformStatus> {
    let mut handles = handles().lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let handle = handles
        .get(&id)
        .ok_or(NativePlatformStatus::INVALID_INPUT)?;

    let matches = handle.lock().is_ok_and(|handle| expected.matches(&handle));

    if !matches {
        return Err(NativePlatformStatus::INVALID_INPUT);
    }

    let Some(handle) = handles.remove(&id) else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    match Arc::try_unwrap(handle) {
        Ok(handle) => handle
            .into_inner()
            .map_err(|_| NativePlatformStatus::OTHER),
        Err(handle) => {
            handles.insert(id, handle);

            Err(NativePlatformStatus::INVALID_INPUT)
        }
    }
}

#[derive(Clone, Copy)]
enum HandleKind {
    File,
    Directory,
}

impl HandleKind {
    const fn matches(self, handle: &NativeHandle) -> bool {
        matches!(
            (self, handle),
            (Self::File, NativeHandle::File(_))
                | (Self::Directory, NativeHandle::Directory(_))
        )
    }
}

pub(super) fn is_file_handle(id: u64) -> bool {
    handle(id).is_ok_and(|handle| {
        handle
            .lock()
            .is_ok_and(|handle| matches!(*handle, NativeHandle::File(_)))
    })
}

pub(super) fn read_file(id: u64, destination: &mut [u8]) -> Result<usize, NativePlatformStatus> {
    with_file(id, |file| file.read(destination))
}

pub(super) fn write_file(id: u64, source: &[u8]) -> Result<usize, NativePlatformStatus> {
    with_file(id, |file| file.write(source))
}

pub(super) fn flush_file(id: u64) -> Result<(), NativePlatformStatus> {
    with_file(id, File::flush)
}

pub(super) fn seek_file(
    id: u64,
    offset: i64,
    origin: u32,
) -> Result<u64, NativePlatformStatus> {
    let position = match origin {
        0 => SeekFrom::Start(
            u64::try_from(offset).map_err(|_| NativePlatformStatus::INVALID_INPUT)?,
        ),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return Err(NativePlatformStatus::INVALID_INPUT),
    };

    with_file(id, |file| file.seek(position))
}

pub(super) fn close_file(id: u64) -> NativePlatformStatus {
    match remove_handle(id, HandleKind::File) {
        Ok(NativeHandle::File(_)) => NativePlatformStatus::SUCCESS,
        Ok(NativeHandle::Directory(_)) => NativePlatformStatus::OTHER,
        Err(status) => status,
    }
}

fn with_file<T>(
    id: u64,
    operation: impl FnOnce(&mut File) -> io::Result<T>,
) -> Result<T, NativePlatformStatus> {
    let handle = handle(id)?;
    let mut handle = handle.lock().map_err(|_| NativePlatformStatus::OTHER)?;

    let NativeHandle::File(file) = &mut *handle else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    operation(file).map_err(|error| platform_io_error(&error))
}

native_platform_export! {
    pub extern "C" fn bray_platform_file_open_v1(
        path: NativePlatformPath,
        options: NativePlatformFileOptions,
        opened: *mut u64,
    ) -> NativePlatformStatus {
        if opened.is_null() || options.reserved() != 0 {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let path = match native_path(path) {
            Ok(path) => path,
            Err(status) => return status,
        };

        let mut open = OpenOptions::new();

        match options.access() {
            0 => {
                open.read(true);
            }
            1 => {
                open.write(true);
            }
            2 => {
                open.read(true).write(true);
            }
            3 => {
                open.append(true);
            }
            _ => return NativePlatformStatus::INVALID_INPUT,
        }

        match options.creation() {
            0 => {}
            1 => {
                open.create(true);
            }
            2 => {
                open.create_new(true);
            }
            3 if matches!(options.access(), 1..=3) => {
                open.truncate(true);
            }
            3 => return NativePlatformStatus::INVALID_INPUT,
            _ => return NativePlatformStatus::INVALID_INPUT,
        }

        let file = match open.open(path) {
            Ok(file) => file,
            Err(error) => return platform_io_error(&error),
        };

        let handle = match insert_handle(NativeHandle::File(file)) {
            Ok(handle) => handle,
            Err(status) => return status,
        };

        unsafe { opened.write(handle) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_file_metadata_v1(
        handle: u64,
        output: *mut NativePlatformFileMetadata,
    ) -> NativePlatformStatus {
        if output.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let metadata = match with_file(handle, |file| file.metadata()) {
            Ok(metadata) => metadata,
            Err(status) => return status,
        };

        unsafe { output.write(file_metadata(&metadata)) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_path_metadata_v1(
        path: NativePlatformPath,
        output: *mut NativePlatformFileMetadata,
    ) -> NativePlatformStatus {
        if output.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let path = match native_path(path) {
            Ok(path) => path,
            Err(status) => return status,
        };

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => return platform_io_error(&error),
        };

        unsafe { output.write(file_metadata(&metadata)) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_directory_open_v1(
        path: NativePlatformPath,
        opened: *mut u64,
    ) -> NativePlatformStatus {
        if opened.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let path = match native_path(path) {
            Ok(path) => path,
            Err(status) => return status,
        };

        let entries = match read_directory(path) {
            Ok(entries) => entries,
            Err(status) => return status,
        };

        let handle = match insert_handle(NativeHandle::Directory(DirectoryTraversal {
            entries,
            pending: None,
        })) {
            Ok(handle) => handle,
            Err(status) => return status,
        };

        unsafe { opened.write(handle) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_directory_next_v1(
        id: u64,
        destination: *mut u8,
        capacity: u64,
        written_or_required: *mut u64,
        end: *mut u32,
        metadata: *mut NativePlatformFileMetadata,
    ) -> NativePlatformStatus {
        if written_or_required.is_null() || end.is_null() || metadata.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let Ok(capacity) = usize::try_from(capacity) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if capacity != 0 && destination.is_null() {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let handle = match handle(id) {
            Ok(handle) => handle,
            Err(status) => return status,
        };

        let Ok(mut handle) = handle.lock() else {
            return NativePlatformStatus::OTHER;
        };

        let NativeHandle::Directory(directory) = &mut *handle else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if directory.pending.is_none() {
            let Some(entry) = directory.entries.pop_front() else {
                unsafe {
                    written_or_required.write(0);
                    end.write(1);
                }

                return NativePlatformStatus::SUCCESS;
            };

            directory.pending = Some(entry);
        }

        let Some(entry) = &directory.pending else {
            return NativePlatformStatus::OTHER;
        };

        let Ok(required) = u64::try_from(entry.path.len()) else {
            return NativePlatformStatus::EXHAUSTED;
        };

        unsafe { written_or_required.write(required) };

        if capacity < entry.path.len() {
            return NativePlatformStatus::INSUFFICIENT_BUFFER;
        }

        if !entry.path.is_empty() {
            unsafe {
                std::ptr::copy_nonoverlapping(entry.path.as_ptr(), destination, entry.path.len())
            };
        }

        unsafe {
            end.write(0);
            metadata.write(entry.metadata);
        }

        directory.pending = None;

        NativePlatformStatus::SUCCESS
    }
}

fn read_directory(path: PathBuf) -> Result<VecDeque<DirectoryEntry>, NativePlatformStatus> {
    let entries = fs::read_dir(path).map_err(|error| platform_io_error(&error))?;

    let mut entries = entries
        .map(|entry| {
            let entry = entry.map_err(|error| platform_io_error(&error))?;

            let metadata = entry
                .metadata()
                .map_err(|error| platform_io_error(&error))?;

            Ok(DirectoryEntry {
                path: native_text(entry.file_name()),
                metadata: file_metadata(&metadata),
            })
        })
        .collect::<Result<Vec<_>, NativePlatformStatus>>()?;

    entries.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(entries.into())
}

native_platform_export! {
    pub extern "C" fn bray_platform_directory_close_v1(id: u64) -> NativePlatformStatus {
        match remove_handle(id, HandleKind::Directory) {
            Ok(NativeHandle::Directory(_)) => NativePlatformStatus::SUCCESS,
            Ok(NativeHandle::File(_)) => NativePlatformStatus::OTHER,
            Err(status) => status,
        }
    }
}

macro_rules! path_operation {
    ($name:ident, $operation:path) => {
        native_platform_export! {
            pub extern "C" fn $name(path: NativePlatformPath) -> NativePlatformStatus {
                let path = match native_path(path) {
                    Ok(path) => path,
                    Err(status) => return status,
                };

                match $operation(path) {
                    Ok(()) => NativePlatformStatus::SUCCESS,
                    Err(error) => platform_io_error(&error),
                }
            }
        }
    };
}

path_operation!(bray_platform_path_create_directory_v1, fs::create_dir);
path_operation!(bray_platform_path_remove_file_v1, fs::remove_file);
path_operation!(bray_platform_path_remove_directory_v1, fs::remove_dir);

native_platform_export! {
    pub extern "C" fn bray_platform_path_rename_v1(
        source: NativePlatformPath,
        destination: NativePlatformPath,
    ) -> NativePlatformStatus {
        let source = match native_path(source) {
            Ok(path) => path,
            Err(status) => return status,
        };

        let destination = match native_path(destination) {
            Ok(path) => path,
            Err(status) => return status,
        };

        match fs::rename(source, destination) {
            Ok(()) => NativePlatformStatus::SUCCESS,
            Err(error) => platform_io_error(&error),
        }
    }
}

fn file_metadata(metadata: &Metadata) -> NativePlatformFileMetadata {
    let kind = if metadata.is_file() {
        0
    } else if metadata.is_dir() {
        1
    } else {
        2
    };

    let modified = metadata.modified().ok().and_then(system_time_parts);

    let (present, modified_seconds, modified_nanoseconds) = modified
        .map(|(seconds, nanoseconds)| (1, seconds, nanoseconds))
        .unwrap_or((0, 0, 0));

    NativePlatformFileMetadata::new(
        kind,
        present,
        metadata.len(),
        modified_seconds,
        modified_nanoseconds,
    )
}

fn system_time_parts(time: std::time::SystemTime) -> Option<(i64, u32)> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => Some((i64::try_from(duration.as_secs()).ok()?, duration.subsec_nanos())),
        Err(error) => {
            let duration = error.duration();
            let seconds = i64::try_from(duration.as_secs()).ok()?;
            let nanoseconds = duration.subsec_nanos();

            if nanoseconds == 0 {
                Some((-seconds, 0))
            } else {
                Some((seconds.checked_neg()?.checked_sub(1)?, 1_000_000_000 - nanoseconds))
            }
        }
    }
}

#[expect(
    unsafe_code,
    reason = "the checked native path boundary reads one call-lifetime ABI byte span"
)]
fn native_path(path: NativePlatformPath) -> Result<PathBuf, NativePlatformStatus> {
    let Ok(length) = usize::try_from(path.length()) else {
        return Err(NativePlatformStatus::INVALID_INPUT);
    };

    if length != 0 && path.address().is_null() {
        return Err(NativePlatformStatus::INVALID_INPUT);
    }

    let bytes = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(path.address(), length) }
    };

    native_path_from_bytes(bytes)
}

#[cfg(windows)]
fn native_path_from_bytes(bytes: &[u8]) -> Result<PathBuf, NativePlatformStatus> {
    use std::os::windows::ffi::OsStringExt;

    let chunks = bytes.chunks_exact(2);

    if !chunks.remainder().is_empty() {
        return Err(NativePlatformStatus::INVALID_INPUT);
    }

    let units = chunks
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();

    Ok(PathBuf::from(OsString::from_wide(&units)))
}

#[cfg(not(windows))]
fn native_path_from_bytes(bytes: &[u8]) -> Result<PathBuf, NativePlatformStatus> {
    use std::os::unix::ffi::OsStringExt;

    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(windows)]
fn native_text(value: OsString) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

#[cfg(not(windows))]
fn native_text(value: OsString) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    value.into_vec()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use bray_runtime_interface::{
        NativePlatformFileMetadata, NativePlatformFileOptions, NativePlatformPath,
        NativePlatformStatus,
    };
    use bray_testing::unique_temporary_directory;

    use super::{
        bray_platform_directory_close_v1, bray_platform_directory_next_v1,
        bray_platform_directory_open_v1, bray_platform_file_metadata_v1,
        bray_platform_file_open_v1, close_file, native_text, read_file, seek_file, write_file,
    };

    #[test]
    fn native_files_support_create_transfer_seek_metadata_and_close() {
        let directory = TestDirectory::new();
        let path = directory.path().join("data.bin");
        let path = NativePath::new(&path);
        let mut handle = 0;

        assert_eq!(
            bray_platform_file_open_v1(
                path.view(),
                NativePlatformFileOptions::new(2, 2),
                &mut handle,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(write_file(handle, b"bray"), Ok(4));
        assert_eq!(seek_file(handle, 0, 0), Ok(0));

        let mut bytes = [0; 4];

        assert_eq!(read_file(handle, &mut bytes), Ok(4));
        assert_eq!(&bytes, b"bray");

        let mut metadata = NativePlatformFileMetadata::new(2, 0, 0, 0, 0);

        assert_eq!(
            bray_platform_file_metadata_v1(handle, &mut metadata),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(metadata.kind(), 0);
        assert_eq!(metadata.bytes(), 4);
        assert_eq!(metadata.reserved(), 0);
        assert_eq!(close_file(handle), NativePlatformStatus::SUCCESS);
        assert_eq!(close_file(handle), NativePlatformStatus::INVALID_INPUT);
    }

    #[test]
    fn native_directory_traversal_is_sorted_and_retries_short_buffers() {
        let directory = TestDirectory::new();

        fs::write(directory.path().join("zeta"), []).unwrap_or_else(|error| {
            panic!("test file should be created: {error:?}");
        });

        fs::write(directory.path().join("alpha"), []).unwrap_or_else(|error| {
            panic!("test file should be created: {error:?}");
        });

        let path = NativePath::new(directory.path());
        let mut handle = 0;

        assert_eq!(
            bray_platform_directory_open_v1(path.view(), &mut handle),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(next_entry(handle), Some(native_name("alpha")));
        assert_eq!(next_entry(handle), Some(native_name("zeta")));
        assert_eq!(next_entry(handle), None);

        assert_eq!(
            bray_platform_directory_close_v1(handle),
            NativePlatformStatus::SUCCESS
        );
    }

    fn next_entry(handle: u64) -> Option<Vec<u8>> {
        let mut required = 0;
        let mut end = 0;
        let mut metadata = NativePlatformFileMetadata::new(2, 0, 0, 0, 0);

        let status = bray_platform_directory_next_v1(
            handle,
            std::ptr::null_mut(),
            0,
            &mut required,
            &mut end,
            &mut metadata,
        );

        if end == 1 {
            assert_eq!(status, NativePlatformStatus::SUCCESS);

            return None;
        }

        assert_eq!(status, NativePlatformStatus::INSUFFICIENT_BUFFER);

        let length = usize::try_from(required)
            .unwrap_or_else(|_| panic!("directory entry length must fit usize"));

        let mut bytes = vec![0; length];

        assert_eq!(
            bray_platform_directory_next_v1(
                handle,
                bytes.as_mut_ptr(),
                required,
                &mut required,
                &mut end,
                &mut metadata,
            ),
            NativePlatformStatus::SUCCESS
        );

        assert_eq!(end, 0);
        assert_eq!(metadata.kind(), 0);

        Some(bytes)
    }

    fn native_name(name: &str) -> Vec<u8> {
        native_text(name.into())
    }

    struct NativePath {
        bytes: Vec<u8>,
    }

    impl NativePath {
        fn new(path: &Path) -> Self {
            Self {
                bytes: native_text(path.as_os_str().to_os_string()),
            }
        }

        fn view(&self) -> NativePlatformPath {
            NativePlatformPath::new(
                self.bytes.as_ptr(),
                u64::try_from(self.bytes.len())
                    .unwrap_or_else(|_| panic!("test path length must fit u64")),
            )
        }
    }

    struct TestDirectory {
        path: std::path::PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let path = unique_temporary_directory();

            fs::create_dir(&path).unwrap_or_else(|error| {
                panic!("temporary test directory should be created: {error:?}");
            });

            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
