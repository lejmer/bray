# Low-level runtime

Low-level runtime machinery is trusted Bray product support, not a source-visible compiler-known package. The compiler
owns source contract checking, cleanup plans, concrete layouts and immutable descriptors. Bray owns runtime policy,
storage ownership, activation, dispatch and reporting. Target-gated Bray code uses explicit OS and system ABIs.

All project-owned runtime and platform support linked into produced programs is intended to be Bray. The pinned temporal
provider is the exception. Compiler, build and test coordination tools may remain Rust outside produced programs.
An ABI that Bray cannot yet express requires compiler or target support, not a permanent custom non-Bray shim.

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

Bootstrap establishes attachment and cleanup services from explicit startup storage and low-level target operations.
It cannot initialize a service through public operations that already require that service, or admit cleanup through
the service being formed. Its own cleanup needs are empty or already backed. Generated entry cannot recursively wrap
the operation that establishes its prerequisites.

The bootstrap thread-storage contract has four target operations. They create a destructor-bearing key, load the
current thread's opaque pointer, store or clear that pointer, and destroy the key after every attached thread has
quiesced. The target clears a nonzero slot before it invokes the destructor on the exiting native thread. Explicitly
clearing a slot does not invoke the destructor, and destroying a key does not clean up another thread.

The trusted runtime owns the value stored in that slot. It assigns a process-unique attachment identity, reuses the
attachment for nested entry, and owns thread-static cleanup on outermost detach or native thread exit. Cleanup uses the
[static lifecycle dependency graph](../declarations/static-storage-declarations.md#lifecycle-dependency-graph) and its
deterministic structural tie order on that exact thread. Registration order is not semantic. The compiler supplies known
within-domain order and keys, while runtime ownership handles dynamic product and attachment relationships.
Cleanup continues after a cleanup panic, reports that incident, and never lets panic or cancellation cross the target
destructor callback. Ordinary `@thread_local` statics still use the runtime attachment and cleanup roles.

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

## Implementation status

These contracts describe the required architecture. The current Rust runtime and adapters do not establish delivery of
the native Bray implementation. [BRA-553](https://linear.app/bray-lang/issue/BRA-553) owns bootstrap and host authority.
[BRA-502](https://linear.app/bray-lang/issue/BRA-502) owns compiler-derived concrete cleanup requirements and physical
backing. [BRA-561](https://linear.app/bray-lang/issue/BRA-561) owns compiler-produced frame descriptors and Bray activation.
[BRA-505](https://linear.app/bray-lang/issue/BRA-505) and [BRA-506](https://linear.app/bray-lang/issue/BRA-506) supply finalizer
and represented-ownership execution under the existing source rules.

[BRA-557](https://linear.app/bray-lang/issue/BRA-557) owns dependency-ordered exact-thread cleanup.
[BRA-555](https://linear.app/bray-lang/issue/BRA-555) supplies whole-product provider retention and replaces fixtures that
expect code-only retention. Existing behavior checks remain enabled until that implementation supplies their replacement.
[BRA-507](https://linear.app/bray-lang/issue/BRA-507) completes suspending host cleanup. Documentation alignment is neither
native execution evidence nor cross-target proof. The WIP design's temporary language appendix remains until every
native consumer is implemented.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
