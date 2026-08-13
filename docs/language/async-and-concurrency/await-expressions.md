# Await expressions

Expression syntax is defined in [Await expressions](../expressions/await-expressions.md).

The operand is evaluated exactly once and must have type `Future<T>` for some `T`.

`await` consumes the `Future<T>` and composes its frame into the current task. It does not create another task or run boundary.

The current task drives the child frame until one of these events occurs:

- normal completion produces the child callable's declared result `T`,
- suspension suspends the current task until the awaited operation can resume,
- panic propagates through the current task to the nearest ordinary panic-catching boundary or task boundary,
- cancellation begins cancellation cleanup for the child and then the containing task.

Direct await does not produce `RunResult<T>` because it does not cross an independent run boundary.

`await` is permitted only in an async callable body or an async-capable lifecycle body. Ordinary block expressions inside that body
inherit the same async execution context. No async block form exists.

Before the child first executes, the checker verifies that the current execution context satisfies the child computation's deferred
body effects, capabilities, lifecycle contract, execution requirements, and thread-affinity constraints. Awaiting a computation
requiring `blocking_execution()`, `compute_execution()`, or `main_thread_execution()` from an incompatible lane is rejected.

Normal completion establishes the postcondition template carried by the consumed computation and applies its `result` conditions to the
produced `T`. Panic and cancellation establish none of those conditions. If control flow merged computations from multiple producers,
only postconditions guaranteed by every possible producer survive in the merged computation contract.

The current task's cancellation request is checked before a child suspends and after it resumes. Cancellation-aware runtime
operations can also observe the request while performing the await.

`try await expression` means `try (await expression)`.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Captured state](captured-state.md)
- Next: [Starting tasks](starting-tasks.md)
