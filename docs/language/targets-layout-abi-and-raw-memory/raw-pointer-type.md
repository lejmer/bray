# Raw pointer type

`RawPointer<T>` is a compiler-known protected-representation value type.

`T` can be a complete data type, an incomplete bodyless struct, a product with flexible trailing storage, or an
ABI-qualified callable type.

`RawPointer<T>` is copyable.

Copying a raw pointer copies the pointer value.

Copying a raw pointer does not copy, borrow, move, initialize, destroy, or otherwise affect the reached storage.

`RawPointer<T>` is not an owner type.

A raw pointer does not carry an automatic lifetime.

A raw pointer does not carry ordinary borrow protection.

A raw pointer does not imply that the address is non-null, valid, aligned, initialized, live, in-bounds, uniquely
reachable, or part of any allocation.

A raw pointer can represent a null address.

A raw pointer can represent a dangling, unaligned, uninitialized, invalid, or otherwise unusable address.

Validity is never implied by `RawPointer<T>` alone.

Raw pointers have no ordinary dereference syntax.

Raw pointers have no indexing syntax.

Raw pointers have no field access syntax.

Raw pointers have no pointer arithmetic syntax.

Raw pointers do not implicitly convert to or from integer types.

Raw pointers are usable only through compiler-known raw memory declarations, standard-library wrappers over those
declarations, ordinary copying, ordinary assignment, ordinary parameter passing, and ordinary return.

Operations impose the pointee conditions they require. Element offsets, typed allocation, typed reads, typed writes, and
typed copies require a complete fixed-size data pointee. Byte offsets can operate on incomplete data pointees when the
target data-address contract permits them. Data-memory operations are invalid for callable pointees.

For an ABI-qualified callable type `F`, `RawPointer<F>` is a raw code address intended to designate a callable with
contract `F`. It remains non-callable until `core.memory.callable_from_pointer<F>` establishes address validity, ABI
agreement, target support, and provider liveness. Targets with distinct data and code pointer representations preserve
that distinction through the concrete `RawPointer<T>` representation and reject unsupported reinterpretations.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Foreign data and symbols](foreign-data-and-symbols.md)
- Next: [Core memory declarations](core-memory-declarations.md)
