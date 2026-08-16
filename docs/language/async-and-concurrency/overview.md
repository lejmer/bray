# Overview

Bray treats asynchronous execution and task observation as part of ownership, control flow, lifecycle, dependency, capability,
effect, and product checking.

The core async surface is:

- `async` functions, methods, lambdas, and async-capable lifecycle declarations,
- the compiler-known owned computation type `Future<T>`,
- `await` expressions,
- the compiler-known owned task type `Task<T>`,
- `Future<T>.start()`,
- `Task<T>.join()` and `Task<T>.cancel()`,
- the compiler-known run-boundary union `RunResult<T>`,
- the compiler-known execution predicates `blocking_execution()`, `compute_execution()`, and `main_thread_execution()`,
- ordinary lexical scopes as structured task ownership boundaries.

There are no async block, spawn, detached-task, thread-spawn, race, select, runtime, blocking, or compute expression forms.

Calling an async callable constructs an owned inactive computation and does not independently start it. Directly awaiting that
computation composes it into the current task. Calling `start()` consumes it and creates an independently running task.

Every `Task<T>` is a linear source-level ownership obligation. Moving the handle transfers the obligation. If the handle remains
owned when a lexical scope exits, scope cleanup requests cancellation and waits for the task before ownership ends. Bray has no
detached task state without a source-level owner.

Every executable has one host-owned root run. A synchronous entrypoint is that run directly. An async entrypoint is transferred
into a host-owned root task with no source-visible `Task<T>`. Child tasks, operating-system threads, and processes remain owned by
that root or by a checked nested source owner until their terminal outcomes and payload lifecycles resolve.

Product-static storage can outlive every individual run while remaining owned by the product. A static carrying `@thread_local` is
owned by one exact native-thread attachment. Dependency contracts retain those roots, constrain run transfer and task migration,
and delay detachment or product shutdown while storage remains reachable. State that belongs only to one child thread can instead
be transferred explicitly through `std.thread.start` or `std.thread.run`.

Operating-system threads, child processes, parallel algorithms, channels, synchronization types, timers, task combinators, and
other concurrency facilities are ordinary standard-library declarations implemented over private trusted ABI operations. They do
not add compiler-known types or syntax.

Their public policy and ownership behavior are Bray source. Portable low-level internals can be trusted Bray. Only irreducible
operating-system mechanisms require a direct platform binding or narrow native shim, and a private ABI or `extern` declaration does
not imply that its implementation language is C or another foreign language.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Next: [Async functions and computations](async-functions-and-computations.md)
