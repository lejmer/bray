## Axiom 12: Asynchronous execution is structured and first-class

Asynchronous execution is part of the language's semantic model.

Calling an async callable creates an owned inactive `Future<T>` whose normal completion type is `T`. Calling it does not execute the
body or create independently running work. Invocation checks argument transfer and value preconditions. Body effects, capabilities,
execution requirements, lifecycle behavior, and postconditions travel with the computation until execution, and postconditions are
established only by normal completion.

Awaiting consumes and composes an async computation into the current task. Starting consumes it into an independently running task
and returns the sole source-level `Task<T>` owner. The distinction between computation and task is explicit in the type system rather
than expressed by additional control-flow keywords.

Every executing operation belongs to one run. Direct calls and direct awaits remain in that run. Tasks, native threads, and child
processes create owned child runs. The executable host owns the root process, main thread, and root run. An async entrypoint is driven
as a host-owned root task without a source-visible task owner.

Suspension preserves every value, borrow, capability, effect, execution requirement, fact dependency, and lifecycle obligation
needed to resume or resolve the computation. Moving `Future<T>` or `Task<T>` transfers these dependencies.

Every ordinary lexical block is a structured task boundary. A task can outlive its creating block only when its handle is moved to an
owner whose contract preserves all dependencies and its eventual resolution. Bray has no source-level detached task without an
owner.

When async scope exit owns unresolved tasks, cancellation is requested for all of them before any is awaited. Cleanup then resolves
them in ordinary lifecycle order before dependent storage or capabilities end. Concrete and erased representations preserve this
as distinct recursive broadcast and lifecycle-resolution phases.

Cancellation is cooperative. It is observed at suspension points, checkpoints, and cancellation-aware operations. Cleanup is
shielded from repeated delivery and uses synchronous infallible destruction as the abnormal-exit fallback when graceful
finalization fails. Unobserved completion values receive their full lifecycle. Suppressed failures remain owned until a panic report
or mandatory host-reporting boundary consumes them.

Async representation is protected. Direct await does not semantically require task creation, scheduler mediation, source boxing, or
source pinning. Independent task storage begins at the start boundary, while dynamically recursive suspended depth can require
runtime-managed storage.

Asynchronous behavior and execution requirements are part of behavioral contracts. `blocking_execution()` and
`compute_execution()` describe progress-policy facts, while `main_thread_execution()` describes the distinguished initial-thread
fact. Async invocation defers them, direct await checks them, and task start selects a compatible lane.

`RunResult<T>` makes a child run's normal, panicked, or cancelled terminal state explicit. Matching preserves that state as a value.
`try` unwraps normal completion and forwards panic or cancellation into the current run. `catch` converts only panic in the current
run into `Result<T, PanicReport>`. It does not intercept cancellation or inspect a nested run-result value implicitly.

Low-level runtime machinery is a versioned trusted product substrate. Channels, operating-system threads, synchronization types,
child processes, parallel algorithms, timers, checkpoints, and concurrent combinators are ordinary standard-library Bray over
private trusted ABI operations. Private ABI bindings receive compiler-readable semantic contracts from closed binary roles, while
their public wrappers expose only ordinary inferred contracts. Their generic cross-run safety does not depend on
compiler-recognized library names or marker types. Parallel algorithms use domain-typed library budgets. Underlying product and
runtime hard limits remain independently enforced.

Public concurrency policy, owners, protocols, combinators, and parallel algorithms are Bray source. Portable low-level internals
should be trusted Bray. Foreign or native code is confined to target mechanisms that Bray cannot perform without the host platform,
and an ABI or `extern` boundary does not imply a foreign implementation language.

The generated root frame resolves every source-owned task, thread, process, payload, and cleanup incident before publishing the
root terminal outcome. The host then maps that outcome and shuts down runtime infrastructure and process-scoped resources. Normal
root return never detaches child work.
