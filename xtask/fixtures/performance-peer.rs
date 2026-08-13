#[cfg(peer_workload = "async_output")]
use std::future::Future;
#[cfg(any(peer_timing, peer_workload = "file_output"))]
use std::fs::File;
#[cfg(peer_workload = "file_output")]
use std::fs::OpenOptions;
#[cfg(any(
    peer_timing,
    peer_workload = "async_output",
    peer_workload = "format_numbers",
    peer_workload = "stream_output",
    peer_workload = "file_output"
))]
use std::io::Write as _;
#[cfg(peer_workload = "async_output")]
use std::sync::Arc;
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

#[cfg(peer_workload = "borrowed_text")]
fn workload() -> bool {
    const LONG: &str = "Bray immutable text pipeline repeated across a deliberately long UTF-8 literal for stable throughput coverage.";

    let literal = std::hint::black_box("borrowed text");
    let duplicate = std::hint::black_box("borrowed text");
    let long = std::hint::black_box(LONG);
    let long_bytes = long.as_bytes();
    let middle = &long_bytes[1..long_bytes.len() - 1];

    let Ok(owned) = String::from_utf8(std::hint::black_box(b"owned text").to_vec()) else {
        return false;
    };

    let parsed = std::hint::black_box("42").parse::<u64>();
    let mut sink = Vec::new();

    for _ in 0..256 {
        if sink.try_reserve(literal.len()).is_err() {
            return false;
        }

        sink.extend_from_slice(literal.as_bytes());
    }

    let escaped_length = escaped_length(long);

    if sink.try_reserve(escaped_length).is_err() {
        return false;
    }

    sink.push(b'"');

    for byte in long.bytes() {
        if let Some(escaped) = escape_code(byte) {
            sink.extend_from_slice(&[b'\\', escaped]);
        } else {
            sink.push(byte);
        }
    }

    sink.push(b'"');

    std::hint::black_box(&sink);

    middle.len() == long_bytes.len() - 2
        && literal == duplicate
        && owned == "owned text"
        && stable_hash(literal) == stable_hash(duplicate)
        && stable_hash(literal) != stable_hash(long)
        && parsed == Ok(42)
        && sink.len() == literal.len() * 256 + escaped_length
}

#[cfg(peer_workload = "borrowed_text")]
fn escaped_length(value: &str) -> usize {
    value
        .bytes()
        .map(|byte| if escape_code(byte).is_some() { 2 } else { 1 })
        .sum::<usize>()
        + 2
}

#[cfg(peer_workload = "borrowed_text")]
const fn escape_code(value: u8) -> Option<u8> {
    match value {
        b'"' => Some(b'"'),
        b'\\' => Some(b'\\'),
        b'\n' => Some(b'n'),
        b'\r' => Some(b'r'),
        b'\t' => Some(b't'),
        _ => None,
    }
}

#[cfg(peer_workload = "borrowed_text")]
fn stable_hash(value: &str) -> u128 {
    const INITIAL: u128 = 14_695_981_039_346_656_037;
    const MULTIPLIER: u128 = 1_099_511_628_211;

    let mut state = multiply_modulo(add_modulo(INITIAL, value.len() as u128), MULTIPLIER);

    for byte in value.bytes() {
        state = multiply_modulo(add_modulo(state, u128::from(byte)), MULTIPLIER);
    }

    state
}

#[cfg(peer_workload = "borrowed_text")]
fn add_modulo(left: u128, right: u128) -> u128 {
    const MODULUS: u128 = 170_141_183_460_469_231_731_687_303_715_884_105_727;

    let reduced_left = left % MODULUS;
    let reduced_right = right % MODULUS;
    let remaining = MODULUS - reduced_right;

    if reduced_left >= remaining {
        reduced_left - remaining
    } else {
        reduced_left + reduced_right
    }
}

#[cfg(peer_workload = "borrowed_text")]
fn multiply_modulo(left: u128, right: u128) -> u128 {
    const MODULUS: u128 = 170_141_183_460_469_231_731_687_303_715_884_105_727;

    let mut factor = left % MODULUS;
    let mut multiplier = right;
    let mut product = 0;

    while multiplier > 0 {
        if multiplier % 2 == 1 {
            product = add_modulo(product, factor);
        }

        factor = add_modulo(factor, factor);
        multiplier /= 2;
    }

    product
}

#[cfg(peer_workload = "format_numbers")]
fn workload() -> bool {
    let mut bytes = Vec::new();

    for value in 0_u32..1_024 {
        let value = std::hint::black_box(value);

        if bytes.try_reserve(10).is_err() || write!(&mut bytes, "{value}").is_err() {
            return false;
        }
    }

    std::hint::black_box(&bytes);

    bytes.len() == 2_986
}

#[cfg(peer_workload = "stream_output")]
fn workload() -> bool {
    for _ in 0..1_024 {
        if !write_standard_output() {
            return false;
        }
    }

    true
}

#[cfg(any(peer_workload = "stream_output", peer_workload = "async_output"))]
fn write_standard_output() -> bool {
    let output = std::io::stdout();
    let mut locked = output.lock();

    locked.write_all(b"x").is_ok() && locked.flush().is_ok()
}

#[cfg(peer_workload = "async_output")]
fn workload() -> bool {
    block_on(async {
        for _ in 0..128 {
            if !write_standard_output_async().await {
                return false;
            }
        }

        true
    })
}

#[cfg(peer_workload = "async_output")]
async fn write_standard_output_async() -> bool {
    write_standard_output()
}

#[cfg(peer_workload = "async_output")]
fn block_on(future: impl Future<Output = bool>) -> bool {
    struct Wake;

    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {}
    }

    let waker = std::task::Waker::from(Arc::new(Wake));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::hint::spin_loop(),
        }
    }
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

#[cfg(peer_workload = "file_output")]
fn workload() -> bool {
    const PATH: &str = "bray-performance-file-output";

    let _ = std::fs::remove_file(PATH);

    let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(PATH) else {
        return false;
    };

    let bytes = [120_u8; 4_096];
    let mut written = 0;

    while written < bytes.len() {
        let Ok(count) = file.write(&bytes[written..]) else {
            return false;
        };

        if count == 0 {
            return false;
        }

        written += count;
    }

    if file.flush().is_err() {
        return false;
    }

    drop(file);

    std::fs::remove_file(PATH).is_ok()
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
