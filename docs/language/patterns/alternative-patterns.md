# Alternative patterns

An alternative pattern matches when any of its alternatives match.

```bray
Error | Cancelled
```

All alternatives are checked against the same subject type.

All alternatives bind the same set of names.

Each shared name denotes one logical binding in the matched region and has the same type in every alternative.
Each occurrence follows the selected [pattern operation mode](pattern-operation-modes.md), including its ownership,
borrow, and mutation authority. The alternatives cannot grant a later use authority missing from a possible binding
source. Dependencies and source projections follow
[flow-state merging](../expressions/expression-results.md#flow-state-merge).

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Box patterns](box-patterns.md)
- Next: [Refutability](refutability.md)
