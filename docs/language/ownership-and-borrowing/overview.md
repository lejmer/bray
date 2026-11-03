# Overview

**Ownership** is responsibility for a value's obligations. These include initialized subvalues, dependency contracts,
destruction, finalization, scoped capabilities, and other ownership-related contracts attached to the value.
**Borrowing** creates temporary access to reached storage without transferring ownership of that storage.

The compiler tracks ownership together with borrowing, initialization, capability, lifecycle, and dependency-contract
state. Expressions can change that state by creating, moving, copying, borrowing, mutably borrowing, consuming, or
partially moving a value. They can also initialize, reinitialize, or destroy storage. The rules for
[storage and access paths](storage-and-access-paths.md) determine which operations are valid for the reached storage.

Bray has no source-level lifetime parameters or lifetime annotations. Lifetime and capability relationships are
inferred and checked as [dependency contracts](dependency-contracts.md).

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Next: [Storage and access paths](storage-and-access-paths.md)
