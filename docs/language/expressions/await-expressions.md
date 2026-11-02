# Await expressions

Syntax follows the [expression roots and precedence structure grammar](../syntax-grammar.md#expression-roots-and-precedence-structure).

An await expression consumes an async computation into the current task.

```bray
await expression
```

[Await semantics](../async-and-concurrency/await-expressions.md) defines operand typing, evaluation, completion,
cancellation, panic, execution requirements, and postconditions. Direct await differs from
[observing an independently started task](../async-and-concurrency/task-handles-and-obligations.md).

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Continue expressions](continue-expressions.md)
- Next: [Conversion expressions](conversion-expressions.md)
