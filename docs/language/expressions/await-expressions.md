# Await expressions

The await expression is:

```bray
await expression
```

The operand is evaluated exactly once.

The operand must produce an async computation.

`await` consumes the async computation and drives it to completion in the current execution flow.

If the async computation completes normally with a value of type `T`, the await expression has type `T`.

Awaiting an async computation directly is not task observation and does not produce `RunResult<T>`.

Await expression ownership, borrowing, cancellation, panic, capability, and effect rules are defined in [Await expressions](../async-and-concurrency/await-expressions.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Continue expressions](continue-expressions.md)
- Next: [Async block expressions](async-block-expressions.md)
