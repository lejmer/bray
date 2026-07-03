# Fact and borrow invalidation

Facts and borrows remain valid only while their required storage, initialization state, capability state, ownership state, and dependency contract remain valid.

Mutation through a valid mutable borrow invalidates facts that depend on the changed storage.

Movement invalidates facts and borrows that depend on the moved storage.

Destruction invalidates facts and borrows that depend on the destroyed storage.

Reinitialization invalidates facts and borrows that depend on the old value in the reinitialized storage.

Finalization can invalidate facts and borrows according to the finalizer's contract.

Active-variant replacement invalidates active-variant facts, payload facts, and borrows that depend on the previous active variant or old payload storage.

Assigning `none` to a nullable access path invalidates facts and borrows that depend on the previous present contained value or borrow.

Capability loss invalidates facts and borrows that require the lost capability.

A borrow becomes invalid when:

- the reached storage stops being valid,
- the reached storage stops being initialized in the way the borrow requires,
- the borrow's capability contract stops being satisfied,
- the nullable access path holding the borrow is assigned `none`,
- a type-specific invalidation rule invalidates the borrow.

Observation through a shared borrow can preserve facts when the observation cannot mutate, move, destroy, reinitialize, finalize, or otherwise change the fact's dependencies.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Scope exits and ownership boundaries](scope-exits-and-ownership-boundaries.md)
- Next: [Summary](summary.md)
