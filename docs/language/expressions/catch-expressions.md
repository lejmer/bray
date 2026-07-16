# Catch expressions

A **catch expression** creates a panic-catching boundary for its operand.

```bray
catch expression
```

The operand is evaluated exactly once. If the operand has type `T`, the catch expression has type `Result<T, PanicReport>`.

Normal completion produces `Result.Ok(value)`. A natural `unit` completion produces `Result.Ok(unit)`. A panic produces
`Result.Error(report)` and does not resume the panicked continuation.

Nested catch expressions catch at the nearest enclosing boundary.

A catch expression can use a block expression as its operand:

```bray
let parsed: Result<Item, PanicReport> = catch
{
    let item = parse(input);
    yield item;
};
```

The block operand is a single-yield region for the caught operand's successful result. Result propagation within that block follows
ordinary propagation rules.

Task observation is not a special catch operand. `await task.join()` and `await task.cancel()` produce `RunResult<T>` directly. A
caller can apply ordinary `catch` around the await expression only to catch a panic occurring in the current task while performing
that operation, in which case the type is `Result<RunResult<T>, PanicReport>`.

`return`, `break`, `continue`, propagation, and other exits targeting an outer boundary leave the catch expression without producing
a result value on that path.

`catch` is not valid in predicate expressions, contract expressions, guard expressions, or pattern contexts.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Panic expressions](panic-expressions.md)
- Next: [Break expressions](break-expressions.md)
