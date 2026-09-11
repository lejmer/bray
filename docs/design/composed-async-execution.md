# Composed async execution

This goal-state design specifies native execution of directly awaited frames and mandatory cleanup within
their owning run.
It refines [Async and runtime design](async-runtime.md#direct-await-lowering). Independent starts retain the task
boundary defined there. Ordinary synchronous calls and foreign callback roots retain their existing language contracts.

## Run and activation ownership

An independently scheduled run owns one scheduler registration, ready slot, cancellation state and dispatch authority.
It also owns admitted event and join notification storage. Only one blocking run wait is active at a time. Event and
join storage may remain separately reserved because their representations differ.

A callable activation owns its stable frame state, captures, local resume position, result storage and lifecycle
progress. An activation has at most one directly composed child. A parent waiting for that child retains its live
locals, borrows, continuation and storage. An activation does not acquire an independent task identity or cancellation
state.

The run retains its current activation and root ownership. Admitted frame storage retains the parent linkage and pending
child ownership needed for composition. Dynamic depth does not require growing a separate execution-stack vector.
Known frame layouts can use enclosing storage. Recursive or erased frames use suitably owned indirect storage. These
choices do not change task identity or introduce a semantic allocation requirement for known direct awaits.

Only the dispatcher holding the run's exclusive dispatch claim can change its activation chain or invoke generated
resume, move or lifecycle operations. Scheduler, registry and wait locks are released before those operations and
foreign callbacks execute. External notifications cannot access activation memory.

## Iterative execution

The driver executes these transitions in a loop:

1. `ComposeAwaitedFrame` transfers the inactive child into the current activation's pending child slot. The transfer
   leaves one owner. If control leaves before awaiting, the parent still owns that child's capture cleanup.
2. The following awaited suspension records the parent's continuation and installs the child as the current activation.
   The parent's generated resume callback returns before the driver invokes the child callback.
3. A child yield or blocking wait yields or suspends the owning run. The parent remains retained without another
   scheduler registration.
4. Child completion makes its outcome available to the parent and restores the parent as current. The child remains
   retained until the parent consumes or resolves that outcome through `ResolveAwaitedFrame` and its cleanup edges.
5. Successful result movement transfers ownership exactly once into admitted parent storage. The driver then resolves
   remaining child lifecycle state and releases the child when its storage is no longer borrowed. Failure or abandonment
   before ordinary result consumption follows the parent's checked cleanup edges.

A child panic or cancellation follows checked current-run propagation and enclosing catch or cleanup boundaries.
Internal ABI outcome tags do not create a source-visible child run or an extra `RunResult` observation boundary.

The driver does not recursively poll a child from inside a parent's native resume invocation. Long chains of immediately
completed calls therefore do not grow the native call stack with async depth. Foreign calls that synchronously reenter
Bray still have ordinary native call-stack nesting.

## Run-directed wakes and readiness

A scheduler wake requests another dispatch of the run. It does not carry an activation pointer or select a child's
resume state. The driver owns the current activation and checks the active wait before advancing generated control flow.

The scheduler retains its exclusive-dispatch and pending-wake handshake:

- Taking ready work changes `Queued` to `Running` and clears the consumed pending notification.
- A wake during `Running` records pending readiness without dispatching the run concurrently.
- The driver registers a blocking wait while it still owns `Running`.
- Releasing dispatch atomically chooses `Queued` if a wake became pending, otherwise `Idle`.
- A wake after release changes `Idle` to `Queued`. Wakes against terminal or retired runs do nothing.

The event-wait record retains a strong event reference, observed event generation and admitted registration storage.
Observation and registration use the event owner's existing generation protocol. Registration against a changed
generation or closed event notifies immediately. Publication of the registration completes before dispatch is released.
On subsequent dispatch, the driver first checks deliverable cancellation. It advances past an ordinary event suspension
only if the retained generation has changed or the event is closed. An unchanged, open event keeps its registration and
observation. The driver releases dispatch without invoking the leaf.

A join wait uses the independently started child's persistent terminal state as its readiness predicate. A notification
never consumes the child's result. The source-level Task owner retains result-observation and lifecycle authority.

Withdraw a wait only when consuming readiness, delivering cancellation, retargeting, or terminating. A signal between a
readiness check and dispatch release remains covered by the installed registration and pending-wake handshake. Retain
existing notification source checks, but a matching source alone does not prove readiness. In particular, cancelling and
rearming a wait on the same event generation must not let a delayed notification complete the replacement wait.

Delayed notifications retain only the run wake record and their source identity. They never retain a child-frame
pointer. An old notification may cause an extra readiness check. It cannot resume a retired activation or manufacture a
completed wait. Withdrawing a subscription prevents new notifications from that subscription, while already selected
notifications may finish safely.

Persistent reactor or I/O subscriptions belong to the operation that created them. The run wait only observes operation
readiness. An operation combining multiple sources owns its required admitted subscriptions and exposes one readiness
condition to the run. Composition does not introduce an allocation for each enclosing await.

No activation generation counter is needed for advisory run wakes. Independent run identities remain non-wrapping
admission identities. Exhaustion fails before accepting a new run. Event generation remains an event-owner contract,
not a second activation identity. On generation exhaustion, the producer preserves its specific failure and closes the
event to wake observers. Close requires no new generation or allocation. Runtime-internal event critical sections
perform no allocation, invoke no callbacks or destructors, and use mutations that cannot unwind midway through an
invariant. Under that enforced contract, event locking recovers poisoned guards with their state intact, consistently
with wait-node locking. This is not permission to recover arbitrary corrupted event state. Source panics and callback
failures execute outside the lock and cannot poison it. Signal or close must not silently abandon a mandatory completion
notification.

## Cancellation and cleanup

The run owns the persistent cancellation-request flag. Delivery is separate from that flag. A new request wakes the run.
At a legal cancellation point, the driver chooses the checked cancellation continuation and records delivery. The driver
performs phase-one cancellation broadcast before phase-two lifecycle resolution, following the checked cleanup plan
through active children and owned independent runs.

Cleanup shields retain the request for observation while preventing repeated delivery. A shielded cleanup wait may sleep
until its event or child becomes ready. The scheduler must not repeatedly enqueue it just because the request flag
remains set. A request deferred by a shield does not keep an unready run queued. Shield exit rechecks that request for
delivery at a legal delivery point.

When cancellation must drain the same independent child, retain its ownership and retarget the existing admitted wait.
Changing cleanup progress does not require another task registration or another cancellation context. Explicit starts
performed by application finalizer code remain new independent starts with their ordinary admission and failure
behavior.

Structural cleanup expansion records each child's immediate-parent failure continuation. A nested failure must finish
remaining siblings and preserve pending return values before leaving the containing cleanup boundary. Outcome collection
and shield balancing have one implementation in lowering's cleanup-outcome owner. Forwarding an existing outcome does
not itself require another accumulator or shield.

## Execution lanes and fairness

Entering a composed child does not choose a new execution lane or grant blocking, compute or main-thread authority.
Its checked requirements must hold in the current run context. Parent state that remains live, including exact-thread
borrows, continues to constrain execution while the child is active.

At dispatch boundaries, scheduler placement uses the current validated run execution state, incorporating retained
parent dependencies and the active child's requirements. A root's initial frame state or a child's unqualified numeric
state ID cannot substitute for that contract. The existing compatible-lane selection remains responsible for placement.
An async executable root stays on the main-thread lane. Migratable runs can use compatible workers where their live
state permits movement.

Run admission reserves queue membership for the finite lane classes permitted by its checked execution contract,
including cleanup and any fixed origin or main-thread identities. An erased child's requirements can restrict this
admitted set but cannot expand it. Its invocation must satisfy the existing checked execution boundary. Composing a
child cannot introduce a new queue allocation or an unadmitted execution capability. Independent starts remain the
boundary for work requiring a separately selected lane.

Explicit yield returns dispatch to the scheduler and queues the same run. The driver also uses a finite transition
budget for repeated activation entry, completion and cleanup transitions. It saves a valid continuation and requeues at
a callback boundary when the budget expires. This scheduling yield does not deliver cancellation at a new source-level
point and does not interrupt an indivisible synchronous operation. The budget is an internal scheduling policy, not a
language or ABI guarantee.

## Host cleanup roots and reentry

Ordinary synchronous scopes may end ownership only when remaining cleanup is synchronous and admissible. Composition
does not add an implicit async driver to ordinary synchronous functions.

Host boundaries that already permit ownership requiring asynchronous cleanup admit the needed driver before accepting
that obligation. Such admission retains the execution domain, scheduler and wake resources, compatible lane and thread
attachment, and required result and incident storage. Driver activation during mandatory cleanup only binds these
resources. It cannot create a runtime, allocate a task registration or attach an unprepared thread.

| Boundary                                   | Admission and retained lifetime                                                                                                                                            |
|--------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Async executable root or independent start | Admit before publishing the run. Retain execution resources through terminal publication and withdrawal of waits.                                                          |
| Product or thread-static owner             | The initialization domain admits cleanup driving before publishing the initialized owner. Retain it through the domain's final cleanup, on the required thread.            |
| Host-owned returned value                  | Secure host cleanup capacity before the transfer that makes the host responsible. Failed admission leaves the previous owner responsible.                                  |
| Foreign ownership transfer                 | The trusted wrapper's ownership contract establishes admission before accepting the value. It retains the required provider and execution dependencies through resolution. |
| Synchronous borrowed callback              | Use the existing synchronous callback root and attachment rules. No async cleanup reservation is introduced without an ownership contract requiring it.                    |

When an entrypoint or foreign call can return ownership requiring host-driven cleanup, establish the host reservation
before invoking that call. Keeping an unadmitted returned value in a host temporary is insufficient, because cleaning
that temporary may require the same driver. The declared ownership contract identifies this admission requirement.

A domain can reuse a driver for statics whose cleanup cannot overlap. Each potentially overlapping host ownership
obligation retains distinct admitted driver capacity before acceptance.
Sharing requires a domain guarantee of nonoverlap. A callback entered during cleanup cannot depend on fresh admission to
clean an owner already accepted by its outer boundary. A single shared emergency driver cannot cover unbounded reentry.
New independently
admitted boundaries may fail before accepting ownership. Cleanup of accepted owners uses the already secured capacity.

A foreign callback cannot recursively dispatch the activation whose native invocation is still executing. Existing
synchronous callback roots retain their own scoped execution context. If an existing host contract requires nested async
cleanup, that boundary uses a separately admitted driver and restores the outer context when it returns. Compatible
ready work may progress while a nested driver waits, but the still-executing outer activation is ineligible for
dispatch.

Nested entry reuses an existing exact-thread attachment according to the foreign-entry contract. Teardown retains it
through pinned work and thread-static cleanup. Generated callbacks execute outside runtime locks. These rules prevent
lock ownership and double-dispatch defects. They do not resolve an application dependency cycle in which a synchronous
callback waits for work that requires that same callback to return.

A product with only synchronous cleanup retains the synchronous host path and does not link scheduler machinery solely
for incident reporting. Reachable execution and ownership requirements select host-driving capabilities through existing
demand-driven product formation.

## Capacity, results and provider lifetime

The detailed ownership and native transfer contracts are defined in
[Cleanup storage and panic report ownership](cleanup-storage-and-reports.md).

Each new owner whose concrete type requires runtime cleanup capacity admits a uniform local bundle before publication.
Current-value completion evidence can skip execution but does not change that bundle. Its credits survive completion and
later mutation until ownership ends. Only a type-universal proof removes a requirement from the bundle. This gives
moves, returns and consuming boundaries a uniform capacity contract without a per-value capacity flag.

Checking accounts for admission and discharge effects in construction and owned cleanup. Allocation, deallocation and
synchronization contribute impurity. An uncaught admission panic prevents totality, while returning a handled admission
error can still satisfy it. Lowering must preserve these checked guarantees. A borrowed complete-state finalizer's proof
remains distinct from the effects of creating or disposing of its owner.

Logical cleanup capacity follows the ownership obligation. Physical activation storage and retained outcomes can outlive
the discharge of that logical reservation. Keep their release conditions separate.

An ordinary nominal owner admits only the additional local cleanup work it introduces. Its represented children retain
their own credits. Moving or wrapping a value transfers responsibility without allocating, storing its previous address,
or adding hidden bookkeeping fields to its ordinary source ABI. Exact-shape fungible capacity inside one validated
ownership domain can implement this rule. Bind receiver addresses when activating stable storage, after ordinary movement.
Discharge the local bundle when its obligation ends, including phases omitted by verified completion evidence.
Activation removes available storage and records a spent logical credit. Whole-owner discharge consumes spent credits
before releasing unused reservation storage. New owner admission supplies its required capacity. Activated storage has
separate ownership and is not reclaimed by credit discharge. This accounting preserves capacity for other live owners.

Reserved storage becomes active through the
[consuming transfer contract](cleanup-storage-and-reports.md#consuming-reserved-frame-storage). The active frame becomes
its sole release owner, while rejected activation preserves the reservation.

An activation remains alive while a child or result path borrows its storage. Successful result transfer and remaining
cleanup must complete before reclamation. Notifications retain run wake storage rather than frame memory. Independent
run results and incident payloads remain in their terminal, report or host owner after execution ends. Reusing a
reservation requires all of its actual storage users to have released it, not merely sequential execution of two phases.

Incident retention uses admitted backing that transfers with the owned payload into a run, panic report or host sink.
Frame completion cannot release that backing. Direct recording and ownership transfer avoid allocating temporary result
vectors at intermediate boundaries. Preserve encounter order and the existing payload owner. Report and destruction
callbacks execute outside collection locks and may produce further incidents under their own admitted obligations.

Inactive frames are independent of their original scheduler. Their allocation and executable-provider dependencies
remain valid through activation, result transfer and storage release. A destination run supplies its own admitted
execution resources and must satisfy the frame's checked requirements. An ordinary move does not perform fallible
runtime rebinding. A separate start or host entry remains an admission boundary.

Providers exchanging ordinary owned values use the same validated
[admission-domain binding](cleanup-storage-and-reports.md#shared-admission-domain-binding) for construction, activation
and discharge. Provider retention alone does not route reservations. Host formation establishes this binding before
accepting transferable ownership, including ownership of concrete generic instantiations. The required provider set and
formation order follow
[provider dependencies and formation](cleanup-storage-and-reports.md#provider-dependencies-and-formation).

Use the existing product dependency and unload-quiescence contract to retain code, descriptors and release callbacks.
Release storage through its owning provider. Frame identity and descriptor equivalence permit pooling only within a
shared, validated ownership domain. They do not authorize transferring credits to an unrelated registry or freeing
storage through a different DLL. Cross-product transfer preserves an explicit provider dependency rather than assuming
identical hashes make providers interchangeable. Unload waits for external owners and active execution, then orders
internal static consumers before their providers.

## Compiler ownership and integration

Checked cleanup plans retain initialization, dependency and execution facts. One closed-action implementation in the
existing lowering lifecycle domain owns structural expansion and required storage. Ordinary lowering supplies source
guards and continuations. Specialization supplies closed substitutions and repairs descriptors. Admission requirements
come from that same selected action and storage description rather than a parallel capacity-prediction type walk.

Published compiler results remain immutable and dependency-tracked. Compute concrete frame layouts and cleanup
requirements only when demanded, using private builders for mutable construction. Independent compilation work retains
bounded parallel execution. This design does not introduce a global compiler planning pass, new proof service, or
universal program object.

Generated frame metadata, MIR validation, native signatures and artifact compatibility must agree on composition
ownership, run-directed wake semantics and result transfer. Update affected contracts together and regenerate disposable
artifacts under the repository's version-1 policy. Increment a version only when retaining an actual supported
compatibility contract. Remove superseded internal task composition and all-runtime frame reservation after their owning
paths are migrated. Do not retain a compatibility mode or a second cleanup scheduler in the completed architecture.
