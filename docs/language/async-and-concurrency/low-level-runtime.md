# Low-level runtime

Low-level async runtime machinery is part of the trusted product substrate and is not a source-visible compiler-known
package.

Its binary symbols, calling conventions, frame descriptors, internal operations, and versioning are outside the
source-language contract and do not reserve Bray declaration names.

## Bootstrap thread attachment

A runtime artifact may bind private trusted Bray declarations to closed runtime and platform roles through build
metadata. The compiler validates the role, declaration shape, ABI, selected target, and semantic contract. A package,
module path, declaration name, native symbol, or implementation language grants no role by itself. Ordinary source can
spell the same declaration and receives no extra authority.

The bootstrap thread-storage contract has four target operations. They create a destructor-bearing key, load the
current thread's opaque pointer, store or clear that pointer, and destroy the key after every attached thread has
quiesced. The target clears a nonzero slot before it invokes the destructor on the exiting native thread. Explicitly
clearing a slot does not invoke the destructor, and destroying a key does not clean up another thread.

The trusted runtime owns the value stored in that slot. It assigns a process-unique attachment identity, reuses the
attachment for nested entry, and drains registered thread-static cleanup in reverse order on outermost detach or native
thread exit. Cleanup continues after a cleanup panic, reports that incident, and never lets panic or cancellation cross
the target destructor callback. Ordinary `@thread_local` statics still use the runtime attachment and cleanup roles.

## Panic ownership at native boundaries

Runtime-invoked Bray callbacks return an explicit outcome. A panicked outcome owns one `PanicReport` handle, while a
cancelled outcome carries no report. Catching, forwarding, reporting, or destroying a report transfers that same owned
handle rather than reconstructing it. Every terminal path must consume the handle exactly once.

Foreign callbacks and target callbacks cannot unwind a Bray panic or propagate Bray cancellation through a non-Bray
ABI frame. Runtime-role binding maps the protected source `PanicReport` representation to its opaque ABI handle without
making representation casts or constructors available to ordinary source.

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

Native thread creation, child process creation and signalling, typed process-protocol transport, process reaping, and
hard product resource limits are ordinary standard-library or product services rather than additional compiler-known
runtime operations. When an async operation integrates an external completion source with task suspension, it must
preserve cancellation, ownership, and visibility semantics without blocking a cooperative worker. Ordinary parallel
budgets remain standard-library permit hierarchies and do not grant product-host capacity authority.

The cleanup-report sink is a product-host service and does not require an async scheduler. Synchronous products using
native threads provide it too. It is mandatory even when a product has no interactive debugger or logging backend. It
accepts an ordered batch of owned type-erased incidents and suppressed child-run panic reports at terminal-boundary
observation, reports each entry's kind, concrete type, producer/source identity, and ordinal to the product host or
persistent inspection stream, then infallibly destroys every payload. A product policy can additionally terminate or
render richer diagnostics, but cannot silently discard the batch or change source `RunResult.Cancelled` into a
payload-bearing variant.

A runtime or wrapper is nonconforming if its safe surface permits data races, dangling dependencies, duplicate child-run
ownership, unsynchronized shared mutation, leaked scoped capabilities, unresolved task, thread, or process obligations,
unbounded hidden parallelism, or destruction of a running frame.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
