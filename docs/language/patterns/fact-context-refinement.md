# Fact-context refinement

Successful pattern matching can add facts to the fact context.

Examples of facts established by patterns:

```text
active union variant,
literal equality,
field availability,
payload initialization,
tuple or array shape,
nullable present or absent state,
narrowed control-flow state.
```

These facts are flow-sensitive.

Facts established by a pattern are valid only within the region where the matched state remains valid.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts that depend on the affected value or storage.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Partial moves through patterns](partial-moves-through-patterns.md)
- Next: [Guards](guards.md)
