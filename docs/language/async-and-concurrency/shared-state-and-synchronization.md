# Shared state and synchronization

Shared mutable state is valid only when access is mediated by a type or declaration contract that defines the synchronization behavior for that state.

A synchronization contract must identify:

- the storage or resource it protects,
- the access capability granted while synchronization is held,
- whether the access is shared observation, exclusive mutation, transfer, or atomic mutation,
- the lifetime of the granted capability,
- the operations that acquire and release the synchronization,
- the synchronization edges established by those operations,
- the cancellation, panic, destruction, and finalization behavior while the synchronization is held.

Scoped synchronization fits the ordinary `enter` and `exit` lifecycle rules.

The `enter` declaration produces a scoped capability that grants access to the protected storage.

The matching `exit` declaration releases that scoped capability and establishes the release behavior declared by the type.

The scoped capability cannot escape its valid scope unless its type contract explicitly preserves the protected storage, synchronization state, and dependency contract.

Library synchronization declarations are ordinary declarations.

They become meaningful to the compiler through their language-defined or recognized standard-library contracts, not through special call syntax.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Data-race prevention](data-race-prevention.md)
- Next: [Atomic operation contracts](atomic-operation-contracts.md)
