# Async and concurrency

Async and concurrency rules define owned async computations, direct awaiting, independently running tasks, structured task
lifetimes, execution roots, cancellation, execution requirements, runtime integration, cross-run memory visibility,
synchronization, atomics, native threads, child processes, parallel algorithms, and the portable standard-library concurrency
surface.

The async-specific syntax consists only of the `async` modifier and the `await` expression. Starting, joining, and cancelling tasks
use ordinary methods on compiler-known owned types.

---

## Contents

- [Overview](async-and-concurrency/overview.md)
- [Async functions and computations](async-and-concurrency/async-functions-and-computations.md)
- [Async representation and storage](async-and-concurrency/async-representation-and-storage.md)
- [Captured state](async-and-concurrency/captured-state.md)
- [Await expressions](async-and-concurrency/await-expressions.md)
- [Starting tasks](async-and-concurrency/starting-tasks.md)
- [Task handles and obligations](async-and-concurrency/task-handles-and-obligations.md)
- [Task transfers and escapes](async-and-concurrency/task-transfers-and-escapes.md)
- [Structured task scope exit](async-and-concurrency/structured-task-scope-exit.md)
- [Cancellation](async-and-concurrency/cancellation.md)
- [Execution requirements](async-and-concurrency/execution-requirements.md)
- [Execution roots and product shutdown](async-and-concurrency/execution-roots-and-product-shutdown.md)
- [Product and thread-local static lifecycle](declarations/static-storage-declarations.md#entry-closure-and-product-cleanup)
- [Entrypoints and runtime selection](async-and-concurrency/entrypoints-and-runtime.md)
- [Cross-run memory model](async-and-concurrency/cross-run-memory-model.md)
- [Data-race prevention](async-and-concurrency/data-race-prevention.md)
- [Shared state and synchronization](async-and-concurrency/shared-state-and-synchronization.md)
- [Atomic operation contracts](async-and-concurrency/atomic-operation-contracts.md)
- [Cancellation and memory visibility](async-and-concurrency/cancellation-and-memory-visibility.md)
- [Capability transfer across run boundaries](async-and-concurrency/capability-transfer-across-run-boundaries.md)
- [Standard-library concurrency](async-and-concurrency/standard-library-concurrency.md)
- [Low-level runtime](async-and-concurrency/low-level-runtime.md)
- [Summary](async-and-concurrency/summary.md)
