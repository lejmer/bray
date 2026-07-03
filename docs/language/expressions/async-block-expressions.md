# Async block expressions

The async block expression is:

```bray
async
{
    ...
}
```

An async block expression is a block expression.

An async block expression introduces an ordinary block scope and a structured async ownership boundary for tasks spawned inside
the block.

An async block expression can use `await`.

An async block expression can use non-detached `spawn`.

An async block expression is a single-yield region in value-producing context.

The async block expression's task-obligation, transfer, escape, cancellation, ownership, borrowing, capability, and effect rules
are defined in [Async block expressions](../async-and-concurrency/async-block-expressions.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Await expressions](await-expressions.md)
- Next: [Spawn expressions](spawn-expressions.md)
