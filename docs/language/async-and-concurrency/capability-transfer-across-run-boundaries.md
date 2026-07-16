# Capability transfer across run boundaries

Every value transferred into a started task carries its dependency contract across the task boundary.

A capability can cross that boundary only when its contract permits the new run to use it. The contract can permit movement,
copying, shared observation, exclusive use, thread-affine use, or synchronized concurrent use. Copyability alone does not grant any
of those permissions.

If a capability depends on the creating task, lexical storage, a resource scope, a runtime lane, a target thread, or a foreign
callback context, `Task<T>` preserves that dependency. Moving the handle is valid only when the destination owner preserves every
lifetime, affinity, synchronization, cancellation, destruction, and finalization requirement.

A thread-affine capability pins the task to a compatible runtime lane. It does not require a different task type. A migratable task
can run on any selected worker satisfying its execution requirements.

There is no detached capability category. Work can outlive its creating block only by moving `Task<T>` to another statically valid
owner. The handle and all dependencies remain source-visible obligations.

Operating-system-thread execution supplied by `std.thread` is an ordinary library boundary whose callable contract states which
values and capabilities can cross it. The compiler enforces that contract through the same dependency and run-boundary rules.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation and memory visibility](cancellation-and-memory-visibility.md)
- Next: [Standard-library concurrency](standard-library-concurrency.md)
