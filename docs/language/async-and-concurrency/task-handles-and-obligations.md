# Task handles and obligations

`Task<T>` is the compiler-known protected-representation handle for an independently running computation whose normal result is `T`.
It is owned, move-only, not directly constructible, and carries a linear task-resolution obligation.

`Task<T>` has this compiler-provided inherent method surface:

```bray
impl Task<T>
{
    consume async func join() -> RunResult<T>;
    consume async func cancel() -> RunResult<T>;
}
```

Both method calls consume the handle into an inactive `Future<RunResult<T>>` according to ordinary async method-call typing. Calling
the method alone neither waits nor requests cancellation; those effects begin when the returned computation is awaited, started, or
resolved by async cleanup.

`join()` waits for the task without requesting cancellation. It produces:

- `RunResult.Completed(value)` when the task completes normally with `value: T`,
- `RunResult.Cancelled` when the task reaches cancellation before normal completion,
- `RunResult.Panicked(report)` when a panic crosses the task boundary.

When driven, `cancel()` first requests cancellation idempotently and wakes a suspended task, then waits for the same terminal
outcomes. A task that completed before the request can therefore produce `Completed`, and a task that panicked before or during
cancellation can produce `Panicked`.

Consuming either method transfers the resolution obligation into the returned async computation. Awaiting that computation resolves
the obligation and produces the `RunResult<T>`. Moving it preserves the obligation through its dependency and lifecycle contract. If
the inactive join or cancel computation reaches async scope cleanup, its captured task is resolved there without silently
abandoning it. It cannot reach the end of a synchronous owner because its captured task requires asynchronous finalization.

`Task<T>` does not expose detach, raw poll, raw wake, task-control-block access, cancellation-token access, completion testing, or a
non-waiting public cancellation method.

## Automatic lifecycle behavior

An unresolved `Task<T>` has a compiler-known asynchronous finalization obligation. Its automatic finalizer:

1. requests cancellation if it has not already been requested,
2. waits in a cancellation-shielded cleanup context for terminal completion,
3. resolves an unobserved `Completed(T)` payload through `T`'s complete finalization, destruction, and represented-part lifecycle,
4. accepts `Cancelled` as expected cleanup,
5. propagates an unobserved child panic on ordinary scope exit,
6. records an unobserved child panic as a suppressed panic when the parent is already panicking or cancelling.

Implicit task resolution on an otherwise-normal scope exit is well formed only when every possible `Completed(T)` payload can be
fully resolved in that async context without a fallible finalizer. An asynchronous but infallible finalizer is driven before
destruction. If `T` has fallible finalization, the checker rejects normal implicit resolution; source must explicitly observe
`await task.join()` or `await task.cancel()` and preserve, transfer, or explicitly handle the `Completed(value)` lifecycle
obligation. This is a static rule because cleanup cannot assume that cancellation will beat an already completed task.

During parent panic or cancellation, an unobserved completed payload follows the universal abnormal-exit lifecycle path. Its
finalizer is attempted in shielded cleanup; an error becomes an ordered cleanup incident and destruction still runs. A cleanup
panic follows the task-boundary panic rules.

After terminal resolution, its synchronous destructor releases the runtime task-control storage.

A synchronous scope cannot end ownership of an unresolved task because it cannot drive asynchronous finalization. Such a scope must
consume the task through an operation whose async result is transferred, or move the task to an owner whose contract preserves the
obligation. Otherwise the program is rejected.

Task-obligation checking follows ownership, movement, partial initialization, lifecycle, borrowing, capability, effect, and
flow-sensitive contract rules. Tasks nested in aggregates carry the same obligation through their initialized access paths.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Starting tasks](starting-tasks.md)
- Next: [Task transfers and escapes](task-transfers-and-escapes.md)
