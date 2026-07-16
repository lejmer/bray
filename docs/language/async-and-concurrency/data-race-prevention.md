# Data-race prevention

A data race is a pair of potentially overlapping accesses from different concurrent runs where:

- both accesses can reach the same storage or overlapping storage,
- at least one access mutates, initializes, reinitializes, moves, destroys, finalizes, replaces an active variant, changes nullable state, writes through raw memory, or performs another operation whose contract modifies storage,
- the accesses are not ordered by a synchronization edge,
- the accesses are not both mediated by a valid atomic or synchronization contract for the reached storage.

A Bray program with a data race is invalid.

Safe Bray source must be data-race-free by construction.

If the compiler cannot prove that potentially overlapping concurrent accesses are disjoint, ordered, immutable, or mediated by a valid synchronization contract, the program is rejected.

Moving owned state into a started task or native thread transfers exclusive ownership of that state to the new in-process run.

After the move, the creating run has no access path that owns the moved state.

Copying a value into a started task or native thread is valid only when the copied value's type contract permits use in that run
boundary.

Copyability does not imply cross-thread sharing, independent-run transfer safety, process encoding, atomic access, or synchronized
interior mutation.

Capturing a shared borrow into a task or scoped thread run keeps the shared borrow active for the dependency lifetime carried by its
owner.

While that shared borrow is active, incompatible mutation, movement, destruction, finalization, reinitialization, or variant replacement of the reached storage remains suspended in every run.

Capturing a mutable borrow into a task or scoped thread run transfers exclusive mutation authority to that run for the dependency
lifetime carried by its owner.

No other run can observe, mutate, move, destroy, finalize, reinitialize, or otherwise incompatibly access the reached storage until the mutable borrow is resolved.

Raw pointers do not create an exception to the data-race rule.

Raw memory access across concurrent runs requires the ordinary raw-memory trusted facts plus a synchronization or atomic contract covering the reached storage.

A child process does not directly participate in the parent process's ordinary storage graph. Values cross through an encoded
protocol. Shared memory or inherited process resources reintroduce cross-process access only through an explicit type contract that
defines lifetime, synchronization, ownership, and cleanup on both sides.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Cross-run memory model](cross-run-memory-model.md)
- Next: [Shared state and synchronization](shared-state-and-synchronization.md)
