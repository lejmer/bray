# Type categories

Bray has multiple type categories.

The type categories are:

- scalar types,
- product types,
- union types,
- tuple types,
- fixed-size and incomplete-extent array types,
- slice types,
- nullable types,
- borrow types,
- raw pointer types,
- trait-view types,
- owned-indirection types,
- callable types.

Scalar types are integers, real floating-point types, complex floating-point types, machine-sized integer types, `bool`,
`char`, `unit`, and `never`.

`char` contains exactly one Unicode scalar value: a code point from U+0000 through U+10FFFF excluding the surrogate
range U+D800 through U+DFFF. Character literals and conversions from integer code points reject values outside this
set. The scalar's UTF-8 encoding may occupy one to four bytes.

Product types are named types with fields or bodyless named storage contracts. A bodyless product can be incomplete or
explicitly sized and aligned opaque storage.

Union types are closed alternatives with one semantic active variant.

Compiler-known result types are named union types with language-defined variant contracts.

The compiler-known `Future<T>` computation and `Task<T>` handle types are protected linear ownership types with
language-defined async and run-boundary contracts.

Tuple types are fixed-size ordered product types.

Fixed-size array types are fixed-size ordered homogeneous product types.

An incomplete-extent array type `[T; ..]` describes unsized trailing storage. It is valid only where an explicit layout
contract admits a final flexible field and has no standalone value construction form.

Slice types are unsized contiguous sequence types.

Nullable types are produced by the postfix nullable type form `T?`.

Borrow types are produced by `&T` and `&mut T`.

Raw pointer types are produced by the compiler-known generic type `RawPointer<T>`.

Trait-view types are produced by the `view` type form.

Owned-indirection types are produced by the `box` type form.

Callable types are produced by the `func(...) -> ...` type form.

The compiler-known `string` type is a named protected-representation value type, not a scalar type.

The scalar and literal rules define the exact scalar type set, literal typing, scalar operation behavior, and scalar
conversion rules.

[Raw pointer type](../targets-layout-abi-and-raw-memory/raw-pointer-type.md) defines `RawPointer<T>`, raw pointer
validity, raw memory operations, raw memory trusted predicates, and raw pointer standard-library helpers.

All other categories listed here are defined by the product, union, compiler-known result union, compiler-known async
computation, compiler-known task handle, and type-form sections of this chapter.

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Previous: [Type identity](type-identity.md)
- Next: [Scalar Types](scalar-types.md)
