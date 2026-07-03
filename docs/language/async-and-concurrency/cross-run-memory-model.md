# Cross-run memory model

Tasks and threads are concurrent runs when their execution can overlap with another live run.

An async computation that is only awaited in the current execution flow is not a concurrent run by itself.

A task becomes a concurrent run when it is spawned.

A thread becomes a concurrent run when it is spawned.

Within one run, ordinary expression evaluation order defines the order of memory effects.

Between concurrent runs, memory effects are ordered only by language-defined synchronization edges.

A synchronization edge is created by:

- transferring captured state into a spawned task or explicit thread entry state into a spawned thread before that run can observe the state,
- joining a task or thread,
- completing task or thread cancellation,
- operations whose type or declaration contract explicitly synchronizes access,
- atomic operations according to their ordering contract,
- trusted runtime declarations whose safe contract establishes synchronization.

Moving a task handle or thread handle transfers the obligation represented by that handle.

Moving a handle does not by itself create a synchronization edge with the run owned by the handle.

Joining a task or thread creates a completion edge from the joined run to the continuation after the `catch handle.join()` expression.

Completing cancellation creates a completion edge from the cancelled run's cleanup path to the continuation after the cancellation operation.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation](cancellation.md)
- Next: [Data-race prevention](data-race-prevention.md)
