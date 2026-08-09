# Summary

Patterns are structural matching forms.

Patterns have a dedicated grammar category.

Pattern names resolve before they bind.

Paths identify named constants, variants, and other pattern-capable declarations.

Variant patterns refine closed union values.

Payload and product fields are matched by name.

Field shorthand binds same-name fields without resolving those names as named patterns.

`..` explicitly accounts for remaining fields or elements.

Nullable patterns match absent state with `none` and present state with `?pattern`.

Pattern operation mode determines observe, borrow, mutable borrow, consume, or copy behavior.

Pattern bindings carry type, ownership, lifetime, capability, and initialization state.

Refutability is tracked.

Irrefutable-only contexts reject refutable patterns.

Union variant matching over closed unions performs exhaustive coverage checking.

Successful patterns make their structural guarantees available in the region governed by the match.

Pattern matching is structural, deterministic, and effect-free.

## Navigation

- [Language index](../index.md)
- [Patterns index](../patterns.md)
- Previous: [Pattern evaluation](pattern-evaluation.md)
