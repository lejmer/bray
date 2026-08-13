use super::model::WorkloadCategory;

pub(super) const CORPUS_REVISION: u32 = 3;

pub(super) struct Workload {
    pub id: &'static str,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: &'static str,
    pub source: &'static str,
    pub standard_library_sources: &'static [&'static str],
    pub expected_output: ExpectedOutput,
    pub storage: Option<StorageExpectation>,
}

#[derive(Clone, Copy)]
pub(super) enum ExpectedOutput {
    Empty,
    Repeated { byte: u8, count: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StorageExpectation {
    pub allocation_count: u64,
    pub allocated_bytes: u64,
    pub copied_bytes: u64,
}

pub(super) const WORKLOADS: [Workload; 9] = [
    Workload {
        id: "small_output",
        category: WorkloadCategory::Small,
        scale: 1,
        units: "executions",
        source: r#"module small_output;

func main() {}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        storage: None,
    },
    Workload {
        id: "incremental_bytes_small",
        category: WorkloadCategory::CoreData,
        scale: 64,
        units: "bytes",
        source: r#"module std.bytes;

using std.bytes;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut buffer: Buffer = try Buffer(capacity = 0);
    let mut index: usize = 0;

    while index < 64
    {
        try push(&mut buffer, value = 65);
        index = index + 1;
    }

    assert(length(&buffer) == 64);
    assert(capacity(&buffer) == 64);
    return Ok(unit);
}
"#,
        standard_library_sources: &[
            "standard-library/std/src/std.bray",
            "standard-library/std/src/memory.bray",
            "standard-library/std/src/bytes/buffer.bray",
        ],
        expected_output: ExpectedOutput::Empty,
        storage: Some(StorageExpectation {
            allocation_count: 5,
            allocated_bytes: 124,
            copied_bytes: 60,
        }),
    },
    Workload {
        id: "incremental_bytes",
        category: WorkloadCategory::CoreData,
        scale: 4096,
        units: "bytes",
        source: r#"module std.bytes;

using std.bytes;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut buffer: Buffer = try Buffer(capacity = 0);
    let mut index: usize = 0;

    while index < 4096
    {
        try push(&mut buffer, value = 65);
        index = index + 1;
    }

    assert(length(&buffer) == 4096);
    assert(capacity(&buffer) == 4096);
    return Ok(unit);
}
"#,
        standard_library_sources: &[
            "standard-library/std/src/std.bray",
            "standard-library/std/src/memory.bray",
            "standard-library/std/src/bytes/buffer.bray",
        ],
        expected_output: ExpectedOutput::Empty,
        storage: Some(StorageExpectation {
            allocation_count: 11,
            allocated_bytes: 8_188,
            copied_bytes: 4_092,
        }),
    },
    Workload {
        id: "format_numbers",
        category: WorkloadCategory::Formatting,
        scale: 1024,
        units: "values",
        source: r#"module std.format;

using std.format;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut sink: ByteSink = try ByteSink(capacity = 0);
    let mut value: u32 = 0;

    while value < 1024
    {
        try write(&mut sink, Argument<u32>(value));
        value = value + 1;
    }

    assert(std.bytes.slice_length(bytes(&sink)) > 0);
    return Ok(unit);
}
"#,
        standard_library_sources: &[
            "standard-library/std/src/std.bray",
            "standard-library/std/src/memory.bray",
            "standard-library/std/src/bytes/buffer.bray",
            "standard-library/std/src/string.bray",
            "standard-library/std/src/character.bray",
            "standard-library/std/src/format/options.bray",
            "standard-library/std/src/format/argument.bray",
            "standard-library/std/src/format/sink.bray",
            "standard-library/std/src/format/rendering.bray",
        ],
        expected_output: ExpectedOutput::Empty,
        storage: None,
    },
    Workload {
        id: "stream_output",
        category: WorkloadCategory::Streaming,
        scale: 1024,
        units: "writes",
        source: r#"module stream_output;

using std.io;

func main() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 1024
    {
        try std.io.print("x");
        index = index + 1;
    }

    return Ok(unit);
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Repeated {
            byte: b'x',
            count: 1024,
        },
        storage: None,
    },
    Workload {
        id: "async_output",
        category: WorkloadCategory::Concurrent,
        scale: 128,
        units: "awaits",
        source: r#"module async_output;

using std.io;

async func main() -> Result<unit, std.io.IoError>
{
    let mut index: usize = 0;

    while index < 128
    {
        try await std.io.print_async("x");
        index = index + 1;
    }

    return Ok(unit);
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Repeated {
            byte: b'x',
            count: 128,
        },
        storage: None,
    },
    Workload {
        id: "filesystem_metadata",
        category: WorkloadCategory::Filesystem,
        scale: 256,
        units: "lookups",
        source: r#"module filesystem_metadata;

using std.fs;
using std.io;
using std.path;
using std.process;

func main() -> Result<unit, std.io.IoError>
{
    let path: std.path.Path = std.process.current_directory();
    let mut index: usize = 0;

    while index < 256
    {
        let _: std.fs.FileMetadata = try std.fs.metadata(&path);
        index = index + 1;
    }

    return Ok(unit);
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        storage: None,
    },
    Workload {
        id: "process_context",
        category: WorkloadCategory::Process,
        scale: 1024,
        units: "lookups",
        source: r#"module process_context;

using std.process;

func main()
{
    let mut index: usize = 0;

    while index < 1024
    {
        let _: std.process.Id = std.process.current_id();
        index = index + 1;
    }
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        storage: None,
    },
    Workload {
        id: "monotonic_clock",
        category: WorkloadCategory::Time,
        scale: 1024,
        units: "readings",
        source: r#"module monotonic_clock;

using std.time;

func main() -> Result<unit, std.time.ClockError>
{
    let mut index: usize = 0;

    while index < 1024
    {
        let _: std.time.Instant = try std.time.monotonic_now();
        index = index + 1;
    }

    return Ok(unit);
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        storage: None,
    },
];

#[cfg(test)]
mod tests {
    use super::WORKLOADS;

    #[test]
    fn incremental_growth_scales_logarithmic_allocations_and_linear_transfer_work() {
        let small = storage("incremental_bytes_small");
        let large = storage("incremental_bytes");

        let additional_allocations = large.allocation_count - small.allocation_count;

        assert_eq!(1_u64 << additional_allocations, 4096 / 64);
        assert_eq!(large.allocated_bytes + 4, (small.allocated_bytes + 4) * 64);
        assert_eq!(large.copied_bytes + 4, (small.copied_bytes + 4) * 64);
    }

    fn storage(identity: &str) -> super::StorageExpectation {
        WORKLOADS
            .iter()
            .find(|workload| workload.id == identity)
            .and_then(|workload| workload.storage)
            .unwrap_or_else(|| panic!("{identity} must define storage work"))
    }
}
