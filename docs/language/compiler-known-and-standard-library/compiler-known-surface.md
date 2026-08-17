# Compiler-known surface

The compiler-known surface consists of:

- the built-in scalar type names `bool`, `char`, `unit`, and `never`, together with the integer, real, complex, and machine-sized
  integer type families defined by the scalar-type rules,
- the compiler-known text type `string`,
- the compiler-known half-open integer range type `Range<T>`,
- the compiler-known raw pointer type `RawPointer<T>`,
- the structural tuple, fixed-size array, slice, nullable, borrow, trait-view, owned-indirection, and callable type forms,
- the compiler-known `Heap` storage-policy type used by default-storage owned indirection,
- compiler-known async and run-boundary types `Future<T>`, `Task<T>`, `RunResult<T>`, and `PanicReport`,
- the compiler-known result and conversion types `Result<T, E>` and `ConversionError`,
- compiler-known execution-context predicates `blocking_execution()`, `compute_execution()`, and `main_thread_execution()`,
- compiler-known raw memory declarations and trusted predicates under `core.memory`,
- compiler-known target properties under the ambient `target` path,
- the compiler-known type-form support trait `Storage<T>`,
- the compiler-known iteration traits `Iterable` and `Iterator`,
- the compiler-known conversion traits `ConvertTo<Target>` and `CheckedConvertTo<Target>`,
- compiler-known indexing traits `ElementIndex<Selector>`, `MutableElementIndex<Selector>`, `SliceIndex<Bound>`, and
  `MutableSliceIndex<Bound>`,
- the compiler-known `Copyable` contract used by static constraints,
- the compiler-known operator traits defined by the type rules, including `Add<Rhs>`, `Equatable<Rhs>`, and `Comparable<Rhs>`,
- the compiler-known `Ordering` result used by relational comparison,
- the compiler-known literals and special values `true`, `false`, `unit`, and `none`.

`Heap` is an ambient compiler-known struct declaration with the stable compiler-known declaration key `Heap`. Default-storage
owned indirection refers to that exact declaration identity, not to a source declaration that happens to use the same spelling.

Each compiler-known entity is governed by the owning rules for that entity.

Some compiler-known entities have target availability rules.

When a compiler-known entity is target-unavailable, source that uses it is rejected before code generation.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Target-conditional declarations](target-conditional-declarations.md)
- Next: [Compiler-known trait implementations](compiler-known-trait-implementations.md)
