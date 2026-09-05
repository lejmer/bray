# Pattern refinement

Successful pattern matching makes the guarantees implied by the matched structure available to the guard and body
governed by that match.

General flow-sensitive contract reasoning rules are defined in
[Contract reasoning](../contracts-and-trust/contract-reasoning.md).

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

These guarantees are available only within the region governed by the successful match and only while the matched state
remains valid. Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss makes a
guarantee unavailable when the guarantee depends on the affected value or storage.

Structural tests and conditional pattern bindings also establish justified guarantees on failure. Failure of `none`
proves nullable presence, and failure of `?_` proves absence. Failure of `?0` alone proves neither presence nor absence.
Success of an alternative grants only facts common to every successful alternative. Failure of an alternative means
every alternative failed. A compound pattern's failure grants only facts independent of which structural test failed.

Facts retain their exact projected subject. A nullable test on a tuple element or product field does not refine the
enclosing tuple or product, a sibling field, or a nested value at a different depth. Facts about equivalent accesses
can survive branch merges when every reachable normal exit establishes them.

Negation reverses a boolean test's outcomes. The right side of `&&` receives the left side's true facts, and the right
side of `||` receives its false facts. A true conjunction and false disjunction retain both operands' guarantees.
Other outcomes retain only guarantees common to every way that outcome can occur. These rules create no bindings.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Partial moves through patterns](partial-moves-through-patterns.md)
- Next: [Guards](guards.md)
