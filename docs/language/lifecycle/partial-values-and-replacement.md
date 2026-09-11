# Partial values and replacement

Whole-value lifecycle declarations require the whole value to be fully initialized whenever the whole-value lifecycle
declaration can run.

Partial values do not run whole-value finalizers, whole-value destructors, or whole-value scope enter or exit behavior.

Partial represented storage resolves only initialized represented parts.

For products, partial represented storage resolves initialized fields.

For unions, partial represented storage resolves initialized active payload fields.

A partial move from a value with whole-value lifecycle behavior is valid only when every reachable path reinitializes
the moved subpart before:

- the value is finalized,
- the value is destroyed as a complete value,
- the value is used as a `with` initializer,
- the value is moved, copied, consumed, borrowed, or observed as a complete value,
- ownership of the value can end.

For unions, the moved active payload field must also be reinitialized before the active variant is replaced.

If the compiler cannot prove that the value becomes fully initialized before one of those events, the partial move is
rejected.

A whole-product assignment or replacement resolves the old product value according to product finalization, destruction,
and field destruction rules before the new product value becomes initialized at that access path.

A whole-union replacement resolves the old active variant payload according to union finalization, destruction, and
active payload destruction rules before the new active variant tag and payload become initialized at that access path.

Assigning `none` to a nullable access path resolves the old present value according to nullable, ownership,
finalization, and destruction rules before the access path becomes absent.

## Replacement and abnormal completion

Replacement establishes the destination access path once, then evaluates the new value before resolving old contents.
The evaluated new value retains its ownership and lifecycle obligations until it is installed at the destination.
Moves performed while evaluating that value affect which old represented parts remain initialized and require cleanup.

If evaluation of the destination or new value panics or cancels, replacement does not begin. Ordinary abnormal-exit
cleanup resolves the ownership state reached by that evaluation, including any moves already performed.

Once old-value cleanup begins, replacement must restore an initialized destination before an abnormal outcome can leave
the assignment. If that cleanup panics, resolve the remaining initialized old contents using abnormal-exit lifecycle
rules, install the already evaluated new value, then propagate the panic. Do not repeat lifecycle operations whose
obligations have already been resolved. Later cleanup incidents follow the ordinary suppressed-incident rules.

This rule applies equally to owned destinations and destinations reached through mutable borrows, including projected
storage. A caller that catches the panic observes the installed replacement, never destroyed or uninitialized storage.
If propagation instead ends the destination owner's scope, ordinary scope-exit cleanup resolves the new value once.

The cleanup and installation interval is cancellation-shielded. A pending cancellation does not interrupt restoration
of the destination. Any cancellation already committed during old-value cleanup propagates only after the replacement
is installed, subject to the ordinary rule that a cleanup panic takes precedence over cancellation.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [With expressions](with-expressions.md)
- Next: [Scope exits, panics, and cancellation](scope-exits-panics-and-cancellation.md)
