# Capability transfer across run boundaries

Every value transferred into a started task, native thread, or typed child-process protocol carries its dependency contract across
that run boundary.

A capability can cross that boundary only when its contract permits the new run to use it. The contract can permit movement,
copying, shared observation, exclusive use, thread-affine use, or synchronized concurrent use. Copyability alone does not grant any
of those permissions.

If a capability depends on the creating task, lexical storage, a resource scope, a runtime lane, a target thread, the current
process, or a foreign callback context, the child-run owner preserves that dependency. Moving the owner is valid only when the
destination owner preserves every lifetime, affinity, synchronization, cancellation, destruction, and finalization requirement.

A thread-affine capability records the exact origin thread or a typed compatible-lane class in the dependency contract. A task
retaining it can run only on that thread or on a lane proven to belong to that class. The requirement is not a boolean “local” flag.
The frame descriptor records the requirement for every control state. An implementation can migrate only between states whose live
dependency sets permit the destination, and must conservatively pin the whole task when it does not implement state-sensitive
affinity. No different task type is required. A migratable task has no live affinity requirement and can run on any selected worker
satisfying its execution requirements.

A borrow or capability derived from thread-static storage records the exact native-thread attachment rather than only a compatible
lane class. A task retaining that dependency is pinned to that exact thread and keeps the attachment open until the dependency is
resolved.

A product-static dependency can cross a run boundary only when the destination preserves provider ownership and the reached value's
sharing and synchronization contract permits concurrent use.

There is no detached capability category. Work can outlive its creating block only by moving its `Task<T>`,
`std.thread.Thread<T>`, or `std.process.Process<T>` owner to another statically valid owner. The owner and all dependencies remain
source-visible obligations.

Operating-system-thread execution supplied by `std.thread` is an ordinary library boundary whose callable contract states which
values and capabilities can cross it. The compiler enforces that contract through the same dependency and run-boundary rules.

Child-process execution supplied by `std.process` is an ordinary library boundary whose checked protocol states which values can be
encoded, transferred, decoded, and owned on each side. A borrow, raw pointer, exact-thread capability, process-local resource, or
unsynchronized shared-memory authority cannot cross merely because its type is copyable.

The executable root itself is established by the product host. It owns the initial process and main-thread lifetime guarantees but does
not expose self-owning `Process<T>` or `Thread<T>` values to source.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cancellation and memory visibility](cancellation-and-memory-visibility.md)
- Next: [Standard-library concurrency](standard-library-concurrency.md)
