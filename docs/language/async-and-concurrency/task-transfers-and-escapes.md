# Task transfers and escapes

Moving a task handle transfers the task obligation to the destination owner.

Task-handle destinations include:

- local bindings,
- fields,
- variant payloads,
- array elements,
- tuple elements,
- function arguments,
- returned values,
- values supplied by `yield`.

A task handle can be transferred only when the destination owner can legally own the task and every state item captured by the task.

A task that captures only owned values and detached-safe capabilities can escape the creating async block through ordinary ownership transfer.

A task that captures a borrow can escape only to an owner whose lifetime is proven not to outlive the borrowed storage.

A task that captures a scoped capability can escape only to an owner whose contract assumes responsibility for releasing that capability before the capability's source scope exits.

A task that captures local storage from its creating async block cannot escape that async block.

A declaration that returns, stores, or otherwise exposes a task handle with non-local dependencies preserves those dependencies through the task handle's inferred lifetime and capability dependency contract.

If the destination type or declaration contract does not preserve the task handle's capture requirements, the transfer is rejected.

This is invalid unless the returned task handle's dependency contract preserves the borrow of `data`:

```bray
func start(pos data: &Data) -> Task<Result<Hash, HashError>>
{
    return async
    {
        let task = spawn hash(data);
        yield task;
    };
}
```

This is valid because the task owns `data`:

```bray
func start(pos data: Data) -> Task<Result<Hash, HashError>>
{
    return async
    {
        let task = spawn hash(data);
        yield task;
    };
}
```

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Task handles and obligations](task-handles-and-obligations.md)
- Next: [Thread spawn expressions](thread-spawn-expressions.md)
