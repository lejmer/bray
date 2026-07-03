# Capability transfer across run boundaries

Every value captured by a spawned task or thread carries its dependency contract across that run boundary.

A capability can cross a run boundary only when its contract permits use in that kind of run.

A capability contract can permit:

- use inside an async computation,
- transfer into a structured task,
- transfer into a detached task,
- transfer into a thread,
- copying into a run boundary,
- movement into a run boundary,
- sharing across concurrent runs through a synchronization contract.

If a capability is tied to the creating run, creating scope, stack storage, local resource scope, target thread, executor, or foreign callback context, the capability cannot cross a run boundary unless its contract preserves that dependency through the task or thread handle.

Detached tasks require captured capabilities whose contracts permit detached execution and whose lifetime and release obligations are independent of the creating async block or are preserved by the returned task handle's dependency contract.

Threads require captured capabilities whose contracts permit thread execution.

When a task or thread handle is transferred, the handle's dependency contract and obligation transfer with it.

The destination must preserve every lifetime, capability, synchronization, cancellation, destruction, and finalization requirement carried by the handle.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation and memory visibility](cancellation-and-memory-visibility.md)
- Next: [Low-level runtime](low-level-runtime.md)
