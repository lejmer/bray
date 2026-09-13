# Composed async execution

Directly awaited frames and mandatory cleanup execute within their owning run. Independent starts retain a separate run
boundary. This design refines [async lowering](async-runtime.md#direct-await-lowering).

## Runs and activations

A run owns scheduling, cancellation, dispatch authority, and admitted wait resources. An activation owns callable frame
state, captures, its continuation, result storage, and lifecycle progress. Its directly awaited child belongs to the
same run. Parent state stays retained while that child executes.

The activation chain uses admitted frame storage rather than growing a separate runtime stack. Known layouts can use
enclosing storage, while recursive or erased frames use owned indirection. Only the run's exclusive dispatcher may
change the chain or invoke generated operations. Runtime locks are released before callbacks execute.

An iterative driver enters children and returns results at callback boundaries. Async depth therefore does not become
native call-stack depth. Results move once into admitted destinations, and frames remain alive until their remaining
cleanup and storage borrows end. Ordinary synchronous foreign reentry retains its native nesting.

## Wakes and readiness

Notifications wake the run, not a particular frame address. The dispatcher consults its current activation and active
wait before advancing. The scheduler combines exclusive dispatch with a pending-wake handshake so a notification racing
with dispatch release cannot disappear or cause concurrent execution.

An event wait retains the event and observed generation. A join wait observes persistent child terminal state without
consuming its result. Installed registrations cover the interval between checking readiness and releasing dispatch.
Delayed notifications may request another check but cannot manufacture readiness or access retired frame memory.

Persistent I/O subscriptions belong to the operation that created them. An operation may combine several sources into
one readiness condition for the run. Enclosing direct awaits do not each allocate another subscription or task record.
Event and notification critical sections contain no allocation, user callback, or destruction that could break their
state halfway through an update.

## Cancellation and cleanup

Cancellation request state belongs to the run, while delivery follows checked cancellation points. Cleanup shields keep
the request observable without repeatedly delivering it or keeping an unready wait queued. The driver follows the
checked broadcast phase before lifecycle resolution and can reuse admitted wait resources while draining children.

Structural cleanup preserves the immediate parent's continuation and pending outcome. Lowering's shared cleanup owner
handles expansion and outcome collection, rather than each runtime adapter implementing its own cleanup scheduler.
Explicit application starts during finalization still require independent admission.

## Execution lanes and fairness

Composition inherits the run's execution context. Retained parent dependencies and active child requirements jointly
constrain placement. An erased child may restrict the admitted lane set but cannot grant an unadmitted capability.
Main-thread roots remain on that lane, and migration requires evidence for the whole live activation state.

Run admission reserves queue membership for the finite lane classes its checked contract permits. A finite transition
budget returns long chains of entry, completion, or cleanup transitions to the scheduler at valid callback boundaries.
This scheduling policy does not add source cancellation points or interrupt synchronous operations.

## Host roots and reentry

A host boundary that accepts ownership requiring async cleanup admits its driver resources before accepting the owner.
This applies to static domains, returned host-owned values, and foreign ownership transfers. Ordinary synchronous
functions retain their synchronous cleanup model.

Drivers may be shared only where the ownership domain proves cleanup cannot overlap. Nested cleanup that can overlap
needs separately admitted capacity. Reentry preserves the exact-thread attachment and restores the outer execution
context. A still-executing outer activation cannot be dispatched again while its callback is active.

Host-driving support is selected by reachable requirements. A synchronous-only product does not retain scheduler
machinery merely to report an incident. Admission and storage remain separate from independent runtime scheduling.

## Capacity and provider lifetime

Cleanup capacity follows the ownership obligation. Physical activation and result storage can outlive its discharge.
Current-value completion evidence may skip work, but the owner's uniform type-based allowance survives until ownership
ends. Represented children retain their own allowances, and ordinary movement transfers responsibility without hidden
capacity fields or fallible rebinding.

Providers exchange ownership through a validated shared admission domain. Frame and report storage retain their release
provider until the last user ends, independently of the run that activated them. Inactive frames can move between
compatible execution contexts without inheriting their original scheduler.

[Cleanup storage and reports](cleanup-storage-and-reports.md) owns the admission, transfer, and provider design.
Compiler planning remains immutable and demand-driven through existing checking, lowering, specialization, and
target-layout queries. Composition adds no universal planning pass or separate capacity-prediction type walk.
