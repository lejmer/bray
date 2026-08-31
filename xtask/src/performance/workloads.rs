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

pub(super) const HASH_MAP_GROWTH_AND_HEALTHY_LOOKUP: Workload = Workload {
    id: "hash_map_growth_and_healthy_lookup",
    category: WorkloadCategory::CoreData,
    scale: 4096,
    units: "insert-and-lookup pairs",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module hash_map_growth_and_healthy_lookup;

using std.collection;
using std.hash.StableHasherSink;
using std.hash.U64Hashable;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut map: std.collection.HashMap<u64, u64> = try trusted std.collection.HashMap<u64, u64>();
    let mut key: u64 = 0;

    while key < 4096
    {
        let value: u64 = key;
        let _: u64? = try trusted map.insert(key, value);

        key += 1;
    }

    key = 0;

    while key < 4096
    {
        let lookup: u64 = key;

        assert(map.contains_key(&lookup));

        key += 1;
    }

    assert(map.length() == 4096);
    assert(map.capacity() == 6144);

    return Ok(unit);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 33,
        allocated_bytes: 540_408,
        copied_bytes: 0,
    }),
};

pub(super) const HASH_MAP_COLLISION_LOOKUP: Workload = Workload {
    id: "hash_map_collision_lookup",
    category: WorkloadCategory::CoreData,
    scale: 4096,
    units: "collision-chain lookups",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module hash_map_collision_lookup;

using std.collection;
using std.hash;
using std.hash.StableHasherSink;
using std.memory;

struct CollisionKey
{
    value: u64;
}

impl CollisionKeyEquatable = CollisionKey(Equatable<CollisionKey>)
{
    func equals(pos rhs: &CollisionKey) -> bool
    {
        return self.value == rhs.value;
    }
}

impl CollisionKeyHashable = CollisionKey(std.hash.Hashable<Sink>)
    with(Sink: std.hash.HashSink)
{
    func contribute(pos sink: &mut Sink) -> unit
    {
        sink.write(0);
    }
}

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut map: std.collection.HashMap<CollisionKey, u64> =
        try trusted std.collection.HashMap<CollisionKey, u64>(capacity = 48);

    let mut value: u64 = 0;

    while value < 48
    {
        let _: u64? = try trusted map.insert(
            {
                value = value,
            },
            value,
        );

        value += 1;
    }

    let mut lookup: usize = 0;

    while lookup < 4096
    {
        let subject: CollisionKey =
        {
            value = (lookup % 48) as u64,
        };

        assert(map.contains_key(&subject));

        lookup += 1;
    }

    assert(map.length() == 48);
    assert(map.capacity() == 48);

    return Ok(unit);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: Some(StorageExpectation {
        allocation_count: 3,
        allocated_bytes: 2_112,
        copied_bytes: 0,
    }),
};

pub(super) const ORDERED_COLLECTIONS_MONOTONIC_UPDATES: Workload = Workload {
    id: "ordered_collections_monotonic_updates",
    category: WorkloadCategory::CoreData,
    scale: 8192,
    units: "monotonic insert-and-lookup pairs",
    batching: BatchingPolicy::SingleExecution,
    source: r#"module ordered_collections_monotonic_updates;

using std.collection;
using std.memory;

func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    let mut map: std.collection.OrderedMap<u64, u64> = try trusted std.collection.OrderedMap<u64, u64>();
    let mut set: std.collection.OrderedSet<u64> = try trusted std.collection.OrderedSet<u64>();
    let mut value: u64 = 0;

    while value < 4096
    {
        let _: u64? = try trusted map.insert(value, value);
        value += 1;
    }

    value = 4096;

    while value > 0
    {
        value -= 1;

        let _: bool = try trusted set.insert(value);
    }

    value = 0;

    while value < 4096
    {
        {
            let lookup: u64 = value;

            assert(map.contains_key(&lookup));
            assert(set.contains(&lookup));
        }

        value += 1;
    }

    assert(map.length() == 4096);
    assert(set.length() == 4096);

    return Ok(unit);
}
"#,
    expected_output: ExpectedOutput::Empty,
    expected_side_effects: ExpectedSideEffects::None,
    platform_operations: &[],
    retention: NO_RETENTION_CONTRACT,
    storage: None,
};
