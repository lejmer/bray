use super::corpus::{
    BatchingPolicy, ExpectedOutput, ExpectedSideEffects, NO_RETENTION_CONTRACT, StorageExpectation,
    Workload,
};
use super::model::WorkloadCategory;

pub(super) const DEQUE_MIXED_ENDS: Workload = Workload {
    id: "deque_mixed_ends",
    category: WorkloadCategory::CoreData,
    scale: 4096,
    units: "mixed-end operations",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module deque_mixed_ends;

using std.collection;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut values: std.collection.Deque<u64> = try trusted std.collection.Deque<u64>(capacity = 64);
    let mut value: u64 = 0;

    while value < 64
    {
        try trusted values.push_back(value);
        value += 1;
    }

    let mut removed: u64 = 0;

    while removed < 32
    {
        match trusted values.pop_front()
        {
            case ?current { assert(current == removed); }
            case none { panic("deque front must exist"); }
        }

        removed += 1;
    }

    while value < 96
    {
        try trusted values.push_back(value);
        value += 1;
    }

    let mut round: usize = 0;

    while round < 2048
    {
        if round % 2 == 0
        {
            let current: u64 = match trusted values.pop_front()
            {
                case ?item { yield item; }
                case none { panic("deque front must exist"); }
            };

            try trusted values.push_back(current);
        }
        else
        {
            let current: u64 = match trusted values.pop_back()
            {
                case ?item { yield item; }
                case none { panic("deque back must exist"); }
            };

            try trusted values.push_front(current);
        }

        round += 1;
    }

    try trusted values.push_back(96);

    assert(values.length() == 65);
    assert(values.capacity() == 128);

    match values.front()
    {
        case ?current { assert(current == 32); }
        case none { panic("deque front must exist"); }
    }

    match values.back()
    {
        case ?current { assert(current == 96); }
        case none { panic("deque back must exist"); }
    }

    return Ok(unit);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 2,
        allocated_bytes: 1_536,
        copied_bytes: 0,
    }),
};
