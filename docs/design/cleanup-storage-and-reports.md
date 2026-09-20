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

## Resident services and execution contexts

Typed native symbols route generated calls to the selected runtime components. Component metadata distinguishes host
services from execution services so synchronous products do not acquire a scheduler dependency.

The resident implementation owns reservations and the run interpreter. Rust activation and terminal owners stay inside
that implementation. Fixed-ABI generated callbacks operate on admitted backing and return through selected symbols for
all context-sensitive operations, including cancellation, incidents, attachment, output, and product execution.

Independent execution contexts retain their own scheduler, budgets, workers, and lane authority. Entry scopes and
restores the entire root context, not only task identity. Cleanup retains its selected context after that host stops
accepting new work. Sharing services does not merge independent schedulers.

Explicit host formation operations establish services before product bindings exist. One typed native-role inventory
supplies compiler service-demand analysis and symbol contracts.

## Consuming reserved frame storage

Activation validates storage identity, shape, and provider compatibility before transferring backing and release
responsibility to one active-frame owner. Rejection leaves the reservation intact. Reservation identities describe
allocation lifetimes, so address reuse cannot authorize a stale activation or release.

Logical credits and physical backing have separate lifetimes. Activated or transferred backing is no longer available
reservation storage and cannot be reclaimed by whole-owner discharge. Final release waits for frame and result borrows,
uses the owning provider's callback, then releases that provider dependency. Callbacks execute outside registry locks.

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

Provider dependencies preserve code, immutable messages, and release callbacks through final use. These references do
not postpone static cleanup, but they do postpone unload until teardown and the last release callback have returned.
Runtime and bootstrap adapters share report construction, movement, suppression, and destruction rather than copying
payloads through parallel collections.

## Related documents

- [Composed async execution](composed-async-execution.md)
- [Async runtime](async-runtime.md)
- [Panic semantics](../language/expressions/panic-expressions.md)
