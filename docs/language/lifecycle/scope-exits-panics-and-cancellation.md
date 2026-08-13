# Scope exits, panics, and cancellation

Scope exit resolves local ownership and lifecycle state for the scope being left. Initialized owned values whose ownership remains
in the scope are resolved. Moved values are not. Partially initialized values resolve only initialized represented parts.

Reachable exits must agree on ownership, borrowing, initialization, destruction, finalization, capabilities, effects, task
obligations, and which contract guarantees remain available.

In async execution, each lexical block is also a structured task boundary. Before ordinary reverse lifecycle resolution, every
unresolved task obligation whose owner ends at that boundary receives cancellation. Only after all requests have been issued does
normal lifecycle resolution wait for individual tasks. Dependency ordering ensures tasks resolve before storage and capabilities
they use.

Panic propagation and cancellation use ordinary lifecycle ordering after the task cancellation-broadcast phase. Cleanup executes in
a cancellation-shielded context when it can suspend.

On ordinary exit, fallible finalization returning `Result.Error` leaves its obligation unresolved. The value cannot be destroyed,
and the program must handle, transfer, or represent the failure through an allowed source-level lifecycle path.

On panic or cancellation exit, cleanup attempts the same finalizer. If it returns `Result.Error`, the failure is recorded as an
owned suppressed cleanup incident and graceful finalization is abandoned. The synchronous infallible destructor and represented-part
destruction then run. A cleanup panic becomes or is attached to the task's `PanicReport` according to whether another panic is
already active. Incident ownership and observation follow the async cancellation rules.

If an ordinary scope cannot resolve a lifecycle obligation, transfer it to a valid owner, or convert it into an explicit fallback
ownership form, the program is rejected. The abnormal-exit fallback does not weaken that rule for normal execution.

For an unresolved `Task<T>`, `std.thread.Thread<T>`, or `std.process.Process<T>`, this normal-exit check includes the lifecycle of a
possible unobserved `Completed(T)` payload. Task cleanup can drive asynchronous infallible finalization. Thread-owner cleanup is
synchronous. The standard-library process owner's finalizer is explicitly fallible, so an unresolved `Process<T>` is always
rejected on normal exit and must be consumed through `join()` or `cancel()` with its outer `Result` handled. Other implicit paths
are rejected when they cannot completely resolve a possible payload and terminal infrastructure outcome in the current context.

The executable root performs the same ownership and lifecycle checks before product shutdown. Root return does not detach an
unresolved child run.

## Navigation

- [Language index](../index.md)
- [Lifecycle index](../lifecycle.md)
- Previous: [Partial values and replacement](partial-values-and-replacement.md)
- Next: [Lifecycle requirements in traits](lifecycle-requirements-in-traits.md)
