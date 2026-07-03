# Low-level runtime

Low-level async runtime machinery is part of the trusted substrate.

Executors, reactors, wakers, completion queues, foreign async callbacks, device async integration, and custom scheduling primitives are implemented through trusted capabilities and exposed through safe async contracts.

Trusted runtime declarations that affect scheduling, cross-run memory visibility, synchronization, cancellation, or foreign callbacks must expose a safe contract that states:

- ownership effects,
- borrow effects,
- synchronization edges,
- cancellation behavior,
- panic behavior,
- trusted facts,
- fact invalidation,
- capability requirements visible to callers.

Trusted runtime declarations can use raw memory, unchecked aliasing, target intrinsics, device memory, or foreign calls only through the trusted capabilities defined by the contract, trust, and [raw memory rules](../targets-layout-abi-and-raw-memory.md).

Foreign or device operations that can access Bray-owned storage must either be represented by a synchronization contract or be treated by their declaration contract as affecting every reachable storage item they can touch.

A trusted declaration cannot expose an ordinary safe API that permits data races, dangling borrows, unsynchronized shared mutation, invalid raw memory access, leaked scoped capabilities, or unresolved run obligations.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Capability transfer across run boundaries](capability-transfer-across-run-boundaries.md)
- Next: [Summary](summary.md)
