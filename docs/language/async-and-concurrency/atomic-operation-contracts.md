# Atomic operation contracts

An atomic operation is an operation whose declaration contract states that it atomically observes or mutates a specific storage location.

Atomic operations can be [compiler-known declarations](../compiler-known-and-standard-library/compiler-known-declarations.md), [compiler-provided declarations](../compiler-known-and-standard-library/compiler-provided-declarations.md), or [recognized standard-library declarations](../compiler-known-and-standard-library/standard-library-recognition.md).

An atomic operation contract must state:

- the storage reached by the operation,
- the element type, size, alignment, initialization, and target-availability requirements,
- whether the operation reads, writes, or read-modify-writes,
- the value produced by the operation, if any,
- the memory ordering used by the operation,
- the failure ordering for compare-exchange style operations,
- the trusted guarantees and trusted implementation capabilities required when the operation reaches raw memory or target intrinsics,
- the panic, cancellation, destruction, finalization, and condition-invalidation behavior.

The required atomic ordering meanings are:

- relaxed ordering: the operation is atomic but creates no synchronization edge,
- acquire ordering: later effects in the acquiring run can observe effects published by a matching release edge,
- release ordering: earlier effects in the releasing run are published to a matching acquire edge,
- acquire-release ordering: the operation has both acquire and release behavior,
- sequentially consistent ordering: the operation participates in one language-defined total order of sequentially consistent atomic operations in addition to its acquire or release behavior.

A compare-exchange failure ordering cannot include release behavior because the failing operation does not write the target storage.

Atomic access to one storage location does not make ordinary non-atomic access to the same storage valid while concurrent access can overlap.

Atomic access to one field, element, or raw location does not authorize concurrent movement, destruction, finalization, reinitialization, or variant replacement of the owning value unless the owning type contract explicitly permits that operation.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Shared state and synchronization](shared-state-and-synchronization.md)
- Next: [Cancellation and memory visibility](cancellation-and-memory-visibility.md)
