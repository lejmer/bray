# Expression results

Every expression has a checked result.

An expression result can be:

- a value,
- an access path,
- a control-flow outcome,
- a compile-time entity,
- `unit`,
- `never`.

An expression of type `unit` represents completion without meaningful data.

An expression of type `never` has no normal continuation.

An expression that produces an access path can be observed, borrowed, mutably borrowed, assigned through, moved from, copied from, consumed, or destroyed according to its type, ownership story, initialization state, and capability state.

An expression that produces a compile-time entity can participate in type checking, path resolution, contract checking, or other compile-time semantics according to the entity kind.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Overview](overview.md)
- Next: [Expression context](expression-context.md)
