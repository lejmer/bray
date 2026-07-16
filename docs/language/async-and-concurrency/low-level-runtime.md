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
- traversing owned task obligations without lifecycle resolution,
- resolving frame lifecycle only after cancellation broadcast,
- accepting, reporting, and destroying ordered cleanup incidents,
- creating, waiting on, signalling, and destroying runtime events,
- selecting compatible cooperative, local, blocking, and compute lanes,
- resolving runtime shutdown.

These operation descriptions do not reserve Bray declaration names. Their binary symbols, calling conventions, frame descriptors,
and versioning belong to the private runtime ABI.

A conforming runtime must preserve:

- `Future<T>` and `Task<T>` ownership and movement,
- dependency and affinity contracts,
- exactly-once frame completion and destruction,
- cancellation request and cleanup shielding rules,
- phase-separated task broadcast through concrete and erased frame state,
- task run-boundary panic capture,
- ownership and mandatory reporting of suppressed cleanup incidents,
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

The cleanup-report sink is a product-host service and does not require an async scheduler; synchronous products using native
threads provide it too. It is mandatory even when a product has no interactive debugger or logging backend. It accepts an ordered
batch of owned type-erased incidents and suppressed child-run panic reports at terminal-boundary observation, reports at least each
entry's kind, concrete type, producer/source identity, and ordinal to the product host or persistent inspection stream, then
infallibly destroys every payload. A product policy
can additionally terminate or render richer diagnostics, but cannot silently discard the batch or change source
`RunResult.Cancelled` into a payload-bearing variant.

A runtime or wrapper is nonconforming if its safe surface permits data races, dangling dependencies, duplicate task ownership,
unsynchronized shared mutation, leaked scoped capabilities, unresolved task obligations, or destruction of a running frame.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
