# Cross-run memory model

Independently running tasks, native threads, and child processes are concurrent runs when their execution can overlap. A
`Future<T>` that is only directly awaited is part of the current run and is not a separate run.

Starting a task creates a start edge: every effect used to initialize and transfer its frame occurs before the new task
can observe that state.

Within one run, ordinary expression evaluation and async resumption order define the order of memory effects. Between
concurrent runs, effects are ordered only by language-defined synchronization edges.

A synchronization edge is created by:

- the task start edge,
- terminal completion observed by `await task.join()` or `await task.cancel()`,
- automatic task finalization completing at scope exit,
- native-thread start and terminal observation through `std.thread.Thread<T>`,
- child-process protocol send, receive, and terminal observation through `std.process.Process<T>`,
- operations whose safe type or declaration contract explicitly synchronizes access,
- atomic operations according to their ordering contract,
- trusted runtime declarations whose safe internal contract establishes synchronization.

Moving `Task<T>` transfers its source-level obligation and dependency contract but does not by itself synchronize with
the running task.

Task completion creates a completion edge from every effect in the completed task, including cleanup effects, to the
continuation that receives its `RunResult<T>`. Automatic finalization creates the same edge before dependent owner
storage is resolved.

Cancellation request alone is not a completion edge. The completion edge is established only when cancellation cleanup
reaches a terminal run outcome and an owner, host, join, cancel, or automatic finalizer observes it.

Operating-system threads created through the standard library participate through the ownership and synchronization
contract of `std.thread.Thread<T>`. Thread start establishes a release-to-acquire edge into the native entry root.
`join`, `cancel`, automatic thread-owner finalization, and `std.thread.run` terminal observation establish the
corresponding completion edge back to the observer. That owner is an ordinary standard-library type, not a separate
compiler-known thread category.

Child processes do not share the Bray memory model merely because they share an operating-system parent. Their typed
input and output protocols create value-transfer and terminal-observation edges. Shared memory, inherited handles, or
memory-mapped storage create cross-process visibility only through the explicit synchronization contract of the
standard-library type that owns them.

The executable root run begins after product-static materialization. Its generated root frame observes every root-scope
owned child and completes root lexical lifecycle cleanup before publishing the final terminal record. The root run ends
at that publication. Host-side outcome mapping, entry closure, run quiescence, thread-local static cleanup,
product-static cleanup, cleanup-sink draining, runtime-infrastructure shutdown, and process-scoped host resource
destruction follow the root completion edge and therefore cannot race source execution.

Synchronization used by an explicit runtime initialization state machine determines when a contained static value is
published. Constant materialization alone does not create an inter-run publication edge for later interior mutation.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Entrypoints and runtime selection](entrypoints-and-runtime.md)
- Next: [Data-race prevention](data-race-prevention.md)
