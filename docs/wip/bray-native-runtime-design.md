# Bray native runtime design proposal

This is a research proposal. It has not changed the language, runtime implementation or Linear roadmap. The baseline is `a65cc2f0` on `docs/bray-native-runtime-design-research`.

The [language notes](bray-runtime-language-notes.md) and [consumer notes](bray-runtime-consumer-notes.md) support the recommendations below. This document records step 2 of the [delivery strategy](https://linear.app/bray-lang/document/bray-native-runtime-and-platform-delivery-plan-46623e2d308d).

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

Independent diagnostic preservation is required runtime behavior. A caller must be able to preserve diagnostic information as independently owned data, dispose of the original provider-dependent report and release its provider dependency. A typed payload that still needs provider code continues to retain the product. This uses ordinary Bray ownership and library operations. The diagnostic representation and conversion API remain implementation choices.

Verify this behavior in the planned loaded-provider migration tests. Repeated plugin loads and unloads must release plugin resources while independently owned diagnostic history remains available. Deliberately retained provider-dependent payloads must keep the product alive. These are acceptance requirements for the existing implementation sequence, not a separate prototype prerequisite.

Align BRA-501's code-only cleanup expectations and conflicting design text with this decision. Preserve the language's dependency rule when refining report representation.

## Recommendation

I recommend building the runtime around Bray's ordinary ownership, dependency, lifecycle, storage and capability contracts. Implement product hosts, attachments, cleanup admission, activations, tasks and reporting policy in Bray. The compiler supplies checked cleanup plans, adapters for protected representations and immutable descriptors. Explicit target ABI bindings supply OS operations.

The destination has no project-owned Rust or custom non-Bray runtime and platform code linked into produced programs. The pinned temporal provider remains the exception. Rust can remain in compiler, build and test coordination tools outside those programs.

The Bray specification can express the required data structures. Existing source already implements owners, `Uninit<T>`, anchored access, atomics, synchronization, native thread creation and TLS keys. Use small native consumers to expose missing compiler lowering or private contracts, then fix those gaps alongside the consumers. This gives us a way to start writing the host in Bray now.

Some boundaries still need shared identity and synchronization. Reference counting can support independently surviving owners at erased or foreign lifetime boundaries. A Bray borrow cannot govern a C caller's lifetime. That does not require copying the Rust object graph, `Arc` and `Weak` families, `Pin`, `Any`, descriptor reconstruction, poisoned-lock errors, duplicate registries or eager pools.

## Ownership boundaries

These names describe private concepts. They propose no new public API or compiler-known source types.

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

Limit erasure to heterogeneous frames and payloads and native transfers. An immutable descriptor contains exact layout, initialized-state operations, dependencies and callbacks, and retains its provider dependency. Activation should not rebuild compiler model objects, symbol strings or vectors. Use trait views when their ordinary operations suffice. Generate concrete descriptor operations for protected representations and heterogeneous disposal that trait views cannot express.

## Host formation and provider lifetime

Immutable product metadata supplies the completed static order, required services and dependency templates. A mutable host instance holds live state. Final-linked code, static archives and imported MIR share the consumer's host. Separately loaded images get distinct activation identities even when artifact bytes or descriptor addresses match.

Formation validates external bindings and transitive provider requirements, secures host and static cleanup backing, constructs the complete host, then publishes entry availability. Rollback owns every accepted input and provider dependency. Static materialization remains constant initialization. Formation does not run a user's lazy initializer. The compiler validates generated inputs, so the runtime needs no second planner for them. Entry still checks external bindings.

Following decision 2, keep one resident Bray service owner for each group of products that exchange owned values. It lives outside the unloadable images whose callbacks it protects. Independent schedulers borrow this domain and retain separate workers, budgets and execution authority. Synchronous products use it without an async runtime. An embedding host supplies an explicit binding. An image-local cached fallback would violate that ownership model.

Foreign handles need a validated table with load and epoch identities that never repeat, plus invalidation at terminal release. Internal paths borrow stable hosts directly. The resident caller retains the last provider lease until image code and all its local destruction have returned. Otherwise an unload callback could unmap the code executing it. Because Bray `with` exits before body-local destruction, keep that lease in an enclosing owner or release it after an inner scope finishes. An entry capability alone cannot establish this ordering.

Decision 3 requires whole-product retention. An external owner that can reach provider code or storage delays static cleanup and unloading. An escaped provider-dependent panic report keeps its product available until disposal. Align BRA-501's code-only cleanup expectation and `docs/design/cleanup-storage-and-reports.md` with this rule. Do not add a code-only exception.

Whole-product retention can keep product resources alive for the lifetime of a small report. It gives symbols, callbacks, reports and storage the same dependency rule. Decision 1 separates retention from a mandatory wait. Entry closure still rejects new source entry immediately, while existing owners retain the operations they need to complete disposal. The runtime shutdown API must preserve responsibility for cleanup when external roots keep a domain ineligible.

Freeze the teardown set before closing admission. External roots block the domains they reach. Static-owned edges inside the set determine consumer-before-provider order. Check cycles and admission before publishing dynamically installed provider edges. Incidents created during teardown are internal terminal work. Drain them before their dependencies become unavailable, and exclude them from external-root waits that would block teardown on itself. Cleanup publishes no new escaping roots.

Within a domain, use compiler-provided dependency order and structural tie keys. Preserve precedence and exact identities across domains without imposing a universal completion order. Clarify the bootstrap text about reverse registration to match these static rules. Exact-thread cleanup stays on its attachment thread.

## Cleanup admission and backing

One compiler cleanup description determines legality, effects, symbolic storage needs and generated actions. Concrete specialization adds layout without changing action selection. Each new local obligation admits its additional allowance before publication. Children already carry theirs. Construction failure leaves initialized inputs and their allowances owned. A completion proof cannot release capacity that later mutation may need again.

The allowance covers the actual mandatory path. That includes activation and control storage, concrete finalizer frames and results, typed incident backing, report and wait links, and bounded callback outcomes. It also covers host detachment acknowledgements and terminal infrastructure that must work when allocation is denied. Application finalizer allocations remain fallible. The guarantee does not cover unlimited explicit retries.

Use caller or enclosing storage when the live shape is known. Give erased and recursive state and escaping payloads separately owned backing. Owners with non-overlapping lifetimes may share storage, but an outstanding result or report prevents reuse of its region. Recursive ownership carries each child's local allowance. Every new dynamic obligation secures its own additional needs before becoming live.

Activation turns a reserved region into one active owner. Rejection leaves the reservation intact. Source discharge releases unused allowance and logical accounting. Backing transferred into an activation or report remains owned until its last typed or erased user resolves. Epoch identity prevents a reused address from authorizing stale activation or disposal.

Pooling is optional. Start with typed allocations and fixed local regions where they fit. Retain slab reuse where measured costs justify it. A process-wide credit counter or emergency pool does not prove that every live frame and typed error has physical backing. The admission domain remains bound across ordinary moves. Public values need no new domain parameter or fallible rebinding.

Return admission failure through caller-owned storage before publication. An allocation-failure report can use an inline header and static message without allocating again. Bootstrap code uses explicit target allocation, raw layout and atomics. Its terminal cleanup needs must be empty or already supplied. Ordinary runtime source receives the same capacity and lifecycle checks as other Bray source. Exempting the whole runtime would make its allocation guarantee circular.

## Runs, frames and scheduling

Generated code owns source control flow, initialized-state tracking, separate task broadcast and resolution, direct-await composition and protected frame operations. Bray runtime code owns admission, stable activation backing, dispatch, waits, cancellation state, terminal publication, observation and infrastructure. A generated ABI adapter establishes the execution authority supplied by its role. Source cannot declare itself authorized to run on a lane.

Give each task one control record. Short synchronization governs transitions among ready, running, waiting and terminal states. Dispatch acquires execution authority under the transition lock, calls source outside it, then commits suspension or terminal state. Queue entries retain enough storage authority for dispatch. They do not create a second source resolution owner.

Cancellation requests and cleanup shielding have explicit scoped state. Requests wake eligible suspended work. Storage reclamation waits for completion. Generated lexical cleanup finishes before terminal publication. Observation establishes the completion visibility edge and transfers one result, followed by host payload and static cleanup. Main stays on main. Origin-thread dependencies pin work to the exact attachment, while other state can migrate across compatible lanes.

Waits observe, register and recheck around a commit point. Cancellation withdraws uncommitted registrations. Committed notifications retain their targets until callbacks return. Coalesce repeated wakes and preserve a wake that arrives while the task runs. Reserve mandatory waiter and queue links before they become necessary. Callbacks and disposal run outside collection locks. Source task handles keep their existing API without raw wake, poll or detach operations.

Start with one scheduler implementation, per-compatible-lane ready queues, ordered deadlines and explicit worker owners. Preserve public channel and budget FIFO contracts. Choose internal queue and worker policies from consumer needs. Separate task storage failure from the hard quota on simultaneous execution. Excess ready tasks queue. Workers and reactor resources initialize when reachable work needs them, with explicit creation and limit policies.

Worker bootstrap uses target thread and TLS operations below public `std.thread`. It must establish run and event services before public wrappers can use them. Synchronous roots use the same host, attachment and report owners without a scheduler. Shutdown keeps the allocator, incident sink, lanes, loaders and providers live through static cleanup. Attachment acknowledgements precede worker join and final release. These operations must avoid self-join and run outside host locks.

## Reports, platform and test products

Preserve owned report and message behavior and iterative suppression. Internally, express ownership through move-only Bray owners. Copyable ABI records encode trusted transfers. They do not duplicate resource ownership. Generate adapters for protected `PanicReport` without exposing its fields to ordinary source. Preserve full message snapshots and forwarding of the same owner.

After child runs quiesce, move each typed finalizer error into admitted payload backing. Its incident retains concrete type, origin, ordinal, provider and attachment dependencies, and a synchronous concrete destructor. This replaces linked Rust `Any`, `Box<dyn ...>`, native unwinding and payload registries. Destructor-generated reports use secured outgoing slots and preserve primary and suppressed ordering. Disposal after producer-thread exit requires a dependency contract that permits it.

Use target-gated Bray bindings for threads, TLS, allocation, atomic waits, clocks, loading, I/O and reactor operations. Share existing internal bindings below standard-library policy. Public wrappers must not establish their own prerequisites. Generate macro constants and target representations from pinned SDK inputs where needed. If Bray cannot represent an ABI, fix the compiler or target support instead of adding a permanent custom shim.

The test coordinator may remain host-side Rust. Everything linked into a test program must be Bray, including root boundaries, session protocol, capture, timeout cancellation and incident disposal. Test execution uses the same host and admission code. Command-wide serial exclusion and shared product statics remain explicit coordinator and product contracts.

## Implementation sequence and removals

A step may need several PRs. Each PR should exercise a real consumer and identify the Rust code it removes. This sequence keeps the dependency order of the delivery strategy.

1. Apply agreed lifetime semantics and settle bootstrap role lowering. Apply decisions 1 and 3, settle the runtime shutdown ownership contract, then align conflicting specification text, design text and fixtures. Prove a typed role-bound Bray owner and TLS destructor entry without recursive trampolines or admission through the service being created. This establishes the first implementation prerequisite.
2. Replace the synchronous host. Produce an executable that uses Bray resident product formation, ordinary owner cleanup, panic and failure reporting, exact-thread statics and shutdown, with mandatory backing for those owners. Remove its linked Rust product, synchronous-root, attachment and report-rendering paths. Use link-map and archive-member evidence to prove that it links no project-owned Rust. Establish this before completing the async scheduler migration.
3. Replace formation and admission for a real loaded provider. Exercise two independent loads, statics, archive and imported-MIR contributions, escaped reports and typed errors, last callback return and reload. Include repeated plugin reloads with independently preserved diagnostic history and resource release, plus intentional retention of provider-dependent payloads. Run terminal disposal with allocation denied. Remove the Rust product registry, retention and static-admission code used by that consumer. Extend compiler metadata for physical cleanup backing as needed.
4. Replace frame and task execution. Prove that direct await creates no child task. Exercise erased and recursive activations, start, join, cancel and abnormal payload cleanup with secured backing, one terminal owner and separate broadcast and resolution. Remove Rust `NativeFrame`, `NativeRun`, task control records and the `Any` bridge from that execution path.
5. Replace lane, event, timer and worker services. Prove wake races, cancellation withdrawal, affinity, queued execution limits, main-thread progress and worker-thread static cleanup. Remove linked Rust scheduler, cancellation, event and platform code as each service switches to Bray.
6. Replace test-host policy and finish packaging. Implement protocol, capture, serial execution and timeouts in Bray. Delete adapter partitioning, Rust common support and runtime platform artifacts when no produced consumer needs them. Audit all six target artifacts and run native fixtures on available target hosts. Cross-compilation alone cannot prove target runtime behavior.

For each implementation PR, record its consumer, ownership invariants, rejection and terminal cases, linked Rust functions and objects removed, and compiler gaps to resolve. Temporary Rust tests can remain outside produced programs as behavior references while Bray fixtures take over their coverage. Remove superseded production implementations as consumers switch.

## Decisions still needing acceptance

- The runtime shutdown ownership API and independent diagnostic representation remain open after decisions 1 and 3.
- Concrete backing sizes, handle-table representation, locks and worker counts remain implementation choices constrained by consumers. The proposal adds no generic reference-counted container framework.

The source research supports trying this design. It does not prove a completed native implementation. The implementation sequence must expose missing compiler support. This research ran no tests or performance measurements and changed no runtime code or Linear issues.
