# Await expressions

The await expression is:

```bray
await expression
```

The operand is evaluated exactly once.

The operand must have type `Future<T>` for some `T`.

`await` consumes the async computation and composes it into the current task.

If the async computation completes normally with a value of type `T`, the await expression has type `T`.

Awaiting an async computation directly is not task observation and does not produce `RunResult<T>`.

Await expression ownership, borrowing, cancellation, panic, capability, and effect rules are defined in
[Await expressions](../async-and-concurrency/await-expressions.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Continue expressions](continue-expressions.md)
- Next: [Conversion expressions](conversion-expressions.md)
