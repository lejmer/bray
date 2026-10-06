# Cleanup storage and panic reports

Cleanup capacity and failure payloads have owners throughout construction, execution, propagation, and disposal. The
runtime admits the capacity needed to resolve an obligation before that obligation becomes live. Mandatory cleanup
consumes that capacity rather than depending on fresh allocation.

## One cleanup description

Checking owns selected lifecycle actions, completion evidence, and admission effects. Guarantee certification and
failure planning consume those semantic facts without depending on generated MIR or physical frame layout.

Lowering's lifecycle domain expands selected actions and symbolic storage requirements together. Source lowering adds
initialization guards and continuations, while specialization supplies substitutions and repairs descriptors. Neither
repeats action selection. Concrete layout is demanded after semantic certification, avoiding a cycle in which cleanup
legality depends on its own generated representation.

Concrete descriptors supply size, alignment, initialized-state operations and dependency metadata. Bray admission and
activation consume these results rather than reconstructing compiler models, symbol strings or descriptor graphs.

Requirements preserve multiplicity and actual storage lifetime. Structural traversal shares enclosing activation state
where possible. Arrays and buffers use element traversal instead of a separate unrolled cleanup body per element.
Recursive ownership refers to a child's local allowance, and erased ownership retains the concrete descriptor and
provider. A helper introduced for compiler organization does not itself justify another reservation.

## Shared admission-domain binding

Providers exchanging owned values bind admission, activation, and discharge to one validated service domain before
accepting transferable ownership. The binding is independent of scheduler identity and remains stable while obligations
or transferred storage depend on it. Ordinary moves need no capacity-domain field or fallible rebinding.

Shape registration may be lazy at the first fallible admission, before ownership commits. Mandatory activation and
discharge use existing registration without growing routing metadata. Concrete backing retains its provider-owned
release operation even when the admission domain is shared.

## Provider dependencies and formation

Code realized into one final product shares its binding, including imported MIR and static-library contributions.
Separately formed providers declare their demanded service and provider dependencies. Formation validates that
transitive set before initialization or callable entry can publish owned values.

The existing product dependency and lifecycle model supplies ordering and cycle checks. Binding is not another
initialization planner. Failed formation keeps the services needed to clean up its accepted state, while independently
live providers remain valid. Lazy source initialization retains its ordinary timing.

Artifact identity identifies reusable code. A load generation identifies one formed image and remains stable through
failure and closure. Its admitted service binding cannot be replaced by a cached or image-local fallback. Retirement
invalidates that generation before the host releases the image, so a later load cannot inherit its binding or lifecycle.

Every provider-dependent report, typed payload, symbol, callback and borrow retains code and product statics together.
Entry closure rejects new admission while existing owners retain completion and disposal authority. Dependency edges
govern cleanup eligibility, with static consumers preceding providers and exact-thread cleanup staying on its attachment.

The caller owns graceful shutdown. A blocked attempt returns with that owner instead of waiting on caller-held
dependencies. The resident Bray host owns product storage and admitted fallback cleanup from formation. Both use one
terminal state. Bound services retain execution, affinity and reporting through fallback completion, so retaining bytes
alone cannot satisfy the duty. The language shutdown chapter defines
[ordinary finalization and abnormal abandonment](../language/async-and-concurrency/execution-roots-and-product-shutdown.md#shutdown-ownership-and-normal-finalization).

## Resident services and execution contexts

Typed native symbols route generated calls to the selected runtime components. Component metadata distinguishes host
services from execution services so synchronous products do not acquire a scheduler dependency.

The resident Bray implementation owns reservations, activations, terminal state and the run driver. Compiler-generated
callbacks operate on admitted backing and use validated roles for context-sensitive operations, including cancellation,
incidents, attachment, output and product execution. Linked Rust ownership is outside the intended architecture.

Independent execution contexts retain their own scheduler, budgets, workers, and lane authority. Entry scopes and
restores the entire root context, not only task identity. Cleanup retains its selected context after that host stops
accepting new work. Sharing services does not merge independent schedulers.

Explicit host formation operations establish services before product bindings exist. One typed native-role inventory
supplies compiler service-demand analysis and symbol contracts.

## Physical local cleanup backing

Admission secures actual backing before a new obligation becomes live. Concrete requirements include activations,
results, typed errors, reports, wait links, bounded callback outcomes and terminal infrastructure. Scalar bookkeeping
credits alone cannot establish that guarantee. Mandatory cleanup uses secured storage when further allocation fails.
Application finalizer allocations and explicit retries retain their ordinary fallible behavior.

Each concrete type has a uniform local allowance throughout ownership. Current-value completion evidence may omit a
finalizer invocation but cannot release backing that later mutation could require. Type-wide proof can remove an action
that is impossible for every value. Moves and wrapping transfer existing allowances, and aggregates admit only their
additional local needs. Partial construction and rejected admission preserve initialized inputs.

Caller storage, enclosing frames, coallocation, separately owned regions or pools can provide backing. Disjoint lifetimes
can share a region. Recursive and erased owners retain adequate storage for each live obligation without an arbitrary
slot limit or a mandatory separate heap allocation. Choose pooling only when lifetime, locality and measured cost justify
it.

## Consuming reserved frame storage

Activation validates storage identity, shape, and provider compatibility before transferring backing and release
responsibility to one active-frame owner. Rejection leaves the reservation intact. Reservation identities describe
allocation lifetimes, so address reuse cannot authorize a stale activation or release.

Source discharge releases unused allowance. Activated or transferred backing is no longer available reservation storage
and cannot be reclaimed by whole-owner discharge. It remains owned through its last activation, result, incident or
internal delivery user. Final release uses the owning provider's callback after storage borrows end, then releases that
provider dependency. Callbacks execute outside registry locks.

## Incidents and report ownership

Each local mandatory action reserves backing for its possible outgoing failure. Child owners bring their own capacity,
and structural forwarding transfers existing records instead of reserving another wrapper. Admission publishes a
complete local bundle atomically with the new owner. Initialized inputs retain their ownership until construction
commits.

An outgoing failure transfers backing into a run, report, or host sink. It can outlive the completed frame without
keeping the entire frame alive. Quiescence resolves child runs before an error becomes an incident whose remaining
operation is synchronous destruction. A bridge able to produce several distinct outcomes admits the corresponding
bounded capacity.

A protected `PanicReport` has a movable inline primary header and separately owned suppressed records. Caller-provided
destinations let allocation-failure reports exist without another allocation or a shared emergency object. Message
ownership distinguishes retained immutable bytes from owned backing with its release provider. Mutable or short-lived
borrowed messages are snapshotted according to the language's panic contract.

Suppression moves primary payloads into admitted records and splices existing chains. Records do not recursively contain
whole reports. Iterative reporting and destruction keep incident count from becoming native recursion depth. Collection
locks protect ownership transfer only, and callbacks consume detached sequences outside the lock. Reentrant reporting
can append newly owned incidents without mutating the sequence being visited.

## Native transfer and retirement

The native outcome model uses a tag and caller-owned report destination. The callee transfers the report before
publishing a panic outcome. Forwarding preserves live ownership across synchronous calls, catches, independent runs, and
foreign entries. Compiler result-place and liveness analysis can reuse non-overlapping destinations.

Provider dependencies preserve code and product statics through final use, including message access and release callbacks.
The resident caller releases the last provider lease only after image code and body-local destruction return.
Source `with` exit occurs before body-local destruction, so it alone is insufficient. Teardown incidents are internal
terminal work and are drained before their dependencies become unavailable, without blocking their own domain as external
roots. Runtime and bootstrap adapters share report construction, movement, suppression and destruction.

## Cost constraints

Physical backing consumes memory before failure, and whole-product retention can keep substantial resources alive for a
small report. A caller needing history after unload can preserve independent diagnostic data through optional library
operations, then dispose of the original report. Provider-dependent typed data continues to retain the product.
Ordinary reporting does not select snapshot, history or symbol-lookup support that its consumer does not use.

Storage and synchronization choices account for allocations, retained bytes, indirection, atomics and contention.
Compiler planning reuses immutable demand-driven analysis, with concrete specialization supplying layout. It does not
add a second cleanup walk or a universal planning pass. Shared service identity does not require a central lock or a
service call for every move or borrowed access.

The destination is execution competitive with Rust and C++, minimum practical linked code and data, and full compilation
no slower than current rustc, preferably faster. Measure comparable native paths as they become available, including small
synchronous and constrained-resource consumers. Gate avoidable regressions then. Enabling work may temporarily regress a
measure when later implementation depends on it. Record the cause, dependent change and stage where reevaluation or
removal becomes possible. Final runtime and footprint comparisons and full build-time measurements remain required.
The [WIP cost analysis](../wip/bray-native-runtime-cost-analysis.md) records the selected architecture's unresolved costs.

## Related documents

- [Composed async execution](composed-async-execution.md)
- [Async runtime](async-runtime.md)
- [Panic semantics](../language/expressions/panic-expressions.md)
