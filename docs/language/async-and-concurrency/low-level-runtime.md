# Low-level runtime

Low-level runtime machinery supplies product support without creating a source-visible compiler-known package. Its
contracts preserve checked cleanup plans, storage ownership, activation, dispatch and reporting across target ABIs.

Its binary symbols, calling conventions, frame descriptors, internal operations, and versioning are outside the
source-language contract and do not reserve Bray declaration names.

## Bootstrap thread attachment

A runtime artifact may bind trusted Bray declarations to closed runtime and platform roles through build
metadata. The compiler validates the role, declaration shape, ABI, selected target, and semantic contract. A package,
module path, declaration name, native symbol, or implementation language grants no role by itself. Ordinary source can
spell the same declaration and receives no extra authority.

Declarations and fields are public by default. Explicit internal-use acknowledgement changes accessibility only. It
does not establish ownership, synchronization, witness validity or runtime-role authority. Runtime operations must
preserve their contracts even when safe source acknowledges internal access.

Attachment and cleanup services must be established without using operations that already require the service being
formed. Any cleanup needed while establishing a service must already be backed. Entry cannot require itself in order
to establish its own prerequisites.

The runtime assigns a process-unique attachment identity to each attached native thread and reuses that attachment for
nested entry. It resolves the attachment's thread-static cleanup on the exact thread at outermost detach or native thread
exit. Detaching one thread does not clean up another thread's statics. Each attachment's cleanup occurs at most once.

Cleanup follows the [static lifecycle dependency graph](../declarations/static-storage-declarations.md#lifecycle-dependency-graph)
and its deterministic structural tie order. Registration order is not semantic. Cleanup continues after a cleanup panic
and reports that incident. Panic and cancellation do not cross a foreign thread-exit boundary. Ordinary `@thread_local`
statics follow the same attachment and cleanup rules.

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
- phase-separated task broadcast across all owned computation state,
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
