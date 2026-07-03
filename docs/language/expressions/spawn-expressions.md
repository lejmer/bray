# Spawn expressions

The task spawn expressions are:

```bray
spawn expression
```

```bray
spawn detached expression
```

The task spawn operand is evaluated exactly once.

The task spawn operand must produce an async computation.

If the async computation's declared result type is `T`, the spawn expression produces `Task<T>`.

Spawning consumes the async computation and schedules it as a task.

The thread spawn expression is:

```bray
spawn thread callee(arguments)
```

The thread spawn form is a thread entry application.

The callee must resolve to a synchronous callable declaration, method, static function, or callable value.

The supplied arguments bind to the selected callable's parameters according to ordinary call argument-binding rules.

If the selected callable's result type is `T`, the thread spawn expression produces `Thread<T>`.

The callee access expression, explicit arguments, and omitted parameter defaults are evaluated in the creating run.

The selected callable body is evaluated in the spawned thread, not in the creating run.

Thread spawning moves, copies, or borrows explicit entry state according to the selected callable's receiver and parameter
contracts.

The full task spawn, detached task spawn, thread spawn, task handle, thread handle, run-boundary observation, transfer, escape,
cancellation, ownership, borrowing, capability, and effect rules are defined by the async and concurrency rules.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Async block expressions](async-block-expressions.md)
- Next: [Conversion expressions](conversion-expressions.md)
