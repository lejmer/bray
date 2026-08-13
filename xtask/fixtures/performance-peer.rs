#[cfg(peer_timing)]
use std::fs::File;
#[cfg(peer_timing)]
use std::io::Write as _;
#[cfg(any(peer_timing, peer_workload = "monotonic_clock"))]
use std::time::Instant;

const OBSERVATION_HEADER: &[u8] = b"BRAYPO01";
const CONTROLLED_DURATION_RECORD: u8 = 3;
const OBSERVATION_PATH: &str = "BRAY_PERFORMANCE_OBSERVATION_PATH";

fn main() {
    #[cfg(peer_timing)]
    let started = Instant::now();

    let valid = workload();

    #[cfg(peer_timing)]
    write_duration(started.elapsed().as_nanos());

    assert!(valid);
}

#[cfg(peer_workload = "small_output")]
fn workload() -> bool {
    true
}

#[cfg(peer_workload = "incremental_bytes_small")]
fn workload() -> bool {
    incremental_bytes(64)
}

#[cfg(peer_workload = "incremental_bytes")]
fn workload() -> bool {
    incremental_bytes(4_096)
}

#[cfg(any(
    peer_workload = "incremental_bytes_small",
    peer_workload = "incremental_bytes"
))]
fn incremental_bytes(count: usize) -> bool {
    let mut bytes = Vec::new();

    for _ in 0..count {
        if bytes.try_reserve(1).is_err() {
            return false;
        }

        bytes.push(65_u8);
    }

    std::hint::black_box(&bytes);

    bytes.len() == count
}

#[cfg(peer_workload = "filesystem_metadata")]
fn workload() -> bool {
    let Ok(path) = std::env::current_dir() else {
        return false;
    };

    for _ in 0..256 {
        let Ok(metadata) = std::fs::metadata(&path) else {
            return false;
        };

        std::hint::black_box(metadata);
    }

    true
}

#[cfg(peer_workload = "process_context")]
fn workload() -> bool {
    for _ in 0..1_024 {
        std::hint::black_box(std::process::id());
    }

    true
}

#[cfg(peer_workload = "monotonic_clock")]
fn workload() -> bool {
    for _ in 0..1_024 {
        std::hint::black_box(Instant::now());
    }

    true
}

#[cfg(peer_timing)]
fn write_duration(nanoseconds: u128) {
    let path = std::env::var_os(OBSERVATION_PATH)
        .unwrap_or_else(|| panic!("performance observation path is required"));

    let duration = u64::try_from(nanoseconds).unwrap_or(u64::MAX);

    let mut output = File::create(path)
        .unwrap_or_else(|error| panic!("performance observation must open: {error}"));

    output
        .write_all(OBSERVATION_HEADER)
        .unwrap_or_else(|error| panic!("performance observation header must write: {error}"));

    output
        .write_all(&[CONTROLLED_DURATION_RECORD])
        .unwrap_or_else(|error| panic!("performance observation kind must write: {error}"));

    output
        .write_all(&duration.to_le_bytes())
        .unwrap_or_else(|error| panic!("performance observation duration must write: {error}"));
}
