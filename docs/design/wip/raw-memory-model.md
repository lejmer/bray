# Raw memory model

## Overview

Raw memory support is the lowest-level part of Bray's trusted substrate.

The raw memory model defines:

- the compiler-known `RawPointer<T>` type,
- the compiler-provided `core.memory` declarations,
- the trusted predicates used to state raw memory facts,
- the trusted capabilities required by raw memory operations,
- the standard-library raw memory surface that wraps `core.memory`.

Raw pointers are not references.

Raw pointers are easy to pass around and impossible to use accidentally as ordinary typed access paths.

Every operation that reads, writes, initializes, copies, reinterprets, aliases, allocates, or deallocates raw memory remains gated by
trusted capabilities, trusted predicate facts, or both.

The `core.memory` declarations in this model are compiler-provided compiler-known declarations.

The Compiler-Known and Standard Library Model defines how compiler-provided declarations exist, how source refers to them, and how
compilers conform to their specified semantics.

`core.memory` is a compiler-known namespace.

It is not a source package.

It is not a standard-library package.

It is not implemented by standard-library source.

Bodyless declarations shown under `core.memory` are specification notation for compiler-provided declarations.

They are not a source-level declaration form that user packages or standard-library packages can write.

Unless a declaration in this model explicitly states target-conditional availability, it is required on every target that a
conforming compiler supports.

---

## Compiler conformance

A conforming compiler must provide the `core.memory` declarations exactly as specified by this model.

A compiler must not add additional `core.memory` declarations.

A compiler must not remove, rename, shadow, overload, replace, or change the signature of a `core.memory` declaration.

A compiler must not change parameter names, parameter modifiers, generic parameters, result types, contracts, trusted obligations,
trusted capabilities, fact production rules, fact invalidation rules, ownership effects, borrowing effects, initialization effects,
destruction effects, finalization effects, allocation effects, aliasing effects, panic behavior, or evaluation-order behavior.

The observable semantics of `core.memory` declarations are the semantics in this model.

The implementation strategy is not observable Bray semantics.

A compiler can lower `core.memory` declarations through target instructions, target intrinsics, runtime calls, allocator hooks,
platform APIs, inline code generation, metadata operations, or any other mechanism that preserves the specified Bray semantics.

If a selected target cannot support any required `core.memory` declaration, the compiler must reject that target before code
generation.

The compiler must not silently substitute a different raw memory contract for that target.

If a target has stricter alignment, address-space, allocation, or ABI constraints than another target, those constraints enter Bray
through target facts used by this model's contracts.

They do not change the declaration surface.

They do not allow a compiler to make a well-formed Bray program mean something different from the semantics defined here.

`address_of` and `address_of_mut` produce raw pointer values to the storage reached by their borrow arguments without extending the
borrow lifetime or creating later ordinary borrow protection.

`null` produces a null raw pointer value and no validity facts.

`is_null` observes the raw pointer value and does not read reached storage.

`offset` and `byte_offset` compute raw pointer values and do not read or write reached storage.

`reinterpret` changes the raw pointer element type and does not read or write reached storage.

`read` observes initialized storage and produces a `T` according to the source type's movement and copying rules.

`write` stores a `T` into raw storage and establishes initialized storage for `T` when its preconditions are satisfied.

`copy` copies the representation bytes of `count` initialized `T` values from source to destination and requires the source and
destination ranges not to overlap.

`copy_overlapping` copies the representation bytes of `count` initialized `T` values from source to destination as if those bytes
were first preserved in temporary storage, so overlapping source and destination ranges are valid when the other preconditions
hold.

On normal completion, `allocate` creates a distinct raw allocation described by the produced `owned_allocation` fact and writable by
the produced `valid_write` fact.

If `allocate` cannot create an allocation satisfying its contract, it panics.

`allocate` must not return a null pointer or any other pointer value that lacks the produced trusted facts.

`deallocate` releases the allocation described by its required `owned_allocation` fact and invalidates every trusted fact that
depends on that allocation.

`std.memory` declarations are not alternate implementations of `core.memory`.

They are ordinary standard-library declarations that can call or wrap `core.memory` declarations while preserving their contracts.

Different standard-library implementations can organize those wrappers differently, but they cannot change the `core.memory`
contract.

---

## Raw pointer type

`RawPointer<T>` is a compiler-known protected-representation value type.

`T` must be a sized type.

`RawPointer<T>` is copyable.

Copying a raw pointer copies the pointer value.

Copying a raw pointer does not copy, borrow, move, initialize, destroy, or otherwise affect the reached storage.

`RawPointer<T>` is not an owner type.

A raw pointer does not carry an automatic lifetime.

A raw pointer does not carry ordinary borrow protection.

A raw pointer does not imply that the address is non-null, valid, aligned, initialized, live, in-bounds, uniquely reachable, or part
of any allocation.

A raw pointer can represent a null address.

A raw pointer can represent a dangling, unaligned, uninitialized, invalid, or otherwise unusable address.

Validity is never implied by `RawPointer<T>` alone.

Raw pointers have no ordinary dereference syntax.

Raw pointers have no indexing syntax.

Raw pointers have no field access syntax.

Raw pointers have no pointer arithmetic syntax.

Raw pointers do not implicitly convert to or from integer types.

Raw pointers are usable only through compiler-known raw memory declarations, standard-library wrappers over those declarations,
ordinary copying, ordinary assignment, ordinary parameter passing, and ordinary return.

---

## Raw pointer creation

The compiler-known raw pointer creation declarations are under `core.memory`.

```bray
func address_of<T>(pos value: &T) -> RawPointer<T>;

func address_of_mut<T>(pos value: &mut T) -> RawPointer<T>;

func null<T>() -> RawPointer<T>;

func is_null<T>(pos pointer: RawPointer<T>) -> bool;

func offset<T>(pos pointer: RawPointer<T>, elements: isize) -> RawPointer<T>;

func byte_offset<T>(pos pointer: RawPointer<T>, bytes: isize) -> RawPointer<T>;
```

`address_of` produces a raw pointer to the storage reached by a shared borrow.

`address_of_mut` produces a raw pointer to the storage reached by a mutable borrow.

Creating a raw pointer from a borrow does not extend the borrow lifetime.

Creating a raw pointer from a borrow does not transfer ownership.

Creating a raw pointer from a borrow does not create ordinary borrow protection for later raw pointer use.

`null<T>()` produces a raw pointer value that carries no validity facts.

`is_null` observes whether a raw pointer value is the null pointer value for its type.

`is_null` does not read reached storage.

`offset` computes a raw pointer offset by a count of `T` elements.

`byte_offset` computes a raw pointer offset by a count of bytes.

Offset operations do not read or write memory.

Offset operations do not prove that the resulting pointer is valid.

Offset operations do not preserve trusted facts unless a trusted predicate or compiler-recognized rule explicitly states that the
fact still holds for the resulting pointer.

---

## Raw pointer reinterpretation

Raw pointer reinterpretation changes the pointer's element type without reading or writing memory.

```bray
trusted func reinterpret<Target, Source>(pos pointer: RawPointer<Source>) -> RawPointer<Target>
    uses(layout_reinterpret);
```

`reinterpret` creates no validity, alignment, initialization, ownership, or aliasing facts.

Using the resulting pointer for memory access requires the ordinary trusted facts for the target type.

---

## Raw memory access

The compiler-known raw memory access declarations are under `core.memory`.

```bray
trusted func read<T>(pos pointer: RawPointer<T>) -> T
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
        trusted core.memory.initialized_as<T>(pointer = pointer),
    )
    uses(raw_memory);

trusted func write<T>(pos pointer: RawPointer<T>, pos value: T) -> unit
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

For a copyable `T`, `read` can preserve the source storage's initialized state.

For a non-copyable `T`, `read` moves the value out of raw storage and invalidates the trusted fact that the source storage remains
initialized as `T`.

`write` stores the supplied value into raw memory.

If the destination storage already contains a live initialized value, the caller must satisfy that value's destruction,
finalization, and ownership obligations before `write` overwrites the storage.

`write` can initialize previously uninitialized storage.

---

## Raw memory copy

The compiler-known raw memory copy declarations are under `core.memory`.

```bray
trusted func copy<T>(
    pos source: RawPointer<T>,
    pos destination: RawPointer<T>,
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
    pos source: RawPointer<T>,
    pos destination: RawPointer<T>,
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

It does not run constructors, finalizers, destructors, assignment behavior, operator behavior, or user-defined copy behavior.

The caller is responsible for ensuring that raw memory copying preserves every ownership, initialization, finalization, aliasing,
and representation invariant required by the involved type and storage.

---

## Manual allocation

The compiler-known manual allocation declarations are under `core.memory`.

```bray
trusted func allocate(bytes: usize, align: usize) -> RawPointer<u8>
    ensures(
        trusted core.memory.owned_allocation(pointer = result, bytes = bytes, align = align),
        trusted core.memory.valid_write(pointer = result, count = bytes),
    )
    uses(manual_alloc);

trusted func deallocate(pos pointer: RawPointer<u8>, bytes: usize, align: usize) -> unit
    requires(
        trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align),
    )
    uses(manual_alloc);
```

`allocate` returns raw byte-addressed storage.

`allocate` does not initialize typed values in the returned storage.

If `allocate` cannot create an allocation satisfying its contract, it panics.

An allocation failure panic is an ordinary panic and can be caught by `catch`.

`allocate` must not return a null pointer or any other sentinel value to report allocation failure.

`deallocate` releases the allocation represented by its trusted ownership fact.

Before deallocation, the caller must satisfy all destruction, finalization, initialization, aliasing, and borrowing obligations for
values stored in that allocation.

After deallocation, trusted facts that depend on the allocation are invalidated.

Raw pointers into a deallocated allocation can still exist as raw pointer values, but they carry no valid access facts.

---

## Trusted raw memory predicates

The compiler-known trusted raw memory predicates are under `core.memory`.

```bray
trusted predicate valid_read<T>(pointer: RawPointer<T>, count: usize);

trusted predicate valid_write<T>(pointer: RawPointer<T>, count: usize);

trusted predicate initialized_as<T>(pointer: RawPointer<T>);

trusted predicate initialized_range_as<T>(pointer: RawPointer<T>, count: usize);

trusted predicate aligned_for<T>(pointer: RawPointer<T>);

trusted predicate non_overlapping<T>(
    left: RawPointer<T>,
    left_count: usize,
    right: RawPointer<T>,
    right_count: usize,
);

trusted predicate owned_allocation(pointer: RawPointer<u8>, bytes: usize, align: usize);

trusted predicate same_allocation<T>(left: RawPointer<T>, right: RawPointer<T>);
```

`valid_read` means the range can be read as raw storage for `count` values of `T`.

`valid_write` means the range can be written as raw storage for `count` values of `T`.

`initialized_as` means the pointed-to storage contains an initialized value of type `T`.

`initialized_range_as` means the pointed-to range contains `count` initialized values of type `T`.

`aligned_for` means the pointer satisfies the alignment requirements of `T`.

`non_overlapping` means the two typed ranges cannot overlap.

`owned_allocation` means the caller owns the allocation described by the pointer, byte count, and alignment.

`same_allocation` means both pointers are derived from the same allocation.

An `owned_allocation` fact can establish `aligned_for<T>` for pointers into the allocation when the allocation alignment, offset,
and target type alignment prove the typed pointer is aligned for `T`.

Trusted raw memory facts are tied to the allocation, storage state, pointer value, element type, count, alignment, capability state,
and epoch they mention.

Trusted raw memory facts are invalidated by deallocation, reallocation, movement out of raw storage, destruction, finalization,
initialization-state changes, layout reinterpretation, device transfer, foreign calls, unchecked aliasing, or any other operation
whose contract can affect the mentioned storage.

---

## Capability mapping

Raw memory operations use trusted capabilities according to the operation they perform.

| Operation kind                                          | Required trusted capability |
|---------------------------------------------------------|-----------------------------|
| Raw read, write, and copy                               | `raw_memory`                |
| Manual initialization or uninitialized storage handling | `unchecked_init`            |
| Pointer reinterpretation across element types           | `layout_reinterpret`        |
| Manual allocation and deallocation                      | `manual_alloc`              |
| Aliasing beyond ordinary proof power                    | `unchecked_alias`           |
| Foreign memory or calls outside Bray's semantic model   | `foreign_call`              |
| Device-owned or accelerator-owned memory                | `device_memory`             |
| Compiler-recognized target operations                   | `intrinsic`                 |

A trusted declaration must declare exactly the trusted capabilities it uses.

A standard-library wrapper over a trusted raw memory operation must preserve the same trusted caller obligations unless it proves or
establishes those obligations itself.

---

## Standard-Library Raw Memory Surface

The standard library provides the ordinary user-facing raw memory surface under `std.memory`.

The standard-library root for these declarations is `std.memory`.

The raw-memory-facing standard-library surface includes:

- raw pointer helpers,
- allocator and allocation-owner helpers,
- raw buffer helpers,
- device memory helpers,
- ABI and layout helpers.

All `std.memory` declarations are ordinary standard-library declarations.

They use ordinary import and path visibility rules.

The compiler can recognize selected `std.memory` declarations by stable declaration identity.

Recognition is used for checking, diagnostics, optimization, const evaluation, target availability, and trusted fact propagation.

Recognition does not make a declaration ambient.

If the relevant `std.memory` declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration has the same name but not the recognized standard-library identity, it is checked as an ordinary
declaration.

A `std.memory` declaration that directly wraps a `core.memory` declaration must preserve that declaration's trusted capability,
trusted predicate, ownership, borrowing, initialization, destruction, finalization, aliasing, panic, cancellation, memory-ordering,
evaluation-order, and fact-invalidation contract.

A `std.memory` declaration can expose an ordinary safe API only when it proves, owns, or establishes every trusted fact required by
the `core.memory` operation it performs.

A `std.memory` declaration that exposes a trusted caller obligation must write that obligation in its own contract.

Calling a trusted `std.memory` declaration follows the ordinary trust model.

The standard library cannot create new raw-memory trusted facts except through compiler-recognized declarations whose contracts are
defined by this model.

---

## Raw Pointer Helpers

The raw pointer helper family mirrors the pointer-producing, pointer-observing, pointer-offsetting, reinterpretation, access, copy,
allocation, and deallocation operations of `core.memory`.

Specification notation for the required helper family:

```bray
module std.memory;

func address_of<T>(pos value: &T) -> RawPointer<T>;

func address_of_mut<T>(pos value: &mut T) -> RawPointer<T>;

func null<T>() -> RawPointer<T>;

func is_null<T>(pos pointer: RawPointer<T>) -> bool;

func offset<T>(pos pointer: RawPointer<T>, elements: isize) -> RawPointer<T>;

func byte_offset<T>(pos pointer: RawPointer<T>, bytes: isize) -> RawPointer<T>;

trusted func reinterpret<Target, Source>(pos pointer: RawPointer<Source>) -> RawPointer<Target>
    uses(layout_reinterpret);

trusted func read<T>(pos pointer: RawPointer<T>) -> T
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
        trusted core.memory.initialized_as<T>(pointer = pointer),
    )
    uses(raw_memory);

trusted func write<T>(pos pointer: RawPointer<T>, pos value: T) -> unit
    requires(
        trusted core.memory.valid_write(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<T>(pointer = pointer),
    )
    ensures(
        trusted core.memory.initialized_as<T>(pointer = pointer),
    )
    uses(raw_memory, unchecked_init);

trusted func copy<T>(
    pos source: RawPointer<T>,
    pos destination: RawPointer<T>,
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
    pos source: RawPointer<T>,
    pos destination: RawPointer<T>,
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

The non-trusted pointer helpers preserve the same semantics as the matching `core.memory` declarations.

The trusted pointer helpers preserve the same caller obligations as the matching `core.memory` declarations.

The helper declarations can add ordinary checked convenience around argument validation, but they cannot weaken the trusted facts
required by the raw operation they perform.

Examples of recognized helper calls:

```bray
let next = std.memory.offset(pointer, elements = 1);
let value = trusted std.memory.read<u32>(pointer);
trusted std.memory.write<u32>(pointer, value);
trusted std.memory.copy(source, destination, count = count);
```

The standard library can expose method-style wrappers when the corresponding declarations are visible:

```bray
let next = pointer.offset(elements = 1);
let value = trusted pointer.read();
trusted pointer.write(value);
```

Method-style wrappers are ordinary standard-library declarations.

They do not grant raw memory capabilities.

They do not hide trusted caller obligations.

They do not make raw pointers behave like references.

---

## ABI And Layout Helpers

ABI and layout helpers expose target-dependent layout facts used by allocation, raw buffers, FFI support, and low-level storage
code.

Specification notation for the required ABI and layout helper family:

```bray
module std.memory;

struct MemoryLayout
{
    bytes: usize;
    align: usize;
}

union MemoryLayoutError
{
    SizeOverflow;
    UnsupportedAlignment;
}

const func size_of<T>() -> usize;

const func align_of<T>() -> usize;

const func stride_of<T>() -> usize;

const func layout_of<T>(count: usize) -> Result<MemoryLayout, MemoryLayoutError>;
```

`size_of<T>()` is the size in bytes of one initialized `T` value for the selected target profile.

`align_of<T>()` is the required alignment in bytes for `T` for the selected target profile.

`stride_of<T>()` is the byte distance between adjacent `T` elements in a contiguous typed allocation.

`layout_of<T>(count = count)` computes the allocation layout required for `count` contiguous `T` slots.

These helpers observe the effective layout contract of `T`.

For user-declared product and union types, the effective layout contract comes from the type's `@layout(...)` directive when one is
declared.

For user-declared product and union types without an explicit layout directive, the effective layout contract is the
compiler-selected default layout for the selected target profile.

For compiler-known type forms and compiler-known protected-representation types, the effective layout contract is defined by the
language model that owns that type form or type.

The helper result for an explicit layout contract is stable according to that layout contract.

The helper result for compiler-defined default layout is valid for the selected target profile but does not create public ABI
stability.

`MemoryLayout` values produced by `layout_of` carry the standard-library allocation-layout contract for their `bytes` and `align`
fields.

Constructing a `MemoryLayout` value from arbitrary field values is valid only when the surrounding context proves the same
allocation-layout contract.

`layout_of<T>(count = count)` returns `Result.Error(error = MemoryLayoutError.SizeOverflow)` when the byte count cannot be
represented as `usize`.

`layout_of<T>(count = count)` returns `Result.Error(error = MemoryLayoutError.UnsupportedAlignment)` when the selected target
cannot represent the required alignment for allocation.

The ABI and layout helper results are target-dependent constants when their inputs are constant.

The compiler records the target facts used by these helpers in compiled interface metadata according to the Compiler-Known and
Standard Library Model.

These helpers do not read memory, write memory, allocate, deallocate, initialize storage, destroy values, create raw pointer
validity facts, or create allocation ownership facts.

---

## Allocation Owners

`RawAllocation` is the standard-library linear owner for one untyped raw allocation.

Specification notation for the required allocation owner family:

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

A `RawAllocation` value carries the trusted allocation ownership fact for its allocation.

The trusted allocation ownership fact is not implied by the visible field values.

Only compiler-recognized allocator declarations, trusted declarations that establish the required facts, or movement of an existing
`RawAllocation` can create a `RawAllocation` value that carries allocation ownership.

Constructing a `RawAllocation` value from arbitrary field values is rejected unless the surrounding trusted context establishes the
required allocation ownership facts for those fields.

`std.memory.allocate(layout)` calls or wraps `core.memory.allocate(bytes = layout.bytes, align = layout.align)`.

On normal completion, `std.memory.allocate(layout)` returns a `RawAllocation` whose fields identify the allocation and whose value
carries the allocation ownership facts.

Allocation failure panics.

`std.memory.allocate` must not report allocation failure through null pointers, sentinel layouts, partially initialized allocation
owners, or target-specific status codes.

`std.memory.deallocate(allocation)` consumes a `RawAllocation`.

`std.memory.deallocate` releases the allocation ownership fact carried by the consumed value.

Before deallocation, every initialized typed value, borrow, scoped capability, finalization obligation, and trusted fact tied to the
allocation must already be resolved or invalidated according to its contract.

Destroying a live `RawAllocation` deallocates the allocation when its contract proves that no initialized typed value, borrow,
scoped capability, finalization obligation, or unresolved trusted fact remains tied to the allocation.

If those obligations cannot be proven resolved at the destruction point, destruction of the `RawAllocation` is rejected.

Moving a `RawAllocation` transfers the allocation ownership fact.

Observing `allocation.pointer`, `allocation.bytes`, or `allocation.align` does not transfer the allocation ownership fact.

Copying the raw pointer field does not copy allocation ownership.

---

## Raw Buffers

`RawBuffer<T>` is the standard-library linear owner for contiguous raw storage intended to hold values of `T`.

Specification notation for the required raw buffer family:

```bray
module std.memory;

struct RawBuffer<T>
{
    pointer: RawPointer<T>;
    capacity: usize;
    initialized: usize;
}

trusted func create_buffer<T>(capacity: usize) -> Result<RawBuffer<T>, MemoryLayoutError>
    uses(manual_alloc, layout_reinterpret);

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

`create_buffer<T>(capacity = capacity)` computes `layout_of<T>(count = capacity)`.

If the layout cannot be represented for the selected target profile, `create_buffer` returns the corresponding
`MemoryLayoutError`.

When `create_buffer` returns `Result.Error`, no allocation owner is created.

If allocation fails after the layout is valid, `create_buffer` panics according to the allocation rules.

When `create_buffer` returns `Result.Ok(value = buffer)`, `buffer.capacity == capacity` and `buffer.initialized == 0`.

`RawBuffer<T>` owns its allocation and deallocates it when destroyed.

`RawBuffer<T>` tracks an initialized prefix.

The `RawBuffer<T>` type contract requires `initialized <= capacity`.

The initialized prefix contains `initialized` contiguous `T` values starting at `pointer`.

Slots from `initialized` up to `capacity` are spare raw storage.

Safe buffer operations can observe, borrow, mutate, move, destroy, and initialize only according to the initialized-prefix contract.

The safe slice-producing helpers expose only the initialized prefix.

`spare_pointer` exposes the beginning of the spare storage and preserves the trusted caller obligations needed to write into that
storage.

`set_initialized_count` is trusted because the caller must prove that the new initialized prefix truly contains initialized `T`
values and that every removed initialized value has had its destruction and finalization obligations resolved.

Destroying a `RawBuffer<T>` destroys or finalizes initialized elements in increasing index order, then deallocates the raw
allocation.

Moving a `RawBuffer<T>` transfers the allocation ownership fact and the initialized-prefix contract.

Observing `buffer.pointer`, `buffer.capacity`, or `buffer.initialized` does not transfer ownership of the allocation or initialized
elements.

Constructing a `RawBuffer<T>` value from arbitrary field values is rejected unless the surrounding trusted context establishes the
required allocation ownership, valid-write, and initialized-prefix facts for those fields.

`RawBuffer<T>` is a low-level storage owner, not a growable collection contract.

Higher-level collections can use `RawBuffer<T>` internally while exposing their own ownership, indexing, iteration, and capacity
contracts.

---

## Device Memory

Device memory support is target-conditional and lives under `std.memory.device`.

When a target profile exposes device memory facts, the standard library can provide recognized device memory declarations.

Device memory declarations are ordinary standard-library declarations whose contracts name the device, address space, access mode,
byte count, alignment, synchronization state, and transfer behavior involved.

The required device-memory declaration families are:

- device allocation owners,
- device buffer owners,
- host-to-device transfer operations,
- device-to-host transfer operations,
- device synchronization operations,
- scoped host mappings for device memory that can be mapped into host-accessible storage.

Device memory operations use the `device_memory` trusted capability.

Device memory operations that call foreign platform APIs also use `foreign_call`.

Device memory operations that read or write host raw memory also use the required `raw_memory`, `unchecked_init`, or
`unchecked_alias` capabilities for that host access.

A device allocation owner is linear and carries the ownership and release obligation for one device allocation.

A device buffer owner is linear and carries the ownership, element type, capacity, initialized-device-state, and release obligation
for one contiguous device allocation.

Device memory is not host-accessible raw memory unless a recognized mapping declaration creates a scoped host mapping.

A scoped host mapping produces the raw pointer validity, alignment, initialization, synchronization, and access facts declared by
the mapping contract.

Those facts remain valid only for the mapping's scoped capability lifetime.

Leaving the mapping scope releases the scoped capability, performs the mapping's required synchronization, and invalidates raw
pointer facts that depend on the mapping.

Transfers between host memory and device memory must state which host facts they require, which device facts they require, which
facts they establish, and which facts they invalidate.

Device transfers that can overlap with task or thread execution participate in the Async Model's cross-task and cross-thread memory
model.

---

## ABI-Oriented Raw Memory Helpers

ABI-oriented raw memory helpers are ordinary declarations that combine layout helpers, raw pointers, and explicit layout contracts.

They are used by FFI bindings, serialization code, binary parsers, packed representations, and target ABI adapters.

ABI-oriented helpers must state:

- the layout contract they rely on,
- the target ABI facts they use,
- the pointer validity and alignment facts they require,
- the initialization facts they require or establish,
- the byte order, scalar representation, padding, and niche assumptions visible in their behavior,
- the trusted capabilities used by any raw memory access, layout reinterpretation, intrinsic, or foreign call.

ABI helpers can use `size_of<T>()`, `align_of<T>()`, `stride_of<T>()`, `layout_of<T>(count = count)`, `std.memory.offset`, and
`std.memory.reinterpret` when their contracts preserve the required facts.

ABI helpers for `@layout(c)`, `@layout(transparent)`, explicit alignment, packed layout, and explicit representation use the
layout rules defined by the Type Model.

ABI helpers for default-layout types cannot expose a stable external ABI unless the type's declaration contract makes that ABI
stable.

ABI helpers cannot make an unstable layout stable by wrapping it.

ABI helpers cannot read padding bytes as meaningful values unless their contract states that the padding bytes are initialized and
observable.

ABI helpers cannot manufacture initialized values by reinterpreting bytes unless their contract establishes the required
`initialized_as<T>` or `initialized_range_as<T>` facts.

ABI helpers that call foreign code or expose foreign-owned memory must preserve the FFI and foreign-call trust boundary rules.

---

## Design principles

Raw pointers are values, not access paths.

Raw pointer use is explicit.

Raw pointer validity is contractual, not implied.

Raw memory operations are visible trust boundaries.

Ergonomic wrappers reduce call noise without reducing trust visibility.

The raw memory substrate is small and compiler-known.

Higher-level allocation, buffer, slice, FFI, and device abstractions are library features built on these contracts.
