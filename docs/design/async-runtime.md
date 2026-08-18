# Async lowering and runtime design

This document defines compiler ownership and implementation boundaries for Bray's async computation, task, cancellation,
and runtime model. Observable semantics remain owned by `docs/language/async-and-concurrency.md`.

`docs/design/lowering.md` defines the general HIR-to-MIR boundary, semantic primitive reuse, MIR ownership, and lazy
lowering model.

---

## Design boundaries

The implementation has three separate layers:

1. Language semantics and compiler-known declaration surfaces.
2. Compiler-generated frame, task, cleanup, and runtime ABI operations.
3. Ordinary standard-library Bray over private trusted ABI declarations.

Within the third layer, implementation responsibility is further divided:

- ordinary safe Bray owns public concurrency policy, typed owners, channels, protocols, codecs, budgets, combinators,
  and parallel algorithms,
- trusted Bray owns portable low-level runtime data structures, raw internal representation, callbacks, and safe
  platform-handle wrappers,
- target bindings own only operating-system mechanisms that cannot be produced inside the Bray abstract machine.

The runtime ABI does not mandate C, Rust, or any other implementation language. An ABI implementation can be separately
compiled Bray. `extern` means the declaration body is supplied by another linked artifact, not that the artifact is
necessarily foreign.

The compiler-known source environment contains only:

- `Future<T>`,
- `Task<T>`,
- `RunResult<T>`,
- `PanicReport`,
- `blocking_execution()`,
- `compute_execution()`,
- `main_thread_execution()`,
- `Future<T>.start()`,
- `Task<T>.join()`,
- `Task<T>.cancel()`.

No runtime, executor, scheduler, reactor, waker, poll, cancellation-token, thread, process, parallel-algorithm, channel,
race, select, or synchronization type is compiler-known. No compiler-known declaration is owned by `std`.

The runtime ABI is not a Bray declaration scope. Standard-library ABI bindings are private ordinary trusted declarations
and are not looked up by source path during compilation.

---

## Syntax and declaration discovery

The parser has no async-block or spawn syntax nodes. It parses `async` only as an allowed callable or lifecycle modifier
and parses `await` as the single async-specific expression form.

Calls such as `read(file).start()` are ordinary call, postfix member access, and method call syntax. Declaration
discovery obtains `Future<T>`, `Task<T>`, their protected representations, inherent members, `RunResult<T>`, and the
execution predicates from the closed compiler-known catalog.

The compiler-known catalog assigns stable semantic identities and representation roles for:

- future computation type,
- task handle type,
- task run-result type,
- panic report type,
- async start method,
- task join method,
- task cancel method,
- blocking-lane context predicate,
- compute-lane context predicate,
- main-thread-lane context predicate.

Representation roles are closed enums owned by `bray-compiler-known`. Compiler behavior must never depend on matching
the textual spelling of one of these declarations.

---

## Binding async calls

Binding first selects and validates an async callable through ordinary receiver, argument, default, generic, overload,
trait, visibility, target, and contract rules.

The bound async invocation records:

- selected callable identity and concrete instantiation,
- declared completion type `T`,
- produced source type `Future<T>`,
- ordered receiver and argument transfers,
- inferred dependency-contract input subjects,
- immediate invocation preconditions,
- deferred body effects, capabilities, execution-context requirements, and lifecycle contract,
- normal-completion postcondition template over `T`,
- source span and origin chain.

Value preconditions, argument transfers, generic constraints, and frame-construction effects remain ordinary invocation
requirements. Body effects and capabilities, `blocking_execution()`, `compute_execution()`, and
`main_thread_execution()`, suspension and cancellation behavior, and `ensures(...)` runtime properties belong to the
deferred execution contract. The binder identifies the execution predicates by compiler-known declaration identity and
preserves all other phase classifications from checked callable metadata.

Direct-await normal completion instantiates the postcondition template against the produced `T`. Started execution
instantiates it only on a control-flow edge refined to `RunResult.Completed(value)`. Frame construction and `Cancelled`
or `Panicked` observation must not publish body postconditions.

An ordinary function returning `Future<T>` binds as an ordinary call and cannot construct a frame. Only invocation of a
callable whose callable contract is async creates an async frame operation.

Member lookup for `.start()`, `.join()`, and `.cancel()` uses the ordinary type-associated member surface and
receiver-mode rules. The selected compiler-provided identity determines the later intrinsic semantic operation. Parser
shape and method spelling do not.

---

## Hidden frame identity

Every concrete async callable instantiation receives a stable hidden frame identity derived from:

- callable semantic identity,
- concrete type and const arguments,
- selected implementation witnesses,
- target and ABI runtime properties that affect representation,
- async lowering revision.

Before reachable concrete instances are selected, generic MIR may carry a target-specific frame-template identity.
Concrete-instance partitioning combines that template with the structural type and const arguments and selected
implementation witnesses to produce the hidden frame identity consumed by code generation and runtime metadata.

The source type remains `Future<T>`. The bound and lowered representations additionally carry a typed `AsyncFrameId` or
equivalent compiler-private identity. It must not be encoded as an ordinary source generic argument.

Frame metadata contains:

- completion type and layout,
- frame size and alignment,
- control-state layout,
- initialized-state maps,
- move-before-start operation,
- resume operation,
- cancellation-entry operation,
- phase-one owned-task broadcast visitor,
- phase-two lifecycle-resolution operation,
- completion-result move operation,
- infallible destruction operation,
- source-correlated suspension and retained-value information,
- execution requirements and affinity runtime properties.

Published metadata is immutable and target-specific where layout requires it.

---

## Async frame checking

The checker computes suspension liveness over the ordinary control-flow graph. Each suspension point records the exact
locals, temporaries, borrows, scoped capabilities, task obligations, lifecycle obligations, selected witnesses, and
runtime properties required after resumption or during cleanup.

The dependency domain derives the contract carried by `Future<T>` from invocation state plus every dependency retained
across suspension. `Task<T>` receives the same contract at start and adds independent-run ownership, affinity,
cancellation, and resolution obligations.

The checker rejects:

- movement of `Future<T>` or `Task<T>` to an owner that cannot preserve a dependency,
- ending an async-finalizable computation or task in a synchronous cleanup context,
- direct await from a lane that cannot satisfy deferred execution predicates,
- direct start in an executable or test product whose selected runtime cannot satisfy a reachable execution requirement,
- use of a task handle after join, cancel, movement, or automatic resolution,
- scope exits with incoherent partial task state,
- destruction of storage or capability release before dependent task resolution,
- task migration when retained state is thread-affine.

Affinity runtime properties identify an exact origin thread or compatible lane class for each live control state. A
backend can use state-sensitive migration only when its descriptor preserves those state-indexed runtime properties.
Otherwise it uses their conservative union and pins the task for its whole lifetime.

The checker represents affinity as a typed dependency property. It does not insert an implicit clone, shared owner,
`'static` conversion, or detached lifetime.

Library checking records reachable requirements in compiled interfaces without requiring the library to select a
runtime. Final executable or test product validation performs the closed-world runtime satisfiability check.

---

## Structured scope-exit plans

Composite storage flow owns task-obligation state and publishes the initialized roots, exact moved access paths, and
active borrows at each async lexical block exit. Async checking combines that state with checked type representations
and emits one cleanup plan containing:

- every owned unresolved task access path at that exit, including tasks held in initialized hidden `Future<T>` frame
  state,
- aggregate projections and active guards needed to find nested tasks,
- tasks already resolved or moved away,
- dependency ordering between tasks and other lifecycle values,
- the ordinary reverse lifecycle sequence,
- abnormal-exit primary and suppressed-report state,
- descriptor traversal roots for concrete, erased, active-child, aggregate, and recursive-indirect frame state,
- whether an unobserved completion payload is infallibly lifecycle-resolvable on the selected exit.

The plan contains two explicit phases:

1. cancellation request for every selected task, with no waits,
2. async finalization and ordinary lifecycle resolution after all requests.

Phase one invokes only descriptor broadcast visitors. A visitor follows initialized ownership paths through inactive
erased frames, the active direct-await child, guarded aggregates, and recursive indirections. It can request
cancellation but cannot resume, wait, finalize, or destroy. Phase two invokes the distinct lifecycle-resolution
operations after the complete traversal returns. MIR validation rejects a descriptor or cleanup plan that can discover a
new phase-one task while phase two is running.

This is one composite plan, not independently recomputed task and lifecycle passes. Control-flow merge preserves a
conservative obligation when any reachable predecessor still owns it. Plans retain exact root and projected access
identities. Partial aggregates use moved access paths as masks so cleanup can traverse the remaining initialized state
without treating the whole aggregate as moved.

Lowering consumes only checked cleanup plans. It does not rediscover which tasks are live from syntax or type recursion.

---

## MIR operations

Bray MIR represents async behavior with typed operations rather than runtime symbol names. The minimum conceptual
operation set is:

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
- append and transfer an owned cleanup incident,
- move terminal `RunResult<T>`,
- destroy terminal task control state.

MIR validation verifies frame identity, state transitions, initialization masks, exactly-once completion, exactly-once
destruction, result compatibility, cleanup-plan ordering, and runtime requirement identities.

No backend can infer async semantics from calls to functions named `start`, `join`, `cancel`, or from `std` paths.

---

## Direct-await lowering

Direct await consumes an inactive child frame into the current task. Lowering can embed the child frame in the parent
frame, use a parent-owned result place, or use another representation that avoids a task boundary.

Direct-await lowering must not semantically:

- allocate a task control block,
- enqueue a child task,
- introduce a child run boundary,
- convert a child panic to `RunResult<T>`,
- create an independent cancellation owner.

The parent task's resume operation delegates to the active child until it completes or suspends. The parent cancellation
path enters the child's phase-one broadcast traversal before any phase-two cleanup, then enters the child's cancellation
cleanup during phase-two resolution.

Every suspend terminator identifies its resume state, normal resume edge, cancellation-cleanup edge,
suspension-registration role, and wake role. The corresponding frame-state descriptor retains the checked initialized
storage, dependency contract, deferred callable set, and execution-lane requirements needed after resumption.

Before the child begins, lowering emits the checked lane-requirement assertion established by semantic analysis. This is
a typed MIR runtime property or validation operation, not a call to the source predicate.

---

## Run-result propagation lowering

`try` on `RunResult<T>` is not lowered as a return of `RunResult<R>` from the nearest callable. The checked operation
has three control successors:

- `Completed(value)` moves `value` into the normal result place,
- `Panicked(report)` transfers the existing report into current-run panic propagation,
- `Cancelled` enters current-run cancellation propagation.

The latter two successors have no ordinary continuation. They execute every intervening checked lexical cleanup plan
while propagating toward a catch boundary or the current task, native-thread, typed child-process, executable, or test
run boundary. They do not select a boundary from an expected or declared source result type and do not construct nested
`RunResult` values.

When a `catch` boundary encloses the operation, its panic successor captures the forwarded report as
`Result.Error(PanicReport)`. Its cancellation successor bypasses the catch because catch boundaries do not intercept
cancellation.

The bound operation records the source run result, current-run identity class, panic-report transfer, cancellation
forwarding, and required cleanup edges. MIR validation rejects lowering that copies the report, resumes an abandoned
continuation, converts cancellation into panic, or skips lifecycle resolution.

Cancellation forwarding, run checkpoints, and cancellation-aware operations add `may_cancel_current_run` to the checked
body effect summary. The term is preserved in declaration metadata but creates no source keyword or callable-type
clause. It is the cancellation counterpart to implicit panic propagation. Lowering uses it to retain abnormal cleanup
edges, while effect-free contexts reject it.

---

## Recursion and representation erasure

A recursive async frame cannot contain an unbounded number of itself by value. Lowering detects recursive
suspended-frame cycles and places an indirection or segmented-frame boundary on a cycle edge. Tail-recursive
transformations are permitted when they preserve destruction, cancellation, panic, and source-debug behavior.

Dynamic storage is required only where runtime depth or representation erasure requires it. Nonrecursive direct-await
composition is not forced through that allocation strategy.

When differently represented `Future<T>` values merge into homogeneous storage, lowering uses checked existential frame
metadata and an appropriate result-place or erased-storage plan. Erasure strategy must preserve movement before first
resume and stable storage after execution begins. Its descriptor retains separate phase-one broadcast and phase-two
lifecycle entry points. An erased generic cleanup callback is insufficient.

---

## Start and task storage

`Future<T>.start()` lowers to the typed task-start operation. Before first resume it:

1. selects a compatible runtime lane from deferred execution and affinity runtime properties,
2. requests task-owned storage sized and aligned for the control block and frame,
3. moves the inactive frame exactly once,
4. initializes cancellation, completion, panic, and join state,
5. publishes the task to the scheduler,
6. returns the source-level `Task<T>` owner.

The baseline ABI supports co-allocation of the task control block and frame. Separate allocation is permitted as an
implementation strategy but is not part of the source contract.

The runtime can retain internal scheduler and wake references. Those references are not source owners, cannot detach the
task, and cannot outlive terminal task storage except through the ABI's internal reclamation protocol.

---

## Cancellation and completion lowering

Every executable root, task, standard-library native thread, and conforming child-process root has one logical
run-cancellation state. An external request atomically marks that state and wakes a cancellation-aware wait. The run
observes the request at its domain-defined await, checkpoint, or cancellation-aware operation.

`try RunResult.Cancelled` uses a distinct checked lowering: it marks the current run requested and immediately commits
control to cancellation cleanup without waiting for another checkpoint. This makes the request observable to lifecycle
code while preventing the abandoned ordinary continuation from resuming.

`std.run.cancellation_requested()` and `std.run.checkpoint()` are ordinary wrappers over the current-root ABI record.
Task and thread helpers delegate to them. Async roots use task observation rules. Synchronous roots use explicit run
checkpoints and cancellation-aware synchronous operations. A conforming child-process host maps the authenticated parent
request to its child root state.

Cancellation cleanup masks further delivery while retaining the request for `std.run.cancellation_requested()`.
Generated cleanup drives async finalizers, records fallible-finalization errors as owned cleanup incidents during
abnormal exit, and always reaches infallible destruction unless cleanup panics or a noncooperative operation never
returns.

Each cleanup incident contains an erased owned error payload, a concrete type-and-destruction descriptor, producer and
source identity, and deterministic encounter ordinal. The active cleanup context owns an ordered incident list. Terminal
observation transfers a cancelled run's list, together with suppressed child-run panic reports, to the mandatory
product-host cleanup-report sink. A panicked run transfers cleanup incidents and later panics into the protected
`PanicReport` suppressed-entry storage. Either the sink reports and then infallibly destroys each payload, or
`PanicReport` retains ownership until its own destruction does so. No ABI path can discard the list. Incident
construction is the abnormal-exit abandonment conversion, so its erased payload descriptor has no remaining graceful
finalization operation.

The task control block has exactly one terminal state:

- completed with initialized `T`,
- cancelled with no `T` plus runtime-owned cleanup incidents and suppressed child-run panics pending sink delivery,
- panicked with initialized `PanicReport` owning optional suppressed panics and cleanup incidents.

A cleanup panic commits or replaces cancellation with the panicked terminal state. A non-panic cleanup incident does not
change the cancelled variant observed by source.

Racing normal completion and cancellation commit through one atomic terminal transition. `cancel()` can therefore
observe normal completion when completion won. Join waiters acquire the terminal state and establish the completion
visibility edge.

---

## Runtime ABI

The runtime ABI is a versioned product contract. It includes typed binary roles equivalent to root execution,
host-to-root cancellation request, task allocation and start, frame resume, suspension registration, wake, cancellation
request and observation, join registration, terminal publication, runtime events, compatible-lane selection,
cleanup-incident transfer and reporting, and structured shutdown. The frame-descriptor ABI versions phase-one broadcast
and phase-two lifecycle operations independently from resume and destruction. Cleanup-report sink support is part of the
product-host ABI and remains available to synchronous products without linking an async scheduler.

The native boundary is a dependency-free catalog of stable symbols, status values, fixed-layout records, handles, and
callback signatures. It does not import compiler target, symbol, source, serialization, diagnostic, or artifact models.
Protected-frame and execution semantics shared by the compiler and runtime occupy a separate dependency-light model.
Compiler-owned role effects, artifact metadata, target selection, and compatibility diagnostics remain outside both
layers.

Runtime dependencies use immutable requirements that retain every compatibility dimension needed for product formation.
Protected frames retain independently versioned resume, task-broadcast, lifecycle-resolution, completion-move, and
destruction operations. Product formation merges reachable requirements deterministically and rejects contradictory
identities, targets, panic ABIs, or incompatible major ABI versions.

Executable and test products select one target-specific runtime after merging reachable requirements. Compatibility
validation requires:

- exact agreement for constrained runtime identity, target identity, and panic ABI identity,
- matching ABI major versions and a provided minor version no older than the required minor version,
- compatible versions for every protected-frame operation,
- every required role, capability, and execution lane,
- the baseline cooperative-execution capability.

ABI symbol spellings and calling conventions are selected by the target/runtime contract. They do not become source
declarations. The ABI artifact also carries immutable compiler-readable semantic-contract records keyed by closed binary
ABI roles. Records cover ownership transfer, open run-transfer subjects, synchronization and visibility edges,
callback-root execution runtime properties, cancellation, panic behavior, lifecycle ownership, and capabilities. A
private standard-library binding is explicitly associated with a compatible role during the trusted
product-and-standard-library build. The compiler validates the role, signature, ABI version, target, and record schema,
checks wrappers using the record, and trusts the substrate implementation. It never discovers these contracts from
source spelling or an extern body.

The role set and each role's semantic effect are closed compiler contracts. A runtime selects implementations for
runtime-owned roles but cannot publish replacement semantics for them. Compiler-lowering roles remain compiler-owned and
cannot be claimed by a runtime artifact.

Each native runtime is packaged as a target-specific component catalog plus bounded compiler-readable metadata.
Ordinary-product adapter components separately own host, scheduler, cancellation, and event roles or capabilities. A
test product instead selects one coherent test-host adapter that owns the complete test execution surface, while memory,
string, and character operations remain independently retainable components. Support components own no semantic surface
and are reachable only as explicit dependencies of those adapters. The metadata records the runtime and artifact
identities, target and panic ABI, runtime and protected-frame ABI versions, capabilities, exact role-to-symbol bindings,
component purposes, archive file names, archive digests, component dependencies, exact role and capability ownership,
and exact platform-service overrides. Catalog validation requires one owner for every advertised role and capability in
each product category, at most one override for each platform service, and a complete acyclic dependency graph. Product
formation derives requirements from reachable MIR, selects only their owning components and transitive dependencies,
then authenticates those archives before link-plan construction. Unselected archives are not read or hashed. The linker
receives the selected archives as opaque typed runtime inputs and does not run the runtime's build system or depend on
its implementation language.

Ordinary-product components contain no test-runner protocol implementation and publish no test-entry selection symbol.
Test products select a test-host adapter whose metadata includes that role and whose build explicitly includes the
bounded test protocol. The runtime implementation crate publishes no stable native symbols itself. Each semantic adapter
publishes only its assigned ABI surface and delegates to the shared implementation support selected through its
component dependency. Artifact construction deduplicates archive members, produces deterministic archives, validates
dependency graphs, and audits exact exported symbols so these boundaries do not regress silently.

The runtime receives compiler-generated frame descriptors and never parses source types or compiled package interfaces.

The product runtime advertises:

- ABI version,
- baseline cooperative execution,
- local and migratable lane support,
- `blocking_execution()` availability,
- `compute_execution()` availability,
- distinguished `main_thread_execution()` lane support,
- reactor and event support required by the selected standard library,
- target and panic ABI compatibility.

Runtime implementations must be deterministic with respect to language-defined ownership and lifecycle outcomes even
though task interleaving is not deterministic.

Product and runtime configuration can impose hard limits on tasks, native threads, and child processes. Task limits
bound simultaneously executing lanes and queue excess ready tasks without changing `Future<T>.start()`. Native-thread
and process limits use recoverable creation failures. All are independent of ordinary library-side parallel budgets.

---

## Compiled package interfaces

An exported async declaration records:

- async callable contract and completion type,
- normalized invocation contract and deferred execution contract,
- normal-completion postcondition template,
- portable dependency-contract template,
- hidden frame identity or generic frame-template identity,
- frame descriptor compatibility reference,
- suspension and cleanup behavior needed by downstream lowering,
- versioned phase-one broadcast and phase-two lifecycle descriptor roles,
- required runtime ABI features.

Public APIs expose `Future<T>` as the invocation type without exposing hidden frame layout through source reflection. A
consuming compiler validates descriptor version and target compatibility before reuse.

Package interfaces correlate each runtime requirement with its declaration owner and any concrete hidden frame. Library
requirements contain portable compatibility runtime properties, never a selected runtime identity, private ABI role
identity, or binary binding. Executable and test product formation derives private roles from selected lowered
operations and selects the runtime before code generation and linking.

---

## Entrypoint and link planning

For an async entrypoint, lowering creates a compiler-owned host stub and root frame descriptor. The link plan explicitly
names:

- selected runtime artifact identity, component identities, and ABI version,
- root entry stub symbol,
- required startup and shutdown roles,
- host-to-root cancellation and wake roles,
- reachable execution-lane requirements,
- distinguished main-thread-lane startup and drive roles,
- configured task, thread, and process hard limits,
- reactor or event features required by linked standard-library code,
- target and panic ABI compatibility runtime properties.

The host process and its initial thread are product roots rather than source-owned standard-library child values. A
synchronous entrypoint executes as the root run and establishes `blocking_execution()`, `compute_execution()`, and
`main_thread_execution()`. An async entrypoint frame becomes a host-owned root task pinned to the distinguished
main-thread lane. that lane establishes `main_thread_execution()` but not the blocking or compute predicates.

The root body first creates an outcome candidate. Before publication, the generated root frame performs its checked
phase-one task broadcast and phase-two ordinary lifecycle resolution, including standard-library thread, process,
budget, and synchronization owners. Cleanup can replace the candidate with a panic outcome. Only then does the runtime
publish the internal `RunResult<T>`-equivalent terminal record and end the root run.

The host observes that final record, owns and maps or reports any `Completed(T)` payload, drains the cleanup-report
sink, then shuts runtime infrastructure and host process resources down. It does not discover source owners, match
standard-library type names, or repeat their lifecycle resolution. Root observation never creates a source `Task<T>` and
cannot resume source execution.

The linker validates every selected component against the runtime artifact identity named by the plan. It does not
choose a runtime, inspect source names, or infer requirements from unresolved symbols.

A product with no reachable runtime-owned role or capability omits the runtime artifact entirely. Runtime selection is
therefore absent rather than an empty artifact selection.

---

## Standard-library boundary

The general public package layout, platform-service roles, and target artifact contract are defined by
[I/O and platform services](io-and-platform-services.md). This section defines the additional ownership and runtime
integration required by concurrency and parallelism.

`std.run`, `std.task`, `std.channel`, `std.concurrent`, `std.parallel`, `std.sync`, `std.thread`, and `std.process` are
ordinary Bray modules. Their public declarations are encoded in package interfaces exactly like user-library
declarations.

Private trusted declarations bind runtime events, current-run cancellation state, checkpoint/yield operations,
native-thread creation, child-process creation and transport, reactor registration, and other nonportable services.
Their associated ABI-role contract records must establish every ownership, dependency, visibility, callback-root,
cancellation, and lifecycle runtime property used by safe wrappers. The association is private product metadata. Public
wrapper interfaces contain only ordinary inferred contracts.

Generic operations that publish values to synchronized shared storage or an independent run produce open run-transfer
terms in the ordinary inferred dependency template. A private ABI operation obtains that semantic boundary from its
ABI-role contract record. the compiler does not recognize its source name. Consumers instantiate the exported term with
concrete value dependencies, so `std.channel`, `std.thread`, `std.process`, and `std.parallel` reject creating-run
borrows, incompatible affinity, unsynchronized mutation, unencodable process-local state, and undrivable lifecycle
obligations without a public marker trait or another compiler-known type.

`std.channel` provides bounded multiple-producer, single-consumer channels with runtime-selected capacity. Capacity zero
is a rendezvous channel, positive capacity bounds queued messages, and no unbounded constructor exists. Send and receive
commits are FIFO, waiter admission is FIFO after registration, and cancellation removes only the uncommitted waiter.
`std.concurrent.first` selects one terminal result and resolves every loser. `std.concurrent.all` returns every terminal
result in input-index order.

`std.thread.Thread<T>` and its entry callable are ordinary standard-library types, allowing synchronous-only products to
use native threads without selecting the async runtime. The standard library's async thread bridge integrates those
owners with runtime events when used from tasks. Synchronous executable roots establish all three execution predicates.
Native-thread roots establish blocking and compute execution but not main-thread execution. Ordinary sync calls only
inherit existing runtime properties. Blocking `Thread<T>` join, cancel, and finalization contracts require
`blocking_execution()`. The async bridge turns current-task cancellation into request-thread-cancellation, shielded
wait, terminal lifecycle resolution, and continuation of the original task cancellation. It uses a private
async-finalizable standard-library owner rather than the public synchronous thread owner, preserving ordinary Bray
expressibility while allowing async lifecycle resolution of `T`.

The thread owner exposes its observational `std.thread.Id`. Creation failures preserve the stable capacity or creation
category and an optional target-native numeric code without exposing a platform-authored message as semantic data. The
public native-thread surface is available only when `target.platform.native_threads` is true.

`std.process.Process<T>` is an ordinary asynchronously finalizable standard-library owner whose explicit finalizer
returns `Result<unit, ProcessError>`. Normal scope exit therefore rejects an unresolved owner and requires an explicit
consuming `join` or `cancel`. Abnormal cleanup records finalizer failure as a cleanup incident. Its outer `Result`
reports process creation, transport, protocol, encoding, decoding, termination, and reaping failures. Its inner
`RunResult<T>` represents a conforming Bray child run.

`Executable`, `ProductDependency`, `Codec<T>`, `TerminationPolicy`, and `Program<Input, T>` are ordinary nonforgeable
standard-library owners with internal represented state. Executable identity comes from a declared executable product
dependency. Dependency lookup uses the current product's selected graph and performs no ambient search. Arbitrary
path-based external programs remain on the raw child-process surface. Codecs contain explicit encoder and decoder
witnesses, use `std.bytes.Buffer`, and carry a 256-bit protocol fingerprint. The compiler synthesizes no serialization.
A child executable's ordinary async main directly awaits `std.process.serve(worker, codecs...)`, which registers a
private host terminal reporter and maps root completion, panic, or cancellation into the authenticated protocol. The
handshake validates executable and protocol identity. Parent observation retains raw payload bytes until termination,
reaping, and all fallible protocol checks complete, and only then decodes and commits `T` or `PanicReport`. No outer
process error is possible after that commit. Raw external programs adapt their explicit exit representation rather than
pretending every nonzero status or signal is a Bray panic.

`Program<Input, T>` carries explicit arguments, environment policy and edits, working directory, and standard-stream
policy. Typed protocol transport is separate from standard streams. Piped stream owners can be taken from `Process<T>`
exactly once. Public codec implementations construct the closed `CodecFailure` categories, while private process
failures retain an optional target-native numeric code under their stable `ProcessError` variant.

`std.parallel` algorithms accept `Budget<TaskDomain>`, `Budget<ThreadDomain>`, or `Budget<ProcessDomain>`. These
ordinary nonforgeable standard-library owners bound one algorithm hierarchy. They are not product capacity authority and
do not change `Future<T>.start()`. `try_acquire`, cancellation-safe FIFO `acquire`, and FIFO `acquire_blocking` produce
one owned `Permit<Domain>` per active child. Permit release follows terminal observation. Nested algorithms split
unacquired and unreserved capacity from a same-domain parent, and child destruction returns the reservation. Independent
budgets can collectively exceed product capacity and remain subject to the underlying creation limits. Scoped task and
thread algorithms can retain checked borrows only while they own and resolve every child before return. Process
algorithms transfer encoded values and cannot borrow process-local memory.

Native-thread creation returns `Result<Thread<T>, ThreadError>` so capacity and operating-system creation failures
remain recoverable. After successful creation, safe `Thread<T>.join()` and `.cancel()` produce `RunResult<T>` without
another operational error layer. The async bridge returns `Result<T, ThreadError>` for creation failure and normal
completion, while forwarding a successfully created child's later panic or cancellation into the awaiting run.

The compiler does not synthesize channel, process, budget, or combinator implementations. Fixed arrays, const generics,
non-capturing callables, ordinary unions and products, `Future<T>`, `Task<T>`, and private trusted event, thread,
process, transport, and operating-system wrappers are sufficient to implement the standard algorithms.

### Implementation-language allocation

The reference architecture implements all public `std` concurrency and parallelism behavior in Bray. This includes
channels, concurrent combinators, thread and process owner state machines, typed process protocols, codecs, termination
policies, budgets, structured cancellation composition, and parallel algorithms.

Target-independent scheduler and reactor policy should be trusted Bray where compiler-provided atomics, memory
operations, runtime roles, and target contracts are sufficient. Suitable Bray-owned internals include ready queues,
work-stealing policy, waiter lists, timer heaps, task registries, join state, cancellation state, cleanup-report
routing, protocol framing, and permit accounting.

The irreducible target boundary supplies native thread and process creation, kernel wait/wake operations, process
signalling and reaping, platform event polling, virtual-memory acquisition, and target-specific unwind, signal,
thread-local, or host-report integration. Direct private FFI declarations are sufficient when the target exposes stable
callable symbols. A custom native shim is allowed only to normalize an otherwise unsuitable platform ABI. It must remain
a mechanism layer and cannot implement Bray ownership, structured concurrency, cancellation policy, process protocol
semantics, budgets, or parallel algorithms.

The backend and link plan record whether each required private role is implemented by compiler lowering, a Bray runtime
artifact, a direct platform binding, or a native shim. This choice is not source-visible and cannot change the role's
checked semantic contract.

---

## Diagnostics and inspection

Async diagnostics use structured identities and typed arguments. Required categories include:

- await outside async execution,
- start outside active runtime execution,
- operand is not `Future<T>`,
- incompatible direct-await execution requirement,
- runtime cannot satisfy a reachable execution requirement,
- async or task dependency escapes its provider,
- thread-affine task cannot move to the requested owner or lane,
- main-thread-required computation cannot execute on the selected lane,
- unresolved async finalization in synchronous scope,
- use of a consumed or resolved task,
- incoherent task state at control-flow merge,
- task cleanup cycle or invalid dependency ordering,
- large async frame,
- large value retained across suspension,
- recursive dynamic frame allocation,
- async loop with no cancellation observation opportunity,
- task started and immediately joined when direct await is equivalent,
- child thread or process ownership escapes its provider,
- process protocol cannot encode or preserve a transferred dependency,
- unresolved fallible `Process<T>` finalization on normal exit,
- private ABI binding role, signature, version, or semantic-contract schema mismatch.

Diagnostics point to the operation, the dependency or requirement origin, and the owner or lane that fails to preserve
it. Compiler logic emits message IDs and typed source/symbol/type/requirement arguments only.

Inspection output exposes frame size and alignment, retained values by suspension point, recursive or erased
dynamic-storage sites, task allocation sites, lane requirements, affinity causes, and structured cleanup obligations.
Runtime tracing correlates task IDs, parent owners, start sites, current suspension sites, wake causes, cancellation
state, join waiters, and cleanup blockers. Compiled-interface inspection renders public open run-transfer terms with
their subject and declaration origins. Trusted product-and-standard-library inspection additionally renders each private
binding's ABI role, contract-record digest, validated signature, target, and ABI version without exposing that role
through ordinary package lookup.

Runtime observation is demand-driven. A task snapshot combines its task identity, parent start site, current execution
and frame state, pending and currently observable cancellation state, join-waiter count, unobserved outcome category,
and immutable frame descriptor. Consumers derive retained storage and cleanup blockers from the descriptor state rather
than copying those tables into another representation. A scheduler snapshot reports registered tasks in task-identity
order together with dispatch ownership, selected lane, checked lane requirements, affinity, accepted wake count, current
wake cause, pending and observable cancellation state, and pending timer count.

Ready-queue timing is an explicit observation cost. Ordinary schedulers do not read the clock or retain enqueue
timestamps. Schedulers created for observation expose queue age in snapshots and queue latency on dispatched work. This
difference must not change dispatch order, wake coalescing, cancellation, or any other language-visible behavior.

---

## Conformance tests

The implementation requires focused tests for:

- async invocation type and nonexecution,
- invocation-versus-execution effects, requirements, and postcondition timing,
- ordinary member resolution of `start`, `join`, and `cancel`,
- direct await without a task boundary,
- recursive async frame formation,
- dependency propagation through `Future<T>` and `Task<T>`,
- two-phase cancellation broadcast before waits,
- phase-separated traversal through erased, active-child, aggregate, and recursive frame state,
- nested aggregate and partial-move task cleanup,
- normal, cancelled, and panicked run results,
- fallible and asynchronous cleanup under normal and abnormal exits,
- unobserved completion-payload lifecycle checks and ordered cleanup-incident reporting,
- lane requirement deferral, direct-await rejection, start routing, and product rejection,
- distinguished main-thread root execution and main-thread requirement checking,
- thread-affinity preservation,
- open generic run-transfer template instantiation and synchronous/native execution-root runtime properties,
- blocking thread-owner contracts and async thread-bridge cancellation,
- typed process protocol layering, cancellation, reaping, and payload lifecycle,
- process executable and codec authentication plus decode-after-reap commit ordering,
- recoverable native-thread capacity and creation failure,
- universal root, task, thread, and child-process cancellation state and forwarded-cancellation entry,
- private ABI role-contract validation and public-wrapper contract erasure,
- explicit parallel resource budgets and scoped task, thread, and process execution,
- `try RunResult<T>` current-run forwarding and its interaction with `catch`,
- compiled-interface round trips for async metadata,
- runtime ABI version and feature mismatch,
- sync and async entrypoint root lowering and structured product shutdown,
- absence of runtime linkage for synchronous-only products,
- deterministic structured diagnostics and inspection runtime properties.

The runtime conformance suite is expressed against observable runtime outcomes rather than one queue, allocator, or
worker implementation. Each conforming runtime adapter runs the same direct-await, erased-frame, task-storage, panic,
unobserved-result, cancellation-shield, scheduling, lane-capability, queue-observation, and cleanup-order scenarios.
Cost assertions identify semantic boundaries: direct await creates no task or scheduler boundary, task start is the
principal stable-storage boundary, and optional runtime infrastructure remains absent until requested.
