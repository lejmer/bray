# Overview

Target, layout, ABI, and raw memory rules define the lowest-level interfaces between Bray source and the selected compilation target.

They cover:

- target profiles and properties,
- target constraints and `@target(...)` gates,
- explicit data layout through `@layout(...)`,
- callable ABI contracts through `@abi(...)`,
- foreign callable and data imports and exports,
- provider-owned native storage and owned dynamic symbols,
- target-specific product-static and thread-local static representation requirements,
- raw pointer values,
- compiler-known `core.memory` declarations,
- trusted raw memory conditions and capabilities,
- standard-library `std.memory` wrappers,
- device memory and ABI-oriented helper contracts.

Default Bray semantics do not expose stable physical layout, foreign ABI representation, raw storage validity, or raw pointer dereference.

Source code can depend on those details only through explicit contracts.

Raw memory support is part of Bray's trusted substrate.

Every operation that reads, writes, initializes, copies, reinterprets, aliases, allocates, deallocates, or exposes raw memory remains gated by trusted capabilities, trusted predicate conditions, or both.

Bray static declarations provide safe address-bearing source storage. A source export preserves the Bray declaration and dependency
contract. `@symbol(...)` explicitly publishes native data storage, while `extern static` names provider-owned storage and produces
a raw address. Neither form grants foreign code or Bray code an implicit safe borrow.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Next: [Target profiles and properties](target-profiles-and-properties.md)
