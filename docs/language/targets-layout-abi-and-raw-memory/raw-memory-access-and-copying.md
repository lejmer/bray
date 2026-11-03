# Raw memory access and copying

The compiler-known raw memory access declarations are under `core.memory`.

```bray
using core.memory;

trusted func read<T>(pos pointer: DataPointer<T>) -> T
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
        trusted core.memory.initialized_as<T>(pointer = pointer),
    )
    uses(raw_memory);

trusted func write<T>(pos pointer: DataPointer<T>, pos value: T) -> unit
    requires(
        trusted core.memory.valid_write(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
    )
    ensures(
        trusted core.memory.initialized_as<T>(pointer = pointer),
    )
    uses(raw_memory, unchecked_init);
```

`read` reads an initialized `T` value from raw memory and produces an owned `T` value.

For a copyable `T`, `read` copies the value and preserves the source storage's initialized state.

For a non-copyable `T`, `read` moves the value out of raw storage and invalidates the trusted guarantee that the source
storage remains initialized as `T`.

`write` stores the supplied value into raw memory.

If the destination storage already contains a live initialized value, the caller must satisfy that value's destruction,
finalization, and ownership obligations before `write` overwrites the storage.

`write` can initialize previously uninitialized storage.

The compiler-known raw memory copy declarations are under `core.memory`.

```bray
using core.memory;

trusted func copy<T>(
    pos source: DataPointer<T>,
    pos destination: DataPointer<T>,
    count: usize,
) -> unit
    requires(
        trusted core.memory.valid_read(pointer = source, count = count),
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.initialized_range_as<T>(pointer = source, count = count),
        trusted core.memory.non_overlapping(
            left = source,
            left_count = count,
            right = destination,
            right_count = count,
        ),
    )
    ensures(
        trusted core.memory.initialized_range_as<T>(pointer = destination, count = count),
    )
    uses(raw_memory, unchecked_init);

trusted func copy_overlapping<T>(
    pos source: DataPointer<T>,
    pos destination: DataPointer<T>,
    count: usize,
) -> unit
    requires(
        trusted core.memory.valid_read(pointer = source, count = count),
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.initialized_range_as<T>(pointer = source, count = count),
    )
    ensures(
        trusted core.memory.initialized_range_as<T>(pointer = destination, count = count),
    )
    uses(raw_memory, unchecked_init);
```

`copy` requires non-overlapping source and destination ranges.

`copy_overlapping` permits overlapping source and destination ranges.

Raw memory copying is a representation-level operation.

It does not run constructors, finalizers, destructors, assignment behavior, operator behavior, or user-defined copy
behavior.

The caller is responsible for ensuring that raw memory copying preserves every ownership, initialization, finalization,
aliasing, and representation invariant required by the involved type and storage.

## Repeated raw read example

Example: the following are alternative body fragments in trusted declarations with `uses(raw_memory)` and
`using core.memory` in their module. Each declaration exposes `valid_read`, `aligned_for`, and `initialized_as`
requirements for one value at its `pointer` parameter.

For `pointer: DataPointer<u8>`, both reads are accepted:

```bray
let first = trusted core.memory.read<u8>(pointer);
let second = trusted core.memory.read<u8>(pointer);
```

The first read copies the byte and preserves initialization. With a non-copyable product instead:

```bray
struct Parcel
{
    code: u8;
}
```

For `pointer: DataPointer<Parcel>`, the second read is rejected:

```bray
let first = trusted core.memory.read<Parcel>(pointer);
let second = trusted core.memory.read<Parcel>(pointer);
```

The first read moves the `Parcel` and invalidates its source initialization condition. The second trust boundary
cannot restore that checked state. The
[trusted caller obligation rules](../contracts-and-trust/trusted-caller-obligations.md#establishing-trusted-guarantees)
do not permit trusted premises to override a conflicting initialization state.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Core memory declarations](core-memory-declarations.md)
- Next: [Manual allocation](manual-allocation.md)
