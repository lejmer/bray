# Async functions and computations

A function, method, static function, lambda, or async-capable lifecycle declaration can use the `async` modifier.

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Asyncness is part of the callable contract. An async trait member requires an async fulfillment, callable values
preserve asyncness, and public API compatibility distinguishes synchronous and asynchronous declarations.

The declared result type describes successful body completion. Calling an async callable declared to return `T`
evaluates and transfers its invocation state but does not execute the body. The invocation expression has type
`Future<T>`.

An async callable therefore has two checked contract phases:

- its invocation contract covers callee and argument evaluation, ownership transfer, generic constraints, value
  preconditions, and construction of the inactive frame.
- its execution contract covers body effects and capabilities, execution requirements, suspension, cancellation,
  lifecycle behavior, and `ensures(...)` conditions established by normal body completion.

The invocation expression must satisfy the first phase and stores the second in the produced `Future<T>`. Normal
direct-await completion establishes the execution postconditions. Across a started task boundary, those postconditions
are available only in a control-flow arm refined to `RunResult.Completed(value)` and apply to `value`. `Cancelled` and
`Panicked` establish none of them.

The async body is checked in an active async execution context. It can use `await`, `Future<T>.start()`, asynchronous
lifecycle resolution, and ordinary lexical structured task scopes.

The complete computation, frame, dependency, execution-requirement, cancellation, task, and runtime rules are defined in
[Async and concurrency](../async-and-concurrency.md).

## Navigation

- [Language index](../index.md)
- [Callables index](../callables.md)
- Previous: [Trusted functions](trusted-functions.md)
- Next: [Effects and capabilities](effects-and-capabilities.md)
