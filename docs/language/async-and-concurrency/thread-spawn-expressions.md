# Thread spawn expressions

Expression syntax is defined in [Spawn expressions](../expressions/spawn-expressions.md).

The thread spawn expression is a thread entry application.

The callee and argument list are checked as a thread entry application.

The callee must resolve to a synchronous callable declaration, method, static function, or callable value.

The selected callable must be callable with the supplied arguments according to ordinary call argument-binding rules.

If the selected callable's result type is `T`, the thread spawn expression produces `Thread<T>`.

`Thread<T>` is the compiler-known linear thread handle type for a spawned synchronous thread whose ordinary result type is `T`.

`spawn thread` evaluates the callee access expression, explicit argument expressions, and omitted parameter defaults in the creating run.

For method entry applications, the receiver expression is evaluated before method arguments.

For function, static function, and callable-value entry applications, explicit argument expressions are evaluated in source order.

Omitted parameter defaults are evaluated after explicit arguments, in parameter declaration order.

The callable body is not evaluated in the creating run.

The evaluated receiver, evaluated arguments, and evaluated defaults are bound into the thread entry state.

The created thread runs the selected callable body with that entry state.

The thread entry callable receives no hidden state from lambda capture or bound-method capture.

Owned entry values are moved or copied into the thread entry state according to ordinary argument-passing rules.

Borrow entry values make the returned `Thread<T>` carry the borrow dependency.

Scoped capabilities and finalization obligations supplied through entry state are carried by the thread handle until the thread is joined, cancelled, or transferred to another owner.

If the returned thread handle could outlive storage, capabilities, facts, or obligations required by its entry state, the transfer or escape is rejected.

`spawn thread` is valid in synchronous and asynchronous execution contexts when thread creation is available for the target.

The source evaluation order guarantees callee evaluation, argument evaluation, default evaluation, entry-state binding, and thread-handle creation before the spawned thread can observe its entry state.

Thread entry scheduling is governed by the runtime and target thread rules.

The created thread can start before or after the creating execution flow continues past the `spawn thread` expression.

Inter-thread visibility and synchronization are governed by the [cross-run memory model](cross-run-memory-model.md).

The thread entry callable must be synchronous.

An async callable produces an async computation and is spawned as a task through `spawn` or `spawn detached`.

Example:

```bray
func hash_data(pos data: Data) -> Hash
{
    return hash(data);
}

let handle: Thread<Hash> = spawn thread hash_data(data);
```

An anonymous callable can be used by binding it explicitly and passing state explicitly:

```bray
let worker = lambda (pos data: Data) -> Hash
{
    return hash(data);
};

let handle: Thread<Hash> = spawn thread worker(data);
```

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Task transfers and escapes](task-transfers-and-escapes.md)
- Next: [Thread entry state](thread-entry-state.md)
