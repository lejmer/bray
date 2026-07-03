# Async functions and computations

An async function uses the `async` modifier:

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Calling an async function evaluates the call arguments and creates an async computation value.

Calling an async function does not create a task.

Calling an async function does not by itself run the function body to completion.

The async computation's declared result type is the async function's declared result type.

The async computation owns or borrows the state supplied to the call according to:

- the function signature,
- argument forms,
- the function body,
- suspension points,
- capability requirements,
- the inferred lifetime dependency contract.

An async computation is not copyable.

Moving an async computation transfers responsibility for completing, spawning, cancelling, or otherwise resolving it.

Destroying an incomplete async computation cancels it, destroys owned captured state, and releases held capabilities according to the async computation's contract.

Async functions can be awaited directly, spawned as tasks, or moved as owned async computation values when the destination can preserve their dependency contract.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Overview](overview.md)
- Next: [Captured state](captured-state.md)
