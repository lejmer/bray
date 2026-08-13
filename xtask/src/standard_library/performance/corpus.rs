use super::model::WorkloadCategory;

pub(super) const CORPUS_REVISION: u32 = 2;

pub(super) struct Workload {
    pub id: &'static str,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: &'static str,
    pub source: &'static str,
    pub standard_library_sources: &'static [&'static str],
    pub expected_output: ExpectedOutput,
    pub platform_operations: &'static [&'static str],
    pub retention: RetentionContract,
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
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
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
        platform_operations: &[
            "platform.standard_output.write",
            "platform.standard_output.flush",
            "platform.standard_output.lock",
            "platform.standard_output.unlock",
        ],
        retention: NO_RETENTION_CONTRACT,
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
        platform_operations: &[
            "platform.context.measure",
            "platform.context.copy",
            "platform.path.metadata",
        ],
        retention: NO_RETENTION_CONTRACT,
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
    match std.path.Path.from_string("bray-performance-file-output")
    {
        case Ok(path) { return path; }
        case Error(_) { panic("performance output path must be valid"); }
    }
}
"#,
        standard_library_sources: &[],
        expected_output: ExpectedOutput::Empty,
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
        platform_operations: &["platform.context.measure", "platform.context.copy"],
        retention: NO_RETENTION_CONTRACT,
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
        platform_operations: &["platform.clock.monotonic_now"],
        retention: NO_RETENTION_CONTRACT,
    },
];
