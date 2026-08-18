# Task transfers and escapes

Moving a task handle transfers its task-resolution obligation and every dependency carried by the task.

Task destinations include local bindings, fields, variant payloads, array elements, tuple elements, parameters, returned
values, and values supplied by `yield`.

A transfer is valid only when the destination owner:

- can own the task-resolution obligation,
- cannot outlive borrowed storage captured by the task,
- preserves scoped capabilities until the task resolves,
- preserves thread-affinity requirements,
- preserves deferred execution and lifecycle requirements,
- can perform asynchronous finalization if ownership can end while the task remains unresolved.

A task depending on local storage can move to an enclosing or sibling owner only when dependency analysis proves that
the new owner resolves before that storage ends. A task owning all captured state can be returned or stored without a
borrow dependency, but it still has a task-resolution obligation.

There is no detached-safe exception. Escaping work is expressed only by ordinary movement of `Task<T>` into an owner
whose inferred or declared contract can preserve all dependencies.

For example, an async function can return a task borrowing its parameter only when the result dependency contract
preserves that borrow:

```bray
async func start_hash(pos data: &Data) -> Task<Result<Hash, HashError>>
{
    return hash(data).start();
}
```

The returned task keeps `data` borrowed until its eventual owner joins, cancels, or automatically resolves it. A caller
cannot store that task where it may outlive `data`.

Moving a task out of a lexical scope excludes it from that scope's automatic task cleanup. The destination becomes
responsible for its eventual resolution.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Task handles and obligations](task-handles-and-obligations.md)
- Next: [Structured task scope exit](structured-task-scope-exit.md)
