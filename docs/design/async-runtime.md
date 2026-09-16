# Async lowering and runtime design

Bray separates language-level computation and ownership, compiler-generated execution state, and runtime mechanisms. The
[language specification](../language/async-and-concurrency.md) owns observable async behavior. This document explains
how the compiler and runtime represent and realize it.

## Responsibility and implementation language

The compiler recognizes the closed computation, task, run-result, panic-report, and execution-predicate identities.
Ordinary member lookup selects operations before their identities determine intrinsic handling. Syntax and method
spelling do not select runtime behavior.

Safe Bray owns public concurrency policy, channels, combinators, typed process protocols, budgets, and parallel
algorithms. Trusted Bray owns portable low-level data structures and wrappers. Scheduler and reactor policy should also
be trusted Bray where the language's memory, atomic, and private runtime contracts suffice. Native shims supply only
mechanisms that the target ABI cannot expose directly, not a second ownership or cancellation system.

The runtime ABI is independent of implementation language. Private trusted source bindings associate declarations with
validated binary roles, while public package interfaces retain ordinary inferred contracts. The ABI is not a source
scope, and no executor, waker, channel, or scheduler type becomes compiler-known.

## Checking and frame identity

Async invocation uses ordinary callable selection. Checked metadata separates immediate argument transfer and frame
construction from deferred body effects, capabilities, execution requirements, and completion postconditions. Binder and
checker results carry that distinction into lowering and imported declarations.

A concrete callable instantiation has a hidden frame identity derived from semantic identity, substitutions, witnesses,
and representation-relevant target and ABI inputs. Generic MIR can retain a template until specialization supplies those
inputs. The source type remains `Future<T>` without a hidden source generic parameter for layout.

Suspension liveness and composite storage flow identify retained values, initialized storage, dependencies, and
lifecycle obligations. State-sensitive affinity can allow migration when the live state permits it. A conservative union
remains valid when the runtime cannot preserve state-specific evidence. The compiler does not erase dependencies by
inserting clones or detached lifetimes.

Frame descriptors connect concrete storage to generated resume, result-move, cancellation, broadcast, lifecycle, and
destruction operations. Layout and execution requirements are available before construction for admission planning.
Descriptors are immutable, source-correlated, and shared by their constructors and host cleanup users.

## Structured cleanup

The checker publishes one composite cleanup plan from the same storage and dependency facts used for legality checks.
The plan retains partial-move masks, guards, and ordering across nested aggregates and frame state. Cancellation
broadcast and lifecycle resolution are distinct phases. Descriptor traversal must discover owned tasks before waits or
destruction begin.

Lowering consumes those plans rather than searching syntax or recursively rediscovering ownership. Typed MIR
operations express frame composition, task admission, waits, completion, report transfer, and cleanup. MIR validation
checks representation and phase consistency, and completed MIR determines the exact generated helpers and runtime roles.

Panic and cancellation forwarding follow the checked current-run boundary and cleanup edges. Caller-owned result and
report destinations preserve ownership across generated calls. Terminal publication resolves the root's source owners
before the host observes or reports the outcome. The host does not repeat source lifecycle analysis.

## Direct-await lowering

Direct await composes an activation inside the current run. It retains the parent's continuation and dependencies
without creating an independent task, scheduler registration, or cancellation owner. Known child frames can occupy
enclosing storage. Recursive or erased representations use owned indirection where their shape requires it.

Independent start is an admission and publication boundary. Until publication commits, the previous owner retains the
inactive frame and responsibility for capture cleanup. Task storage can co-allocate control and frame state. Scheduler
references remain internal reclamation references rather than source owners.

[Composed async execution](composed-async-execution.md) describes dispatch, wakes, lanes, and host driving. [Cleanup
storage and reports](cleanup-storage-and-reports.md) describes admitted capacity and payload lifetime.

## Runtime contracts and product formation

The native ABI owns fixed layouts, handles, signatures, and role identities without importing compiler models. Shared
protected-frame and execution semantics occupy a dependency-light model. Compiler-facing artifact metadata and typed
compatibility diagnostics remain outside both layers.

One role catalog drives native signatures, compiler mappings, trusted bindings, and component demand. Runtime artifacts
provide implementations of those roles, not replacement semantics. Native signatures describe the C boundary, while
compiler signatures describe MIR values whose native arguments may come from generated frame or host state.

Product formation merges reachable requirements and selects compatible target, panic ABI, frame operations, roles, and
lanes. Libraries publish portable requirements without choosing a runtime. Executable and test products select only the
owning runtime components and their dependencies. Unselected archives need not be loaded or authenticated.

Host, scheduler, cancellation, event, and observation support have separate retention boundaries. Test products select a
coherent test-host adapter, while ordinary adapters have no test protocol dependency. Synchronous host cleanup and
native-thread support do not require an async scheduler. A product with no runtime demand has no runtime selection.

The trusted bootstrap owns thread attachment and exit cleanup. Target thread-storage callbacks supply mechanisms, while
Bray outcomes cross native calls through tagged results and owned destinations rather than foreign unwinding.

## Standard-library integration

Ordinary library implementations express transfer to independent runs through inferred dependency contracts. Private ABI
records provide the evidence safe wrappers need for ownership, synchronization, callbacks, and cancellation. Open terms
survive generic publication and are instantiated at use sites without adding public marker types.

Native-thread and typed-process helpers own their lifecycle and operational failure boundaries. A typed process protocol
uses explicit codecs and authenticated product identity, with transport separate from standard streams. Payload commit
follows reaping and fallible protocol validation. Library budgets control algorithm hierarchies independently of product
capacity, and scoped algorithms retain ownership until every child resolves.

## Inspection

Compiler inspection derives frame costs, retained values, dynamic storage, affinity, and cleanup obligations from
checked metadata. Runtime observations correlate run identity and source sites with dispatch, wait, cancellation, and
terminal state. Descriptors remain the shared source of retained-storage information.

Observation is demand-driven. Queue timing may add clock reads and timestamps only when selected, and must not change
scheduling or language behavior. Volatile observations remain separate from reproducible compiler records.
