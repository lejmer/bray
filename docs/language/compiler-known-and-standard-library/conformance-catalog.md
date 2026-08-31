# Conformance catalog

The compiler-known and recognized standard-library conformance catalog is closed for a conforming Bray implementation.

A conforming compiler and standard library must preserve the declaration identity, availability, contract, and
observable semantics of each catalog entry.

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
- `Range<T>`,
- `Result<T, E>`,
- `RunResult<T>`,
- `PanicReport`,
- `ConversionError`,
- `Ordering`,
- `Future<T>`,
- `Task<T>`,
- `Heap`,
- structural tuple type forms,
- structural fixed-size and incomplete-extent array type forms,
- slice type forms,
- nullable type forms,
- borrow type forms,
- trait-view type forms,
- owned-indirection type forms,
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

The compiler-provided structural sequence member entries are:

- `[T; N].length()`,
- `[T; N].is_empty()`,
- `[T].length()`,
- `[T].is_empty()`.

The fixed-array members use the compile-time extent `N`. The slice members observe the runtime extent carried by the
slice indirection.

## Static-storage conformance requirements

Product-static and `@thread_local` static declarations are language declaration forms. They are not compiler-known
declarations and do not reserve any standard-library type name.

A conforming compiler, compiled-interface implementation, linker, product host, and standard library preserve:

- constant materialization with no execution on import,
- arbitrary valid closed type and const substitutions,
- the canonical static instance identity defined by the declaration rules,
- selected implementation witnesses and target-profile identity,
- one owning product instance and one exact native-thread attachment where applicable,
- open generic templates in compiled interfaces and demand-driven closed realization,
- stable address identity and static movement restrictions,
- product-rooted and exact-thread-rooted dependency contracts,
- entry closure, external-root quiescence, foreign-thread attach and detach, and provider unload safety,
- consumer-before-provider cleanup for static-owned dependencies inside a teardown set,
- the stable total node order within each cleanup domain and explicit concurrency between independent domains,
- exactly-once cleanup ownership and domain-keyed reporting of static cleanup incidents.

`std.sync.Once<T>` and other synchronization or interior-mutation abstractions remain ordinary standard-library
declarations. Their names are not recognized by the compiler. Their safe contracts must preserve the static storage,
publication, synchronization, dependency, reentrancy, active-caller failure ownership, waiter retry, cancellation, and
cleanup rules they expose.

## Target-available compiler-known entries

The target-available compiler-known entries are:

- `r16`,
- `r128`,
- `c32`,
- `c256`,
- target-conditional scalar operations,
- target-conditional raw memory declarations under `core.memory`,
- target-conditional atomic declarations and capabilities,
- target-conditional ABI declarations and properties,
- target-conditional address-space declarations and properties,
- target-conditional allocation declarations and properties,
- target-conditional platform-service availability.

## Compiler-known traits and contracts

The compiler-known traits and contracts are:

- `Storage<T>`,
- `Iterable`,
- `Iterator`,
- `ConvertTo<Target>`,
- `CheckedConvertTo<Target>`,
- `ElementIndex<Selector>`,
- `MutableElementIndex<Selector>`,
- `SliceIndex<Bound>`,
- `MutableSliceIndex<Bound>`,
- `Copyable`,
- overloadable operator traits defined by the type rules.

The compiler-known default storage policy is `Heap`.

The compiler provides the named generic implementation `impl HeapStorage = Heap(Storage<T>)` with the exact `Storage<T>`
members and contracts defined by [type forms](../types/type-forms.md).

For each target-available integer scalar type `T`, the compiler provides exact implementations for:

- `Range<T>(Iterator)`,
- `&Range<T>(Iterable)`,
- `Range<T>(Iterable)`.

Their associated types, cursor behavior, order, finiteness, and cardinality follow the
[range-expression contract](../expressions/range-expressions.md).

## Compiler-known paths

The reserved compiler-known paths are:

- `core.memory`,
- `target`.

No compiler-known declaration is owned by the `std` package. The `std` root is reserved exclusively for ordinary
standard-library packages and their declarations.

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

The recognized declarations under `std.ffi` are:

- `CallbackContext<State>` and its state-consuming primary constructor,
- `callback_state<State>(context)`.

`CallbackContext<State>` is non-copyable and owns stable-address storage for exactly one initialized `State`. The
trusted `callback_state` operation returns a borrow tied to that owner only inside the matching exported ABI callback
entry, as defined by the foreign-callback contract.

The recognized declarations under `std.string` are:

- `Utf8Error`,
- `Utf8Error.InvalidEncoding`,
- the inherent `impl string` and its `length`, `is_empty`, `get`, `slice`, `characters`, `as_bytes`, and static
  `from_utf8` members,
- `impl StringEquatable = string(Equatable<string>)` and its `equals` fulfillment,
- the internal primitives `scalar_count`, `empty`, `equal`, `scalar_at`, `scalar_slice`, `utf8`, and `decode_utf8`,
- `ScalarCursor`, `impl ScalarCursorIterator = ScalarCursor(Iterator)`, and its `next` fulfillment.

`Utf8Error.InvalidEncoding` has ordinal zero.

The recognized declarations under `std.character` are:

- `Utf8Encoding`, its internal `bytes` and `length` fields, and its public `as_slice` member,
- the inherent `impl char` and its `code_point`, static `from_code_point`, `encode_utf8`, `is_alphabetic`, `is_numeric`,
  and `is_whitespace` members,
- the internal primitives `scalar_value`, `from_scalar_value`, `utf8_length`, `utf8_byte`, `alphabetic`, `numeric`, and
  `whitespace`.

The public string and character members are ordinary Bray bodies. Their internal primitive calls carry the
compiler-provided behavior; source spelling alone never receives that behavior.

The recognized declarations under `std.memory` are:

- `address_of<T>(value)`,
- `address_of_mut<T>(value)`,
- `null<T>()`,
- `is_null<T>(pointer)`,
- `offset<T>(pointer, elements)`,
- `byte_offset<T>(pointer, bytes)`,
- `reinterpret<Target, Source>(pointer)`,
- `callable_from_pointer<F>(pointer)`,
- `pointer_from_callable<F>(value)`,
- `read<T>(pointer)`,
- `write<T>(pointer, value)`,
- `copy<T>(source, destination, count)`,
- `copy_overlapping<T>(source, destination, count)`,
- `uninit<T>()`,
- `uninit_pointer<T>(storage)`,
- `uninit_pointer_mut<T>(storage)`,
- `uninit_write<T>(storage, value)`,
- `assume_initialized<T>(storage)`,
- `move_initialized<T>(storage)`,
- `destroy_initialized<T>(storage)`,
- `borrow_from<T, Owner>(owner, pointer)`,
- `borrow_mut_from<T, Capability>(capability, pointer)`,
- `MemoryLayout` and its `bytes` and `align` fields,
- `MemoryLayoutError` and its `SizeOverflow` and `UnsupportedAlignment` variants,
- `size_of<T>()`,
- `align_of<T>()`,
- `stride_of<T>()`,
- `layout_of<T>(count)`,
- `trailing_layout_of<T>(count)`,
- `RawAllocation` and its `pointer`, `bytes`, and `align` fields,
- `allocate(layout)`,
- `deallocate(allocation)`,
- `RawBuffer<T>` and its `pointer`, `capacity`, and `initialized` fields,
- `Output<T>` and `InPlace<T>` protected output construction,
- `AnchoredView<T, Owner>` and `AnchoredViewMut<T, Capability>` dependency-bearing views,
- the `RawBuffer<T>(capacity)` primary constructor,
- `capacity<T>(buffer)`,
- `initialized_count<T>(buffer)`,
- `pointer<T>(buffer)`,
- `initialized_slice<T>(buffer)`,
- `initialized_slice_mut<T>(buffer)`,
- `spare_pointer<T>(buffer)`,
- `set_initialized_count<T>(buffer, count)`.

`MemoryLayoutError.SizeOverflow` and `MemoryLayoutError.UnsupportedAlignment` have ordinals zero and one respectively.

The exact `std.memory` signatures and contracts are defined by
[the standard-library memory surface](../targets-layout-abi-and-raw-memory/standard-library-memory-surface.md),
[layout helpers](../targets-layout-abi-and-raw-memory/layout-helpers.md), and
[raw allocation and buffers](../targets-layout-abi-and-raw-memory/raw-allocation-and-buffers.md), and
[uninitialized storage and anchored borrows](../targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md).

The raw-pointer declarations are available only when the target provides raw memory.

`RawAllocation`, `RawBuffer<T>`, and their allocation operations are available only when the target provides allocation.

Layout declarations are available on every target.

Device-memory declarations are target-specific ordinary standard-library APIs and are not part of the closed recognized
catalog.

Channels, operating-system threads, child processes, parallel algorithms, task combinators, synchronization types,
universal run and task checkpoints, cancellation observation, and runtime selection types are ordinary standard-library
or product declarations. They are not compiler-known or recognized by source name.

Recognized standard-library entries are usable only through ordinary visibility, import, and path rules.

A same-named declaration with a different stable imported identity is ordinary code and is not recognized.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Availability summary](availability-summary.md)
- Next: [Summary](summary.md)
