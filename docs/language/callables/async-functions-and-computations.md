# Async functions and computations

## Async functions

A function can be asynchronous.

The `async` keyword appears before function declarations and lifecycle declarations whose lifecycle kind permits asynchronous
execution.

For lifecycle declarations:

```bray
async finalize() -> Result<unit, FileError>
{
    ...
}
```

For ordinary functions:

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Calling an async function creates an async computation.

The async computation is an owned value representing suspendable execution.

Awaiting the computation drives it to completion.

Async computation ownership, captured state, await behavior, async block expressions, spawning, detached spawning, task handles,
task-obligation checking, transfer, escape, and cancellation rules are defined by the async and concurrency rules.

## Async computation ownership

Async computations are owned values.

Calling an async function creates an async computation value.

That value owns or borrows the state captured by the async function according to the function's signature, body, suspension
points, and lifetime contract.

Suspension captures live values, borrows, capabilities, effects, and finalization obligations into the async computation's
contract.

The async and concurrency rules define the full ownership, borrowing, movement, escape, and cancellation rules for async computations.

## Await

`await` drives an async computation to completion.

```bray
await expression
```

Awaiting produces the async function's declared result when the computation completes successfully.

Awaiting an async computation directly is not a run-boundary observation. Panic from the async computation propagates to the awaiting
execution flow unless a panic-catching boundary handles it.

Applying `catch` to a spawned task join observes a run boundary and produces `RunResult<T>`, where `T` is the spawned
computation's declared result type.

Awaiting must satisfy the computation's ownership, borrowing, capability, effect, cancellation, and finalization obligations.

Await expression syntax and semantics are defined by the async and concurrency rules.

## Async cancellation

Destroying an incomplete async computation cancels it.

Cancellation destroys owned state and releases capabilities according to the async computation's contract.

Ordinary destruction remains synchronous.

Async finalization obligations must be completed through asynchronous execution, transferred, or converted into an explicit
fallback ownership form before the owning scope exits.

Async cancellation is defined by the async and concurrency rules.

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Trusted functions](trusted-functions.md)
- Next: [Effects and capabilities](effects-and-capabilities.md)
