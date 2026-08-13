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
}

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
    },
];
