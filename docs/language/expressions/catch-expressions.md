# Catch expressions

A **catch expression** creates a panic-catching boundary for its operand.

```bray
catch expression
```

The operand is evaluated exactly once.

The operand form selects the catch behavior. Expected result type does not select a catch behavior or overload.

For an ordinary operand of type `T`, `catch expression` has type `Result<T, PanicReport>`.

If the ordinary operand completes normally with a value of type `T`, the catch expression produces `Result.Ok(value)`.

If the ordinary operand completes naturally as `unit`, the catch expression produces `Result.Ok(unit)`.

If the ordinary operand panics, the catch expression produces `Result.Error(report)`.

For a task or thread observation operand whose joined computation has declared result type `T`, `catch expression` has type
`RunResult<T>`.

If the observed run completes normally with a value of type `T`, the catch expression produces
`RunResult.Completed(value)`.

If the observed run panics, the catch expression produces `RunResult.Panicked(report)`.

If the observed run is cancelled before normal completion, the catch expression produces `RunResult.Cancelled`.

Panics caught by a catch expression do not resume the panicked continuation.

Nested catch expressions create nested panic-catching boundaries. A panic is caught by the nearest enclosing catch boundary.

A catch expression can use a block expression as its operand.

```bray
let parsed: Result<Item, PanicReport> = catch
{
    let item = parse(input);
    yield item;
};
```

The block operand is a single-yield region for the caught operand's successful result.

`yield value` exits the block operand and supplies the successful result.

`yield;` exits the block operand and supplies `unit`.

Result propagation inside a block operand whose result type is `Result<T, E>` supplies `Result.Error(error)` as the
block operand's value according to ordinary result propagation rules.

For a task or thread observation whose declared result type is `Result<T, E>`, a recoverable error from the observed run is a
normal completion value:

```bray
let loaded: RunResult<Result<User, LoadError>> = catch task.join();
```

That value is `RunResult.Completed(Result.Error(error))`.

`return`, `break`, `continue`, nullable propagation, result propagation, run-result propagation, and other exits that target an
outer boundary leave the catch expression without producing a result value on that path.

`catch` is not valid in predicate expressions, contract expressions, guard expressions, or pattern contexts.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Panic expressions](panic-expressions.md)
- Next: [Break expressions](break-expressions.md)
