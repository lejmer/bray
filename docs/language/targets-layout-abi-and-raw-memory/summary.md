# Summary

This summary is informative. The detailed rules include:

- [target profiles and properties](target-profiles-and-properties.md).
- [layout contracts](layout-contracts.md).
- [callable abi](callable-abi.md).

Target profiles supply compiler-known properties for the selected target, and `@target(...)` gates module contributions
using target-selection expressions. Default physical layout is compiler-defined rather than public ABI. `@layout(...)`
provides explicit product and union layout contracts.

Bodyless structs describe incomplete or explicitly sized opaque storage. `[T; ..]` permits a flexible final `@layout(c)`
field, while `@layout(c, tag = none)` provides overlapping untagged C storage with semantic active-variant rules.

`@abi(...)` declares a callable ABI contract. An ellipsis after fixed parameters declares a target-supported variadic
foreign contract. `extern` imports callable bodies or static storage from another linked artifact, which can contain
Bray or non-Bray code. `@link(...)` and `@symbol(...)` identify external artifacts and symbols.

Bray statics have product or exact-thread address identity and do not implicitly export foreign data symbols.
`@symbol(...)` explicitly exports one non-generic static, while `extern static` imports provider-owned storage as a
provider-rooted `DataPointer<T>`. Target profiles contribute to closed static identity, and native-thread availability
also constrains `@thread_local` statics.

`DataPointer<T>` addresses data storage, including stored callable values. `FunctionPointer<F>` addresses raw function
entries intended for contract `F`. Both are copyable addresses that can be null or unusable and confer no access,
callability, ownership, or lifetime authority. Ordinary APIs represent absence with `T?` and `none`.

The compiler-known `core.memory` scope is reserved. Its raw memory operations require trusted guarantees, capabilities,
or both. Ordinary `std.memory` wrappers expose those contracts without making raw memory ambient. `Uninit<T>` provides
protected storage without creating a `T`, and trusted raw-to-borrow operations retain an explicit owner or scoped
capability as the borrow's dependency root.

Device memory and ABI-oriented helper declarations preserve their target, layout, ABI, synchronization, and trusted
raw-memory obligations as ordinary declaration contracts.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Previous: [Target control and inline assembly](target-control-and-inline-assembly.md)
