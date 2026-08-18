# Starting tasks

`Future<T>` has this compiler-provided inherent method surface:

```bray
impl Future<T>
{
    consume func start() -> Task<T>;
}
```

The bodyless form describes a compiler-provided declaration and is not source syntax that packages can write.

The receiver is evaluated before the empty argument list according to ordinary method-call order. `start()` consumes the
owned inactive computation, creates an independently running task, and returns its sole source-level `Task<T>` owner.

`start()` is synchronous because it performs only the transition into independently schedulable work. It does not wait
for the task to begin or complete. After the method returns, the task is eligible to run and can already be running on
another compatible worker.

`start()` is valid only while executing an async callable, an async-capable lifecycle body, an async entrypoint, or
another context that the product runtime contract defines as an active async execution context. Constructing or moving
`Future<T>` does not have this restriction.

Starting performs these semantic steps:

1. Evaluate and own the `Future<T>` receiver.
2. Select a runtime lane satisfying its deferred execution requirements and affinity conditions.
3. Obtain stable task-owned frame and control storage.
4. Move the inactive frame into that storage before its first resume.
5. Create the cancellation state, completion state, and join-waiter state.
6. Make the task schedulable.
7. Return `Task<T>` carrying the computation's dependency, execution, and normal-completion postcondition contracts.

Observing `RunResult.Completed(value)` establishes the postcondition template preserved by that particular task and
applies its `result` conditions to `value`. Moving a task preserves the template. Merging tasks or computations from
different producers retains only postconditions guaranteed by every reachable producer. The common source type `Task<T>`
does not invent producer-specific conditions.

If the selected product runtime cannot provide a required lane, product validation rejects the program before execution.
If task storage or another runtime resource cannot be acquired before publication, `start()` panics in the calling task
and resolves the consumed inactive frame during panic cleanup. Once the task has been published, `start()` returns its
handle and later runtime failure is represented only through that task's terminal outcome or the product's catastrophic
runtime-failure policy.

There is no detached start operation. Moving `Task<T>` transfers its ownership obligation, but an independently running
task always has a source-level owner until its terminal result is resolved.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Await expressions](await-expressions.md)
- Next: [Task handles and obligations](task-handles-and-obligations.md)
