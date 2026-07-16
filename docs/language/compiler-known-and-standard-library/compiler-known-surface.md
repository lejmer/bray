# Compiler-known surface

The compiler-known surface includes:

- built-in scalar type names such as `bool`, `char`, `unit`, `never`, integer types, real types, complex types, and machine-sized integer types,
- the compiler-known text type `string`,
- the compiler-known raw pointer type `RawPointer<T>`,
- structural type forms such as tuple types, fixed-size array types, slice types, nullable types, borrow types, trait-view types, owned-indirection types, and callable types,
- compiler-known async and run-boundary types `Async<T>`, `Task<T>`, `RunResult<T>`, and `PanicReport`,
- compiler-known result and conversion types such as `Result<T, E>` and `ConversionError`,
- compiler-known execution-context predicates `blocking_execution()` and `compute_execution()`,
- compiler-known raw memory declarations and trusted predicates under `core.memory`,
- compiler-known target facts under the ambient `target` path,
- compiler-known type-form support traits such as `Storage<T>`,
- compiler-known iteration traits such as `Iterable` and `Iterator`,
- compiler-known conversion traits such as `ConvertTo<Target>` and `CheckedConvertTo<Target>`,
- the compiler-known `Copyable` contract used by static constraints,
- compiler-known operator traits such as `Add<Rhs>`, `Equatable<Rhs>`, `Comparable<Rhs>`, and the other operator traits defined by the type rules,
- compiler-known literals and special values such as `true`, `false`, `unit`, and `none`.

Each compiler-known entity is governed by the owning rules for that entity.

Some compiler-known entities have target availability rules.

When a compiler-known entity is target-unavailable, source that uses it is rejected before code generation.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Target-conditional declarations](target-conditional-declarations.md)
- Next: [Compiler-known trait implementations](compiler-known-trait-implementations.md)
