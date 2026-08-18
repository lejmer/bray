# Atomic operation contracts

An atomic operation is an operation whose declaration contract states that it atomically observes or mutates a specific
storage location.

The protected `core.atomic.Atomic<T>` representation and its primitive operations are compiler-provided. `std.atomic` is
an ordinary safe wrapper module and is not recognized by declaration name.

`Atomic<T>` is available only when `T` has an eligible integer, Boolean, raw-pointer, transparent, or plain-storage
representation and the selected target provides the required atomic operation and alignment. Unsupported types and
targets are rejected during checking. The compiler never implements an unavailable atomic operation with a lock.

An atomic operation contract must state:

- the storage reached by the operation,
- the element type, size, alignment, initialization, and target-availability requirements,
- whether the operation reads, writes, or read-modify-writes,
- the value produced by the operation, if any,
- the memory ordering used by the operation,
- the failure ordering for compare-exchange style operations,
- the trusted guarantees and trusted implementation capabilities required when the operation reaches raw memory or
  target intrinsics,
- the panic, cancellation, destruction, finalization, and condition-invalidation behavior.

The required atomic ordering meanings are:

- relaxed ordering: the operation is atomic but creates no synchronization edge,
- acquire ordering: later effects in the acquiring run can observe effects published by a matching release edge,
- release ordering: earlier effects in the releasing run are published to a matching acquire edge,
- acquire-release ordering: the operation has both acquire and release behavior,
- sequentially consistent ordering: the operation participates in one language-defined total order of sequentially
  consistent atomic operations in addition to its acquire or release behavior.

A compare-exchange failure ordering cannot include release behavior because the failing operation does not write the
target storage.

Load and wait accept relaxed, acquire, and sequentially consistent ordering. Store accepts relaxed, release, and
sequentially consistent ordering. A fence cannot be relaxed. Exchange, fetch, and successful compare-exchange accept
every ordering. The compare-exchange failure ordering must be no stronger than its success ordering and cannot be
release or acquire-release. Every order is a closed compile-time `usize` argument. Values outside the five
language-defined orders are diagnosed before MIR construction.

Strong compare-exchange fails only when the observed value differs from the expected value. Weak compare-exchange may
also fail spuriously. Both return the observed value and a Boolean success flag. Fetch operations return the value that
preceded the update.

`wait` repeatedly observes one atomic location and returns after it observes a value unequal to the supplied expected
value. It may also return spuriously, so callers must recheck their condition. Notification is advisory and does not
itself create a memory-order edge. `notify_one` permits at least one eligible waiter to resume and `notify_all` permits
every eligible waiter to resume. Waiting and notification are available only when the selected representation's target
properties enable them. Atomic waiting is not a cancellation point and none of the atomic operations panic, destroy,
finalize, or invalidate the containing value.

Atomic initialization creates live protected storage exactly once. Moving, copying, destroying, finalizing, or
ordinarily accessing that storage while an atomic operation may overlap is invalid. Atomic operations do not make the
containing value or adjacent storage safe for concurrent ordinary access.

An acquire operation that reads a value published by a release operation synchronizes with that release. The
synchronization edge makes effects sequenced before the release visible to effects sequenced after the acquire. Relaxed
operations participate only in the per-location modification order. Sequentially consistent operations additionally
participate in one total order consistent with program order.

Atomic access to one storage location does not make ordinary non-atomic access to the same storage valid while
concurrent access can overlap.

Atomic access to one field, element, or raw location does not authorize concurrent movement, destruction, finalization,
reinitialization, or variant replacement of the owning value unless the owning type contract explicitly permits that
operation.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Shared state and synchronization](shared-state-and-synchronization.md)
- Next: [Cancellation and memory visibility](cancellation-and-memory-visibility.md)
