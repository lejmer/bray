use super::model::WorkloadCategory;

pub(super) const CORPUS_REVISION: u32 = 5;

pub(super) struct Workload {
    pub id: &'static str,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: &'static str,
    pub source: &'static str,
    pub standard_library_sources: &'static [&'static str],
    pub expected_output: ExpectedOutput,
    pub expected_side_effects: ExpectedSideEffects,
    pub platform_operations: &'static [&'static str],
    pub retention: RetentionContract,
    pub storage: Option<StorageExpectation>,
}

pub(super) struct RetentionContract {
    pub required_symbols: &'static [&'static str],
    pub forbidden_symbols: &'static [&'static str],
    pub required_provenance: &'static [&'static str],
    pub forbidden_provenance: &'static [&'static str],
}

const NO_RETENTION_CONTRACT: RetentionContract = RetentionContract {
    required_symbols: &[],
    forbidden_symbols: &[],
    required_provenance: &[],
    forbidden_provenance: &[],
};

#[derive(Clone, Copy)]
pub(super) enum ExpectedOutput {
    Empty,
    Repeated { byte: u8, count: u64 },
}

#[derive(Clone, Copy)]
pub(super) enum ExpectedSideEffects {
    None,
    AbsentPath(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StorageExpectation {
    pub allocation_count: u64,
    pub allocated_bytes: u64,
    pub copied_bytes: u64,
}

pub(super) const WORKLOADS: [Workload; 11] = [
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
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
    return Ok(unit);
}
"#,
        standard_library_sources: &[
            "standard-library/std/src/std.bray",
            "standard-library/std/src/memory.bray",
            "standard-library/std/src/bytes/buffer.bray",
        ],
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        storage: Some(StorageExpectation {
            allocation_count: 5,
            allocated_bytes: 124,
            copied_bytes: 60,
        }),
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
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
    return Ok(unit);
}
"#,
        standard_library_sources: &[
            "standard-library/std/src/std.bray",
            "standard-library/std/src/memory.bray",
            "standard-library/std/src/bytes/buffer.bray",
        ],
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
        storage: Some(StorageExpectation {
            allocation_count: 11,
            allocated_bytes: 8_188,
            copied_bytes: 4_092,
        }),
    },
    Workload {
        id: "borrowed_text",
        category: WorkloadCategory::CoreData,
        scale: 256,
        units: "text pipelines",
        source: r#"module borrowed_text;

using std.bytes;
using std.format;
using std.format.StringFormat;
using std.hash;
using std.hash.StableHasherSink;
using std.hash.StringHashable;
using std.numeric;
using std.string;

func parsed_literal() -> bool
{
    match consume std.numeric.parse_u64(&"42")
    {
        case Ok(value) { return value == 42; }
        case Error(_) { return false; }
    }
}

func main()
{
    let literal: string = "borrowed text";
    let duplicate: string = "borrowed text";
    let long: string = "Bray immutable text pipeline repeated across a deliberately long UTF-8 literal for stable throughput coverage.";
    let long_bytes: &[u8] = std.string.utf8(&long);
    let byte_count: usize = std.bytes.slice_length(long_bytes);
    let middle: &[u8] = &long_bytes[1.. byte_count - 1];

    let decoded: Result<string, std.string.Utf8Error> = std.string.from_utf8(std.string.utf8(&"owned text"));
    let owned: string = match consume decoded
    {
        case Ok(value) { yield value; }
        case Error(_) { assert(false); yield ""; }
    };

    let quoted: std.format.Options = std.format.Options(
        radix = std.format.Radix.Decimal,
        precision = 0,
        width = 0,
        alignment = std.format.Alignment.Left,
        sign = std.format.Sign.NegativeOnly,
        escaping = std.format.Escaping.Quoted,
    );

    let mut sink: std.format.ByteSink = match consume std.format.ByteSink(capacity = 0)
    {
        case Ok(value) { yield value; }
        case Error(_) { panic("text sink allocation failed"); }
    };
    let mut index: usize = 0;

    assert(std.bytes.slice_length(middle) == byte_count - 2);
    assert(std.string.equals(&literal, &duplicate));
    assert(std.string.equals(&owned, &"owned text"));
    assert(std.hash.stable_hash(&literal) == std.hash.stable_hash(&duplicate));
    assert(std.hash.stable_hash(&literal) != std.hash.stable_hash(&long));
    assert(parsed_literal());

    while index < 256
    {
        match consume std.format.write<string>(&mut sink, std.format.Argument<string>(&literal))
        {
            case Ok(_) {}
            case Error(_) { panic("raw text formatting failed"); }
        }

        index = index + 1;
    }

    match consume std.format.write<string>(
        &mut sink,
        std.format.Argument.with_options<string>(&long, options = quoted),
    )
    {
        case Ok(_) {}
        case Error(_) { panic("escaped text formatting failed"); }
    }

    assert(std.bytes.slice_length(std.format.bytes(&sink)) > 0);
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        storage: None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
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
        let formatted: u32 = value;

        {
            try write(&mut sink, Argument<u32>(&formatted));
        }

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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
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
        try std.io.print(&"x");
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[
            "platform.standard_output.write",
            "platform.standard_output.flush",
            "platform.standard_output.lock",
            "platform.standard_output.unlock",
        ],
        retention: RetentionContract {
            required_symbols: &[
                "bray_platform_standard_output_write",
                "bray_platform_standard_output_flush",
                "bray_platform_standard_output_lock",
                "bray_platform_standard_output_unlock",
            ],
            forbidden_symbols: &[
                "bray_platform_standard_input_read",
                "bray_platform_standard_error_write",
                "bray_platform_file_read",
                "bray_platform_file_write",
                "bray_platform_file_flush",
                "bray_platform_file_seek",
                "bray_platform_file_close",
                "bray_platform_process_pipe_read",
                "bray_platform_process_pipe_write",
                "bray_platform_process_pipe_flush",
                "bray_platform_process_pipe_close",
            ],
            required_provenance: &["bray_platform_standard_streams", "-output.o"],
            forbidden_provenance: &[
                "-input.o",
                "-error.o",
                "bray_platform_core",
                "bray_platform_filesystem",
                "bray_platform_process",
                "bray_runtime_test_host",
                "run_output_context",
            ],
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
        try await std.io.print_async(&"x");
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[
            "platform.standard_output.write",
            "platform.standard_output.flush",
            "platform.standard_output.lock",
            "platform.standard_output.unlock",
        ],
        retention: NO_RETENTION_CONTRACT,
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[
            "platform.context.measure",
            "platform.context.copy",
            "platform.path.metadata",
        ],
        retention: NO_RETENTION_CONTRACT,
        storage: None,
    },
    Workload {
        id: "file_output",
        category: WorkloadCategory::Filesystem,
        scale: 4096,
        units: "bytes",
        source: r#"module file_output;

using std.fs;
using std.io;
using std.path;

func main() -> Result<unit, std.io.IoError>
{
    let path: std.path.Path = output_path();

    match std.fs.remove_file(&path)
    {
        case Ok(_) {}
        case Error(_) {}
    }

    let options: std.fs.OpenOptions = std.fs.OpenOptions(
        access = std.fs.FileAccess.Write,
        creation = std.fs.FileCreation.CreateNew,
    );

    let mut file: std.fs.File = try std.fs.File.open(&path, options = options);
    let bytes: [u8; 4096] = [120; 4096];
    let mut written: usize = 0;

    while written < 4096
    {
        let remaining: &[u8] = &bytes[written.. 4096];

        written = written + try file.write(remaining);
    }

    try file.flush();
    try file.close();
    try std.fs.remove_file(&path);

    return Ok(unit);
}

func output_path() -> std.path.Path
{
    match std.path.Path.from_string(&"bray-performance-file-output")
    {
        case Ok(path) { return path; }
        case Error(_) { panic("performance output path must be valid"); }
    }
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::AbsentPath("bray-performance-file-output"),
        platform_operations: &[
            "platform.context.measure",
            "platform.context.copy",
            "platform.file.open",
            "platform.file.write",
            "platform.file.flush",
            "platform.file.close",
            "platform.path.remove_file",
        ],
        retention: RetentionContract {
            required_symbols: &[
                "bray_platform_context_measure",
                "bray_platform_context_copy",
                "bray_platform_file_open",
                "bray_platform_file_write",
                "bray_platform_file_flush",
                "bray_platform_file_close",
                "bray_platform_path_remove_file",
            ],
            forbidden_symbols: &[
                "bray_platform_standard_output_write",
                "bray_platform_standard_error_write",
                "bray_platform_process_pipe_read",
                "bray_platform_process_pipe_write",
                "bray_platform_process_pipe_flush",
                "bray_platform_process_pipe_close",
            ],
            required_provenance: &["bray_platform_core", "bray_platform_filesystem"],
            forbidden_provenance: &[
                "bray_platform_process",
                "bray_platform_standard_streams",
                "bray_runtime_test_host",
                "run_output_context",
            ],
        },
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &["platform.context.measure", "platform.context.copy"],
        retention: NO_RETENTION_CONTRACT,
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
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &["platform.clock.monotonic_now"],
        retention: NO_RETENTION_CONTRACT,
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
