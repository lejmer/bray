# Cross-run memory model

Independently running tasks are concurrent runs when their execution can overlap. An `Async<T>` that is only directly awaited is
part of the current task and is not a separate run.

Starting a task creates a start edge: every effect used to initialize and transfer its frame occurs before the new task can observe
that state.

Within one run, ordinary expression evaluation and async resumption order define the order of memory effects. Between concurrent
runs, effects are ordered only by language-defined synchronization edges.

A synchronization edge is created by:

- the task start edge,
- terminal completion observed by `await task.join()` or `await task.cancel()`,
- automatic task finalization completing at scope exit,
- operations whose safe type or declaration contract explicitly synchronizes access,
- atomic operations according to their ordering contract,
- trusted runtime declarations whose safe internal contract establishes synchronization.

Moving `Task<T>` transfers its source-level obligation and dependency contract but does not by itself synchronize with the running
task.

Task completion creates a completion edge from every effect in the completed task, including cleanup effects, to the continuation
that receives its `RunResult<T>`. Automatic finalization creates the same edge before dependent owner storage is resolved.

Cancellation request alone is not a completion edge. The completion edge is established only when cancellation cleanup reaches a
terminal task outcome and a join, cancel, or automatic finalizer observes it.

Operating-system threads created through the standard library participate through the ownership and synchronization contract of
`std.thread.Handle<T>`. Thread start establishes a release-to-acquire edge into the native entry root. `join`, `cancel`, automatic
handle finalization, and `std.thread.run` terminal observation establish the corresponding completion edge back to the observer.
That handle is an ordinary standard-library type, not a separate compiler-known thread-handle category.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Entrypoints and runtime selection](entrypoints-and-runtime.md)
- Next: [Data-race prevention](data-race-prevention.md)
