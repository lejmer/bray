use std::fs::File;
use std::io::Write as _;
use std::panic::panic_any;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const ALLOCATION_RECORD: u8 = 1;
const COPY_RECORD: u8 = 2;
const CONTROLLED_DURATION_RECORD: u8 = 3;

static STATE: OnceLock<Mutex<ObservationState>> = OnceLock::new();

#[derive(Debug)]
struct NativePerformanceObservationFailure;

struct ObservationState {
    output: File,
    event_count: u64,
    interval_started: Option<Instant>,
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_observation_begin_v1() {
        drop(state());
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_allocation_observation_v1(bytes: usize) {
        record(ALLOCATION_RECORD, as_u64(bytes));
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_memory_copy_observation_v1(bytes: usize) {
        record(COPY_RECORD, as_u64(bytes));
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_performance_interval_begin_v1() {
        let mut state = state();

        if state.interval_started.replace(Instant::now()).is_some() {
            panic_any(NativePerformanceObservationFailure);
        }
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_performance_interval_end_v1() {
        let ended = Instant::now();
        let mut state = state();
        let started = state
            .interval_started
            .take()
            .unwrap_or_else(|| panic_any(NativePerformanceObservationFailure));

        let nanoseconds = u64::try_from(ended.duration_since(started).as_nanos())
            .unwrap_or(u64::MAX);

        write_record(&mut state, CONTROLLED_DURATION_RECORD, nanoseconds);
    }
}

fn record(kind: u8, value: u64) {
    write_record(&mut state(), kind, value);
}

fn write_record(state: &mut ObservationState, kind: u8, value: u64) {
    if state.event_count >= bray_runtime_abi::MAX_PERFORMANCE_OBSERVATION_RECORDS {
        panic_any(NativePerformanceObservationFailure);
    }

    state.event_count += 1;

    let mut record = [0; 9];

    record[0] = kind;
    record[1..].copy_from_slice(&value.to_le_bytes());

    state
        .output
        .write_all(&record)
        .unwrap_or_else(|_| panic_any(NativePerformanceObservationFailure));
}

fn state() -> std::sync::MutexGuard<'static, ObservationState> {
    STATE
        .get_or_init(|| Mutex::new(open()))
        .lock()
        .unwrap_or_else(|_| panic_any(NativePerformanceObservationFailure))
}

fn open() -> ObservationState {
    let path = std::env::var_os(bray_runtime_abi::PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT)
        .unwrap_or_else(|| panic_any(NativePerformanceObservationFailure));

    let mut output =
        File::create(path).unwrap_or_else(|_| panic_any(NativePerformanceObservationFailure));

    output
        .write_all(&bray_runtime_abi::PERFORMANCE_OBSERVATION_HEADER)
        .unwrap_or_else(|_| panic_any(NativePerformanceObservationFailure));

    ObservationState {
        output,
        event_count: 0,
        interval_started: None,
    }
}

fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or_else(|_| panic_any(NativePerformanceObservationFailure))
}
