# Await expressions

Expression syntax is defined in [Await expressions](../expressions/await-expressions.md).

The operand is evaluated exactly once.

The operand must produce an async computation.

`await` consumes the async computation and drives it to completion in the current execution flow.

If the async computation completes normally with a value of type `T`, the await expression has type `T`.

If the async computation panics, the panic propagates to the nearest panic-catching boundary.

Awaiting an async computation directly is not task observation and does not produce `RunResult<T>`.

If the current async computation or async block is cancelled while an await is active, the awaited computation is cancelled according to its cancellation contract.

`await` requires async execution capability.

Async function bodies have async execution capability.

`async` block expressions have async execution capability.

`try await expression` means `try (await expression)`.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Captured state](captured-state.md)
- Next: [Async block expressions](async-block-expressions.md)
