# Async block expressions

Expression syntax is defined in [Async block expressions](../expressions/async-block-expressions.md).

An async block expression introduces an ordinary block scope.

An async block expression also introduces a structured async ownership boundary for tasks spawned inside the block.

An async block expression is not a task.

An async block expression is not detached work.

An async block expression is evaluated as part of the current execution flow.

An async block expression can use `await`.

An async block expression can use non-detached `spawn`.

An async block expression can produce `unit`, `never`, or another value type according to ordinary block-expression rules.

In value-producing context, an async block expression receives its value through `yield`.

Local bindings introduced inside an async block are scoped to that block.

Owned values whose ownership remains in the async block are destroyed when the async block exits.

Tasks owned by the async block are task obligations.

Every non-panic source-level path that leaves the async block must complete, cancel, or transfer each task obligation owned by the block.

If a panic reaches an async block while the block owns live task obligations, the async block cancels those tasks before the panic continues outward.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Await expressions](await-expressions.md)
- Next: [Task spawn expressions](task-spawn-expressions.md)
