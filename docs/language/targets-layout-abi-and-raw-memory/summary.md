# Summary

Target profiles define the selected compilation target through compiler-known target properties.

`@target(...)` gates module contributions using target-selection expressions.

Default physical layout is compiler-defined and not public ABI.

`@layout(...)` creates explicit data layout contracts for products and unions.

Bodyless structs describe incomplete or explicitly sized opaque storage. `[T; ..]` describes a flexible final
`@layout(c)` field. `@layout(c, tag = none)` describes overlapping untagged C storage while preserving Bray's semantic
active-variant rules.

`@abi(...)` creates explicit callable ABI contracts.

An ellipsis after fixed parameters declares a target-supported variadic foreign callable contract.

`extern` imports callable bodies or static storage supplied by another linked artifact. The artifact can contain Bray or
non-Bray code.

`@link(...)` and `@symbol(...)` bind ABI-facing declarations to external artifacts and symbols.

Bray static declarations have product or exact-thread address identity and do not implicitly create foreign data-symbol
exports. `@symbol(...)` explicitly exports one non-generic Bray static. `extern static` imports provider-owned storage
and produces a provider-rooted `RawPointer<T>`. Target profiles participate in closed static identity, and native-thread
availability also governs `@thread_local` static availability.

`RawPointer<T>` is a copyable raw pointer value, not a reference or owner. Its pointee can be complete data, incomplete
data, or an ABI-qualified callable type, with operations restricted by the pointee category.

`core.memory` is the compiler-known raw memory declaration scope and reserved path.

Raw memory operations require trusted guarantees, trusted capabilities, or both.

`std.memory` provides ordinary standard-library wrappers over the raw memory substrate without making raw memory
ambient.

`Uninit<T>` provides protected storage without creating a `T`. Trusted raw-to-borrow operations retain an explicit owner
or scoped capability as the borrow dependency root.

Device memory and ABI-oriented helper declarations are ordinary declarations whose contracts preserve target, layout,
ABI, synchronization, and trusted raw-memory obligations.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Target control and inline assembly](target-control-and-inline-assembly.md)
