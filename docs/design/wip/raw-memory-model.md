# Raw memory model

## Overview

Raw memory support is the lowest-level part of Bray's trusted substrate.

The raw memory model defines:

- the compiler-known `RawPointer<T>` type,
- the compiler-provided `core.memory` declarations,
- the trusted predicates used to state raw memory facts,
- the trusted capabilities required by raw memory operations,
- the standard-library convenience surface for raw pointers.

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

## Standard-library convenience surface

The standard library can provide convenience declarations for raw pointer work.

The standard-library root for these declarations is `std.memory`.

Standard-library raw pointer helpers are ordinary standard-library declarations.

They are not syntax.

They are not traits.

They use ordinary import and path visibility rules.

The compiler can recognize selected `std.memory` declarations by stable declaration identity.

Recognized helpers must preserve the same trusted capability and trusted predicate contracts as the compiler-known `core.memory`
declaration they wrap.

Examples of recognized helper calls:

```bray
let next = std.memory.offset(pointer, elements = 1);
let value = trusted std.memory.read<u32>(pointer);
trusted std.memory.write<u32>(pointer, value);
trusted std.memory.copy(source, destination, count = count);
```

The standard library can also expose method-style wrappers when the corresponding declarations are visible:

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

## Design principles

Raw pointers are values, not access paths.

Raw pointer use is explicit.

Raw pointer validity is contractual, not implied.

Raw memory operations are visible trust boundaries.

Ergonomic wrappers reduce call noise without reducing trust visibility.

The raw memory substrate is small and compiler-known.

Higher-level allocation, buffer, slice, FFI, and device abstractions are library features built on these contracts.
