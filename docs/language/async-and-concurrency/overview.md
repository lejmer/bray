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
- ordinary lexical scopes as structured task ownership boundaries.

There are no async block, spawn, detached-task, thread-spawn, race, select, runtime, blocking, or compute expression forms.

Calling an async callable constructs an owned inactive computation and does not independently start it. Directly awaiting that
computation composes it into the current task. Calling `start()` consumes it and creates an independently running task.

Every `Task<T>` is a linear source-level ownership obligation. Moving the handle transfers the obligation. If the handle remains
owned when a lexical scope exits, scope cleanup requests cancellation and waits for the task before ownership ends. Bray has no
detached task state without a source-level owner.

Operating-system threads, channels, synchronization types, timers, task combinators, and other concurrency facilities are ordinary
standard-library declarations implemented over the private runtime ABI. They do not add compiler-known types or syntax.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Next: [Async functions and computations](async-functions-and-computations.md)
