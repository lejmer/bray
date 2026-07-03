# Task spawn expressions

Expression syntax is defined in [Spawn expressions](../expressions/spawn-expressions.md).

The non-detached spawn expression consumes an async computation and schedules it as a task.

The operand is evaluated exactly once.

The operand must produce an async computation.

If the async computation's declared result type is `T`, the spawn expression produces `Task<T>`.

`Task<T>` is the compiler-known linear task handle type for a spawned asynchronous computation whose ordinary result type is `T`.

`spawn` requires async execution capability.

A non-detached `spawn` expression is valid only inside an `async` block.

The spawned task initially belongs to the nearest enclosing `async` block.

The nearest enclosing `async` block owns the task obligation until the task is joined, cancelled, or transferred.

The spawned task owns all state moved into the async computation.

The spawned task borrows all state borrowed by the async computation.

The spawned task holds all scoped capabilities held by the async computation.

While the spawned task is live, captured borrows and capabilities remain active.

## Detached task spawn

The detached spawn expression consumes an async computation and schedules it as a task whose lifetime is represented by the returned task handle.

If the async computation's declared result type is `T`, the detached spawn expression produces `Task<T>`.

`spawn detached` requires async execution capability.

`spawn detached` is valid only when the spawned computation is detached-safe.

A detached-safe computation captures only:

- owned values,
- copied values,
- shared state whose lifetime is independent of the current block,
- capabilities whose contract permits transfer to detached execution.

A detached task cannot capture:

- borrows of local storage from the current block,
- borrows of local storage from an enclosing block whose lifetime is not carried by the task handle,
- scoped capabilities that must be released by the current block,
- finalization obligations that must be completed before the current block exits.

The returned task handle is still a linear obligation.

It must be joined, cancelled, or transferred before the owning scope exits.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Async block expressions](async-block-expressions.md)
- Next: [Task handles and obligations](task-handles-and-obligations.md)
