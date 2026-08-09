# Overview

Target, layout, ABI, and raw memory rules define the lowest-level interfaces between Bray source and the selected compilation target.

They cover:

- target profiles and properties,
- target constraints and `@target(...)` gates,
- explicit data layout through `@layout(...)`,
- callable ABI contracts through `@abi(...)`,
- foreign callable imports and exports,
- raw pointer values,
- compiler-known `core.memory` declarations,
- trusted raw memory conditions and capabilities,
- standard-library `std.memory` wrappers,
- device memory and ABI-oriented helper contracts.

Default Bray semantics do not expose stable physical layout, foreign ABI representation, raw storage validity, or raw pointer dereference.

Source code can depend on those details only through explicit contracts.

Raw memory support is part of Bray's trusted substrate.

Every operation that reads, writes, initializes, copies, reinterprets, aliases, allocates, deallocates, or exposes raw memory remains gated by trusted capabilities, trusted predicate conditions, or both.

## Navigation

- [Language index](../index.md)
- [Targets, layout, ABI, and raw memory index](../targets-layout-abi-and-raw-memory.md)
- Next: [Target profiles and properties](target-profiles-and-properties.md)
