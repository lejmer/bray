# Conformance catalog

The compiler-known and recognized standard-library conformance catalog is closed for a conforming Bray implementation.

A conforming compiler and standard library must preserve the declaration identity, availability, contract, and observable semantics of each catalog entry.

## Always-available compiler-known type entries

The always-available compiler-known type entries are:

- `bool`,
- `char`,
- `unit`,
- `never`,
- `i8`,
- `i16`,
- `i32`,
- `i64`,
- `i128`,
- `u8`,
- `u16`,
- `u32`,
- `u64`,
- `u128`,
- `r32`,
- `r64`,
- `c64`,
- `c128`,
- `usize`,
- `isize`,
- `string`,
- `Result<T, E>`,
- `RunResult<T>`,
- `PanicReport`,
- `ConversionError`,
- `Ordering`,
- `Future<T>`,
- `Task<T>`,
- structural tuple type forms,
- structural fixed-size array type forms,
- slice type forms,
- nullable type forms,
- borrow type forms,
- trait-view type forms,
- callable type forms,
- `RawPointer<T>`.

The always-available compiler-known value entries are:

- `true`,
- `false`,
- `unit`,
- `none`.

The always-available compiler-known predicate entries are:

- `blocking_execution()`,
- `compute_execution()`,
- `main_thread_execution()`.

The compiler-provided inherent async member entries are:

- `Future<T>.start()`,
- `Task<T>.join()`,
- `Task<T>.cancel()`.

## Target-available compiler-known entries

The target-available compiler-known entries are:

- `r16`,
- `r128`,
- `c32`,
- `c256`,
- target-conditional scalar operations,
- target-conditional raw memory declarations under `core.memory`,
- target-conditional atomic declarations and facts,
- target-conditional ABI declarations and facts,
- target-conditional address-space declarations and facts,
- target-conditional allocation declarations and facts.

## Compiler-known traits and contracts

The compiler-known traits and contracts include:

- `Storage<T>`,
- `Iterable`,
- `Iterator`,
- `ConvertTo<Target>`,
- `CheckedConvertTo<Target>`,
- `ElementIndex<Selector>`,
- `SliceIndex<Bound>`,
- `Copyable`,
- overloadable operator traits defined by the type rules.

The compiler-known default storage policy is `Heap`.

The compiler provides the named generic implementation
`impl HeapStorage = Heap(Storage<T>)` with the exact `Storage<T>` members and contracts defined by
[type forms](../types/type-forms.md).

## Compiler-known paths

The reserved compiler-known paths include:

- `core.memory`,
- `target`.

No compiler-known declaration is owned by the `std` package. The `std` root is reserved exclusively for ordinary standard-library
packages and their declarations.

## Recognized standard-library entries

The recognized declarations under the `std` root are:

- `convert<Target, Source>(source)`,
- `RoundingRule`,
- `RoundingRule.NearestEven`,
- `RoundingRule.TowardZero`,
- `RoundingRule.TowardNegativeInfinity`,
- `RoundingRule.TowardPositiveInfinity`,
- `RoundingRule.AwayFromZero`,
- `round_to<Target, Source>(source, rule = RoundingRule.NearestEven)`,
- `truncate_to<Target, Source>(source)`,
- `saturate_to<Target, Source>(source)`,
- `wrap_to<Target, Source>(source)`.

The `RoundingRule` variants have the listed ordinal order from zero through four.

The recognized declarations under `std.string` are:

- `Utf8Error`,
- `Utf8Error.InvalidEncoding`,
- `scalar_count(value)`,
- `is_empty(value)`,
- `equals(left, right)`,
- `scalar_at(value, index)`,
- `scalar_slice(value, start, end)`,
- `utf8(value)`,
- `from_utf8(bytes)`.

`Utf8Error.InvalidEncoding` has ordinal zero.

The recognized declarations under `std.memory` are:

- `address_of<T>(value)`,
- `address_of_mut<T>(value)`,
- `null<T>()`,
- `is_null<T>(pointer)`,
- `offset<T>(pointer, elements)`,
- `byte_offset<T>(pointer, bytes)`,
- `reinterpret<Target, Source>(pointer)`,
- `read<T>(pointer)`,
- `write<T>(pointer, value)`,
- `copy<T>(source, destination, count)`,
- `copy_overlapping<T>(source, destination, count)`,
- `MemoryLayout` and its `bytes` and `align` fields,
- `MemoryLayoutError` and its `SizeOverflow` and `UnsupportedAlignment` variants,
- `size_of<T>()`,
- `align_of<T>()`,
- `stride_of<T>()`,
- `layout_of<T>(count)`,
- `RawAllocation` and its `pointer`, `bytes`, and `align` fields,
- `allocate(layout)`,
- `deallocate(allocation)`,
- `RawBuffer<T>` and its `pointer`, `capacity`, and `initialized` fields,
- `create_buffer<T>(capacity)`,
- `capacity<T>(buffer)`,
- `initialized_count<T>(buffer)`,
- `pointer<T>(buffer)`,
- `initialized_slice<T>(buffer)`,
- `initialized_slice_mut<T>(buffer)`,
- `spare_pointer<T>(buffer)`,
- `set_initialized_count<T>(buffer, count)`.

`MemoryLayoutError.SizeOverflow` and `MemoryLayoutError.UnsupportedAlignment` have ordinals zero and
one respectively.

The exact `std.memory` signatures and contracts are defined by
[the standard-library memory surface](../targets-layout-abi-and-raw-memory/standard-library-memory-surface.md),
[layout helpers](../targets-layout-abi-and-raw-memory/layout-helpers.md), and
[raw allocation and buffers](../targets-layout-abi-and-raw-memory/raw-allocation-and-buffers.md).

The raw-pointer declarations are available only when the target provides raw memory.

`RawAllocation`, `RawBuffer<T>`, and their allocation operations are available only when the target
provides allocation.

Layout declarations are available on every target.

Device-memory declarations are target-specific ordinary standard-library APIs and are not part of
the closed recognized catalog.

Channels, operating-system threads, child processes, parallel algorithms, task combinators, synchronization types, universal run
and task checkpoints, cancellation observation, and runtime selection types are ordinary standard-library or product declarations.
They are not compiler-known or recognized by source name.

Recognized standard-library entries are usable only through ordinary visibility, import, and path rules.

A same-named declaration with a different stable imported identity is ordinary code and is not
recognized.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Availability summary](availability-summary.md)
- Next: [Summary](summary.md)
