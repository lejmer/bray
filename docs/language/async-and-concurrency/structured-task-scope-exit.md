# Structured task scope exit

Every ordinary lexical block is a structured task ownership boundary. No additional async scope syntax exists.

When an asynchronous execution path leaves a block, every unresolved `Task<T>` obligation whose owner ends at that boundary is
resolved before any storage or dependency required by those tasks can end. This applies to natural completion, `yield`, `return`,
`break`, `continue`, propagation, panic, cancellation, and every other path leaving the block.

Task obligations can occur in local bindings, initialized represented parts of aggregates, or initialized hidden frame state owned
by an inactive `Async<T>`, such as a not-yet-awaited join or cancel computation. Flow-sensitive partial-move and initialization facts
determine which obligations remain owned at each exit.

Task cleanup occurs in two phases.

## Phase one: cancellation broadcast

Before waiting for or normally finalizing any owned unresolved task, cleanup requests cancellation for every such task. Requests are
issued in deterministic reverse ownership-resolution order, but no task is awaited during this phase.

This broadcast prevents one child from blocking cleanup before its siblings have received cancellation. Requesting cancellation for
an already terminal task has no effect.

## Phase two: lifecycle resolution

After cancellation has been requested for all owned tasks, ordinary reverse lifecycle ordering resumes. Each task's automatic async
finalizer waits for terminal completion and applies the unobserved-result rules.

Task cleanup completes before destruction or finalization of any value that an owned task depends on. Dependencies therefore impose
ordering edges in addition to ordinary reverse declaration and represented-part order.

Explicitly joined or cancelled tasks and tasks moved out of the scope are absent from automatic cleanup. A handle consumed into an
unawaited join or cancel computation remains represented by that computation's transferred obligation and is resolved through its
owner's cleanup.

Automatic task cleanup can suspend the exiting task and has no finite completion-time guarantee. A child that does not cooperate
with cancellation can delay scope exit indefinitely. Runtime inspection must expose which task is preventing cleanup and its last
known suspension or checkpoint state.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Task transfers and escapes](task-transfers-and-escapes.md)
- Next: [Cancellation](cancellation.md)
