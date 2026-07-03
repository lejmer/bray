# Async functions and computations

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

Async computation ownership, captured state, await behavior, async block expressions, spawning, detached spawning, task handles,
task-obligation checking, transfer, escape, and cancellation rules are defined in [Async and concurrency](../async-and-concurrency.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Trusted functions](trusted-functions.md)
- Next: [Effects and capabilities](effects-and-capabilities.md)
