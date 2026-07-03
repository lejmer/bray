# Task handles and obligations

`Task<T>` is an owned task handle.

`Task<T>` is not copyable.

Moving a `Task<T>` transfers the task obligation.

A task handle can be joined, cancelled, or moved to transfer the task obligation to another owner.

The join operation consumes the task handle.

Task joins are observed with `catch`:

```bray
let result: RunResult<T> = catch task.join();
```

A task join expression is valid only as the operand of `catch`.

If the task completed normally with a value of type `T`, `catch task.join()` produces `RunResult.Completed(value)`.

If the task panicked, `catch task.join()` produces `RunResult.Panicked(report)`.

If the task was cancelled before normal completion, `catch task.join()` produces `RunResult.Cancelled`.

Joining a task resolves the task obligation regardless of which `RunResult` variant is produced.

The cancellation operation consumes the task handle:

```bray
task.cancel();
```

Cancelling a task drives cancellation to completion according to the task contract.

Cancelling a task destroys owned captured state and releases captured capabilities according to ordinary destruction and finalization rules.

Cancelling a task resolves the task obligation.

## Task obligation checking

The compiler tracks live task obligations in the same control-flow state as ownership, initialization, movement, destruction, finalization, borrow, and capability state.

Each `spawn` creates one live task obligation.

Each `spawn detached` creates one live task obligation owned by the returned task handle.

The obligation is resolved when the task is joined or cancelled.

The obligation is transferred when the task handle is moved to another valid owner.

Every non-panic source-level path that exits an `async` block must leave no unresolved task obligation owned by that block.

Source-level exits that require resolved task obligations include:

- natural block completion,
- `yield`,
- `return`,
- `continue`,
- nullable propagation,
- result propagation,
- run-result propagation.

An explicit `panic` expression inside an async block cancels task obligations owned by that block before the panic propagates.

An exit path can transfer a task obligation as part of the exit by yielding or returning the task handle.

After a task handle is transferred, the old owner no longer has the task obligation.

At control-flow merge points, task-obligation state must be coherent.

A task handle cannot be joined, cancelled, transferred, or used on a path where that handle is no longer live.

This is valid:

```bray
async
{
    let task = spawn hash_file(file);

    if cancelled
    {
        task.cancel();
        yield Result.Error(HashError.Cancelled);
    }

    let result = try catch task.join();
    yield result;
}
```

This is invalid because the `yield` path leaves the async block while `task` is still owned by the block:

```bray
async
{
    let task = spawn hash_file(file);

    if cancelled
    {
        yield Result.Error(HashError.Cancelled);
    }

    let result = try catch task.join();
    yield result;
}
```

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Task spawn expressions](task-spawn-expressions.md)
- Next: [Task transfers and escapes](task-transfers-and-escapes.md)
