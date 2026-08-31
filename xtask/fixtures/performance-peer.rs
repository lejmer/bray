#[cfg(peer_workload = "async_output")]
use std::future::Future;
#[cfg(any(peer_timing, peer_workload = "file_output"))]
use std::fs::File;
#[cfg(peer_workload = "file_output")]
use std::fs::OpenOptions;
#[cfg(any(
    peer_timing,
    peer_workload = "async_output",
    peer_workload = "captured_output",
    peer_workload = "format_large_width",
    peer_workload = "format_numbers",
    peer_workload = "format_writer",
    peer_workload = "stream_output",
    peer_workload = "contended_output",
    peer_workload = "file_output",
    peer_workload = "process_pipe_transfer"
))]
use std::io::Write as _;
#[cfg(peer_workload = "process_pipe_transfer")]
use std::io::Read as _;
#[cfg(peer_workload = "process_pipe_transfer")]
use std::process::{Command, Stdio};
#[cfg(peer_workload = "deque_mixed_ends")]
use std::collections::VecDeque;
#[cfg(any(
    peer_workload = "hash_map_collision_lookup",
    peer_workload = "hash_map_growth_and_healthy_lookup"
))]
use std::collections::HashMap;
#[cfg(peer_workload = "hash_map_collision_lookup")]
use std::hash::{BuildHasherDefault, Hasher};
#[cfg(peer_workload = "async_output")]
use std::sync::Arc;
#[cfg(any(peer_timing, peer_workload = "monotonic_clock"))]
use std::time::Instant;

#[cfg(peer_timing)]
const OBSERVATION_HEADER: &[u8] = b"BRAYPO01";
#[cfg(peer_timing)]
const CONTROLLED_DURATION_RECORD: u8 = 3;
#[cfg(peer_timing)]
const OBSERVATION_PATH: &str = "BRAY_PERFORMANCE_OBSERVATION_PATH";

fn main() {
    #[cfg(peer_workload = "process_pipe_transfer")]
    if std::env::args_os().len() > 1 {
        let mut bytes = [0_u8; 4_096];

        std::io::stdin()
            .read_exact(&mut bytes)
            .unwrap_or_else(|error| panic!("pipe child must read: {error}"));

        return;
    }

    #[cfg(peer_timing)]
    let inner_iterations = inner_iterations();

    #[cfg(peer_timing)]
    let started = Instant::now();

    #[cfg(peer_timing)]
    let valid = {
        let mut valid = true;

        for _ in 0..inner_iterations {
            valid &= std::hint::black_box(workload());
        }

        valid
    };

    #[cfg(not(peer_timing))]
    let valid = workload();

    #[cfg(peer_timing)]
    write_duration(started.elapsed().as_nanos());

    assert!(valid);
}

#[cfg(peer_timing)]
fn inner_iterations() -> u64 {
    match env!("BRAY_PERFORMANCE_INNER_ITERATIONS").parse() {
        Ok(value) if value > 0 => value,
        _ => std::process::abort(),
    }
}

#[cfg(peer_workload = "small_output")]
fn workload() -> bool {
    true
}

#[cfg(peer_workload = "hash_map_growth_and_healthy_lookup")]
fn workload() -> bool {
    let mut map = HashMap::new();

    for key in 0_u64..4_096 {
        map.insert(key, key);
    }

    for key in 0_u64..4_096 {
        if !map.contains_key(&key) {
            return false;
        }
    }

    std::hint::black_box(&map);

    map.len() == 4_096
}

#[cfg(peer_workload = "hash_map_collision_lookup")]
#[derive(Default)]
struct CollisionHasher;

#[cfg(peer_workload = "hash_map_collision_lookup")]
impl Hasher for CollisionHasher {
    fn finish(&self) -> u64 {
        0
    }

    fn write(&mut self, _bytes: &[u8]) {}
}

#[cfg(peer_workload = "hash_map_collision_lookup")]
fn workload() -> bool {
    let mut map = HashMap::with_capacity_and_hasher(
        48,
        BuildHasherDefault::<CollisionHasher>::default(),
    );

    for key in 0_u64..48 {
        map.insert(key, key);
    }

    for lookup in 0_usize..4_096 {
        let key = (lookup % 48) as u64;

        if !map.contains_key(&key) {
            return false;
        }
    }

    std::hint::black_box(&map);

    map.len() == 48
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

#[cfg(peer_workload = "deque_mixed_ends")]
fn workload() -> bool {
    let mut values = VecDeque::with_capacity(64);

    values.extend(0_u64..64);

    for expected in 0_u64..32 {
        if values.pop_front() != Some(expected) {
            return false;
        }
    }

    values.extend(64_u64..96);

    for round in 0_usize..2_048 {
        if round % 2 == 0 {
            let Some(value) = values.pop_front() else {
                return false;
            };

            values.push_back(value);
        } else {
            let Some(value) = values.pop_back() else {
                return false;
            };

            values.push_front(value);
        }
    }

    values.push_back(96);
    std::hint::black_box(&values);

    values.len() == 65 && values.front() == Some(&32) && values.back() == Some(&96)
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
    let mut bytes = Vec::with_capacity(2_986);

    for value in 0_u32..1_024 {
        let value = std::hint::black_box(value);

        if write!(&mut bytes, "{value}").is_err() {
            return false;
        }
    }

    std::hint::black_box(&bytes);

    bytes.len() == 2_986
}

#[cfg(peer_workload = "format_large_width")]
fn workload() -> bool {
    let mut bytes = Vec::with_capacity(133_120);

    for _ in 0..1_024 {
        let value = std::hint::black_box(42_u32);

        if write!(&mut bytes, "{value:>130}").is_err() {
            return false;
        }
    }

    std::hint::black_box(&bytes);

    bytes.len() == 133_120
        && bytes.chunks_exact(130).all(|formatted| {
            formatted[..128].iter().all(|byte| *byte == b' ')
                && formatted[128..] == *b"42"
        })
}

#[cfg(peer_workload = "format_writer")]
struct ValidatingWriter {
    length: usize,
    valid: bool,
}

#[cfg(peer_workload = "format_writer")]
impl std::io::Write for ValidatingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for byte in bytes {
            let expected = if self.length % 2 == 0 { b'4' } else { b'2' };

            if *byte != expected {
                self.valid = false;
            }

            self.length += 1;
        }

        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(peer_workload = "format_writer")]
fn workload() -> bool {
    let mut writer = ValidatingWriter {
        length: 0,
        valid: true,
    };

    for _ in 0..1_024 {
        let value = std::hint::black_box(42_u32);

        if write!(&mut writer, "{value}").is_err() {
            return false;
        }
    }

    std::hint::black_box(&writer);

    writer.valid && writer.length == 2_048
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

#[cfg(any(
    peer_workload = "stream_output",
    peer_workload = "async_output",
    peer_workload = "contended_output"
))]
fn write_standard_output() -> bool {
    let output = std::io::stdout();
    let mut locked = output.lock();

    locked.write_all(b"x").is_ok() && locked.flush().is_ok()
}

#[cfg(peer_workload = "contended_output")]
fn workload() -> bool {
    std::thread::scope(|scope| {
        let first = scope.spawn(write_standard_output_repeatedly);
        let second = scope.spawn(write_standard_output_repeatedly);

        first.join().unwrap_or(false) && second.join().unwrap_or(false)
    })
}

#[cfg(peer_workload = "contended_output")]
fn write_standard_output_repeatedly() -> bool {
    (0..64).all(|_| write_standard_output())
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

#[cfg(peer_workload = "captured_output")]
fn workload() -> bool {
    let mut output = Vec::with_capacity(4_096);
    let bytes = [120_u8; 4_096];

    output.write_all(&bytes).is_ok() && output.len() == bytes.len()
}

#[cfg(peer_workload = "process_pipe_transfer")]
fn workload() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };

    let Ok(mut child) = Command::new(executable)
        .arg("pipe-child")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    let Some(mut input) = child.stdin.take() else {
        return false;
    };

    let bytes = [120_u8; 4_096];

    if input.write_all(&bytes).is_err() {
        return false;
    }

    drop(input);

    child.wait().is_ok_and(|status| status.success())
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
