# Pattern refinement

Successful pattern matching makes the guarantees implied by the matched structure available to the guard and body governed by
that match.

General flow-sensitive contract reasoning rules are defined in [Contract reasoning](../contracts-and-trust/contract-reasoning.md).

Pattern guarantees include:

```text
active union variant,
literal equality,
field availability,
payload initialization,
tuple or array shape,
nullable present or absent state,
narrowed control-flow state.
```

These guarantees are available only within the region governed by the successful match and only while the matched state remains
valid. Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss makes a guarantee
unavailable when the guarantee depends on the affected value or storage.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Partial moves through patterns](partial-moves-through-patterns.md)
- Next: [Guards](guards.md)
