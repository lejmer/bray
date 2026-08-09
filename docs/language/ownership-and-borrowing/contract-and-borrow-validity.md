# Contract and borrow validity

An established guarantee and a borrow remain usable only while their required storage, initialization state, capability state,
ownership state, and dependency contract remain valid.

Mutation through a valid mutable borrow makes guarantees that depend on the changed storage unavailable.

Movement makes guarantees and borrows that depend on the moved storage unavailable.

Destruction makes guarantees and borrows that depend on the destroyed storage unavailable.

Reinitialization makes guarantees and borrows that depend on the old value unavailable.

Finalization can make guarantees and borrows unavailable according to the finalizer's contract.

Active-variant replacement makes active-variant guarantees, payload guarantees, and borrows that depend on the previous active
variant or old payload storage unavailable.

Assigning `none` to a nullable access path makes guarantees and borrows that depend on the previous present contained value or
borrow unavailable.

Capability loss makes guarantees and borrows that require the lost capability unavailable.

A borrow becomes invalid when:

- the reached storage stops being valid,
- the reached storage stops being initialized in the way the borrow requires,
- the borrow's capability contract stops being satisfied,
- the nullable access path holding the borrow is assigned `none`,
- a type-specific invalidation rule invalidates the borrow.

Observation through a shared borrow preserves guarantees when the observation cannot mutate, move, destroy, reinitialize,
finalize, or otherwise change their dependencies.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Scope exits and ownership boundaries](scope-exits-and-ownership-boundaries.md)
- Next: [Summary](summary.md)
