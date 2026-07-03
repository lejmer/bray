# Thread handles and obligations

`Thread<T>` is an owned thread handle.

`Thread<T>` is not copyable.

Moving a `Thread<T>` transfers the thread obligation.

A thread handle can be joined, cancelled when its contract permits cancellation, or moved to transfer the thread obligation to another owner.

The join operation consumes the thread handle.

Thread joins are observed with `catch`:

```bray
let result: RunResult<T> = catch thread.join();
```

A thread join expression is valid only as the operand of `catch`.

If the thread completed normally with a value of type `T`, `catch thread.join()` produces `RunResult.Completed(value)`.

If the thread panicked, `catch thread.join()` produces `RunResult.Panicked(report)`.

If the thread was cancelled before normal completion, `catch thread.join()` produces `RunResult.Cancelled`.

Joining a thread resolves the thread obligation regardless of which `RunResult` variant is produced.

Thread cancellation is valid when the thread handle's contract permits cancellation.

The cancellation operation consumes the thread handle:

```bray
thread.cancel();
```

Thread cancellation reaches completion through cancellation points and operation contracts declared by the thread entry and the values it uses.

Cancelling a thread destroys owned thread entry state and releases thread entry capabilities according to ordinary destruction and finalization rules.

Cancelling a thread resolves the thread obligation.

## Thread obligation checking

The compiler tracks live thread obligations in the same control-flow state as ownership, initialization, movement, destruction, finalization, borrow, capability, and task-obligation state.

Each `spawn thread` creates one live thread obligation owned by the returned thread handle.

The obligation is resolved when the thread is joined or cancelled.

The obligation is transferred when the thread handle is moved to another valid owner.

Every non-panic source-level path that exits a scope owning a thread handle must leave no unresolved thread obligation owned by that scope.

An exit path can transfer a thread obligation as part of the exit by yielding, returning, assigning, or otherwise moving the thread handle into a valid destination.

After a thread handle is transferred, the old owner no longer has the thread obligation.

At control-flow merge points, thread-obligation state must be coherent.

A thread handle cannot be joined, cancelled, transferred, or used on a path where that handle is no longer live.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Thread entry state](thread-entry-state.md)
- Next: [Tasks and threads as run boundaries](tasks-and-threads-as-run-boundaries.md)
