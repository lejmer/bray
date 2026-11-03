# Overview

A **type** defines a set of values and the operations permitted on those values and their storage. It also determines
which obligations those operations create.

Every value has a type. Every expression has a type or produces a compile-time entity governed by type-checking rules.
An access path has both a type and a capability state.

Initialization, construction, pattern checking, conversion, and callable checking use the types of the values and
storage involved. Ownership rules govern movement, copying, borrowing, and mutation authority, including the resulting
destruction and finalization obligations. Contract checking and condition refinement constrain which operations are
valid at a program point, together with their effects and required capabilities.

A type can define structure, behavior, lifecycle rules, construction rules, conversion rules, and visibility rules.
Its representation follows the [layout contracts](../targets-layout-abi-and-raw-memory/layout-contracts.md). Public type
contracts also constrain API compatibility, as described in [type identity](type-identity.md).

## Navigation

- [Language index](../index.md)
- [Types index](../types.md)
- Next: [Type identity](type-identity.md)
