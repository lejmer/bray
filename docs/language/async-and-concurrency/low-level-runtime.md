# Low-level runtime

Low-level async runtime machinery is part of the trusted product substrate and is not a source-visible compiler-known package.

The runtime ABI is an implementation boundary, not an implementation-language requirement. A conforming runtime can be implemented
in trusted Bray, another language, compiler-generated code, or a mixture. The preferred architecture keeps target-independent
runtime policy in trusted Bray and restricts foreign code to mechanisms that Bray cannot express without a platform call.

The language-defined runtime contract provides semantic operations equivalent to:

- running a root frame,
- driving the distinguished main-thread lane,
- starting a task from an inactive frame,
- resuming and suspending a frame,
- waking a task,
- requesting cancellation of a runtime-owned run,
- registering and waking join waiters,
- observing cancellation and checkpoints,
- traversing owned task obligations without lifecycle resolution,
- resolving frame lifecycle only after cancellation broadcast,
- accepting, reporting, and destroying ordered cleanup incidents,
- creating, waiting on, signalling, and destroying runtime events,
- selecting compatible cooperative, local, main-thread, blocking, and compute lanes,
- resolving runtime shutdown.

These operation descriptions do not reserve Bray declaration names. Their binary symbols, calling conventions, frame descriptors,
and versioning belong to the private runtime ABI.

A conforming runtime must preserve:

- `Future<T>` and `Task<T>` ownership and movement,
- dependency and affinity contracts,
- exactly-once frame completion and destruction,
- cancellation request and cleanup shielding rules,
- phase-separated task broadcast through concrete and erased frame state,
- run-boundary panic capture,
- ownership and mandatory reporting of suppressed cleanup incidents,
- join and cancellation completion visibility edges,
- lane execution facts,
- structured root shutdown.

Native thread creation, child process creation and signalling, typed process-protocol transport, process reaping, and hard product
resource limits are ordinary standard-library services over private trusted product or platform ABI declarations. They are not
additional compiler-known runtime operations. When an async wrapper integrates them with task suspension, its trusted contract must
connect the external completion event to the runtime without blocking a cooperative worker. Ordinary parallel budgets are
standard-library permit hierarchies and require no product-host budget authority.

Private standard-library implementation modules bind the ABI through trusted private declarations. A binding can have a trusted
Bray body, be compiler-lowered, or be `extern` with its body supplied by another linked artifact. An `extern` body can itself have
been compiled from Bray; only a binding that crosses a foreign ABI is a foreign call. These declarations remain ordinary private
`std` source declarations and are not recognized by their source paths. A different standard library can organize its wrappers
differently while targeting the same ABI.

The product and runtime ABI artifact includes an immutable compiler-readable semantic-contract table keyed by closed binary ABI
roles rather than source symbol names. A table entry can encode ownership transfer, open run-transfer subjects, synchronization and
visibility edges, callback execution-root facts, cancellation state and observation behavior, panic behavior, lifecycle ownership,
and capability requirements. During the trusted product-and-standard-library build, each private ABI binding declaration is
explicitly associated with one compatible ABI role. The compiler validates the role, signature, target, ABI version, and contract
record schema, then checks the binding and its ordinary Bray wrappers using that record. It trusts the selected substrate to
implement the record.

The association is private build metadata, not source syntax, a package path convention, or a compiler-known declaration. It is not
published through the public standard-library surface. Public wrapper bodies infer ordinary portable dependency and callable
contracts from their checked uses, so consumer interfaces contain no private ABI role identities.

Trusted runtime declarations that affect scheduling, memory visibility, synchronization, cancellation, foreign callbacks, or
device access must expose safe internal contracts covering ownership, borrowing, visibility edges, cancellation, panic, fact
invalidation, and capability requirements. They can use raw memory, target intrinsics, device memory, platform APIs, and foreign
calls only through the corresponding trusted capabilities.

## Implementation allocation

The following facilities are expected to be implementable in ordinary safe Bray:

- public channel, synchronization, task-combinator, thread-owner, process-owner, codec, protocol, budget, and parallel-algorithm
  behavior,
- lifecycle and cancellation policy,
- process framing, authentication, and decoded-value commit ordering,
- composition of `Future<T>`, `Task<T>`, and `RunResult<T>`.

The following facilities are expected to be implementable in trusted Bray over compiler-provided atomics, raw-memory operations,
manual allocation, and private runtime roles:

- ready queues, waiter lists, timer heaps, task registries, permit counters, and scheduler policy,
- task control blocks, type-erased descriptor storage, cleanup-incident storage, and raw platform-handle owners,
- runtime wake registration and conversion of an external completion event into a Bray task wake,
- exported thread-entry and foreign-callback trampolines that establish a Bray run before invoking safe Bray code.

The following operations necessarily require a product or platform mechanism:

- creating, joining, waiting on, or waking native operating-system threads,
- creating, signalling, terminating, waiting for, and reaping child processes,
- polling or registering with platform event and timer facilities,
- acquiring platform virtual memory and process-wide runtime resources,
- platform-specific unwind, signal, thread-local, debugger, or host-report integration where required by the target.

Those mechanisms do not necessarily require custom C or Rust source. Stable operating-system or system-library symbols can be bound
directly through private Bray FFI declarations. A custom native shim is justified only when the target surface depends on macros,
inline-only APIs, variadic or otherwise unrepresentable calling conventions, unstable native layouts, assembly trampolines, or
toolchain unwind and thread-local integration. Such a shim exposes narrow runtime mechanisms and cannot absorb source-language
ownership, lifecycle, cancellation, protocol, or scheduling policy.

The cleanup-report sink is a product-host service and does not require an async scheduler; synchronous products using native
threads provide it too. It is mandatory even when a product has no interactive debugger or logging backend. It accepts an ordered
batch of owned type-erased incidents and suppressed child-run panic reports at terminal-boundary observation, reports at least each
entry's kind, concrete type, producer/source identity, and ordinal to the product host or persistent inspection stream, then
infallibly destroys every payload. A product policy
can additionally terminate or render richer diagnostics, but cannot silently discard the batch or change source
`RunResult.Cancelled` into a payload-bearing variant.

A runtime or wrapper is nonconforming if its safe surface permits data races, dangling dependencies, duplicate child-run ownership,
unsynchronized shared mutation, leaked scoped capabilities, unresolved task, thread, or process obligations, unbounded hidden
parallelism, or destruction of a running frame.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Standard-library concurrency](standard-library-concurrency.md)
- Next: [Summary](summary.md)
