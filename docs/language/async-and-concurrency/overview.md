# Overview

Bray treats asynchronous execution and run-boundary observation as part of ownership, control flow, lifecycle, and capability checking.

The core async and concurrency forms are:

- async functions,
- async computations,
- `await` expressions,
- `async` block expressions,
- `spawn` expressions,
- `spawn detached` expressions,
- `spawn thread` expressions,
- task handles,
- thread handles,
- task and thread observation through `catch`.

An async computation is an owned value.

It can own or borrow state, hold capabilities, carry effects, and carry finalization obligations.

Spawned work is structured by default.

A non-detached spawned task belongs to the nearest enclosing `async` block until the task is completed, cancelled, or transferred to another owner.

Detached work is explicit.

`spawn detached` creates a task whose lifetime is represented by the returned task handle and whose captured state must be detached-safe.

Thread work is explicit.

`spawn thread` creates a thread whose lifetime is represented by the returned thread handle and whose entry state must be valid for thread execution.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Next: [Async functions and computations](async-functions-and-computations.md)
