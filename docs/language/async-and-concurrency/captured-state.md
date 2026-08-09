# Captured state

Creation and suspension preserve every live value, borrow, capability, effect, contract dependency, execution requirement, and lifecycle
obligation needed to begin, resume, cancel, or destroy an async computation.

A receiver or argument moved into an async call is owned by the returned `Future<T>` until it is returned, moved elsewhere,
destroyed, or transferred into a task by `start()`.

A borrow transferred into an async call remains active for the async computation's dependency lifetime. A mutable borrow remains
exclusive for that lifetime. A scoped capability remains held until the computation releases it or its owner resolves the
computation.

An async computation cannot move to an owner that may outlive borrowed storage or a scoped capability it depends on. These
dependencies are inferred from the selected callable contract, argument mapping, defaults, body, suspension liveness, and selected
implementations and are carried by `Future<T>` even though they are not written as source lifetime parameters.

Direct await transfers the captured state into the current task for the duration of the await. `start()` transfers it into the new
task, and `Task<T>` preserves the same dependency contract. Moving either handle transfers, rather than duplicates, the dependency
obligations.

Captured-state analysis distinguishes lifetime from mobility. A task whose state is safe to migrate can execute on compatible
runtime workers. A task containing thread-affine state remains pinned to a compatible execution lane. Thread affinity is an inferred
dependency condition, not a separate public task type.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Async representation and storage](async-representation-and-storage.md)
- Next: [Await expressions](await-expressions.md)
