# Summary

This summary is informative. The detailed rules include:

- [pattern contexts](pattern-contexts.md).
- [pattern operation modes](pattern-operation-modes.md).
- [pattern evaluation](pattern-evaluation.md).

Patterns are structural matching forms with a dedicated grammar. Pattern names resolve before they bind. Paths identify
constants, variants, and other pattern-capable declarations, while field shorthand binds same-name fields without
resolving those names as named patterns.

Variant patterns refine closed union values, and payload and product fields are matched by name. `..` explicitly
accounts for remaining fields or elements. Nullable patterns distinguish absence with `none` and presence with
`?pattern`.

The operation mode determines observation, borrowing, mutable borrowing, consumption, or copying. Pattern bindings
retain type, ownership, lifetime, capability, and initialization state. Irrefutable-only contexts reject refutable
patterns, and closed-union variant matching checks exhaustive coverage.

Pattern matching is structural, deterministic, and effect-free. Successful matches make their structural guarantees
available in the region governed by the match.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern evaluation](pattern-evaluation.md)
