# Raw allocation and buffers

`RawAllocation` is the standard-library linear owner for one untyped raw allocation.

```bray
module std.memory;

struct RawAllocation
{
    pointer: RawPointer<u8>;
    bytes: usize;
    align: usize;
}

trusted func allocate(pos layout: MemoryLayout) -> RawAllocation
    ensures(
        trusted core.memory.owned_allocation(pointer = result.pointer, bytes = result.bytes, align = result.align),
        trusted core.memory.valid_write(pointer = result.pointer, count = result.bytes),
    )
    uses(manual_alloc);

trusted func deallocate(pos allocation: RawAllocation) -> unit
    uses(manual_alloc);
```

`RawAllocation` is not copyable.

A `RawAllocation` value carries the trusted allocation ownership condition for its allocation.

The trusted allocation ownership condition is not implied by the visible field values.

Only compiler-recognized allocator declarations, trusted declarations that establish the required conditions, or
movement of an existing `RawAllocation` can create a `RawAllocation` value that carries allocation ownership.

Constructing a `RawAllocation` value from arbitrary field values is rejected unless the surrounding trusted context
establishes the required allocation ownership conditions for those fields.

`std.memory.allocate(layout)` calls or wraps `core.memory.allocate(bytes = layout.bytes, align = layout.align)`.

On normal completion, `std.memory.allocate(layout)` returns a `RawAllocation` whose fields identify the allocation and
whose value carries the allocation ownership conditions.

Allocation failure panics.

`std.memory.allocate` must not report allocation failure through null pointers, sentinel layouts, partially initialized
allocation owners, or target-specific status codes.

`std.memory.deallocate(allocation)` consumes a `RawAllocation`.

`std.memory.deallocate` releases the allocation ownership condition carried by the consumed value.

Before deallocation, every initialized typed value, borrow, scoped capability, finalization obligation, and trusted
guarantee tied to the allocation must already be resolved or invalidated according to its contract.

Destroying a live `RawAllocation` deallocates the allocation when its contract proves that no initialized typed value,
borrow, scoped capability, finalization obligation, or unresolved trusted guarantee remains tied to the allocation.

If those obligations cannot be proven resolved at the destruction point, destruction of the `RawAllocation` is rejected.

Moving a `RawAllocation` transfers the allocation ownership condition.

Observing `allocation.pointer`, `allocation.bytes`, or `allocation.align` does not transfer the allocation ownership
condition.

Copying the raw pointer field does not copy allocation ownership.

## Raw buffers

`RawBuffer<T>` is the standard-library linear owner for contiguous raw storage intended to hold values of `T`.

```bray
module std.memory;

struct RawBuffer<T>
{
    pointer: RawPointer<T>;
    capacity: usize;
    initialized: usize;

    trusted construct(capacity: usize) -> Result<Self, MemoryLayoutError>
        uses(manual_alloc, layout_reinterpret);
}

func capacity<T>(pos buffer: &RawBuffer<T>) -> usize;

func initialized_count<T>(pos buffer: &RawBuffer<T>) -> usize;

func pointer<T>(pos buffer: &RawBuffer<T>) -> RawPointer<T>;

trusted func initialized_slice<T>(pos buffer: &RawBuffer<T>) -> &[T]
    uses(raw_memory);

trusted func initialized_slice_mut<T>(pos buffer: &mut RawBuffer<T>) -> &mut [T]
    uses(raw_memory);

trusted func spare_pointer<T>(pos buffer: &mut RawBuffer<T>) -> RawPointer<T>
    ensures(
        trusted core.memory.valid_write(pointer = result, count = buffer.capacity - buffer.initialized),
        trusted core.memory.aligned_for<T>(pointer = result),
    )
    uses(raw_memory);

trusted func set_initialized_count<T>(pos buffer: &mut RawBuffer<T>, count: usize) -> unit
    requires(
        count <= buffer.capacity,
        trusted core.memory.initialized_range_as<T>(pointer = buffer.pointer, count = count),
    )
    ensures(
        buffer.initialized == count,
    )
    uses(unchecked_init);
```

`RawBuffer<T>` is not copyable.

`RawBuffer<T>(capacity = capacity)` computes `layout_of<T>(count = capacity)`.

If the layout cannot be represented for the selected target profile, the constructor returns the corresponding
`MemoryLayoutError`.

When the constructor returns `Result.Error`, no allocation owner is created.

If allocation fails after the layout is valid, the constructor panics according to the allocation rules.

When the constructor returns `Result.Ok(buffer)`, `buffer.capacity == capacity` and `buffer.initialized == 0`.

`RawBuffer<T>` owns its allocation and deallocates it when destroyed.

`RawBuffer<T>` tracks an initialized prefix.

The `RawBuffer<T>` type contract requires `initialized <= capacity`.

The initialized prefix contains `initialized` contiguous `T` values starting at `pointer`.

Slots from `initialized` up to `capacity` are spare raw storage.

Safe buffer operations can observe, borrow, mutate, move, destroy, and initialize only according to the
initialized-prefix contract.

The safe slice-producing helpers expose only the initialized prefix.

`spare_pointer` exposes the beginning of the spare storage and preserves the trusted caller obligations needed to write
into that storage.

`set_initialized_count` is trusted because the caller must prove that the new initialized prefix truly contains
initialized `T` values and that every removed initialized value has had its destruction and finalization obligations
resolved.

Destroying a `RawBuffer<T>` destroys or finalizes initialized elements in reverse initialization order, then deallocates
the raw allocation.

Moving a `RawBuffer<T>` transfers the allocation ownership condition and the initialized-prefix contract.

The standard library uses one internal `RawBuffer<T>` reservation policy for growable contiguous storage.

When growth is required, exact reservation selects `initialized + additional` after checked addition. Amortized
reservation starts at four elements and doubles until it reaches the required length. If the next doubling would
overflow, it selects the already-validated required length. A request that fits the existing spare capacity does not
allocate.

Reservation allocates one replacement owner and relocates the initialized prefix with a compiler-recognized operation.
Relocation requires distinct mutable buffer owners and enough destination capacity. It transfers the initialized values
as one representation range, sets the destination initialized length, and clears the source initialized length as one
semantic ownership operation. It does not invoke element copy behavior, constructors, finalizers, or destructors. This
contract permits native bulk transfer while ensuring the old allocation cannot destroy relocated non-copy values.

Zero-sized values follow the same element-count and initialized-prefix rules even though their transferred byte count is
zero. The selected target layout supplies the alignment for non-zero bulk transfers.

Observing `buffer.pointer`, `buffer.capacity`, or `buffer.initialized` does not transfer ownership of the allocation or
initialized elements.

Constructing a `RawBuffer<T>` value from arbitrary field values is rejected unless the surrounding trusted context
establishes the required allocation ownership, valid-write, and initialized-prefix conditions for those fields.

`RawBuffer<T>` is a low-level storage owner, not a growable collection contract.

Higher-level collections can use `RawBuffer<T>` internally while exposing their own ownership, indexing, iteration, and
capacity contracts.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Standard-library memory surface](standard-library-memory-surface.md)
- Next: [Device memory](device-memory.md)
