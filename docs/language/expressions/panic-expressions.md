# Panic expressions

A **panic expression** raises an exceptional failure outside the ordinary callable result contract.

```bray
panic(message);
```

The panic message must be compatible with `string`.

The requested panic is raised only after its message can be preserved through propagation and report ownership.
Immutable backing can be retained, and owned backing can be transferred when doing so preserves the expression's
semantics. A borrowed message whose bytes may change or cease to exist requires an owned snapshot.

If allocating a required snapshot fails, the expression raises a catchable allocation-failure panic with the panic
expression's source context. The requested panic has not been raised in that case. The compiler does not truncate its
message or substitute text while reporting it as the requested panic. Failure while evaluating the message expression
follows the ordinary failure rules for that evaluation.

Forwarding or attaching an already-owned panic report preserves its message without copying it again.

A panic expression has type `never` because the current normal continuation does not run.

Panic is used for programmer errors, violated invariants, failed assertions, failed runtime contract checks, bounds
failures in asserted access forms, and states the program did not represent as ordinary failure.

Recoverable domain failure is represented with `Result<T, E>` values, not panic.

A panic propagates outward until it reaches a panic-catching boundary.

If a panic reaches a panic-catching boundary, that boundary receives the panic report and handles recovery according to
the boundary's contract.

A panic-catching boundary does not resume the panicked continuation.

The `catch` expression is the source-level panic-catching expression.

`catch` reports a caught panic as `Result.Error(report)`.

A panic crossing an independently running task, standard-library native-thread, or conforming child-process boundary is
captured by that boundary and observed as `RunResult.Panicked(report)`.

`try` on that run result can forward the existing report back into panic propagation in the observing run. This is panic
propagation, not exception throwing or a try-catch statement. An enclosing `catch` can then convert the forwarded panic
into `Result.Error(report)`.

If a panic reaches the program root without being caught, the program terminates.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Return expressions](return-expressions.md)
- Next: [Catch expressions](catch-expressions.md)
