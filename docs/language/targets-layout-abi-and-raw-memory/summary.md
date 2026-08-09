# Summary

Target profiles define the selected compilation target through compiler-known target properties.

`@target(...)` gates module contributions using target-selection expressions.

Default physical layout is compiler-defined and not public ABI.

`@layout(...)` creates explicit data layout contracts for products and unions.

`@abi(...)` creates explicit callable ABI contracts.

`extern` imports callable bodies supplied by another linked artifact. The artifact can contain Bray or non-Bray code.

`@link(...)` and `@symbol(...)` bind ABI-facing declarations to external artifacts and symbols.

`RawPointer<T>` is a copyable raw pointer value, not a reference or owner.

`core.memory` is the compiler-known raw memory declaration scope and reserved path.

Raw memory operations require trusted guarantees, trusted capabilities, or both.

`std.memory` provides ordinary standard-library wrappers over the raw memory substrate without making raw memory ambient.

Device memory and ABI-oriented helper declarations are ordinary declarations whose contracts preserve target, layout, ABI, synchronization, and trusted raw-memory obligations.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [ABI-oriented memory helpers](abi-oriented-memory-helpers.md)
