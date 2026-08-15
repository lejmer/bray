# Standard-library memory surface

The standard library provides the ordinary user-facing raw memory surface under `std.memory`.

The standard-library root for these declarations is `std.memory`.

The raw-memory-facing standard-library surface consists of:

- raw pointer helpers,
- allocator and allocation-owner helpers,
- raw buffer helpers,
- device memory helpers,
- ABI and layout helpers.

All `std.memory` declarations are ordinary standard-library declarations.

They use ordinary import and path visibility rules.

The compiler can recognize selected `std.memory` declarations by stable declaration identity.

Recognition is used for checking, optimization, const evaluation, target availability, and trusted guarantee propagation.

Recognition does not make a declaration ambient.

If the relevant `std.memory` declaration is not visible, the call is rejected by ordinary name resolution.

If a visible declaration has the same name but not the recognized standard-library identity, it is checked as an ordinary declaration.

A `std.memory` declaration that directly wraps a `core.memory` declaration must preserve that declaration's trusted capability, trusted predicate, ownership, borrowing, initialization, destruction, finalization, aliasing, panic, cancellation, memory-ordering, evaluation-order, and condition-invalidation contract.

A `std.memory` declaration can expose an ordinary safe API only when it proves, owns, or establishes every trusted guarantee required by the `core.memory` operation it performs.

A `std.memory` declaration that exposes a trusted caller obligation must write that obligation in its own contract.

Calling a trusted `std.memory` declaration follows the ordinary trust rules.

The standard library cannot create new raw-memory trusted guarantees except through compiler-recognized declarations whose contracts are defined by this chapter.

## Raw pointer helpers

The raw pointer helper family mirrors the pointer-producing, pointer-observing, pointer-offsetting, reinterpretation, access, copy, allocation, and deallocation operations of `core.memory`.

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

`std.memory` also exposes protected uninitialized storage and dependency-anchored borrow helpers.
`uninit_write` is safe because it consumes an owned value and commits initialization as one checked
operation. `assume_initialized` and `move_initialized` remain trusted and require the matching
initialization state. `borrow_from` and `borrow_mut_from` remain trusted and require explicit owner
or scoped-capability authority in addition to the raw memory predicates.

The result of an anchored borrow retains the exact authority argument as a dependency. The compiler
does not recognize synchronization guard, mapped region, output wrapper, or foreign owner names.
Ordinary library products build safe accessors by storing authority and using these operations.

`Output<T>` owns one protected output slot. `InPlace<T>` borrows an existing protected slot for
scoped construction. Their safe `write` operations commit Bray values. Native boundaries use the
trusted pointer and value-extraction methods after establishing the initialization predicate.
`destroy_initialized` consumes the tracked value through ordinary lifecycle cleanup.
`AnchoredView<T, Owner>` and
`AnchoredViewMut<T, Capability>` store the checked borrow with its exact authority. Their safe
accessors use ordinary reborrowing and never perform another raw-to-borrow conversion.

The helper declarations can add ordinary checked convenience around argument validation, but they cannot weaken the trusted guarantees required by the raw operation they perform.

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

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Uninitialized storage and anchored borrows](uninitialized-storage-and-anchored-borrows.md)
- Next: [Raw allocation and buffers](raw-allocation-and-buffers.md)
