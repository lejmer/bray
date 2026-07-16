# Low-level runtime

Low-level async runtime machinery is part of the trusted product substrate and is not a source-visible compiler-known package.

The language-defined runtime contract provides semantic operations equivalent to:

- running a root frame,
- starting a task from an inactive frame,
- resuming and suspending a frame,
- waking a task,
- requesting task cancellation,
- registering and waking join waiters,
- observing cancellation and checkpoints,
- creating, waiting on, signalling, and destroying runtime events,
- selecting compatible cooperative, local, blocking, and compute lanes,
- resolving runtime shutdown.

These operation descriptions do not reserve Bray declaration names. Their binary symbols, calling conventions, frame descriptors,
and versioning belong to the private runtime ABI.

A conforming runtime must preserve:

- `Async<T>` and `Task<T>` ownership and movement,
- dependency and affinity contracts,
- exactly-once frame completion and destruction,
- cancellation request and cleanup shielding rules,
- task run-boundary panic capture,
- join and cancellation completion visibility edges,
- lane execution facts,
- structured root shutdown.

Private standard-library implementation modules can bind the ABI through trusted foreign declarations. Those declarations remain
ordinary private `std` source declarations and are not recognized by their source paths. A different standard library can organize
its wrappers differently while targeting the same ABI.

Trusted runtime declarations that affect scheduling, memory visibility, synchronization, cancellation, foreign callbacks, or
device access must expose safe internal contracts covering ownership, borrowing, visibility edges, cancellation, panic, fact
invalidation, and capability requirements. They can use raw memory, target intrinsics, device memory, platform APIs, and foreign
calls only through the corresponding trusted capabilities.

A runtime or wrapper is nonconforming if its safe surface permits data races, dangling dependencies, duplicate task ownership,
unsynchronized shared mutation, leaked scoped capabilities, unresolved task obligations, or destruction of a running frame.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
