# Summary

Ownership tracks which owner is responsible for a value's obligations.

Access paths describe storage and substorage that operations can observe, mutate, move, borrow, or destroy.

Moves transfer ownership obligations.

Copies are allowed only by copy contracts and do not move from the source.

Borrows create temporary non-owning access to reached storage.

Mutable borrows require temporary exclusive mutation authority.

Shared borrows can coexist when their capabilities are compatible.

Dependency contracts replace source-level lifetime annotations.

Product-static borrows carry product roots. Thread-local static borrows additionally carry exact native-thread attachment roots.

Partial moves are allowed only when the remaining partial state is accounted for.

Values can cross ownership boundaries only when the destination preserves every dependency contract carried by the value.

Conditions and borrows are invalidated when their storage, capability, ownership, initialization, or dependency requirements stop holding.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Condition and borrow invalidation](contract-and-borrow-validity.md)
