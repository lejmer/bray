# Bray native runtime design proposal

This is a research proposal. It has not changed the language, runtime implementation or Linear roadmap. The baseline is `a65cc2f0` on `docs/bray-native-runtime-design-research`.

The [language notes](bray-runtime-language-notes.md) and [consumer notes](bray-runtime-consumer-notes.md) support the recommendations below. This document records step 2 of the [delivery strategy](https://linear.app/bray-lang/document/bray-native-runtime-and-platform-delivery-plan-46623e2d308d).

Evaluate every design choice for correctness, execution speed, binary size and compiler speed. [Decision 12](#12-correctness-execution-speed-binary-size-and-compiler-speed-govern-the-design) records the destination requirements and when to apply performance gates. The [cost review](bray-runtime-cost-review.md) checks the agreed architecture against these requirements.

## Agreed decisions

### 1. Cleanup eligibility does not require a blocking wait

Cleanup must preserve every live dependency. An unresolved dependency prevents cleanup from beginning, but does not require a blocking wait.

The language defines cleanup legality through existing dependency and lifecycle contracts. The runtime defines how a caller requests shutdown and retains responsibility when cleanup cannot begin. Those operations can use ordinary Bray types. This decision introduces no new compiler-known owner or outcome type and makes no language-wide deadlock guarantee.

A shutdown call that waits for a report which its caller can dispose of only after the call returns creates a circular wait. The host contract must avoid that cycle. A shutdown attempt that returns the unresolved owner is one proposed API, still under discussion. Pending cleanup keeps an explicit owner, execution capacity and error-reporting responsibility.

Implementation must replace mandatory external-root waiting in the static and root shutdown chapters with cleanup eligibility. Decision 3 settles retention granularity. The shutdown API remains open, including the illustrative `ProductOwner` and `ShutdownAttempt` types.

### 2. Products that exchange owners share cleanup admission services

Products that exchange owned values bind to one cleanup admission service domain. An owned error or report keeps valid cleanup backing through transfer and eventual disposal. The binding remains stable across ordinary moves, and the service stays live while transferred backing depends on it.

Each scheduler retains its own workers, execution limits, budgets and thread affinity. Synchronous products can use the shared service without a scheduler. Unrelated hosts can have separate service owners. The agreement does not require one process-wide service.

Implement the service in ordinary Bray code. Loaded products receive an explicit host binding. Concrete backing layout, synchronization and pooling remain implementation choices.

### 3. Provider dependencies retain the whole product

An owner that depends on a provider keeps that provider's code and product statics alive until the dependency ends. Apply the same rule to reports, symbols, callbacks and borrowed data. There is no code-only exception or selective static-cleanup proof. Decision 1 still applies, so retention makes cleanup ineligible without requiring a blocking wait.

Independent diagnostic preservation must be available to callers that need diagnostic history after provider release. A caller must be able to preserve diagnostic information as independently owned data, dispose of the original provider-dependent report and release its provider dependency. A typed payload that still needs provider code continues to retain the product. This uses ordinary Bray ownership and optional library operations under decision 11. Reuse existing report access or formatting operations where they suffice. A dedicated diagnostic representation or conversion API has not been agreed.

Verify this behavior in the planned loaded-provider migration tests. Repeated plugin loads and unloads must release plugin resources while independently owned diagnostic history remains available. Deliberately retained provider-dependent payloads must keep the product alive. These are acceptance requirements for the existing implementation sequence, not a separate prototype prerequisite.

Align BRA-501's code-only cleanup expectations and conflicting design text with this decision. Preserve the language's dependency rule when refining report representation.

### 4. Bootstrap uses target bindings and explicit startup storage

Build bootstrap from Bray's existing low-level OS bindings, with explicitly supplied startup storage. Establish thread attachment and cleanup services before using higher-level APIs that depend on them. In particular, public thread APIs cannot initialize the runtime services they already require.

Bootstrap follows ordinary Bray ownership, lifecycle and cleanup-capacity checks. Existing compiler-validated runtime and platform roles establish the required contracts and ABI boundaries. Runtime source receives no blanket exemption, and this decision adds no source-language feature.

Verify typed bootstrap owners, role lowering and TLS destructor entry during the planned synchronous-host replacement. Resolve recursive entry and admission dependencies there, with no separate experiment stage.

### 5. Thread-local and product statics use dependency cleanup order

Thread-local statics follow the same dependency rule as product statics. If cleanup of `A` needs `B`, finish cleaning `A` before starting cleanup of `B`. Independent statics within one domain use the specification's deterministic structural order. Thread-local cleanup stays on the exact owning thread.

Registration order has no semantic significance. Correct the bootstrap chapter's reverse-registration wording to match the static specification. Preserve dependency precedence across domains, with no universal completion order for independent domains.

### 6. Cleanup admission secures actual backing before ownership begins

Secure the actual storage required for mandatory cleanup before each new obligation becomes live. Preserve its allowance through ownership transfers. A possible typed cleanup error needs enough correctly aligned backing, and mandatory activation, result transfer and incident preservation must work when further allocation fails. Bookkeeping credits alone do not establish that backing exists.

Storage may come from an enclosing frame, a combined allocation, separately owned regions or a pool. This decision requires no separate heap allocation per value. Moves and wrapping preserve existing allowances without charging twice. Aggregates admit only their additional local obligations, and transferred backing stays live until its last user resolves.

Allocations performed by application finalizers remain ordinary fallible operations. The guarantee covers the runtime's mandatory cleanup work. Verify secured backing in the planned host, provider and frame migration tests, including cleanup with allocation denied.

### 7. Started tasks have one control record and one source resolution owner

Use one internal Bray control record per started task. One source owner holds the responsibility to resolve the task, and its result transfers exactly once. Scheduler entries and wake registrations retain the storage needed for safe dispatch and notification without acquiring another source resolution obligation.

The compiler supplies frame layout and generated cleanup operations. The Bray control record manages scheduling, cancellation, waits and terminal state. Direct await stays within the current run and creates no additional task control record. Keep backing live through the last activation, result or retained internal user.

Remove duplicated task state and descriptor graphs across task, run and scheduler representations. Preserve existing protected source task semantics through ordinary internal Bray runtime code. Verify this architecture during the planned frame and task replacement.

### 8. Internal access does not establish runtime authority

Bray declarations and fields are public by default. The visibility modifiers are `public` and `internal`. Explicit internal-use acknowledgement permits access outside the intended scope. It does not establish ownership, validity, synchronization, trusted guarantees or compiler-role authority.

Every runtime guarantee must hold when safe source explicitly acknowledges internal access. Ordinary source records remain accessible under those rules. A record's name, copied ordinary fields or a state flag cannot establish the witness conditions required to operate on live runtime storage. Construction must establish those conditions or preserve them through valid owned fields. Relevant mutation invalidates affected guarantees, and subsequent operations must establish or receive the conditions they require.

Internal low-level helpers must check their requirements or expose them in their callable and lifecycle contracts. `uses(...)` grants implementation capabilities without imposing caller obligations by itself. Preserve trusted requirements through wrappers, callbacks and lifecycle operations. Bind witness guarantees to their live owners, storage identities, epochs and scoped capabilities.

Compiler-protected representations such as `Task<T>` and `PanicReport` remain a separate language mechanism. Ordinary runtime types receive no extra representation protection. Build metadata and compiler validation establish runtime roles, regardless of declaration visibility or spelling.

During the planned migrations, verify that internal acknowledgement alone cannot authorize duplicate result transfer, stale storage use or invalid task transitions. Treat missing compiler enforcement as an implementation gap to resolve there.

### 9. Execution limits count running tasks, not every live task

Storage admission governs capacity for live task backing. Execution limits govern simultaneous execution. Queue excess ready tasks, and release an execution slot when a task suspends so eligible children and other tasks can run. Queued and suspended tasks retain their admitted backing and ownership obligations.

Preserve the specification's distinction between runtime execution limits and explicit algorithm budgets. Replace the current Rust interpretation that caps registered task count as an execution quota. Verify queuing, suspension and compatible-lane progress in the planned scheduler migration.

### 10. Wait registrations transfer ownership at notification commit

Represent a pending wait with an ordinary Bray registration owner. Cancellation before notification commit withdraws the registration. Once notification commits, delivery retains the target storage until the callback returns. Run callbacks outside collection locks.

Remember a wake that arrives while its task is running, and coalesce repeated wakes without losing pending work. A cancellation request does not establish completion. Keep task storage live until execution completes and every outstanding storage user resolves.

Internal access grants no additional authority over registrations or task storage. Enforce the owner and state contracts from decision 8. Preserve the existing public task operations. Verify cancellation races, callback lifetime and wakes during execution in the planned event and scheduler migration.

### 11. Unused optional services stay out of produced programs

Programs must link only the runtime and library support their reachable behavior requires. Preserve mandatory ownership, cleanup and reporting guarantees. Keep optional diagnostic preservation, symbol lookup and history storage out of programs that do not use them. Provider retention alone must not pull in diagnostic conversion or storage for diagnostic history.

Implement diagnostic preservation as an explicitly called library operation if existing operations cannot satisfy the consumer. Ordinary report handling must not automatically create independent snapshots. Do not add a dedicated representation or framework without a consumer requirement that existing operations cannot express.

Lazy initialization reduces resource use but does not prove that unused code and data are absent from a binary. Avoid unconditional registrations and descriptor references that retain optional implementations. Verify emitted code and data through link maps and archive-member inspection for consumers with and without the optional operations. Make these checks part of the planned migrations.

### 12. Correctness, execution speed, binary size and compiler speed govern the design

Correctness alone does not make a design acceptable. Bray programs must compete with equivalent Rust and C++ programs in execution speed. Minimize linked code and data so programs remain practical on integrated hardware with limited resources. Compiler speed must be no slower than current rustc on comparable workloads, with faster compilation as the goal. These are requirements for the destination, not measured claims about the current implementation.

Evaluate these requirements while choosing ownership, storage, synchronization, compiler metadata and generated operations. Account for allocations, indirection, atomic operations, contention, frame size and work on ordinary execution paths. Secure mandatory cleanup backing without assuming a heap allocation for every owner or reserving unused maximum-sized structures. Consider code duplication and metadata size when choosing specialization or erasure.

Compiler-enforced guarantees also have a cost. Share and reuse analysis results where their validity permits it. Avoid repeated dependency analysis, descriptor reconstruction and generated adapters that duplicate equivalent work. Measure clean and incremental compilation, including optimization and linking. Faster checking that shifts extra work into code generation or linking does not meet the total build-time goal.

Measure representative native consumers when the relevant paths can run and meaningful comparisons are possible. Compare equivalent behavior on the same target with recorded toolchain versions and comparable build settings. Include small synchronous programs and constrained hardware alongside async and loaded-provider consumers. Record execution latency or throughput, linked code and data size, peak memory and compilation time where relevant. Inspect emitted code and link maps to explain costs. Use the current Bray implementation as a regression baseline and Rust or C++ equivalents as external comparisons.

These are architecture and destination requirements, not a requirement for every intermediate issue to match Rust or C++. Gate performance and size where meaningful evidence is available. A prerequisite change may temporarily regress a measure when its improvement depends on later work that the change enables. Record the reason, the enabling dependency and the stage where the cost can be evaluated or removed. Architecture can finish before performance tuning. Architecture reasoning must still account for the costs and leave a credible path to the destination requirements.

Make tradeoffs explicit. If a mechanism prevents the destination requirements, reconsider the mechanism and any language rule that requires it. Do not reject a necessary migration step solely because later dependent work is unfinished. This adds evidence where useful without requiring a separate prototype or benchmark for every decision.

## Recommendation

I recommend building the runtime around Bray's ordinary ownership, dependency, lifecycle, storage and capability contracts. Implement product hosts, attachments, cleanup admission, activations, tasks and reporting policy in Bray. The compiler supplies checked cleanup plans, adapters for protected representations and immutable descriptors. Explicit target ABI bindings supply OS operations.

The destination has no project-owned Rust or custom non-Bray runtime and platform code linked into produced programs. The pinned temporal provider remains the exception. Rust can remain in compiler, build and test coordination tools outside those programs.

The Bray specification can express the required data structures. Existing source already implements owners, `Uninit<T>`, anchored access, atomics, synchronization, native thread creation and TLS keys. Use small native consumers to expose missing compiler lowering or runtime role contracts, then fix those gaps alongside the consumers. This gives us a way to start writing the host in Bray now.

Some boundaries still need shared identity and synchronization. Reference counting can support independently surviving owners at erased or foreign lifetime boundaries. A Bray borrow cannot govern a C caller's lifetime. That does not require copying the Rust object graph, `Arc` and `Weak` families, `Pin`, `Any`, descriptor reconstruction, poisoned-lock errors, duplicate registries or eager pools.

## Ownership boundaries

These names describe runtime implementation concepts expressed through ordinary Bray types and contracts. They add no compiler-known source types. Their invariants must hold under explicit internal access, as required by decision 8.

| Owner or capability | Obligation | Expression in Bray |
| --- | --- | --- |
| Product host | Own one activation's static domains, service binding and terminal cleanup | Noncopy owner with an explicit fallible, cancellation-shielded terminal operation and synchronous backing disposal |
| Attachment | Preserve exact native-thread identity, entered products and thread-static cleanup | Stable owned TLS state, an exact-thread dependency and nested entry capability |
| Provider lease | Keep originating product code and storage available until the last callback returns | Noncopy retained owner with explicit duplication and borrowed internal uses |
| Cleanup allowance | Supply backing for the concrete owner's additional local mandatory work | Compiler-managed association between ownership and concrete backing that survives moves |
| Activation | Own a stable active frame, initialized parts, continuation and result storage | Owned storage with a checked descriptor and scoped anchored views |
| Task | Preserve the sole source resolution duty and internal dispatch and wake lifetimes | Protected source handle backed by one Bray control owner and limited retained internal references |
| Wait registration | Withdraw before commit or retain one committed wake | Noncopy guard with explicit state and cancellation-safe disposal |
| Incident or report | Own a quiesced typed payload, backing, origin and disposal dependencies | Move-only owner with a generated concrete disposal adapter and ordered owned chains |

Use direct typed borrows inside a host, attachment or activation when the enclosing lifetime is provable. Short lock or atomic capabilities govern access to shared storage through anchored views. Runtime synchronization guards must end before arbitrary callbacks or suspension. Existing library guards show how to express this in Bray.

Limit erasure to heterogeneous frames and payloads and native transfers. An immutable descriptor contains exact layout, initialized-state operations, dependencies and callbacks. A live provider dependency must cover the descriptor and callbacks throughout use. An enclosing owner can supply that dependency without a separately retained lease for each descriptor. Activation should not rebuild compiler model objects, symbol strings or vectors. Use trait views when their ordinary operations suffice. Generate concrete descriptor operations for protected representations and heterogeneous disposal that trait views cannot express.

## Contracts required by the ownership design

| Runtime operation | Required guarantee |
| --- | --- |
| Construct or adopt an owner | Transfer real resource ownership and establish or preserve the required witness conditions. Field names and ordinary representation values alone establish no trusted guarantee. |
| Access backing through an anchored view | Hold live authority for the exact allocation, initialized epoch, layout and allowed aliases. Copying an address establishes none of these conditions. |
| Resume a task or consume its result | Hold the matching execution or resolution authority and state conditions. A task identifier or state flag alone grants neither. |
| Change shared state | Acquire the required synchronization capability and preserve or re-establish affected conditions. A lock does not by itself prove that every written state is valid. |
| Resolve a foreign handle or invoke a callback | Validate the external identity and acquire the required retained owners and attachment. Invoke unchecked operations only under their declared caller requirements. |

These are requirements for the proposed source implementation, not claims that all compiler checks already work. [Visibility](../language/declarations/visibility-and-reachability.md), [Witnesses](../language/contracts-and-trust/trusted-witness-values.md), [Caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md), [Obligation propagation](../language/contracts-and-trust/obligation-propagation.md), [Protected representation](../language/compiler-known-and-standard-library/protected-representation.md).

## Host formation and provider lifetime

Immutable product metadata supplies the completed static order, required services and dependency templates. A mutable host instance holds live state. Final-linked code, static archives and imported MIR share the consumer's host. Separately loaded images get distinct activation identities even when artifact bytes or descriptor addresses match.

Formation validates external bindings and transitive provider requirements, secures host and static cleanup backing, constructs the complete host, then publishes entry availability. Rollback owns every accepted input and provider dependency. Static materialization remains constant initialization. Formation does not run a user's lazy initializer. The compiler validates generated inputs, so the runtime needs no second planner for them. Entry still checks external bindings.

Following decision 2, keep one resident Bray service owner for each group of products that exchange owned values. It lives outside the unloadable images whose callbacks it protects. Independent schedulers borrow this domain and retain separate workers, budgets and execution authority. Synchronous products use it without an async runtime. An embedding host supplies an explicit binding. An image-local cached fallback would violate that ownership model.

Sharing a service domain does not require a shared lock or service call for every move, local admission or borrowed access. Use already secured local backing where its contracts permit it. Independently surviving users still retain the backing and services they need. Internal borrows do not require foreign-handle lookup or a new retained owner when the enclosing owner already proves the lifetime.

Foreign handles need a validated table with load and epoch identities that never repeat, plus invalidation at terminal release. Internal paths borrow stable hosts directly. The resident caller retains the last provider lease until image code and all its local destruction have returned. Otherwise an unload callback could unmap the code executing it. Because Bray `with` exits before body-local destruction, keep that lease in an enclosing owner or release it after an inner scope finishes. An entry capability alone cannot establish this ordering.

Decision 3 requires whole-product retention. An external owner that can reach provider code or storage delays static cleanup and unloading. An escaped provider-dependent panic report keeps its product available until disposal. Align BRA-501's code-only cleanup expectation and `docs/design/cleanup-storage-and-reports.md` with this rule. Do not add a code-only exception.

Whole-product retention can keep product resources alive for the lifetime of a small report. It gives symbols, callbacks, reports and storage the same dependency rule. Decision 1 separates retention from a mandatory wait. Entry closure still rejects new source entry immediately, while existing owners retain the operations they need to complete disposal. The runtime shutdown API must preserve responsibility for cleanup when external roots keep a domain ineligible.

Freeze the teardown set before closing admission. External roots block the domains they reach. Static-owned edges inside the set determine consumer-before-provider order. Check cycles and admission before publishing dynamically installed provider edges. Incidents created during teardown are internal terminal work. Drain them before their dependencies become unavailable, and exclude them from external-root waits that would block teardown on itself. Cleanup publishes no new escaping roots.

Decision 5 requires compiler-provided dependency order and structural tie keys within a domain. Preserve precedence and exact identities across domains without imposing a universal completion order. Correct the bootstrap text about reverse registration to match this decision. Exact-thread cleanup stays on its attachment thread.

## Cleanup admission and backing

Decision 6 requires physical cleanup backing throughout ownership. One compiler cleanup description determines legality, effects, symbolic storage needs and generated actions. Concrete specialization adds layout without changing action selection. Each new local obligation admits its additional allowance before publication. Children already carry theirs. Construction failure leaves initialized inputs and their allowances owned. A completion proof cannot release capacity that later mutation may need again.

The allowance covers the actual mandatory path. That includes activation and control storage, concrete finalizer frames and results, typed incident backing, report and wait links, and bounded callback outcomes. It also covers host detachment acknowledgements and terminal infrastructure that must work when allocation is denied. Application finalizer allocations remain fallible. The guarantee does not cover unlimited explicit retries.

Use caller or enclosing storage when the live shape is known. Erased and recursive state and escaping payloads need backing that preserves their actual lifetime and address requirements. Use separately owned backing when enclosing or coallocated storage cannot satisfy those requirements. Erasure alone does not require a separate heap allocation. Owners with non-overlapping lifetimes may share storage, but an outstanding result or report prevents reuse of its region. Recursive ownership carries each child's local allowance. Every new dynamic obligation secures its own additional needs before becoming live.

Activation turns a reserved region into one active owner. Rejection leaves the reservation intact. Source discharge releases unused allowance and logical accounting. Backing transferred into an activation or report remains owned until its last typed or erased user resolves. Epoch identity prevents a reused address from authorizing stale activation or disposal.

Choose caller, enclosing, coallocated or pooled backing from the required lifetimes and measured costs. Pooling is optional. Evaluate allocation frequency, locality, reserved memory and linked support before choosing it. A process-wide credit counter or emergency pool does not prove that every live frame and typed error has physical backing. The admission domain remains bound across ordinary moves. Public values need no new domain parameter or fallible rebinding.

Return admission failure through caller-owned storage before publication. An allocation-failure report can use an inline header and static message without allocating again. Bootstrap code uses explicit target allocation, raw layout and atomics. Its terminal cleanup needs must be empty or already supplied. Ordinary runtime source receives the same capacity and lifecycle checks as other Bray source. Exempting the whole runtime would make its allocation guarantee circular.

## Runs, frames and scheduling

Generated code owns source control flow, initialized-state tracking, separate task broadcast and resolution, direct-await composition and protected frame operations. Bray runtime code owns admission, stable activation backing, dispatch, waits, cancellation state, terminal publication, observation and infrastructure. A generated ABI adapter establishes the execution authority supplied by its role. Source cannot declare itself authorized to run on a lane.

Decision 7 gives each started task one control record. Dispatch acquires execution authority through a valid state transition, calls source, then commits suspension or terminal state. Synchronize state that is shared across threads. Thread-confined state needs no transition lock when confinement is established, including the absence of concurrent foreign wake access. Call source outside synchronization guards. Queue entries retain enough storage authority for dispatch. They do not create a second source resolution owner.

Cancellation requests and cleanup shielding have explicit scoped state. Requests wake eligible suspended work. Storage reclamation waits for completion. Generated lexical cleanup finishes before terminal publication. Observation establishes the completion visibility edge and transfers one result, followed by host payload and static cleanup. Main stays on main. Origin-thread dependencies pin work to the exact attachment, while other state can migrate across compatible lanes.

Decision 10 governs wait registration ownership and notification commit. Waits observe, register and recheck around that commit point. Reserve mandatory waiter and queue links before they become necessary. Callbacks and disposal run outside collection locks. Source task handles keep their existing API without raw wake, poll or detach operations.

Start with one scheduler implementation, per-compatible-lane ready queues, ordered deadlines and explicit worker owners. Preserve public channel and budget FIFO contracts. Choose internal queue and worker policies from consumer needs. Decision 9 separates task storage admission from the hard quota on simultaneous execution. Excess ready tasks queue, and suspended tasks release execution slots while retaining backing. Workers and reactor resources initialize when reachable work needs them, with explicit creation and limit policies.

Worker bootstrap uses target thread and TLS operations below public `std.thread`. It must establish run and event services before public wrappers can use them. Synchronous roots use the same host, attachment and report owners without a scheduler. Shutdown keeps the allocator, incident sink, lanes, loaders and providers live through static cleanup. Attachment acknowledgements precede worker join and final release. These operations must avoid self-join and run outside host locks.

## Reports, platform and test products

Preserve owned report and message behavior and iterative suppression. Internally, express ownership through move-only Bray owners. Copyable ABI records encode trusted transfers. They do not duplicate resource ownership. Generate adapters for protected `PanicReport` without exposing its fields to ordinary source. Preserve full message snapshots and forwarding of the same owner.

After child runs quiesce, move each typed finalizer error into admitted payload backing. Its incident retains concrete type, origin, ordinal, provider and attachment dependencies, and a synchronous concrete destructor. This replaces linked Rust `Any`, `Box<dyn ...>`, native unwinding and payload registries. Destructor-generated reports use secured outgoing slots and preserve primary and suppressed ordering. Disposal after producer-thread exit requires a dependency contract that permits it.

Use target-gated Bray bindings for threads, TLS, allocation, atomic waits, clocks, loading, I/O and reactor operations. Share existing internal bindings below standard-library policy. Public wrappers must not establish their own prerequisites. Generate macro constants and target representations from pinned SDK inputs where needed. If Bray cannot represent an ABI, fix the compiler or target support instead of adding a permanent custom shim.

The test coordinator may remain host-side Rust. Everything linked into a test program must be Bray, including root boundaries, session protocol, capture, timeout cancellation and incident disposal. Test execution uses the same host and admission code. Command-wide serial exclusion and shared product statics remain explicit coordinator and product contracts.

## Implementation sequence and removals

A step may need several PRs. Each PR should exercise a real consumer and identify the Rust code it removes. This sequence keeps the dependency order of the delivery strategy.

1. Apply agreed lifetime and bootstrap decisions. Apply decisions 1, 3, 4 and 5, settle the runtime shutdown ownership contract, then align conflicting specification text, design text and fixtures. Prove a typed role-bound Bray owner and TLS destructor entry without recursive trampolines or admission through the service being created. This establishes the first implementation prerequisite.
2. Replace the synchronous host. Produce an executable that uses Bray resident product formation, ordinary owner cleanup, panic and failure reporting, exact-thread statics and shutdown, with mandatory backing for those owners. Remove its linked Rust product, synchronous-root, attachment and report-rendering paths. Use link-map and archive-member evidence to prove that it links no project-owned Rust. Establish this before completing the async scheduler migration.
3. Replace formation and admission for a real loaded provider. Exercise two independent loads, statics, archive and imported-MIR contributions, escaped reports and typed errors, last callback return and reload. Include repeated plugin reloads with independently preserved diagnostic history and resource release, plus intentional retention of provider-dependent payloads. Run terminal disposal with allocation denied. Remove the Rust product registry, retention and static-admission code used by that consumer. Extend compiler metadata for physical cleanup backing as needed.
4. Replace frame and task execution. Prove that direct await creates no child task. Exercise erased and recursive activations, start, join, cancel and abnormal payload cleanup with secured backing, one terminal owner and separate broadcast and resolution. Remove Rust `NativeFrame`, `NativeRun`, task control records and the `Any` bridge from that execution path.
5. Replace lane, event, timer and worker services. Prove wake races, cancellation withdrawal, affinity, queued execution limits, main-thread progress and worker-thread static cleanup. Remove linked Rust scheduler, cancellation, event and platform code as each service switches to Bray.
6. Replace test-host policy and finish packaging. Implement protocol, capture, serial execution and timeouts in Bray. Delete adapter partitioning, Rust common support and runtime platform artifacts when no produced consumer needs them. Audit all six target artifacts and run native fixtures on available target hosts. Cross-compilation alone cannot prove target runtime behavior.

For each implementation PR, record its consumer, ownership invariants, rejection and terminal cases, linked Rust functions and objects removed, and compiler gaps to resolve. Record execution, size and compilation evidence when meaningful comparisons are possible under decision 12. Explain temporary regressions and their enabling dependencies. Temporary Rust tests can remain outside produced programs as behavior references while Bray fixtures take over their coverage. Remove superseded production implementations as consumers switch.

## Shutdown lifecycle proposal awaiting agreement

The user supports preserving caller ownership when shutdown is blocked, subject to settling the complete lifecycle first. This section is a proposal, not an agreed decision. It defines ownership behavior without choosing exact type or method names.

The caller's owner holds the graceful shutdown obligation. The resident host owns the live product storage and an admitted fallback cleanup obligation from formation. These roles share one terminal cleanup state and cannot execute cleanup twice. That distinction permits abnormal abandonment of the caller's graceful obligation without abandoning product resources. Formation must secure the fallback's storage, reporting and execution requirements before publishing the owner.

An explicit shutdown attempt closes new entry and checks cleanup eligibility. If external dependencies prevent cleanup, it returns a retryable status and preserves the caller's unresolved owner. It does not wait for those dependencies to disappear. When cleanup is eligible, the host can perform its terminal cleanup, including asynchronous work where required.

Normal scope exit follows the existing Bray finalization rules. A pending, possibly fallible implicit finalizer is rejected. The caller must explicitly complete shutdown, transfer the unresolved owner to a valid enclosing owner, or use an explicit ownership transfer into the admitted fallback. A blocked attempt does not establish completion. Wrapping the owner in a result or nullable value preserves its obligation.

During panic or cancellation, attempt the same finalizer under the ordinary cleanup rules. A blocked or failed attempt becomes a cleanup incident. Abandon the caller's graceful obligation and destroy its control owner synchronously. The resident host retains its already admitted fallback obligation, product storage and provider dependencies. The destructor releases the caller's ownership without creating a new asynchronous obligation or letting a borrow from the destroyed owner escape.

Dependency release can make fallback cleanup eligible. Dispatch it through the bound execution services with the required affinity. Keep those services and reporting live until cleanup completes. No helper thread or periodic polling service is required. This is valid only when the binding provides continued execution for the required work. A binding that cannot preserve that capability cannot admit this fallback. The service's own shutdown must preserve unresolved ownership rather than discard pending products or wait on dependencies held by its caller.

Once terminal static cleanup begins, failed finalizers produce incidents and use the existing terminal abandonment rules. Continue eligible cleanup and destroy each initialized static once. A cleanup failure can remain observable even when checked postconditions establish that shutdown completed. Do not return new escaping provider-dependent incidents from a product already being torn down. Drain internal incidents while their dependencies remain available.

Use the existing host record and secured terminal backing for the fallback. Avoid a separate allocation or general deferred-work framework for every source owner. Verify the ownership transfer, abnormal destruction, continued execution and exact-thread cleanup during the planned host and provider migrations. The allocation and execution costs remain subject to decision 12.

[Finalization](../language/lifecycle/finalization.md), [Scope exit](../language/lifecycle/scope-exits-panics-and-cancellation.md), [Destruction](../language/lifecycle/destruction.md), [Static cleanup](../language/declarations/static-storage-declarations.md).

## Decisions still needing acceptance

- The shutdown lifecycle proposal above remains open after decision 1. The user has not committed to the shutdown API. Decision 11 requires checking existing library operations before proposing an independent diagnostic representation or conversion API for decision 3.
- Concrete backing sizes, handle-table representation, locks and worker counts remain implementation choices constrained by consumers. The proposal adds no generic reference-counted container framework.

The source research supports trying this design. It does not prove a completed native implementation. The implementation sequence must expose missing compiler support. This research ran no tests or performance measurements and changed no runtime code or Linear issues.
