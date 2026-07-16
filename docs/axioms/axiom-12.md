## Axiom 12: Asynchronous execution is structured and first-class

Asynchronous execution is part of the language's semantic model.

Calling an async callable creates an owned inactive `Async<T>` whose normal completion type is `T`. Calling it does not execute the
body or create independently running work. Invocation checks argument transfer and value preconditions; body effects, capabilities,
execution requirements, lifecycle behavior, and postconditions travel with the computation until execution, and postconditions are
established only by normal completion.

Awaiting consumes and composes an async computation into the current task. Starting consumes it into an independently running task
and returns the sole source-level `Task<T>` owner. The distinction between computation and task is explicit in the type system rather
than expressed by additional control-flow keywords.

Suspension preserves every value, borrow, capability, effect, execution requirement, fact dependency, and lifecycle obligation
needed to resume or resolve the computation. Moving `Async<T>` or `Task<T>` transfers these dependencies.

Every ordinary lexical block is a structured task boundary. A task can outlive its creating block only when its handle is moved to an
owner whose contract preserves all dependencies and its eventual resolution. Bray has no source-level detached task without an
owner.

When async scope exit owns unresolved tasks, cancellation is requested for all of them before any is awaited. Cleanup then resolves
them in ordinary lifecycle order before dependent storage or capabilities end. Concrete and erased representations preserve this
as distinct recursive broadcast and lifecycle-resolution phases.

Cancellation is cooperative. It is observed at suspension points, checkpoints, and cancellation-aware operations. Cleanup is
shielded from repeated delivery and uses synchronous infallible destruction as the abnormal-exit fallback when graceful
finalization fails. Unobserved completion values receive their full lifecycle; suppressed failures remain owned until a panic report
or mandatory host-reporting boundary consumes them.

Async representation is protected. Direct await does not semantically require task creation, scheduler mediation, source boxing, or
source pinning. Independent task storage begins at the start boundary, while dynamically recursive suspended depth can require
runtime-managed storage.

Asynchronous behavior and execution requirements are part of behavioral contracts. `blocking_execution()` and
`compute_execution()` describe runtime-lane facts; async invocation defers them, direct await checks them, and task start selects a
compatible lane.

Low-level runtime machinery is a versioned trusted product substrate. Channels, operating-system threads, synchronization types,
timers, checkpoints, and concurrent combinators are ordinary standard-library Bray over that private ABI. Their generic cross-run
safety comes from ordinary inferred open dependency contracts, not compiler-recognized library names or marker types.
