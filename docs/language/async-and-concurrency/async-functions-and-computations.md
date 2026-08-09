# Async functions and computations

An async function uses the `async` modifier:

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

The declared result type is the type produced by successful completion of the body. The body of `fetch` is therefore checked as
producing `Result<Response, FetchError>`.

Calling an async callable evaluates its receiver, explicit arguments, and omitted defaults exactly once in ordinary call evaluation
order. Those values are transferred into a newly created owned async computation according to the selected callable's receiver and
parameter modes. Invocation checks generic constraints and value preconditions at that point.

If the selected async callable declares result type `T`, its invocation expression has type `Future<T>`.

Calling an async callable:

- does not execute its body,
- does not create a task,
- does not interact with the scheduler,
- does not require a running async execution context,
- does not by itself incur body effects or establish body postconditions,
- does not by itself satisfy the deferred execution contract.

`async func(A...) -> T` remains distinct from `func(A...) -> Future<T>`. Asyncness is part of the callable contract, the async body is
checked as producing `T`, and only an async callable invocation can create its protected frame representation. An ordinary function
can accept, move, store, or return an existing `Future<T>` value but cannot construct one directly.

`Future<T>` is compiler-known, protected-representation, owned, move-only, and not directly constructible. It contains the inactive
computation frame and every owned value, borrow, capability, contract dependency, execution requirement, effect, and lifecycle
obligation required to execute or discard that frame. It also carries the body's normal-completion postcondition template. Direct
await applies that template only after normal completion; a started task preserves it only for a `RunResult.Completed` refinement.

An `Future<T>` value can be:

- consumed by `await`,
- consumed by `start()`,
- moved to another owner that preserves its dependency contract,
- resolved without executing its body when its owner ends.

Resolving an inactive `Future<T>` does not begin its body. It resolves the receiver, arguments, defaults, and other initialized frame
state according to ordinary lifecycle ordering. If that state has asynchronous finalization obligations, the `Future<T>` carries
those obligations and can end only in an async cleanup context. A synchronous owner must move such a computation to a compatible
owner or is rejected when its scope exits. A standalone `Future<T>` cannot be partially driven: `await` consumes it into the current
task and `start()` consumes it into a new task.

An async lambda follows the same rules. Lambdas do not capture enclosing state; all async lambda state is supplied through explicit
parameters.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Overview](overview.md)
- Next: [Async representation and storage](async-representation-and-storage.md)
