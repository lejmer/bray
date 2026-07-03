# Cancellation

Cancellation is an exit path for an incomplete async computation, task, or thread.

Cancelling an async computation destroys owned captured state and releases held capabilities according to the computation's contract.

Cancelling a task consumes the task handle and resolves the task obligation.

Cancelling a thread consumes the thread handle and resolves the thread obligation.

Cancellation of a task observed through `catch task.join()` produces `RunResult.Cancelled`.

Cancellation of a thread observed through `catch thread.join()` produces `RunResult.Cancelled`.

Cancellation does not produce the task's or thread's ordinary result type.

Cancellation must satisfy all ownership, destruction, finalization, borrow, capability, and effect obligations of the cancelled state.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Tasks and threads as run boundaries](tasks-and-threads-as-run-boundaries.md)
- Next: [Cross-run memory model](cross-run-memory-model.md)
