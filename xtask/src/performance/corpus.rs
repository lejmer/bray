use super::model::WorkloadCategory;

pub(super) const CORPUS_REVISION: u32 = 1;
pub(super) const CALIBRATION_SEED_INNER_ITERATIONS: u64 = 1_000_000;
pub(super) const CALIBRATION_SAMPLE_COUNT: u32 = 3;
pub(super) const CALIBRATION_TARGET_NANOSECONDS: u64 = 100_000_000;

pub(super) struct Workload {
    pub id: &'static str,
    pub category: WorkloadCategory,
    pub scale: u64,
    pub units: &'static str,
    pub batching: BatchingPolicy,
    pub source: &'static str,
    pub expected_output: ExpectedOutput,
    pub expected_side_effects: ExpectedSideEffects,
    pub platform_operations: &'static [&'static str],
    pub retention: RetentionContract,
    pub storage: Option<StorageExpectation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BatchingPolicy {
    SingleExecution,
    Calibrated,
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

pub(super) const WORKLOADS: [Workload; 16] = [
    Workload {
        id: "small_output",
        category: WorkloadCategory::Small,
        scale: 1,
        units: "executions",
        batching: BatchingPolicy::Calibrated,
        source: r#"module small_output;

func main() {}
"#,
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
        batching: BatchingPolicy::SingleExecution,
        source: r#"module incremental_bytes_small;

using std.bytes;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut buffer: std.bytes.Buffer = try std.bytes.Buffer(capacity = 0);
    let mut index: usize = 0;

    while index < 64
    {
        try trusted std.bytes.push(&mut buffer, value = 65);
        index += 1;
    }

    assert(std.bytes.length(&buffer) == 64);
    return Ok(unit);
}
"#,
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
        batching: BatchingPolicy::SingleExecution,
        source: r#"module incremental_bytes;

using std.bytes;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut buffer: std.bytes.Buffer = try std.bytes.Buffer(capacity = 0);
    let mut index: usize = 0;

    while index < 4096
    {
        try trusted std.bytes.push(&mut buffer, value = 65);
        index += 1;
    }

    assert(std.bytes.length(&buffer) == 4096);
    return Ok(unit);
}
"#,
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
        batching: BatchingPolicy::SingleExecution,
        source: r#"module borrowed_text;

using std.bytes;
using std.format;
using std.format.ByteSinkFormatting;
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
    requires(blocking_execution())
{
    let literal: string = "borrowed text";
    let duplicate: string = "borrowed text";

    let long: string =
        "Bray immutable text pipeline repeated across a deliberately long UTF-8 literal for stable throughput coverage.";

    let long_bytes: &[u8] = std.string.utf8(&long);
    let byte_count: usize = std.bytes.slice_length(long_bytes);
    let middle: &[u8] = &long_bytes[1.. byte_count - 1];

    let decoded: Result<string, std.string.Utf8Error> = std.string.from_utf8(std.string.utf8(&"owned text"));

    let owned: string = match consume decoded
    {
        case Ok(value) { yield value; }
        case Error(_)
        {
            assert(false);
            yield "";
        }
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
        match consume trusted std.format.write<string>(&mut sink, std.format.Argument<string>(&literal))
        {
            case Ok(_) {}
            case Error(_) { panic("raw text formatting failed"); }
        }

        index += 1;
    }

    match consume trusted std.format.write<string>(
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
        batching: BatchingPolicy::SingleExecution,
        source: r#"module format_numbers;

using std.bytes;
using std.format;
using std.format.ByteSinkFormatting;
using std.format.U32Format;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
    requires(blocking_execution())
{
    let mut sink: std.format.ByteSink = try std.format.ByteSink(capacity = 2986);
    let mut value: u32 = 0;

    while value < 1024
    {
        let formatted: u32 = value;

        {
            try trusted std.format.write<u32>(&mut sink, std.format.Argument<u32>(&formatted));
        }

        value += 1;
    }

    assert(std.bytes.slice_length(std.format.bytes(&sink)) == 2986);

    return Ok(unit);
}
"#,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
        storage: Some(StorageExpectation {
            allocation_count: 1,
            allocated_bytes: 2_986,
            copied_bytes: 2_986,
        }),
    },
    Workload {
        id: "format_large_width",
        category: WorkloadCategory::Formatting,
        scale: 1024,
        units: "values",
        batching: BatchingPolicy::SingleExecution,
        source: r#"module format_large_width;

using std.bytes;
using std.format;
using std.format.ByteSinkFormatting;
using std.format.U32Format;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
    requires(blocking_execution())
{
    let mut sink: std.format.ByteSink = try std.format.ByteSink(capacity = 133120);
    let value: u32 = 42;
    let mut formatted: usize = 0;

    while formatted < 1024
    {
        let options: std.format.Options = std.format.Options(
            radix = std.format.Radix.Decimal,
            precision = 0,
            width = 130,
            alignment = std.format.Alignment.Right,
            sign = std.format.Sign.NegativeOnly,
            escaping = std.format.Escaping.Raw,
        );

        try trusted std.format.write<u32>(&mut sink, std.format.Argument.with_options<u32>(&value, options = options));
        formatted += 1;
    }

    let output: &[u8] = std.format.bytes(&sink);
    let length: usize = std.bytes.slice_length(output);
    let mut index: usize = 0;

    assert(length == 133120);

    while index < length
    {
        let offset: usize = index % 130;

        if offset < 128
        {
            assert(output[index] == 32);
        }
        else if offset == 128
        {
            assert(output[index] == 52);
        }
        else
        {
            assert(output[index] == 50);
        }

        index += 1;
    }

    return Ok(unit);
}
"#,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
        storage: Some(StorageExpectation {
            allocation_count: 1,
            allocated_bytes: 133_120,
            copied_bytes: 2_048,
        }),
    },
    Workload {
        id: "format_writer",
        category: WorkloadCategory::Formatting,
        scale: 1024,
        units: "values",
        batching: BatchingPolicy::SingleExecution,
        source: r#"module format_writer;

using std.bytes;
using std.format;
using std.format.U32Format;
using std.io;
using std.io.WriterFormattingSink;

struct ValidatingWriter
{
    mut length: usize;
    mut valid: bool;

    construct() -> Self
    {
        return
        {
            length = 0,
            valid = true,
        };
    }
}

impl ValidatingWriterIo = ValidatingWriter(std.io.Writer)
{
    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution())
    {
        let count: usize = std.bytes.slice_length(source);
        let mut index: usize = 0;

        while index < count
        {
            let absolute: usize = self.length + index;

            if absolute % 2 == 0
            {
                if source[index] != 52
                {
                    self.valid = false;
                }
            }
            else if source[index] != 50
            {
                self.valid = false;
            }

            index += 1;
        }

        self.length += count;

        return Ok(count);
    }

    mut func flush() -> Result<unit, std.io.IoError>
        requires(blocking_execution())
    {
        return Ok(unit);
    }
}

func main()
    requires(blocking_execution())
{
    let mut writer: ValidatingWriter = ValidatingWriter();

    let mut sink: std.io.FormattingSink<ValidatingWriter> = std.io.FormattingSink<ValidatingWriter>(&mut writer);

    let value: u32 = 42;
    let mut formatted: usize = 0;

    while formatted < 1024
    {
        match consume trusted std.format.write_to<u32, std.io.FormattingSink<ValidatingWriter>, std.io.IoError>(
            &mut sink,
            std.format.Argument<u32>(&value)
        )
        {
            case Ok(_) {}
            case Error(_) { assert(false); }
        }

        formatted += 1;
    }

    assert(writer.valid);
    assert(writer.length == 2048);
}
"#,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
        storage: Some(StorageExpectation {
            allocation_count: 0,
            allocated_bytes: 0,
            copied_bytes: 0,
        }),
    },
    Workload {
        id: "stream_output",
        category: WorkloadCategory::Streaming,
        scale: 1024,
        units: "writes",
        batching: BatchingPolicy::SingleExecution,
        source: super::source::STREAM_OUTPUT,
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
            required_provenance: &["bray_platform_standard_streams"],
            forbidden_provenance: &[
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
        batching: BatchingPolicy::SingleExecution,
        source: super::source::ASYNC_OUTPUT,
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
        id: "contended_output",
        category: WorkloadCategory::Concurrent,
        scale: 128,
        units: "writes",
        batching: BatchingPolicy::SingleExecution,
        source: super::source::CONTENDED_OUTPUT,
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
        id: "captured_output",
        category: WorkloadCategory::Streaming,
        scale: 4096,
        units: "captured bytes",
        batching: BatchingPolicy::SingleExecution,
        source: super::source::CAPTURED_OUTPUT,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[],
        retention: NO_RETENTION_CONTRACT,
        storage: Some(StorageExpectation {
            allocation_count: 1,
            allocated_bytes: 4096,
            copied_bytes: 4096,
        }),
    },
    Workload {
        id: "process_pipe_transfer",
        category: WorkloadCategory::Process,
        scale: 4096,
        units: "pipe bytes",
        batching: BatchingPolicy::SingleExecution,
        source: super::source::PROCESS_PIPE_TRANSFER,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::AbsentPath(
            "bray-performance-pipe-child-observations",
        ),
        platform_operations: &[
            "platform.child.spawn",
            "platform.child.wait",
            "platform.process_pipe.write",
            "platform.process_pipe.flush",
            "platform.process_pipe.close",
            "platform.path.remove_file",
        ],
        retention: NO_RETENTION_CONTRACT,
        storage: None,
    },
    Workload {
        id: "filesystem_metadata",
        category: WorkloadCategory::Filesystem,
        scale: 256,
        units: "lookups",
        batching: BatchingPolicy::SingleExecution,
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
        index += 1;
    }

    return Ok(unit);
}
"#,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &[
            "platform.context.working_directory",
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
        batching: BatchingPolicy::SingleExecution,
        source: super::source::FILE_OUTPUT,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::AbsentPath("bray-performance-file-output"),
        platform_operations: &[
            "platform.context.native_text_width",
            "platform.file.open",
            "platform.file.write",
            "platform.file.flush",
            "platform.file.close",
            "platform.path.remove_file",
        ],
        retention: RetentionContract {
            required_symbols: &[
                "bray_platform_file_open",
                "bray_platform_file_write",
                "bray_platform_file_flush",
                "bray_platform_file_close",
                "bray_platform_path_remove_file",
            ],
            forbidden_symbols: &[
                "bray_platform_context_native_text_width",
                "bray_platform_standard_output_write",
                "bray_platform_standard_error_write",
                "bray_platform_process_pipe_read",
                "bray_platform_process_pipe_write",
                "bray_platform_process_pipe_flush",
                "bray_platform_process_pipe_close",
            ],
            required_provenance: &["bray_platform_filesystem"],
            forbidden_provenance: &[
                "bray_platform_core",
                "bray_platform_process",
                "bray_platform_standard_streams",
                "bray_runtime_test_host",
                "run_output_context",
            ],
        },
        storage: Some(StorageExpectation {
            allocation_count: 5,
            allocated_bytes: 486,
            copied_bytes: 168,
        }),
    },
    Workload {
        id: "process_context",
        category: WorkloadCategory::Process,
        scale: 1024,
        units: "lookups",
        batching: BatchingPolicy::SingleExecution,
        source: r#"module process_context;

using std.process;

func main()
{
    let mut index: usize = 0;

    while index < 1024
    {
        let _: std.process.Id = std.process.current_id();
        index += 1;
    }
}
"#,
        expected_output: ExpectedOutput::Empty,
        expected_side_effects: ExpectedSideEffects::None,
        platform_operations: &["platform.context.identity"],
        retention: RetentionContract {
            required_symbols: &[],
            forbidden_symbols: &[
                "bray_platform_context_argument",
                "bray_platform_context_argument_count",
                "bray_platform_context_environment_entry",
                "bray_platform_context_environment_count",
                "bray_platform_context_environment_key_equals",
                "bray_platform_context_identity",
                "bray_platform_context_native_text_width",
                "bray_platform_context_working_directory",
            ],
            required_provenance: &[],
            forbidden_provenance: &[
                "bray_platform_core",
                "bray_platform_process",
                "bray_platform_standard_streams",
                "bray_platform_filesystem",
                "bray_runtime_test_host",
                "run_output_context",
            ],
        },
        storage: Some(StorageExpectation {
            allocation_count: 0,
            allocated_bytes: 0,
            copied_bytes: 0,
        }),
    },
    Workload {
        id: "monotonic_clock",
        category: WorkloadCategory::Time,
        scale: 1024,
        units: "readings",
        batching: BatchingPolicy::SingleExecution,
        source: r#"module monotonic_clock;

using std.time;

func main() -> Result<unit, std.time.ClockError>
{
    let mut index: usize = 0;

    while index < 1024
    {
        let _: std.time.Instant = try std.time.monotonic_now();
        index += 1;
    }

    return Ok(unit);
}
"#,
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

    #[test]
    fn number_formatting_allocates_exact_final_storage_and_copies_each_byte_once() {
        let formatting = storage("format_numbers");

        assert_eq!(formatting.allocation_count, 1);
        assert_eq!(formatting.allocated_bytes, 2_986);
        assert_eq!(formatting.copied_bytes, formatting.allocated_bytes);
    }

    #[test]
    fn large_width_formatting_fills_padding_and_copies_only_digits() {
        let workload = workload("format_large_width");
        let storage = storage("format_large_width");

        assert!(workload.source.contains("width = 130"));
        assert!(workload.source.contains("while index < length"));
        assert_eq!(storage.allocation_count, 1);
        assert_eq!(storage.allocated_bytes, 133_120);
        assert_eq!(storage.copied_bytes, 2_048);
        assert_eq!(storage.allocated_bytes - storage.copied_bytes, 128 * 1_024);
    }

    #[test]
    fn writer_formatting_validates_each_byte_without_dynamic_storage() {
        let workload = workload("format_writer");
        let storage = storage("format_writer");

        assert!(
            workload
                .source
                .contains("std.io.FormattingSink<ValidatingWriter>")
        );

        assert!(workload.source.contains("std.format.write_to<"));
        assert!(workload.source.contains("source[index] != 52"));
        assert_eq!(storage.allocation_count, 0);
        assert_eq!(storage.allocated_bytes, 0);
        assert_eq!(storage.copied_bytes, 0);
    }

    #[test]
    fn contended_output_reports_synchronization_and_write_throughput() {
        let workload = workload("contended_output");

        assert_eq!(workload.scale, 128);
        assert_eq!(workload.units, "writes");

        assert_eq!(
            workload.platform_operations,
            [
                "platform.standard_output.write",
                "platform.standard_output.flush",
                "platform.standard_output.lock",
                "platform.standard_output.unlock",
            ]
        );
    }

    #[test]
    fn file_output_exercises_large_buffer_bypass() {
        let workload = workload("file_output");

        assert!(
            workload
                .source
                .contains("std.io.BufferedWriter<std.fs.File>")
        );

        assert!(workload.source.contains("capacity = 256"));
        assert!(workload.source.contains("std.io.write_all<"));

        let storage = workload.storage.expect("file output storage contract");

        assert!(storage.allocated_bytes < workload.scale);
        assert!(storage.copied_bytes < workload.scale);
    }

    #[test]
    fn transfer_matrix_covers_capture_pipe_async_and_contention() {
        for identity in [
            "stream_output",
            "captured_output",
            "file_output",
            "process_pipe_transfer",
            "async_output",
            "contended_output",
        ] {
            let _ = workload(identity);
        }
    }

    fn workload(identity: &str) -> &super::Workload {
        WORKLOADS
            .iter()
            .find(|workload| workload.id == identity)
            .unwrap_or_else(|| panic!("{identity} must exist"))
    }

    fn storage(identity: &str) -> super::StorageExpectation {
        workload(identity)
            .storage
            .unwrap_or_else(|| panic!("{identity} must define storage work"))
    }
}
