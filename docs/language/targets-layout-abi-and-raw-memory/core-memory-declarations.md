# Core memory declarations

The `core.memory` declarations are compiler-provided compiler-known declarations.

`core.memory` is a compiler-known declaration scope and reserved path.

It is not a source package.

It is not a standard-library package.

It is not implemented by standard-library source.

Bodyless declarations shown under `core.memory` describe compiler-provided declaration surfaces and contracts.

They are not a source-level declaration form that user packages or standard-library packages can write.

Unless a declaration explicitly states target-conditional availability, it is required on every target that a conforming
compiler supports.

A conforming compiler must provide the `core.memory` declarations exactly as specified by this chapter.

A compiler must not add additional `core.memory` declarations.

A compiler must not remove, rename, shadow, overload, replace, or change the signature of a `core.memory` declaration.

A compiler must not change parameter names, parameter modifiers, generic parameters, result types, contracts, trusted
obligations, trusted capabilities, rules for establishing and retaining guarantees, ownership effects, borrowing
effects, initialization effects, destruction effects, finalization effects, allocation effects, aliasing effects, panic
behavior, or evaluation-order behavior.

The observable semantics of `core.memory` declarations are the semantics in this chapter.

The implementation strategy is not observable Bray semantics.

A compiler can lower `core.memory` declarations through target instructions, target intrinsics, runtime calls, allocator
hooks, platform APIs, inline code generation, metadata operations, or another mechanism that preserves the specified
Bray semantics.

If a selected target cannot support any required `core.memory` declaration, the compiler must reject that target before
code generation.

The compiler must not silently substitute a different raw memory contract for that target.

If a target has stricter alignment, address-space, allocation, or ABI constraints than another target, those constraints
enter Bray through target properties used by this chapter's contracts.

They do not change the declaration surface.

They do not allow a compiler to make a well-formed Bray program mean something different from the semantics defined
here.

The compiler-known raw pointer creation declarations are under `core.memory`.

```bray
using core.memory;

func address_of<T>(pos value: &T) -> DataPointer<T>
    ensures(
        trusted core.memory.valid_read<T>(pointer = result, count = 1),
        trusted core.memory.aligned_for<T>(pointer = result),
        trusted core.memory.initialized_as<T>(pointer = result),
        trusted core.memory.shared_alias_valid<T>(pointer = result),
        trusted core.memory.epoch_current<T>(pointer = result),
        trusted core.memory.synchronized_access<T>(pointer = result),
        trusted core.memory.movement_stable<T>(pointer = result),
        trusted core.memory.finalization_pending<T>(pointer = result),
    );

func address_of_mut<T>(pos value: &mut T) -> DataPointer<T>
    ensures(
        trusted core.memory.valid_read<T>(pointer = result, count = 1),
        trusted core.memory.valid_write<T>(pointer = result, count = 1),
        trusted core.memory.aligned_for<T>(pointer = result),
        trusted core.memory.initialized_as<T>(pointer = result),
        trusted core.memory.exclusive_alias_valid<T>(pointer = result),
        trusted core.memory.epoch_current<T>(pointer = result),
        trusted core.memory.synchronized_access<T>(pointer = result),
        trusted core.memory.movement_stable<T>(pointer = result),
        trusted core.memory.finalization_pending<T>(pointer = result),
    );

func null<T>() -> DataPointer<T>;

func is_null<T>(pos pointer: DataPointer<T>) -> bool;

func offset<T>(pos pointer: DataPointer<T>, elements: isize) -> DataPointer<T>;

func byte_offset<T>(pos pointer: DataPointer<T>, bytes: isize) -> DataPointer<T>;

trusted func callable_from_pointer<F>(pos pointer: FunctionPointer<F>) -> F
    requires(trusted core.memory.callable_address_valid<F>(pointer = pointer))
    uses(layout_reinterpret);

trusted func function_address<F>(pos value: F) -> FunctionPointer<F>
    ensures(trusted core.memory.callable_address_valid<F>(pointer = result))
    uses(layout_reinterpret);
```

`address_of` produces a raw pointer to the storage reached by a shared borrow.

`address_of_mut` produces a raw pointer to the storage reached by a mutable borrow.

Creating a raw pointer from a borrow does not extend the borrow lifetime.

Creating a raw pointer from a borrow does not transfer ownership.

Creating a raw pointer from a borrow does not create ordinary borrow protection for later raw pointer use.

The declared guarantees observe the reached storage at normal completion. They remain available only while the
source storage and the named access conditions remain valid. Copying the pointer does not extend that storage lifetime
or preserve conditions invalidated by mutation, movement, finalization, destruction, or an intervening opaque operation.

`null<T>()` produces a raw pointer value that carries no validity conditions.

`is_null` observes whether a raw pointer value is the null pointer value for its type.

`is_null` does not read reached storage.

`offset` computes a raw pointer offset by a count of `T` elements.

`offset` requires `T` to be a complete fixed-size data type.

`byte_offset` computes a raw pointer offset by a count of bytes.

`byte_offset` accepts complete or incomplete data pointees, including stored callable values. It is unavailable for
function entry addresses and target address spaces that do not support byte-address calculation.

`callable_from_pointer` and `function_address` require `F` to be an ABI-qualified capture-free callable type and a
target that supports the corresponding code-address representation. `callable_from_pointer` creates a callable value
only after its trusted precondition establishes that the pointer designates executable code with the exact contract `F`.
`function_address` preserves the callable's provider dependency on the resulting pointer.

Offset operations do not read or write memory.

Offset operations do not prove that the resulting pointer is valid.

Offset operations do not preserve trusted guarantees unless a trusted predicate or compiler-recognized rule explicitly
states that the condition still holds for the resulting pointer.

A byte-address projection observes the supplied data pointer with a byte element type.

```bray
const func byte_address<T>(pos pointer: DataPointer<T>) -> DataPointer<u8>;
```

The projection preserves the exact address, provenance and existing dependencies without accessing the reached storage.
It creates no validity, alignment, initialization, ownership or aliasing conditions and does not extend a storage lifetime.
It is defined for null and dangling pointer values. Incomplete data pointees are allowed because their layout is not
inspected. Stored callable values are data and are allowed. Function pointers and conversions between address spaces
are excluded by the parameter type.

The projection is observational, pure and total, and requires no trusted capability. Its operand must still be valid
in the constant or predicate context in which it appears. Contract reasoning preserves the pointer's symbolic identity
without exposing a numerical address or reviving invalidated evidence.

Raw pointer reinterpretation changes the pointer's element type without reading or writing memory.

```bray
trusted func reinterpret<Target, Source>(pos pointer: DataPointer<Source>) -> DataPointer<Target>
    uses(layout_reinterpret);
```

`reinterpret` creates no validity, alignment, initialization, ownership, or aliasing conditions.

This declaration changes only the element type of a data pointer. A callable stored as a value remains data.
Representation conversion for function entry addresses uses the distinct `core.memory.function` scope.

```bray
using core.memory.function;

trusted func from_data<F>(pos pointer: DataPointer<u8>) -> FunctionPointer<F>
    uses(layout_reinterpret);

trusted func to_data<F>(pos pointer: FunctionPointer<F>) -> DataPointer<u8>
    uses(layout_reinterpret);

trusted func reinterpret<Target, Source>(
    pos pointer: FunctionPointer<Source>
) -> FunctionPointer<Target>
    uses(layout_reinterpret);
```

These operations are available only when the target supports the corresponding address representations and conversion.
They preserve the represented address and existing input dependencies. They do not access storage, grant memory or
callable validity, extend provider lifetimes, or establish that a retyped function contract is valid. Null, dangling and
other unusable native values remain raw values. Creating an ordinary callable uses `callable_from_pointer` with its
existing exact-contract, ABI, target and provider-liveness requirements.

Using the resulting pointer for memory access requires the ordinary trusted guarantees for the target type.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Raw pointer type](raw-pointer-type.md)
- Next: [Raw memory access and copying](raw-memory-access-and-copying.md)
