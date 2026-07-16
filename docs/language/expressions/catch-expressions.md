# Catch expressions

A **catch expression** creates a panic-catching boundary for its operand.

It is an expression-level panic conversion, not the second half of a try-catch statement. Bray has no exception objects, exception
handler search, or resumable caught continuation.

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

Run observation is not a special catch operand. `await task.join()` and `await task.cancel()` produce `RunResult<T>` directly. A
caller can apply ordinary `catch` around the await expression only to catch a panic occurring in the current run while performing
that operation, in which case the type is `Result<RunResult<T>, PanicReport>`. The same value-versus-propagation distinction applies
to standard-library thread and process observation contracts.

Applying `try` to the observed run result first forwards a child panic or cancellation into the current run:

```bray
let recovered: Result<T, PanicReport> = catch (try await task.join());
```

For `RunResult.Completed(value)`, the expression produces `Result.Ok(value)`. For `RunResult.Panicked(report)`, `try` forwards the
existing report as a panic in the current run and `catch` converts it to `Result.Error(report)`. For `RunResult.Cancelled`, `try`
forwards cancellation out of the catch expression because `catch` catches panic, not cancellation.

Calling an async callable does not execute its body. Therefore `catch operation()` catches only a panic during argument evaluation
or inactive-frame construction and has type `Result<Future<T>, PanicReport>`. To catch a panic produced while directly executing the
computation in the current run, the operand must include the await:

```bray
let recovered: Result<T, PanicReport> = catch (await operation());
```

Direct await creates no child run boundary, so a body panic reaches the surrounding catch directly.

`return`, `break`, `continue`, propagation, and other exits targeting an outer boundary leave the catch expression without producing
a result value on that path.

`catch` is not valid in predicate expressions, contract expressions, guard expressions, or pattern contexts.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Panic expressions](panic-expressions.md)
- Next: [Break expressions](break-expressions.md)
