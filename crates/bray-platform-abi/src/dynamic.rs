use std::collections::BTreeMap;
use std::ffi::{CString, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bray_runtime_interface::{NativePlatformPath, NativePlatformStatus};
use libloading::Library;

use super::filesystem::native_path;
use super::region::{MemoryRegion, disjoint};

const FIRST_DYNAMIC_LIBRARY_HANDLE: u64 = 1;
const C_RUNTIME_IDENTITY: u32 = 0;

fn libraries() -> &'static Mutex<BTreeMap<u64, Arc<Library>>> {
    static LIBRARIES: OnceLock<Mutex<BTreeMap<u64, Arc<Library>>>> = OnceLock::new();

    LIBRARIES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn insert_library(library: Library) -> Result<u64, NativePlatformStatus> {
    static NEXT_HANDLE: AtomicU64 = AtomicU64::new(FIRST_DYNAMIC_LIBRARY_HANDLE);

    let mut libraries = libraries().lock().map_err(|_| NativePlatformStatus::OTHER)?;
    let mut library = Some(Arc::new(library));

    for _ in 0..128 {
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);

        if handle == 0 || libraries.contains_key(&handle) {
            continue;
        }

        let Some(library) = library.take() else {
            return Err(NativePlatformStatus::OTHER);
        };

        libraries.insert(handle, library);

        return Ok(handle);
    }

    Err(NativePlatformStatus::EXHAUSTED)
}

fn library(handle: u64) -> Result<Arc<Library>, NativePlatformStatus> {
    libraries()
        .lock()
        .map_err(|_| NativePlatformStatus::OTHER)?
        .get(&handle)
        .cloned()
        .ok_or(NativePlatformStatus::INVALID_INPUT)
}

fn remove_library(handle: u64) -> Result<Arc<Library>, NativePlatformStatus> {
    libraries()
        .lock()
        .map_err(|_| NativePlatformStatus::OTHER)?
        .remove(&handle)
        .ok_or(NativePlatformStatus::INVALID_INPUT)
}

native_platform_export! {
    pub extern "C" fn bray_platform_dynamic_library_open_path(
        path: NativePlatformPath,
        policy: u32,
        opened: *mut u64,
    ) -> NativePlatformStatus {
        let path = match native_path(path) {
            Ok(path) => path,
            Err(status) => return status,
        };

        open_library(policy, opened, || load_library(&path))
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_dynamic_library_open_system(
        identity: u32,
        policy: u32,
        opened: *mut u64,
    ) -> NativePlatformStatus {
        if identity != C_RUNTIME_IDENTITY {
            return NativePlatformStatus::INVALID_INPUT;
        }

        open_library(policy, opened, load_system_library)
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_dynamic_library_symbol(
        handle: u64,
        name: *const u8,
        length: u64,
        address: *mut *mut u8,
    ) -> NativePlatformStatus {
        let Ok(length) = usize::try_from(length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(name_region) = MemoryRegion::read(name, length) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        let Some(address_region) = MemoryRegion::write(address) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        if !disjoint(&[name_region, address_region]) || length == 0 {
            return NativePlatformStatus::INVALID_INPUT;
        }

        let name = unsafe { std::slice::from_raw_parts(name, length) };
        let Ok(name) = CString::new(name) else {
            return NativePlatformStatus::INVALID_INPUT;
        };

        unsafe { address.write(std::ptr::null_mut()) };

        let library = match library(handle) {
            Ok(library) => library,
            Err(status) => return status,
        };

        let resolved = match resolve_symbol(&library, &name) {
            Ok(resolved) => resolved,
            Err(status) => return status,
        };

        unsafe { address.write(resolved) };

        NativePlatformStatus::SUCCESS
    }
}

native_platform_export! {
    pub extern "C" fn bray_platform_dynamic_library_close(handle: u64) -> NativePlatformStatus {
        match remove_library(handle) {
            Ok(library) => {
                drop(library);

                NativePlatformStatus::SUCCESS
            }
            Err(status) => status,
        }
    }
}

#[expect(
    unsafe_code,
    reason = "the validated dynamic-loader boundary initializes and returns its output handle"
)]
fn open_library(
    policy: u32,
    opened: *mut u64,
    load: impl FnOnce() -> Result<Library, NativePlatformStatus>,
) -> NativePlatformStatus {
    let Some(_) = MemoryRegion::write(opened) else {
        return NativePlatformStatus::INVALID_INPUT;
    };

    unsafe { opened.write(0) };

    match policy {
        0 => {}
        1..=3 => return NativePlatformStatus::UNSUPPORTED,
        _ => return NativePlatformStatus::INVALID_INPUT,
    }

    let library = match load() {
        Ok(library) => library,
        Err(status) => return status,
    };

    let handle = match insert_library(library) {
        Ok(handle) => handle,
        Err(status) => return status,
    };

    unsafe { opened.write(handle) };

    NativePlatformStatus::SUCCESS
}

#[expect(
    unsafe_code,
    reason = "the validated dynamic-loader boundary transfers an explicit path to the native loader"
)]
fn load_library(path: &Path) -> Result<Library, NativePlatformStatus> {
    unsafe { Library::new(path) }.map_err(|_| NativePlatformStatus::NOT_FOUND)
}

#[expect(
    unsafe_code,
    reason = "the trusted symbol boundary resolves an exact name while retaining the library owner"
)]
fn resolve_symbol(library: &Library, name: &CString) -> Result<*mut u8, NativePlatformStatus> {
    let symbol = unsafe { library.get::<*mut c_void>(name.as_bytes_with_nul()) }
        .map_err(|_| NativePlatformStatus::NOT_FOUND)?;

    let address = (*symbol).cast::<u8>();

    if address.is_null() {
        return Err(NativePlatformStatus::OTHER);
    }

    Ok(address)
}

#[cfg(unix)]
fn load_system_library() -> Result<Library, NativePlatformStatus> {
    Ok(libloading::os::unix::Library::this().into())
}

#[cfg(windows)]
fn load_system_library() -> Result<Library, NativePlatformStatus> {
    libloading::os::windows::Library::open_already_loaded("ucrtbase.dll")
        .map(Into::into)
        .map_err(|_| NativePlatformStatus::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_runtime_interface::NativePlatformPath;

    use super::{
        bray_platform_dynamic_library_close, bray_platform_dynamic_library_open_path,
        bray_platform_dynamic_library_open_system, bray_platform_dynamic_library_symbol,
    };

    #[cfg(unix)]
    fn native_path_bytes(path: &Path) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;

        path.as_os_str().as_bytes().to_vec()
    }

    #[cfg(windows)]
    fn native_path_bytes(path: &Path) -> Vec<u8> {
        use std::os::windows::ffi::OsStrExt;

        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_ne_bytes)
            .collect()
    }

    #[test]
    fn c_runtime_library_resolves_exact_symbols_and_consumes_owners() {
        let mut handle = 0;

        assert_eq!(
            bray_platform_dynamic_library_open_system(0, 0, &mut handle).category(),
            0
        );

        let mut address = std::ptr::null_mut();
        let name = b"strlen";

        assert_eq!(
            bray_platform_dynamic_library_symbol(
                handle,
                name.as_ptr(),
                name.len() as u64,
                &mut address,
            )
            .category(),
            0
        );

        assert!(!address.is_null());
        assert_eq!(bray_platform_dynamic_library_close(handle).category(), 0);
        assert_eq!(bray_platform_dynamic_library_close(handle).category(), 5);
    }

    #[test]
    fn loader_rejects_unknown_identities_policies_and_names() {
        let mut handle = 0;

        assert_eq!(
            bray_platform_dynamic_library_open_system(1, 0, &mut handle).category(),
            5
        );

        handle = 41;

        assert_eq!(
            bray_platform_dynamic_library_open_system(0, 1, &mut handle).category(),
            1
        );

        assert_eq!(handle, 0);

        let path = std::env::temp_dir().join("bray-dynamic-library-that-does-not-exist");
        let bytes = native_path_bytes(&path);
        let path = NativePlatformPath::new(bytes.as_ptr(), bytes.len() as u64);

        assert_eq!(
            bray_platform_dynamic_library_open_path(path, 0, &mut handle).category(),
            3
        );

        assert_eq!(handle, 0);

        let mut address = std::ptr::null_mut();

        assert_eq!(
            bray_platform_dynamic_library_symbol(0, std::ptr::null(), 0, &mut address).category(),
            5
        );
    }
}
