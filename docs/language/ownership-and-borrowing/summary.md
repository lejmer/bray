# Summary

This summary is informative. The detailed rules include:

- [borrow rules](borrow-rules.md).
- [dependency contracts](dependency-contracts.md).
- [scope exits and ownership boundaries](scope-exits-and-ownership-boundaries.md).

Ownership tracks responsibility for a value's obligations. Access paths identify storage that operations can observe,
mutate, move, borrow, or destroy. Moves transfer ownership obligations, while copies follow copy contracts and leave the
source unmoved. Partial moves require accounting for the remaining partial state.

Borrows provide temporary non-owning access to reached storage. Mutable borrows require exclusive mutation authority,
and shared borrows coexist when their capabilities are compatible. Dependency contracts record inferred lifetime and
capability relationships without source-level lifetime annotations.

Product-static borrows carry product roots, and thread-local static borrows also carry exact native-thread attachment
roots. Extern static and dynamic-symbol pointers carry provider roots, with an additional exact-thread attachment root
for thread-local foreign storage.

A value can cross an ownership boundary only when the destination preserves all its dependency contracts. Conditions
and borrows become invalid when their required storage, capability, ownership, initialization, or dependencies cease to
hold.

## Navigation

- [Language index](../index.md)
- [Ownership and borrowing index](../ownership-and-borrowing.md)
- Previous: [Condition and borrow invalidation](contract-and-borrow-validity.md)
