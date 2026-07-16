# Async lowering and runtime design

This document defines compiler ownership and implementation boundaries for Bray's async computation, task, cancellation, and runtime
model. Observable semantics remain owned by `docs/language/async-and-concurrency.md`.

---

## Design boundaries

The implementation has three separate layers:

1. Language semantics and compiler-known declaration surfaces.
2. Compiler-generated frame, task, cleanup, and runtime ABI operations.
3. Ordinary standard-library Bray over private trusted ABI declarations.

The compiler-known source environment contains only:

- `Async<T>`,
- `Task<T>`,
- `RunResult<T>`,
- `PanicReport`,
- `blocking_execution()`,
- `compute_execution()`,
- `Async<T>.start()`,
- `Task<T>.join()`,
- `Task<T>.cancel()`.

No runtime, executor, scheduler, reactor, waker, poll, cancellation-token, thread-handle, channel, race, select, or synchronization
type is compiler-known. No compiler-known declaration is owned by `std`.

The runtime ABI is not a Bray declaration scope. Standard-library ABI bindings are private ordinary trusted declarations and are not
looked up by source path during compilation.

---

## Syntax and declaration discovery

The parser has no async-block or spawn syntax nodes. It parses `async` only as an allowed callable or lifecycle modifier and parses
`await` as the single async-specific expression form.

Calls such as `read(file).start()` are ordinary call, postfix member access, and method call syntax. Declaration discovery obtains
`Async<T>`, `Task<T>`, their protected representations, inherent members, `RunResult<T>`, and the execution predicates from the
closed compiler-known catalog.

The compiler-known catalog assigns stable semantic identities and representation roles for:

- async computation type,
- task handle type,
- task run-result type,
- panic report type,
- async start method,
- task join method,
- task cancel method,
- blocking-lane context predicate,
- compute-lane context predicate.

Representation roles are closed enums owned by `bray-compiler-known`. Compiler behavior must never depend on matching the textual
spelling of one of these declarations.

---

## Binding async calls

Binding first selects and validates an async callable through ordinary receiver, argument, default, generic, overload, trait,
visibility, target, and contract rules.

The bound async invocation records:

- selected callable identity and concrete instantiation,
- declared completion type `T`,
- produced source type `Async<T>`,
- ordered receiver and argument transfers,
- inferred dependency-contract input subjects,
- immediate invocation preconditions,
- deferred execution-context requirements,
- source span and origin chain.

Only `blocking_execution()` and `compute_execution()` requirements are deferred from call time to execution time. Other preconditions
remain ordinary invocation requirements. The binder identifies these predicates by compiler-known declaration identity.

An ordinary function returning `Async<T>` binds as an ordinary call and cannot construct a frame. Only invocation of a callable whose
callable contract is async creates an async frame operation.

Member lookup for `.start()`, `.join()`, and `.cancel()` uses the ordinary associated type surface and receiver-mode rules. The
selected compiler-provided identity determines the later intrinsic semantic operation; parser shape and method spelling do not.

---

## Hidden frame identity

Every concrete async callable instantiation receives a stable hidden frame identity derived from:

- callable semantic identity,
- concrete type and const arguments,
- selected implementation witnesses,
- target and ABI facts that affect representation,
- async lowering revision.

The source type remains `Async<T>`. The bound and lowered representations additionally carry a typed `AsyncFrameId` or equivalent
compiler-private identity. It must not be encoded as an ordinary source generic argument.

Frame metadata contains:

- completion type and layout,
- frame size and alignment,
- control-state layout,
- initialized-state maps,
- move-before-start operation,
- resume operation,
- cancellation-entry operation,
- completion-result move operation,
- cleanup and destruction operations,
- source-correlated suspension and retained-value information,
- execution requirements and affinity facts.

Published metadata is immutable and target-specific where layout requires it.

---

## Async frame checking

The checker computes suspension liveness over the ordinary control-flow graph. Each suspension point records the exact locals,
temporaries, borrows, scoped capabilities, task obligations, lifecycle obligations, selected witnesses, and facts required after
resumption or during cleanup.

The dependency domain derives the contract carried by `Async<T>` from invocation state plus every dependency retained across
suspension. `Task<T>` receives the same contract at start and adds independent-run ownership, affinity, cancellation, and resolution
obligations.

The checker rejects:

- movement of `Async<T>` or `Task<T>` to an owner that cannot preserve a dependency,
- ending an async-finalizable computation or task in a synchronous cleanup context,
- direct await from a lane that cannot satisfy deferred execution predicates,
- direct start in an executable or test product whose selected runtime cannot satisfy a reachable execution requirement,
- use of a task handle after join, cancel, movement, or automatic resolution,
- scope exits with incoherent partial task state,
- destruction of storage or capability release before dependent task resolution,
- task migration when retained state is thread-affine.

The checker represents affinity as a typed dependency property. It does not insert an implicit clone, shared owner, `'static`
conversion, or detached lifetime.

Library checking records reachable requirements in compiled interfaces without requiring the library to select a runtime. Final
executable or test product validation performs the closed-world runtime satisfiability check.

---

## Structured scope-exit plans

Composite storage flow owns task-obligation state. For every async lexical block exit, it emits one checked cleanup plan containing:

- every owned unresolved task access path at that exit, including tasks held in initialized hidden `Async<T>` frame state,
- aggregate projections and active guards needed to find nested tasks,
- tasks already resolved or moved away,
- dependency ordering between tasks and other lifecycle values,
- the ordinary reverse lifecycle sequence,
- abnormal-exit primary and suppressed-report state.

The plan contains two explicit phases:

1. cancellation request for every selected task, with no waits,
2. async finalization and ordinary lifecycle resolution after all requests.

This is one composite plan, not independently recomputed task and lifecycle passes. Control-flow merge preserves a conservative
obligation when any reachable predecessor still owns it. Partial aggregates use initialized and moved-part facts.

Lowering consumes only checked cleanup plans. It does not rediscover which tasks are live from syntax or type recursion.

---

## MIR operations

Bray MIR represents async behavior with typed operations rather than runtime symbol names. The minimum conceptual operation set is:

- create inactive frame in a supplied result place,
- move inactive frame,
- enter or resume frame state,
- suspend current task with a resume state,
- compose direct-awaited child frame,
- read and commit child completion,
- enter cancellation cleanup,
- request cancellation for a task,
- start task from inactive frame,
- register and resolve task join,
- complete task with value, cancellation, or panic,
- execute checked cleanup phase one,
- execute checked cleanup phase two,
- move terminal `RunResult<T>`,
- destroy terminal task control state.

MIR validation verifies frame identity, state transitions, initialization masks, exactly-once completion, exactly-once destruction,
result compatibility, cleanup-plan ordering, and runtime requirement identities.

No backend can infer async semantics from calls to functions named `start`, `join`, `cancel`, or from `std` paths.

---

## Direct-await lowering

Direct await consumes an inactive child frame into the current task. Lowering can embed the child frame in the parent frame, use a
parent-owned result place, or use another representation that avoids a task boundary.

Direct-await lowering must not semantically:

- allocate a task control block,
- enqueue a child task,
- introduce a child run boundary,
- convert a child panic to `RunResult<T>`,
- create an independent cancellation owner.

The parent task's resume operation delegates to the active child until it completes or suspends. The parent cancellation path enters
the child's cancellation cleanup before continuing parent cleanup.

Before the child begins, lowering emits the checked lane-requirement assertion established by semantic analysis. This is a typed MIR
fact or validation operation, not a call to the source predicate.

---

## Recursion and representation erasure

A recursive async frame cannot contain an unbounded number of itself by value. Lowering detects recursive suspended-frame cycles and
places an indirection or segmented-frame boundary on a cycle edge. Tail-recursive transformations are permitted when they preserve
destruction, cancellation, panic, and source-debug behavior.

Dynamic storage is required only where runtime depth or representation erasure requires it. Nonrecursive direct-await composition is
not forced through that allocation strategy.

When differently represented `Async<T>` values merge into homogeneous storage, lowering uses checked existential frame metadata and
an appropriate result-place or erased-storage plan. Erasure strategy must preserve movement before first resume and stable storage
after execution begins.

---

## Start and task storage

`Async<T>.start()` lowers to the typed task-start operation. Before first resume it:

1. selects a compatible runtime lane from deferred execution and affinity facts,
2. requests task-owned storage sized and aligned for the control block and frame,
3. moves the inactive frame exactly once,
4. initializes cancellation, completion, panic, and join state,
5. publishes the task to the scheduler,
6. returns the source-level `Task<T>` owner.

The baseline ABI supports co-allocation of the task control block and frame. Separate allocation is permitted as an implementation
strategy but is not part of the source contract.

The runtime can retain internal scheduler and wake references. Those references are not source owners, cannot detach the task, and
cannot outlive terminal task storage except through the ABI's internal reclamation protocol.

---

## Cancellation and completion lowering

A cancellation request atomically marks the task and makes a suspended cancellation-aware operation resumable. The generated frame
checks the request at each language-defined observation point.

Cancellation cleanup masks further delivery while retaining the request for `std.task.cancellation_requested()`. Generated cleanup
drives async finalizers, records fallible-finalization errors as suppressed cleanup data during abnormal exit, and always reaches
infallible destruction unless cleanup panics or a noncooperative operation never returns.

The task control block has exactly one terminal state:

- completed with initialized `T`,
- cancelled with no `T`,
- panicked with initialized `PanicReport` and optional suppressed reports.

Racing normal completion and cancellation commit through one atomic terminal transition. `cancel()` can therefore observe normal
completion when completion won. Join waiters acquire the terminal state and establish the completion visibility edge.

---

## Runtime ABI

The runtime ABI is a versioned product contract. It includes typed binary roles equivalent to root execution, task allocation and
start, frame resume, suspension registration, wake, cancellation request and observation, join registration, terminal publication,
runtime events, compatible-lane selection, and structured shutdown.

ABI symbol spellings and calling conventions are selected by the target/runtime contract. They do not become source declarations.
The runtime receives compiler-generated frame descriptors and never parses source types or compiled package interfaces.

The product runtime advertises:

- ABI version,
- baseline cooperative execution,
- local and migratable lane support,
- `blocking_execution()` availability,
- `compute_execution()` availability,
- reactor and event support required by the selected standard library,
- target and panic ABI compatibility.

Runtime implementations must be deterministic with respect to language-defined ownership and lifecycle outcomes even though task
interleaving is not deterministic.

---

## Compiled package interfaces

An exported async declaration records:

- async callable contract and completion type,
- normalized immediate and deferred requirements,
- portable dependency-contract template,
- hidden frame identity or generic frame-template identity,
- frame descriptor compatibility reference,
- suspension and cleanup behavior needed by downstream lowering,
- required runtime ABI features.

Public APIs expose `Async<T>` as the invocation type without exposing hidden frame layout through source reflection. A consuming
compiler validates descriptor version and target compatibility before reuse.

Libraries record runtime requirements but never select a runtime. Executable and test product formation unions reachable runtime
requirements and selects one runtime implementation before code generation and linking.

---

## Entrypoint and link planning

For an async entrypoint, lowering creates a compiler-owned host stub and root frame descriptor. The link plan explicitly names:

- selected runtime artifact identity and ABI version,
- root entry stub symbol,
- required startup and shutdown roles,
- reachable execution-lane requirements,
- reactor or event features required by linked standard-library code,
- target and panic ABI compatibility facts.

The linker validates the selected runtime artifact metadata against the plan. It does not choose a runtime, inspect source names, or
infer requirements from unresolved symbols.

A product with no reachable async root or task start omits the async runtime unless another selected dependency explicitly requires
it.

---

## Standard-library boundary

`std.task`, `std.channel`, `std.concurrent`, `std.sync`, and `std.thread` are ordinary Bray modules. Their public declarations are
encoded in package interfaces exactly like user-library declarations.

Private trusted declarations bind runtime events, current-task cancellation state, checkpoint/yield operations, native thread
creation, reactor registration, and other nonportable services. Their trusted contracts must establish every ownership, dependency,
visibility, cancellation, and lifecycle fact used by safe wrappers.

`std.thread.Handle<T>` and its entry callable are ordinary standard-library types, allowing synchronous-only products to use native
threads without selecting the async runtime. The standard library's async thread bridge integrates those handles with runtime events
when used from tasks.

The compiler does not synthesize channel or combinator implementations. Fixed arrays, const generics, non-capturing async lambdas,
ordinary unions and products, `Async<T>`, `Task<T>`, and private event wrappers are sufficient to implement the standard algorithms.

---

## Diagnostics and inspection

Async diagnostics use structured identities and typed arguments. Required categories include:

- await outside async execution,
- start outside active runtime execution,
- operand is not `Async<T>`,
- incompatible direct-await execution requirement,
- runtime cannot satisfy a reachable execution requirement,
- async or task dependency escapes its provider,
- thread-affine task cannot move to the requested owner or lane,
- unresolved async finalization in synchronous scope,
- use of a consumed or resolved task,
- incoherent task state at control-flow merge,
- task cleanup cycle or invalid dependency ordering,
- large async frame,
- large value retained across suspension,
- recursive dynamic frame allocation,
- async loop with no cancellation observation opportunity,
- task started and immediately joined when direct await is equivalent.

Diagnostics point to the operation, the dependency or requirement origin, and the owner or lane that fails to preserve it. Compiler
logic emits message IDs and typed source/symbol/type/requirement arguments only.

Inspection output exposes frame size and alignment, retained values by suspension point, recursive or erased dynamic-storage sites,
task allocation sites, lane requirements, affinity causes, and structured cleanup obligations. Runtime tracing correlates task IDs,
parent owners, start sites, current suspension sites, wake causes, cancellation state, join waiters, and cleanup blockers.

---

## Conformance tests

The implementation requires focused tests for:

- async invocation type and nonexecution,
- ordinary member resolution of `start`, `join`, and `cancel`,
- direct await without a task boundary,
- recursive async frame formation,
- dependency propagation through `Async<T>` and `Task<T>`,
- two-phase cancellation broadcast before waits,
- nested aggregate and partial-move task cleanup,
- normal, cancelled, and panicked run results,
- fallible and asynchronous cleanup under normal and abnormal exits,
- lane requirement deferral, direct-await rejection, start routing, and product rejection,
- thread-affinity preservation,
- compiled-interface round trips for async metadata,
- runtime ABI version and feature mismatch,
- async entrypoint root lowering,
- absence of runtime linkage for synchronous-only products,
- deterministic structured diagnostics and inspection facts.
