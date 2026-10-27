use crate::performance::model::WorkloadCategory;

use super::model::{
    BatchingPolicy, ExpectedOutput, ExpectedSideEffects, NO_RETENTION_CONTRACT, StorageExpectation,
    Workload,
};

pub(super) const SHORTEST: Workload = Workload {
    id: "format_real_shortest",
    category: WorkloadCategory::Formatting,
    scale: 1024,
    units: "values",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module format_real_shortest;

using std.bytes;
using std.format;
using std.format.R64Format;
using std.format.FixedSinkFormatting;

func main()
    requires(blocking_execution())
{
    let mut total: usize = 0;
    let mut index: u32 = 0;

    while index < 1024
    {
        let mut storage: bytes<128> = [0; 128];
        let mut sink: std.format.FixedSink = std.format.FixedSink(&mut storage[..]);
        let value: r64 = index as r64 + 0.125;

        match consume trusted std.format.write_to<r64, std.format.FixedSink, std.format.FixedSinkError>(
            &mut sink, std.format.Argument<r64>(&value, options = std.format.Options()))
        {
            case Ok(_) {}
            case Error(_) { assert(false); }
        }

        total += std.format.finish_fixed(sink);
        index += 1;
    }

    assert(total == 7082);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 0,
        allocated_bytes: 0,
        copied_bytes: 7_082,
    }),
};

pub(super) const PRECISION: Workload = Workload {
    id: "format_real_precision",
    category: WorkloadCategory::Formatting,
    scale: 1024,
    units: "values",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module format_real_precision;

using std.bytes;
using std.format;
using std.format.R64Format;
using std.format.FixedSinkFormatting;

func main()
    requires(blocking_execution())
{
    let mut total: usize = 0;
    let mut index: u32 = 0;

    while index < 1024
    {
        let mut storage: bytes<128> = [0; 128];
        let mut sink: std.format.FixedSink = std.format.FixedSink(&mut storage[..]);
        let value: r64 = 0.1;
        let precision: usize = if index % 2 == 0 { yield 40; } else { yield 80; };

        match consume trusted std.format.write_to<r64, std.format.FixedSink, std.format.FixedSinkError>(
            &mut sink, std.format.Argument<r64>(&value, options = std.format.Options(precision = precision)))
        {
            case Ok(_) {}
            case Error(_) { assert(false); }
        }

        total += std.format.finish_fixed(sink);
        index += 1;
    }

    assert(total == 63488);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 0,
        allocated_bytes: 0,
        // The last 25 places at precision 80 are filled, rather than copied.
        copied_bytes: 50_688,
    }),
};

pub(super) const LARGE_WIDTH: Workload = Workload {
    id: "format_real_large_width",
    category: WorkloadCategory::Formatting,
    scale: 1024,
    units: "values",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module format_real_large_width;

using std.bytes;
using std.format;
using std.format.R64Format;
using std.format.CountingSinkFormatting;

func main()
    requires(blocking_execution())
{
    let mut sink: std.format.CountingSink = std.format.CountingSink();
    let value: r64 = 1.5;
    let mut index: usize = 0;

    while index < 1024
    {
        match consume trusted std.format.write_to<r64, std.format.CountingSink, std.format.CountingSinkError>(
            &mut sink, std.format.Argument<r64>(&value, options = std.format.Options(precision = 3, width = 1000000)))
        {
            case Ok(_) {}
            case Error(_) { assert(false); }
        }

        index += 1;
    }

    assert(std.format.formatted_length(&sink) == 1024000000);
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
};

pub(super) const COLLECTION: Workload = Workload {
    id: "format_real_collection",
    category: WorkloadCategory::Formatting,
    scale: 1024,
    units: "values",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module format_real_collection;

using std.bytes;
using std.format;
using std.format.R64Format;
using std.format.ByteSinkFormatting;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
    requires(blocking_execution())
{
    let mut sink: std.format.ByteSink = try std.format.ByteSink(capacity = 7082);
    let mut index: u32 = 0;

    while index < 1024
    {
        let value: r64 = index as r64 + 0.125;

        try trusted std.format.write<r64>(&mut sink, std.format.Argument<r64>(&value));
        index += 1;
    }

    assert(std.format.bytes(&sink).length() == 7082);

    return Ok(unit);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 1,
        allocated_bytes: 7_082,
        copied_bytes: 7_082,
    }),
};

pub(super) const COMPLEX_WRITER: Workload = Workload {
    id: "format_complex_writer",
    category: WorkloadCategory::Formatting,
    scale: 1024,
    units: "values",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module format_complex_writer;

using std.bytes;
using std.format;
using std.format.C128Format;
using std.io;
using std.io.WriterFormattingSink;
using std.string;

struct ValidatingWriter
{
    mut length: usize;
    mut valid: bool;
}

impl ValidatingWriterIo = ValidatingWriter(std.io.Writer)
{
    mut func write(pos source: &[u8]) -> Result<usize, std.io.IoError>
        requires(blocking_execution())
    {
        let pattern: &[u8] = "1.25-2.50i".as_bytes();
        let count: usize = source.length();
        let mut index: usize = 0;

        while index < count
        {
            if source[index] != pattern[(self.length + index) % 10]
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
    let mut writer: ValidatingWriter = { length = 0, valid = true };

    {
        let mut sink: std.io.FormattingSink<ValidatingWriter> = std.io.FormattingSink<ValidatingWriter>(&mut writer);
        let value: c128 = 1.25 - 2.5i;
        let mut index: usize = 0;

        while index < 1024
        {
            match consume trusted std.format.write_to<c128, std.io.FormattingSink<ValidatingWriter>, std.io.IoError>(
                &mut sink, std.format.Argument<c128>(&value, options = std.format.Options(precision = 2)))
            {
                case Ok(_) {}
                case Error(_) { assert(false); }
            }

            index += 1;
        }
    }

    assert(writer.valid);
    assert(writer.length == 10240);
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
};
