# Summary

Async computations are owned suspendable values.

Awaiting an async computation drives it in the current execution flow.

Spawning an async computation creates a task and a linear task obligation.

`spawn detached` requires detached-safe captured state and returns a task handle that carries the task obligation.

`spawn thread` creates a synchronous thread from explicit entry state and returns a linear thread obligation.

Task and thread joins are observed through `catch handle.join()` as `RunResult<T>`.

Cancellation resolves incomplete async computations, tasks, and threads by satisfying ownership, lifecycle, capability, and effect obligations.

Concurrent runs communicate safely only through ownership transfer, borrow contracts, synchronization contracts, atomic contracts, and trusted runtime contracts.

Safe Bray source is data-race-free by construction.

Low-level async runtime behavior belongs behind trusted declarations with safe contracts.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Low-level runtime](low-level-runtime.md)
