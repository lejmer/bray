# Ownership states

An access path that can own storage has an initialization and ownership state.

The ordinary states are:

- uninitialized,
- partially initialized,
- fully initialized,
- moved from,
- destroyed.

An uninitialized access path does not contain a value.

A partially initialized access path contains some initialized subparts but is not usable as a complete value.

A fully initialized access path contains a complete value of its type.

A moved-from access path no longer owns the value that was moved out of it.

A destroyed access path no longer contains a usable value.

Construction expressions create fully initialized values when all required parts are initialized.

Assignment can initialize, reinitialize, or replace storage when the destination state and type contract permit that
operation.

Reinitializing initialized storage first resolves the old value according to assignment, destruction, finalization,
nullable, union replacement, and type-specific rules.

A fully initialized value can be observed, borrowed, moved, copied, consumed, assigned, finalized, or destroyed only
when the selected operation is valid for the value's type, capability state, ownership state, dependency contract, and
lifecycle obligations.

A partially initialized value can be accessed only through initialized parts when the selected operation permits
partial-state access.

Destruction of partially initialized storage destroys only initialized subparts.

Whole-value lifecycle declarations require the whole value to be fully initialized at the point where that lifecycle
declaration can run.

## Ownership state merging

At a [flow-state merge](../expressions/expression-results.md#flow-state-merge), an access path is definitely initialized
only when it is initialized on every reaching path. A move on any reaching path prevents a later use that requires the
moved value, unless the program reinitializes that storage before the use. Partial moves retain the state of the
individual parts under [partial moves](partial-moves.md).

Storage that can still be live on any path remains relevant to ownership and cleanup. A value is definitely absent only
when every reaching alternative establishes absence. Conditional initialization or movement does not authorize
whole-value observation, movement, or a whole-value lifecycle call without the state required by that operation.
Cleanup resolves the initialized parts on each path under [destruction](../lifecycle/destruction.md).

A possibly active borrow remains a restriction. An operation that needs a definitely active borrow or capability must
establish that authority on every reaching path. Finalization and task obligations cannot be discharged by a path that
does not own the obligation. Each path must complete or transfer its own obligations before ownership ends.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Storage and access paths](storage-and-access-paths.md)
- Next: [Moves, copies, and consumption](moves-copies-and-consumption.md)
