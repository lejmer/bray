# Partial values and replacement

Whole-value lifecycle declarations require the whole value to be fully initialized whenever the whole-value lifecycle declaration can run.

Partial values do not run whole-value finalizers, whole-value destructors, or whole-value scope enter or exit behavior.

Partial represented storage resolves only initialized represented parts.

For products, partial represented storage resolves initialized fields.

For unions, partial represented storage resolves initialized active payload fields.

A partial move from a value with whole-value lifecycle behavior is valid only when every reachable path reinitializes the moved subpart before:

- the value is finalized,
- the value is destroyed as a complete value,
- the value is used as a `with` initializer,
- the value is moved, copied, consumed, borrowed, or observed as a complete value,
- ownership of the value can end.

For unions, the moved active payload field must also be reinitialized before the active variant is replaced.

If the compiler cannot prove that the value becomes fully initialized before one of those events, the partial move is rejected.

A whole-product assignment or replacement resolves the old product value according to product finalization, destruction, and field destruction rules before the new product value becomes initialized at that access path.

A whole-union replacement resolves the old active variant payload according to union finalization, destruction, and active payload destruction rules before the new active variant tag and payload become initialized at that access path.

Assigning `none` to a nullable access path resolves the old present value according to nullable, ownership, finalization, and destruction rules before the access path becomes absent.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [With expressions](with-expressions.md)
- Next: [Scope exits, panics, and cancellation](scope-exits-panics-and-cancellation.md)
