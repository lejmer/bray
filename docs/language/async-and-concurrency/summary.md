# Summary

Calling an async callable produces an owned inactive `Future<T>` and does not execute its body.

`await` consumes and directly composes that computation into the current task. `Future<T>.start()` consumes it into independently
running work and returns the sole source-level `Task<T>` owner.

`Task<T>.join()` and `Task<T>.cancel()` are consuming async methods that produce `RunResult<T>`. `try` on a run result forwards
panic or cancellation into the current run; `catch` catches only a panic that occurs while evaluating its operand in the current
run.

Every lexical block is a structured task boundary. Scope exit requests cancellation for all owned unresolved tasks before awaiting
any of them, then resolves them through ordinary reverse lifecycle order.

Cancellation is cooperative, cleanup is shielded, and abnormal cleanup can fall back from failed graceful finalization to infallible
destruction. Noncooperative work can delay scope exit indefinitely.

`blocking_execution()`, `compute_execution()`, and `main_thread_execution()` are ambient execution-context predicates. Async
invocation defers them into the computation, direct await checks the current lane, and start selects a compatible lane.

Async frames are opaque compiler-managed values. Direct await does not semantically require task allocation or scheduler mediation;
task-owned storage begins at `start()`. Recursive suspended depth can require dynamic storage without source boxing or pinning.

The executable host owns the root process, main thread, and root run. A synchronous main is the root run directly; an async main is
driven as a host-owned root task on the distinguished main-thread lane. Structured product shutdown resolves all source-owned
tasks, threads, processes, payloads, and cleanup incidents before runtime infrastructure and process-scoped resources end.

The executable product selects one conforming runtime. `std.thread.Thread<T>`, `std.process.Process<T>`, parallel algorithms,
channels, synchronization, checkpoints, timers, and concurrent combinators remain ordinary standard-library Bray over private
trusted ABI operations.

Parallel algorithms use owned `std.parallel.Budget` authorities reserved from product-configured task, thread, or process capacity.
Nested algorithms share or split those authorities, so ordinary library composition cannot multiply a numeric limit into hidden
unbounded parallelism.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Low-level runtime](low-level-runtime.md)
