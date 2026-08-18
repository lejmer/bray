# Overview

Bray values are checked through ownership, borrowing, initialization, capability, lifecycle, and dependency-contract
state.

An expression can:

- create a value,
- move a value,
- copy a value,
- borrow a value,
- mutably borrow a value,
- consume a value,
- partially move a value,
- initialize storage,
- reinitialize storage,
- destroy initialized storage.

Ownership means responsibility for a value's obligations.

Those obligations include initialized subvalues, dependency contracts, destruction obligations, finalization
obligations, scoped capabilities, and other ownership-related contracts attached to the value.

Borrowing creates temporary non-owning access to reached storage.

Borrowing does not transfer ownership of the reached storage.

Bray does not expose source-level lifetime parameters or source-level lifetime annotations.

Lifetime and capability relationships are inferred and checked as dependency contracts.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Next: [Storage and access paths](storage-and-access-paths.md)
