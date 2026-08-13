use std::fs::File;
use std::io::Write as _;
use std::panic::panic_any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const ALLOCATION_RECORD: u8 = 1;
const COPY_RECORD: u8 = 2;

static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);
static OUTPUT: OnceLock<Mutex<File>> = OnceLock::new();

#[derive(Debug)]
struct NativeMemoryObservationFailure;

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_allocation_observation_v1(bytes: usize) {
        record(ALLOCATION_RECORD, bytes);
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_copy_observation_v1(bytes: usize) {
        record(COPY_RECORD, bytes);
    }
}

fn record(kind: u8, value: usize) {
    let event = EVENT_COUNT.fetch_add(1, Ordering::Relaxed);

    if event >= bray_runtime_abi::MAX_MEMORY_OBSERVATION_RECORDS {
        panic_any(NativeMemoryObservationFailure);
    }

    let output = OUTPUT.get_or_init(|| {
        let path = std::env::var_os(bray_runtime_abi::MEMORY_OBSERVATION_PATH_ENVIRONMENT)
            .unwrap_or_else(|| panic_any(NativeMemoryObservationFailure));

        let mut file = File::create(path)
            .unwrap_or_else(|_| panic_any(NativeMemoryObservationFailure));

        file.write_all(&bray_runtime_abi::MEMORY_OBSERVATION_HEADER)
            .unwrap_or_else(|_| panic_any(NativeMemoryObservationFailure));

        Mutex::new(file)
    });

    let value = u64::try_from(value).unwrap_or_else(|_| panic_any(NativeMemoryObservationFailure));
    let mut record = [0; 9];

    record[0] = kind;
    record[1..].copy_from_slice(&value.to_le_bytes());

    output
        .lock()
        .unwrap_or_else(|_| panic_any(NativeMemoryObservationFailure))
        .write_all(&record)
        .unwrap_or_else(|_| panic_any(NativeMemoryObservationFailure));
}
