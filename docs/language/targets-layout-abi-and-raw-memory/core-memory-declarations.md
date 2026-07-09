# Core memory declarations

The `core.memory` declarations are compiler-provided compiler-known declarations.

`core.memory` is a compiler-known declaration scope and reserved path.

It is not a source package.

It is not a standard-library package.

It is not implemented by standard-library source.

Bodyless declarations shown under `core.memory` describe compiler-provided declaration surfaces and contracts.

They are not a source-level declaration form that user packages or standard-library packages can write.

Unless a declaration explicitly states target-conditional availability, it is required on every target that a conforming compiler supports.

A conforming compiler must provide the `core.memory` declarations exactly as specified by this chapter.

A compiler must not add additional `core.memory` declarations.

A compiler must not remove, rename, shadow, overload, replace, or change the signature of a `core.memory` declaration.

A compiler must not change parameter names, parameter modifiers, generic parameters, result types, contracts, trusted obligations, trusted capabilities, fact production rules, fact invalidation rules, ownership effects, borrowing effects, initialization effects, destruction effects, finalization effects, allocation effects, aliasing effects, panic behavior, or evaluation-order behavior.

The observable semantics of `core.memory` declarations are the semantics in this chapter.

The implementation strategy is not observable Bray semantics.

A compiler can lower `core.memory` declarations through target instructions, target intrinsics, runtime calls, allocator hooks, platform APIs, inline code generation, metadata operations, or another mechanism that preserves the specified Bray semantics.

If a selected target cannot support any required `core.memory` declaration, the compiler must reject that target before code generation.

The compiler must not silently substitute a different raw memory contract for that target.

If a target has stricter alignment, address-space, allocation, or ABI constraints than another target, those constraints enter Bray through target facts used by this chapter's contracts.

They do not change the declaration surface.

They do not allow a compiler to make a well-formed Bray program mean something different from the semantics defined here.

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

Offset operations do not preserve trusted facts unless a trusted predicate or compiler-recognized rule explicitly states that the fact still holds for the resulting pointer.

Raw pointer reinterpretation changes the pointer's element type without reading or writing memory.

```bray
trusted func reinterpret<Target, Source>(pos pointer: RawPointer<Source>) -> RawPointer<Target>
    uses(layout_reinterpret);
```

`reinterpret` creates no validity, alignment, initialization, ownership, or aliasing facts.

Using the resulting pointer for memory access requires the ordinary trusted facts for the target type.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Raw pointer type](raw-pointer-type.md)
- Next: [Raw memory access and copying](raw-memory-access-and-copying.md)
