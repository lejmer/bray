# Summary

Calling an async callable produces an owned inactive `Future<T>` and does not execute its body.

`await` consumes and directly composes that computation into the current task. `Future<T>.start()` consumes it into independently
running work and returns the sole source-level `Task<T>` owner.

`Task<T>.join()` and `Task<T>.cancel()` are consuming async methods that produce `RunResult<T>`. `try` on a run result forwards
panic or cancellation into the current run. `catch` catches only a panic that occurs while evaluating its operand in the current
run.

Every lexical block is a structured task boundary. Scope exit requests cancellation for all owned unresolved tasks before awaiting
any of them, then resolves them through ordinary reverse lifecycle order.

Cancellation is cooperative, cleanup is shielded, and abnormal cleanup can fall back from failed graceful finalization to infallible
destruction. Noncooperative work can delay scope exit indefinitely.

`blocking_execution()`, `compute_execution()`, and `main_thread_execution()` are ambient execution-context predicates. Async
invocation defers them into the computation, direct await checks the current lane, and start selects a compatible lane.

Async frames are opaque compiler-managed values. Direct await does not semantically require task allocation or scheduler mediation.
task-owned storage begins at `start()`. Recursive suspended depth can require dynamic storage without source boxing or pinning.

The executable host owns the root process, main thread, and root run. A synchronous main is the root run directly. An async main is
driven as a host-owned root task on the distinguished main-thread lane. The generated root frame resolves source-owned tasks,
threads, processes, budgets, and cleanup incidents before terminal publication. Host shutdown then maps the terminal record, drains
reports, closes entry, reaches run quiescence, cleans exact-thread and product statics, and only then ends runtime infrastructure and
process-scoped resources.

The executable product selects one conforming runtime. `std.thread.Thread<T>`, `std.process.Process<T>`, parallel algorithms,
channels, synchronization, checkpoints, timers, and concurrent combinators remain ordinary standard-library declarations. Their
source-visible semantics do not depend on the private runtime mechanism used by the product.

Parallel algorithms use domain-typed `std.parallel.Budget<Domain>` values as owned library-side bounds. Nested algorithms share or
split those bounds. Independent budgets remain subject to the underlying runtime or product hard limits and do not modify
`Future<T>.start()` semantics.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Low-level runtime](low-level-runtime.md)
