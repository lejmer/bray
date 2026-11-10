# Async representation and storage

`Future<T>` has protected representation. Source can name its completion type `T`, move the value, and call its
language-defined methods, but cannot name, inspect, construct, project, lay out, or depend on its concrete representation.

An async computation retains the state needed to resume and resolve it, including:

- receiver and argument values not yet consumed by the body,
- locals live across suspension,
- active child computations composed by direct await,
- cleanup state needed for cancellation, panic, and lifecycle resolution.

The source type `Future<T>` is an owned value with a protected representation. Two async callables with the same declared
result type can have different representations while both invocation expressions have source type `Future<T>`.

Ordinary control-flow merges, parameters, returns, and homogeneous aggregates can contain `Future<T>` values from
different producers. Supporting computations with different representations can require dynamic storage. Direct await
of a known producer does not itself require dynamic storage.

Compiled interfaces preserve a computation representation's identity, size, alignment, and operations for movement,
resumption, cancellation, result transfer, and destruction across package boundaries. These are not source declarations,
generic arguments, reflection results, or name-resolution entries.

## Storage rules

Calling an async callable does not semantically require heap allocation. An inactive computation can use any storage
that preserves the observable ownership and lifecycle rules.

Direct await does not semantically create a task, enqueue work, or require task-control storage. The child computation
can run as part of the current computation without scheduler mediation.

Before execution begins, an inactive computation can be relocated through ordinary movement of its `Future<T>` owner.
Once execution has begun, any state whose representation requires a stable address remains stable until that state is
completed or destroyed. This requirement is not exposed through a source-level pinning type or operation.

Calling `start()` is the independent-execution storage boundary. The runtime establishes stable task-owned storage
for the computation before its first resume, creates the task's execution state, and returns the source-level `Task<T>`
owner. Allocation strategy is not observable language behavior.

Recursive async execution whose dynamically suspended depth is not statically bounded requires dynamic storage for the
recursive activations. Source never introduces boxing or pinning solely to make async recursion well formed.

## Cleanup capacity

Establishing an ownership or child-run obligation secures the storage required for its mandatory cleanup. This includes
the state needed to suspend cleanup, resolve dependent child runs, transfer terminal results, and retain owned cleanup
incidents. Once the obligation is established, these operations use the secured capacity even when further allocation
fails. Ownership transfer preserves this capacity until the obligation is resolved or transferred again.

Capacity is physical backing with the concrete size, alignment and lifetime required for cleanup. It includes bounded
outgoing errors and callback outcomes, wait links, host-detachment acknowledgements and terminal reporting infrastructure
where those are required. Scalar bookkeeping credits alone do not establish admission.

If the required capacity cannot be secured, the operation fails before establishing the new obligation. Existing owners
retain their values and cleanup capacity through failure propagation. The same rule applies to partially initialized
values, inactive captures, and static owners.

Owners requiring runtime cleanup capacity use a uniform local allowance for their concrete type throughout ownership.
A proof that the current value has completed its finalization can omit the finalizer invocation, but does not release
that allowance. Later mutation can invalidate the proof without requiring new cleanup admission. A proof that an action
is unnecessary for every value of the concrete type can remove that action's capacity requirement entirely.

An aggregate admits capacity only for additional local obligations it establishes. Existing represented children retain
their own capacity through moves and wrapping. Creating an additional owner must secure its additional capacity before
publication. Partial construction retains the initialized inputs and their allowances until ownership transfer commits.

The local allowance is discharged when ownership ends. It covers the mandatory terminal cleanup path, not unlimited
explicit finalizer calls or retries on a surviving owner. Those calls retain their ordinary execution requirements.
Storage already transferred into a retained incident or used by a live activation remains owned until that use ends.

Admission and discharge contribute their actual allocation, deallocation, synchronization and failure behavior to
[execution guarantees](../contracts-and-trust/execution-guarantees.md). A currently complete value's construction or
disposal is not automatically pure or total merely because its finalizer invocation can be omitted.

Capacity can be supplied by existing frame storage, co-allocated backing storage, or separately reserved storage.
Cleanup steps with disjoint storage lifetimes can reuse capacity. Recursive and erased representations retain enough
capacity for each live obligation rather than relying on a fixed global allowance.

Using secured capacity for an activation preserves exactly one owner of its storage and release obligation. Rejected
activation leaves the previous owner and its capacity intact. Storage and provider dependencies remain valid until all
users of that storage have finished. Ending the original ownership obligation cannot release storage still owned by an
activation or retained outcome, and reuse of an address does not make a stale storage identity valid again.

Allocations and external operations performed by application finalizers follow their ordinary failure semantics.
Their failures are handled by the [abnormal-exit cleanup rules](../lifecycle/scope-exits-panics-and-cancellation.md).

## Cost transparency

Compiler inspection information for an async callable must make these properties available:

- total retained computation storage size and alignment for each concrete compiled instantiation,
- values retained across each suspension point,
- the largest retained values and their source locations,
- whether starting requires task-owned dynamic storage,
- dynamic-storage sites introduced for recursion or representation erasure,
- execution requirements and thread-affinity constraints.

This information supports compiler inspection and diagnostics. It is not a source-visible layout guarantee. Optimization
can change it without changing the callable's source contract unless an explicit ABI rule states otherwise.

## Navigation

- [Language index](../index.md)
- [Async and concurrency index](../async-and-concurrency.md)
- Previous: [Async functions and computations](async-functions-and-computations.md)
- Next: [Captured state](captured-state.md)
