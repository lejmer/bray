# Borrow rules

A borrow is temporary non-owning access to reached storage.

Borrow expressions are defined in [Borrow expressions](../expressions/borrow-expressions.md).

Borrow type forms are defined in [Borrow type forms](../types/type-forms.md#borrow-type-forms).

A shared borrow allows observation.

A mutable borrow grants temporary exclusive mutation authority over the reached storage.

Borrowing an access path requires the reached storage to be initialized and reachable.

A shared borrow requires an access path that can be observed.

A mutable borrow requires mutation authority over the reached storage and compatible exclusivity for the duration of the borrow.

Borrow compatibility is based on reached storage, not only on the spelling of the access path.

Multiple compatible shared borrows can exist at the same time.

A mutable borrow of reached storage is incompatible with any other active borrow or operation that observes, mutates, moves, destroys, finalizes, reinitializes, or changes that reached storage, except through a valid reborrow derived from the mutable borrow.

Borrowing suspends movement, destruction, mutation, variant replacement, reinitialization, finalization, or other incompatible operations on the reached storage for the duration of the borrow.

Two borrow access paths are compatible when their required capabilities are compatible and the compiler proves that the reached storage is the same shared-readable storage or statically disjoint storage.

Disjoint field projections, tuple element projections, active payload projections, indexed element projections, and slice ranges can be borrowed independently when the compiler proves that the reached storage cannot overlap.

If overlap cannot be proven statically, the borrow access paths are treated as conflicting.

Borrow values are created only by language constructs that establish borrow capability.

These include borrow expressions, borrow-typed parameter passing, receiver calls, pattern borrow modes, and projection through compiler-known type forms.

A shared borrow of product-static storage depends on the exact owning product and static instance. It can outlive the function that
formed it when the destination preserves that product dependency.

A shared borrow of thread-static storage additionally depends on the exact native-thread attachment. It cannot be used on another
native thread, and any retained task is pinned while the dependency is live.

Static declaration paths provide no direct mutable-borrow capability. A mutable borrow of interior state can be produced only
through a valid scoped capability and remains dependent on that capability as well as the static roots.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Moves, copies, and consumption](moves-copies-and-consumption.md)
- Next: [Reborrowing and borrow values](reborrowing-and-borrow-values.md)
