# Low-level runtime

Low-level async runtime machinery is part of the trusted product substrate and is not a source-visible compiler-known package.

Its binary symbols, calling conventions, frame descriptors, internal operations, and versioning are outside the source-language
contract and do not reserve Bray declaration names.

A conforming runtime must preserve:

- `Future<T>` and `Task<T>` ownership and movement,
- dependency and affinity contracts,
- exactly-once frame completion and destruction,
- cancellation request and cleanup shielding rules,
- phase-separated task broadcast through concrete and erased frame state,
- run-boundary panic capture,
- ownership and mandatory reporting of suppressed cleanup incidents,
- join and cancellation completion visibility edges,
- lane execution requirements,
- structured root shutdown.

Native thread creation, child process creation and signalling, typed process-protocol transport, process reaping, and hard product
resource limits are ordinary standard-library or product services rather than additional compiler-known runtime operations. When an
async operation integrates an external completion source with task suspension, it must preserve cancellation, ownership, and
visibility semantics without blocking a cooperative worker. Ordinary parallel budgets remain standard-library permit hierarchies
and do not grant product-host capacity authority.

The cleanup-report sink is a product-host service and does not require an async scheduler. Synchronous products using native
threads provide it too. It is mandatory even when a product has no interactive debugger or logging backend. It accepts an ordered
batch of owned type-erased incidents and suppressed child-run panic reports at terminal-boundary observation, reports each
entry's kind, concrete type, producer/source identity, and ordinal to the product host or persistent inspection stream, then
infallibly destroys every payload. A product policy can additionally terminate or render richer diagnostics, but cannot silently
discard the batch or change source
`RunResult.Cancelled` into a payload-bearing variant.

A runtime or wrapper is nonconforming if its safe surface permits data races, dangling dependencies, duplicate child-run ownership,
unsynchronized shared mutation, leaked scoped capabilities, unresolved task, thread, or process obligations, unbounded hidden
parallelism, or destruction of a running frame.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
