# Tasks and threads as run boundaries

Tasks and threads are both run boundaries.

Both are represented by linear owned handles after spawning.

Both are observed through `catch handle.join()` as `RunResult<T>`.

Both record panic at the run boundary and report it as `RunResult.Panicked(report)`.

Both report completed cancellation as `RunResult.Cancelled`.

A task runs an async computation.

A thread runs a synchronous callable value.

Non-detached task spawning is structured by the nearest enclosing `async` block.

Thread spawning is represented directly by the returned `Thread<T>` handle.

Task spawning with `spawn` requires async execution capability.

Thread spawning with `spawn thread` requires thread creation availability for the target.

Await drives an async computation in the current execution flow.

Join observes a task or thread run boundary from its handle.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Thread handles and obligations](thread-handles-and-obligations.md)
- Next: [Cancellation](cancellation.md)
