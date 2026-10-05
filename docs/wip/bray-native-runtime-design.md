# Bray native runtime and platform design

## Status and scope

This document is a WIP design for implementing the runtime and platform support linked into Bray programs in Bray. It covers product hosts, thread attachments, provider lifetime, cleanup storage, task execution, reporting, native bindings and test-product support.

The architecture is selected. Native implementation, compiler support and integration with the existing documentation remain incomplete. This document describes the intended design and identifies the required changes to the current specification. It does not amend the language specification by itself.

The [language specification](../language/index.md) defines source semantics. The existing [async runtime](../design/async-runtime.md), [cleanup storage](../design/cleanup-storage-and-reports.md) and [platform services](../design/io-and-platform-services.md) documents describe the current design. The integration section below identifies conflicts that must be resolved during implementation.

The migration follows the [native runtime and platform delivery strategy](https://linear.app/bray-lang/document/bray-native-runtime-and-platform-delivery-plan-46623e2d308d). Supporting material appears in the appendices. Appendix A is temporary and must be removed when this design is fully implemented in Bray.

## Goals and constraints

All project-owned runtime and platform code linked into produced programs must be implemented in Bray. The pinned temporal provider is the exception. Explicit OS and system ABI dependencies remain permitted. Compiler, build and test coordination tools may remain Rust when they run outside produced programs.

The runtime uses ordinary Bray ownership, dependency, lifecycle, storage and capability contracts. Protected compiler-known representations retain their specified lowering. This design adds no compiler-known host, scheduler, cleanup allowance or shutdown result type.

| Requirement | Design constraint |
| --- | --- |
| Correctness | Preserve ownership, initialization, dependency, lifecycle, cancellation, affinity and reporting contracts across source and native boundaries. |
| Execution speed | Match equivalent Rust or C++ programs on representative workloads through suitable storage, direct execution and synchronization choices. |
| Binary size | Minimize linked code and data so small programs remain practical on integrated hardware with limited resources. Omit unused optional services. |
| Compiler speed | Compile comparable workloads no slower than current rustc, with faster compilation as the goal. Include optimization and linking in build-time comparisons. |

These are destination requirements. Intermediate work may temporarily regress performance when it enables dependent changes that remove the cost. Architecture choices must account for those costs and preserve a credible path to the destination.

## Responsibilities

| Component | Responsibility |
| --- | --- |
| Compiler | Check source contracts and lifecycle legality. Produce cleanup plans, concrete frame layouts, immutable descriptors, protected-representation adapters and validated runtime-role metadata. |
| Bray runtime | Implement formation, attachment, admission, activation, scheduling, waits, cancellation state, terminal publication, observation and reporting. |
| Bray platform code | Implement target-gated bindings and native mechanisms through explicit system ABIs. |
| External tooling | Build artifacts, coordinate tests and inspect emitted objects without supplying code linked into produced programs. |

One compiler cleanup description determines selected actions, legality, effects and symbolic storage needs. Concrete specialization supplies layout without repeating action selection. Runtime code consumes completed metadata and checked operations. It does not reconstruct compiler models, symbol strings, state vectors or descriptor graphs during activation.

## Ownership and authority

### Ownership boundaries

The following names identify implementation responsibilities. Concrete types and method names remain implementation choices.

| Owner or capability | Responsibility |
| --- | --- |
| Product host | Own one activation's static domains, service binding, terminal cleanup state and admitted fallback cleanup. |
| Caller shutdown owner | Hold the graceful shutdown obligation and preserve it when a shutdown attempt is blocked. |
| Thread attachment | Own stable TLS state, exact-thread identity, entered products and thread-local cleanup. |
| Provider lease | Retain provider code and product storage through dependent use and callback return. |
| Cleanup allowance | Preserve backing for an owner's additional local mandatory work through ownership transfer. |
| Activation | Own stable active frame storage, initialized state, continuation and result storage. |
| Task control record | Coordinate one started task, one source resolution obligation and retained internal dispatch users. |
| Wait registration | Own withdrawal before notification commit and retained delivery after commit. |
| Incident or report | Own the payload, backing, origin and dependencies needed for reporting and disposal. |

Use typed borrows inside a host, attachment or activation when an enclosing owner proves the lifetime. A borrow does not require an additional retained lease or foreign-handle lookup. Independently surviving users must retain the backing and services they need.

Erasure is limited to heterogeneous frames and payloads and native ownership transfers. Immutable descriptors contain layout, initialized-state operations, dependencies and callbacks. A live provider dependency covers each descriptor and callback throughout use. An enclosing owner may supply that dependency without a separately retained lease per descriptor. Trait views are suitable where their ordinary operations express the required behavior.

### Accessibility and runtime authority

Bray declarations and fields are public by default. `public` and `internal` are visibility modifiers. Explicit internal-use acknowledgement permits access outside the intended scope, subject to the ordinary language rules. It grants no ownership, validity, synchronization, trusted guarantees or compiler-role authority.

Runtime guarantees must hold when safe source acknowledges internal access. Construction establishes witness conditions or preserves them through valid owned fields. Copied ordinary fields, record names and state flags do not establish those conditions. Relevant mutation invalidates affected guarantees.

Low-level helpers establish their requirements or expose them in callable and lifecycle contracts. `uses(...)` grants implementation capabilities without imposing caller obligations by itself. Wrappers, callbacks and lifecycle operations preserve trusted requirements. Witness guarantees remain tied to live owners, storage identity, epochs and scoped capabilities.

Protected representations such as `Task<T>` and `PanicReport` use the existing compiler mechanism. Ordinary runtime records receive no extra representation protection. Build metadata and compiler validation establish runtime roles independently of declaration visibility or spelling.

### Runtime operation contracts

| Operation | Required authority |
| --- | --- |
| Construct or adopt an owner | Transfer actual resource ownership and establish or preserve its witness conditions. |
| Access anchored storage | Hold authority for the exact allocation, initialized epoch, layout and allowed aliases. |
| Resume or observe a task | Hold matching execution or resolution authority and valid state conditions. |
| Change shared state | Hold the required synchronization capability and preserve or establish the resulting state conditions. |
| Resolve a foreign handle | Validate the external identity and acquire its retained owners and attachment. |

Synchronization guards end before arbitrary source callbacks, disposal or suspension. A lock does not establish validity of arbitrary state written while it is held. Missing compiler enforcement is an implementation gap, not permission to rely on restricted visibility.

## Product hosts and service domains

### Formation and identity

Immutable product metadata supplies static cleanup order, required services and dependency templates. A mutable host owns activation state. Final-linked code, static archives and imported MIR share the consumer product's host. Separately loaded images have distinct activation identities even when artifact bytes or descriptor addresses match.

Formation validates external bindings and transitive provider requirements, secures host and static cleanup backing, constructs the complete host, then publishes entry availability. Failure retains every accepted input and the services needed for rollback. Runtime validation does not repeat compiler planning for generated metadata.

Static materialization remains constant initialization. Formation does not run user lazy initializers or alter their timing. Dynamic provider edges undergo cycle and admission checks before publication.

Foreign handles use validated load and epoch identities that do not repeat. Terminal release invalidates the identity before image release. Internal execution borrows stable hosts directly.

### Service domains

Products that exchange owned values bind to one cleanup admission service domain. The binding remains stable through moves, and the service outlives every transferred backing owner that depends on it. The resident Bray service owner lives outside unloadable images whose callbacks it protects. A cached image-local fallback cannot replace that binding.

Schedulers retain separate workers, execution limits, budgets and lane authority. Synchronous products use admission and reporting without an async scheduler. Unrelated hosts may own separate service domains. The design requires no process-wide singleton.

Sharing a service domain does not prescribe a central lock or service call for every move, local admission or borrowed access. Already secured local backing can satisfy admission where its contracts permit it.

### Bootstrap and thread attachment

Bootstrap uses existing low-level target bindings and explicit startup storage. It establishes attachment and cleanup services before high-level APIs that depend on them. Public thread operations cannot initialize the runtime services they already require. Bootstrap cleanup needs must be empty or supplied by startup backing, without admission through the service being constructed.

Bootstrap remains subject to ordinary lifecycle, capacity and capability checks. Compiler-validated roles establish ABI and execution contracts. Function spelling or membership in a runtime module provides no exemption. Generated entry must not recursively wrap the operation that establishes its own prerequisites.

Thread attachment owns stable TLS state and exact-thread cleanup authority. Nested native entry reuses the attachment. TLS destructor entry clears thread-storage state before invoking exact-thread cleanup. Native callbacks contain abnormal outcomes and do not unwind through the ABI.

## Provider lifetime and static cleanup

### Provider lifetime

An owner depending on a provider retains that provider's code and product statics until the dependency ends. Reports, typed payloads, symbols, callbacks and borrowed data follow this same rule. Retention prevents cleanup eligibility without requiring a blocking wait. There is no code-only retention exception or selective static-cleanup proof.

Closing entry rejects new admission while retained owners preserve the operations required for their completion and disposal. Entry closure does not invalidate those owners or their dependencies.

The resident caller retains the last provider lease until image code and all local destruction have returned. Releasing that lease from image code could unmap the callback currently executing. Because Bray `with` exits before body-local destruction, the lease belongs to an enclosing owner or is released after the inner scope finishes.

Whole-product retention can preserve substantial resources for the lifetime of a small report. A caller requiring diagnostic history after unload can preserve independently owned diagnostic data, dispose of the original report and release its provider dependency. Typed payloads that require provider code continue to retain the product. This optional library operation is described under reports and diagnostics.

### Static cleanup order

Product and thread-local statics use dependency order. If cleanup of `A` requires `B`, `A` completes before cleanup of `B` begins. Independent nodes within a domain use deterministic structural tie order. Thread-local cleanup stays on the exact attachment thread. Registration order has no semantic significance.

The compiler supplies known within-domain order and structural keys. Runtime work handles relationships that depend on loaded products and attachments. Independent domains have no universal completion order.

The host fixes the teardown set before closing admission. External roots prevent cleanup of the domains they reach. Static-owned edges within the set determine consumer-before-provider order and do not cause the host to wait on its own statics. Cleanup preserves authority over existing dependencies and creates no new escaping roots.

Incidents created during teardown are internal terminal work. Drain them before their dependencies become unavailable. They must not become external-root waits that block their own cleanup domain.

## Cleanup storage and admission

Each new obligation secures physical backing for mandatory cleanup before becoming live. Admission covers concrete size and alignment for activations, results, typed errors, reports, wait links, bounded callback outcomes, host-detachment acknowledgements and terminal infrastructure. Scalar bookkeeping credits alone cannot establish this guarantee.

The local allowance remains uniform for the concrete type throughout ownership. A current-value completion proof may omit a finalizer invocation but cannot release backing that later mutation could need. An action impossible for every value of the concrete type can lose its capacity requirement.

Moves and wrapping preserve existing allowances without duplicate admission. Aggregates secure only additional local obligations. Recursive ownership carries each child's local allowance. Rejection leaves existing owners and initialized inputs intact.

Backing may use caller storage, an enclosing frame, coallocation, separately owned regions or a pool. Erasure and recursion require storage satisfying actual lifetime and address requirements, not a separate heap allocation by default. Disjoint storage lifetimes can share a region. A retained activation, incident or result prevents premature reuse.

Activation transfers one reservation into one active owner. Rejection leaves the reservation intact. Source discharge releases unused allowance, while backing transferred into an activation or report remains owned through its last user. Epoch identity prevents address reuse from authorizing stale access or disposal.

Pooling is optional. Choose storage from lifetime, allocation, locality, reserved-memory and binary-size costs. Ordinary values require no public domain parameter or fallible rebinding.

Mandatory cleanup must operate when further allocation fails. Application finalizer allocations remain ordinary fallible operations, and the allowance does not cover unlimited explicit retries. Admission failure uses caller-owned storage before publication. An allocation-failure report may use an inline header and static message without allocating again.

## Execution and scheduling

### Frames and task ownership

The compiler owns source control flow, initialized-state tracking, direct-await composition, task cancellation broadcast and lifecycle resolution. Bray runtime code owns admission, activation, dispatch, waits, cancellation state, terminal publication and observation.

Each started task has one control record and one source resolution owner. The result transfers exactly once. Queue entries and wait registrations retain enough storage for dispatch and delivery without acquiring a second source resolution obligation. Duplicate task, run and scheduler descriptor graphs are removed.

Direct await remains within the current run and creates no additional task control record or scheduler registration. Known child frames can use enclosing storage. Stable backing remains live through the last activation, result or retained internal user.

Dispatch obtains execution authority through a valid state transition, executes source, then commits suspension or terminal state. Shared state requires synchronization. Thread-confined state needs no transition lock when confinement includes the absence of concurrent foreign wake access. Generated role adapters establish lane authority. Observing an identity or declaring a source record does not.

Generated lexical cleanup completes before terminal publication. Observation establishes completion visibility and transfers the result. Main remains on the main-thread lane. Exact-thread dependencies pin execution to their attachment, while other live states may migrate across compatible lanes.

### Execution limits and queues

Storage admission governs live backing. Runtime execution limits govern simultaneous running tasks. Excess ready work queues. Suspension releases an execution slot while preserving admitted storage and resolution duties, allowing eligible children and other tasks to run.

Explicit algorithm budgets remain separate from runtime execution quotas. An execution limit does not bound queued-task memory.

Use one scheduler implementation with compatible-lane ready queues, ordered deadlines and explicit worker owners. Public channel and budget FIFO contracts remain intact. Those contracts do not prescribe a globally locked FIFO scheduler. Queue representation, scheduling policy and worker counts follow consumer requirements and measured costs.

Worker and reactor resources initialize when reachable work needs them. Worker bootstrap establishes attachment, run and event services through low-level target operations. Detachment acknowledgements precede worker join and final release. Cleanup must not self-join or join workers while holding host locks.

### Wait registration and cancellation

Waits observe, register and recheck around notification commit. An ordinary noncopy registration owner withdraws an uncommitted registration. Committed delivery retains target storage until the callback returns. Callbacks and disposal run outside collection locks.

Coalesce repeated wakes and preserve a wake arriving while its task runs. Cancellation requests wake eligible work and remain observable through cleanup shielding. A request does not establish completion or authorize reclaiming storage. Task storage remains live until execution completes and all outstanding users resolve.

Waiter and queue links needed for mandatory cleanup are admitted before use. Explicit internal access cannot bypass registration ownership or state contracts. Public task handles retain their specified API without raw poll, wake or detach operations.

## Shutdown ownership and lifecycle

### Shutdown responsibility

The caller's owner holds the graceful shutdown obligation. The resident host owns product storage and admitted fallback cleanup from formation. Both use one terminal cleanup state and cannot perform cleanup twice. Formation secures the fallback's storage, reporting and execution requirements before publishing the caller owner.

An explicit shutdown attempt closes new entry and checks eligibility. External dependencies that prevent cleanup produce a retryable status while preserving the caller's unresolved owner. The attempt does not wait for those dependencies to disappear. Eligible terminal cleanup may perform asynchronous work where required.

This avoids a circular wait when the caller holds a report that it can release only after shutdown returns. Cleanup legality follows ordinary dependency and lifecycle rules. The API adds no language-wide deadlock guarantee.

### Normal finalization

Shutdown owners use the existing conditional execution and finalization rules. Available facts proving `executes(pure, total)` and a `unit` or `Ok(unit)` outcome discharge the completed whole-value finalizer step before optimization. Destruction, backing release and represented parts retain their obligations.

If the selected implicit finalizer remains possibly fallible on the available input domain, normal ownership end is rejected. The caller must complete shutdown, transfer the owner or explicitly adopt a valid fallback ownership form. A result or nullable wrapper preserves the contained obligation.

`total` permits `Result.Error`. A total shutdown attempt returning a retained status does not prove completion. Checked postconditions determine completion after successful and failed operations. A reported cleanup failure can therefore remain observable after terminal cleanup has completed.

### Abnormal finalization and fallback

Panic or cancellation cleanup attempts the same finalizer under the ordinary shielding and abandonment rules. A blocked or failed attempt produces a cleanup incident. Any unresolved caller graceful obligation is abandoned, and its control owner is destroyed synchronously.

If product cleanup remains incomplete, the resident host preserves its admitted fallback duty, product storage and provider dependencies. The destructor releases caller ownership without creating a new unresolved asynchronous obligation or allowing a borrow from the destroyed owner to escape.

Dependency release can make fallback cleanup eligible. Bound services dispatch it with the required affinity and remain live through reporting and completion. A binding unable to preserve continued execution cannot admit this fallback. Retaining storage alone is insufficient, especially for exact-thread cleanup. The service's own shutdown must preserve pending product ownership and avoid waiting on dependencies held by its caller.

Pending state uses the existing host record and secured terminal backing. No helper thread, periodic polling service or general deferred-work framework is required by the design.

Once terminal static cleanup begins, failed finalizers produce incidents and use the existing terminal abandonment rules. Destruction runs once, and independent eligible cleanup continues. Internal incidents are drained before their dependencies are released. Teardown does not return newly escaping provider-dependent incidents.

## Reports and diagnostics

Report and message state uses move-only Bray owners. Copyable ABI records encode trusted transfers without duplicating resource ownership. Generated adapters handle protected `PanicReport` representation. Full message snapshots, forwarding of the same report and iterative suppressed-entry disposal remain intact.

After child work quiesces, typed finalizer errors move into admitted payload backing. An incident retains concrete type, origin, ordinal, provider and attachment dependencies, and its synchronous concrete destructor. Destructor-generated reports use secured outgoing storage and preserve primary and suppressed ordering. This replaces linked Rust `Any`, downcasts, native unwinding and payload registries.

Disposal after producer-thread exit is valid only when the carried dependencies permit it. Mandatory reporting works without an async scheduler and cannot depend on new allocation during terminal handling.

Independent diagnostic preservation is an explicit optional library operation. Reuse existing report access or formatting where sufficient. Ordinary report handling does not automatically create an independent snapshot. A dedicated representation or conversion API requires a consumer need that existing operations cannot express.

## Platform, tests and component selection

### Native platform bindings

Target-gated Bray code supplies threads, TLS, allocation, atomic waits, clocks, loading, I/O and reactor mechanisms through explicit ABIs. Reuse existing low-level bindings below standard-library policy. Generate macro constants and target representations from pinned SDK inputs where needed.

If Bray cannot express a required ABI, resolve the compiler or target-support gap. A permanent custom non-Bray shim is outside the implementation goal. OS and system libraries remain external dependencies permitted by that goal.

### Test products

The test coordinator may remain Rust outside produced programs. Root boundaries, session protocol, capture, timeout cancellation, incident handling and platform support linked into a test program are Bray.

Test execution shares runtime host and admission mechanisms. Each product retains its specified shared statics. Command-wide serial exclusion remains a coordinator contract. Product cleanup failures belong to the product rather than an arbitrary test entry.

### Component selection and binary size

Products select only support required by reachable behavior and retained ABI contracts. Synchronous reporting and observation do not select an async scheduler. Libraries publish portable requirements without selecting a runtime implementation.

Optional diagnostic preservation, symbol lookup, history storage and dynamic-loading services must not enter programs that do not use them. Provider retention alone cannot retain diagnostic conversion code or storage. Unconditional registrations and descriptor references must not pull in optional components.

Lazy initialization reduces resource use but does not prove that code and data are absent from a binary. Component boundaries must support selective linking. Link maps and archive-member inspection establish implementation provenance and retention. Role symbol names alone cannot prove that linked support is Bray.

## Performance requirements and evidence

Storage, synchronization and compiler choices account for allocations, indirection, atomics, contention, frame size, code duplication and metadata. Specialization and erasure remain representation choices. Neither is a universal policy. Share equivalent generated operations where ABI and ownership allow it, and keep concrete operations where their layout or behavior differs.

Compiler checking and emission reuse valid lifecycle and dependency analysis. Summaries require correct invalidation when contracts, layout or dependencies change. Faster checking that moves excess work into optimization or linking does not satisfy the build-time goal.

The largest unresolved storage costs are mandatory cleanup backing and resources retained by whole-product dependencies. These requirements remain selected, but their concrete costs are unmeasured. The [cost analysis](bray-native-runtime-cost-analysis.md) identifies representation choices and other risks.

Measure representative native consumers when meaningful comparisons are possible. Record toolchain versions, targets, equivalent behavior and comparable build settings. Evidence includes latency or throughput, linked code and data, peak memory, and clean and incremental build time where relevant. Small synchronous programs and constrained targets are included alongside async and loaded-provider consumers.

Gate avoidable regressions when comparable paths exist. A necessary prerequisite may temporarily regress a measure when later work depends on it. Record the cause, enabling dependency and stage where evaluation or removal becomes possible. Architecture may finish before performance tuning. Cost reasoning remains part of architecture design throughout implementation.

If a selected mechanism prevents the destination requirements, reconsider the mechanism and any language rule that requires it. Passing correctness checks does not settle the cost question. A separate prototype or benchmark is not required for every decision.

## Documentation integration

The WIP remains separate until its changes are implemented and reviewed. Enduring architecture belongs in `docs/design/`. Observable source behavior belongs in `docs/language/`. The supporting implementation inventory is migration evidence, not a replacement specification.

| Document | Required integration |
| --- | --- |
| [Static storage](../language/declarations/static-storage-declarations.md) and [product shutdown](../language/async-and-concurrency/execution-roots-and-product-shutdown.md) | Replace mandatory external-root waiting with cleanup eligibility and explicit retained cleanup ownership. Preserve whole-product dependencies and terminal ordering. |
| [Low-level runtime](../language/async-and-concurrency/low-level-runtime.md) | Align thread-local cleanup with dependency order and structural ties. Remove conflicting reverse-registration wording. Preserve compiler-validated bootstrap and TLS contracts. |
| [Cleanup storage design](../design/cleanup-storage-and-reports.md) | Replace code-only residency and Rust activation ownership with whole-product retention and Bray ownership. Describe physical backing and resident fallback cleanup. |
| [Async runtime design](../design/async-runtime.md) and [composed execution](../design/composed-async-execution.md) | Integrate Bray control records, immutable descriptors, direct await, waits and shared-state synchronization. |
| [Platform services design](../design/io-and-platform-services.md) and [native interoperability](../design/foreign-and-platform-interoperability.md) | Align the implementation boundary with direct Bray ABI bindings and the temporal exception. |
| [BRA-501](https://linear.app/bray-lang/issue/BRA-501) fixtures | Replace code-only cleanup expectations with whole-product retention, retained shutdown ownership and optional independent diagnostics. |

Existing finalization and execution guarantees provide the owner lifecycle rules. Their implementation must support this design without adding a separate runtime-specific source lifecycle policy.

## Implementation sequence

The sequence follows dependencies between native consumers. Each stage may require several bounded PRs. Each change identifies the consumer, preserved ownership contracts, compiler gaps and linked Rust implementation it replaces.

1. Align lifetime, shutdown, bootstrap and static-order documentation and fixtures. Establish a typed role-bound Bray host owner and exact-thread TLS destructor entry with explicit startup storage and admitted fallback cleanup. Resolve recursive entry and circular admission before publication.
2. Replace the synchronous host. Produce a native executable with Bray formation, ordinary cleanup, reporting, attachment, statics and shutdown. Remove its linked Rust product, synchronous-root, attachment and report-rendering paths. Establish a complete executable without project-owned Rust before completing async migration.
3. Replace loaded-provider formation and admission. Cover two independent loads, statics, static archives, imported MIR, escaped reports and typed errors, producer-thread exit, last callback return and reload. Preserve independent diagnostic history while plugin resources release, and verify intentional provider retention. Exercise terminal disposal with allocation denied. Remove the corresponding Rust registry, retention and static-admission implementation.
4. Replace frame and task execution. Cover direct await without another task, erased and recursive activations, start, join, cancellation and abnormal payload cleanup. Preserve admitted backing, one terminal result owner, and separate cancellation broadcast and resolution. Remove Rust `NativeFrame`, `NativeRun`, task records and the `Any` bridge from this path.
5. Replace lanes, events, timers and workers. Cover wake races, cancellation withdrawal, compatible affinity, queued execution limits, main-thread progress and worker-thread statics. Remove linked Rust scheduler, cancellation, event and platform support as consumers migrate.
6. Replace test-host support and complete packaging. Implement child protocol, capture, serial behavior and timeouts in Bray. Remove Rust adapter partitioning, common support and runtime-platform artifacts once no produced consumer needs them. Audit all six native target artifacts and execute fixtures on available target hosts. Cross-compilation alone does not establish native behavior.

Each consumer includes rejection and terminal cases, implementation provenance, and appropriate execution or size evidence where comparisons are meaningful. Temporary Rust tests may remain outside produced programs as behavior references. Superseded production implementations are removed as consumers migrate.

## Remaining implementation details

- Concrete host, shutdown and fallback type and operation names.
- Existing library operations sufficient for independent diagnostic preservation.
- Backing sizes, alignment, coallocation and pooling suitable for actual live shapes.
- Handle-table, queue, synchronization and worker representations that satisfy sharing and affinity contracts.
- Compiler enforcement, descriptor emission and incremental analysis required by native consumers.
- Measured costs of cleanup backing, provider retention, emitted metadata and generated adapters.

These details are resolved during the implementation sequence. They do not authorize a generic reference-counted container framework or additional source concepts without a consumer requirement.

## Appendix A. Language foundations

This temporary appendix preserves the language facts used to design and implement the runtime. The linked language chapters are authoritative for current source behavior. Compiler support for the required contracts still needs native evidence.

Remove this appendix when the entire design is fully implemented in Bray. Before removal, integrate enduring architecture and source behavior into their official design and language documents. The appendix does not become permanent duplicate language documentation.

### Accessibility

- Bray has no `private` visibility. Declarations and fields are public by default. `internal` permits outside access through explicit acknowledgement such as `using internal`. Acknowledgement is lexical, applies to the named path and does not propagate through re-exports. It grants no ownership or trusted authority. [Visibility](../language/declarations/visibility-and-reachability.md).
- Runtime records are ordinary Bray types. Their contracts must hold even when safe source acknowledges internal access. Record shape and copied ordinary fields confer no witness guarantees on their own. Preserve guarantees through live owners and valid owned fields. Mutation invalidates affected conditions tied to storage, epochs and capabilities. [Witnesses](../language/contracts-and-trust/trusted-witness-values.md).
- `uses(...)` is implementation authority, not a caller precondition. Low-level helpers must establish required validity themselves or declare matching `requires(...)`. Wrappers and lifecycle operations preserve those obligations, including internal declarations. [Caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md), [Propagation](../language/contracts-and-trust/obligation-propagation.md).
- Protected representations such as `Task<T>` and `PanicReport` use a separate compiler mechanism. Ordinary internal types receive no such protection. Build metadata validates runtime roles, and declaration names or visibility cannot confer them. [Protected representation](../language/compiler-known-and-standard-library/protected-representation.md), [Roles](../language/async-and-concurrency/low-level-runtime.md).

### Ownership and lifecycle

- Owners carry initialized subvalues, dependencies, capabilities and lifecycle obligations. Moves transfer them. Copies require a copy contract. Shared borrows copy, while mutable borrows move or reborrow to preserve exclusive authority. [Ownership](../language/ownership-and-borrowing.md).
- `finalize` can fail or suspend and leaves the value initialized. `destruct` is synchronous and infallible and consumes the remaining parts. Scope capabilities use `enter`, `exit` and `with`. These contracts define Bray cleanup. [Lifecycle](../language/lifecycle.md).
- The specification uses inferred dependency metadata rather than source lifetime parameters or `Send` and `Sync` markers. That metadata carries storage, capability, lifecycle and independent-run transfer requirements through generic interfaces. Structured thread owners may retain borrows from the creating run. Unscoped runs may not. [Dependency contracts](../language/ownership-and-borrowing/dependency-contracts.md).
- Static paths provide shared access. They do not permit movement or direct mutation. Interior mutation needs a proven synchronization, atomic, single-assignment or scoped capability. Exact-thread borrows pin live tasks to that attachment. [Access paths](../language/ownership-and-borrowing/storage-and-access-paths.md).
- Dependency-anchored borrows do not extend source storage lifetime. Moves, fields and returns must preserve every root. Product statics cannot contain exact-thread dependencies. Raw code and data symbols carry provider roots, including attachment roots for TLS. [Boundaries](../language/ownership-and-borrowing/scope-exits-and-ownership-boundaries.md).
- Copy cannot allocate, fail, run user code or acquire an external resource. An owning provider lease must therefore be noncopyable and offer explicit duplication if needed. Copying a raw pointer or callable creates no new ownership. [Copy contracts](../language/types/copy-contracts.md).
- Callables have explicit state and no hidden captures. Their contracts preserve ABI, execution, ownership, caller obligations and guarantees. A trusted implementation does not make its public caller trusted. [Callables](../language/callables/callable-types-and-values.md), [Trust](../language/callables/trusted-functions.md).
- A partial move from a type with whole-value lifecycle requires proven reinitialization before whole-value use, lifecycle invocation or ownership end. Runtime code must account for this when taking fields out of owners. [Partial moves](../language/ownership-and-borrowing/partial-moves.md).
- Checked postconditions can establish completion after a finalizer error. Otherwise normal ownership end must handle or transfer the unresolved owner. Abnormal cleanup attempts graceful finalization, records failure, then resolves synchronous fallback and remaining parts. Host and static teardown have explicit terminal rules. [Finalization](../language/lifecycle/finalization.md), [Scope exits](../language/lifecycle/scope-exits-panics-and-cancellation.md).
- `with` exits before body-local destruction, on every exit from an entered body. The capability remains live during `exit`. Keep the last provider lease outside that body if local disposal can still invoke provider code. [With](../language/lifecycle/with-expressions.md).
- Construction must finish before it publishes stable observable identity. Partial construction unwinds only initialized parts. Replacement must restore an initialized destination before it propagates failure from cleaning up the old value. [Construction](../language/lifecycle/construction.md), [Replacement](../language/lifecycle/partial-values-and-replacement.md).
- `pure` excludes temporary mutation, allocation, synchronization, inactive-frame creation and cancellation observation. `total` requires normal termination, including cleanup, and permits `Result.Error`. Conditional `pure` and `total` guarantees with a proven `unit` or `Ok(unit)` outcome discharge a completed whole-value finalizer before optimization. Backing, result and child obligations remain. A total shutdown attempt returning a retained status does not prove shutdown completed. Use these existing rules for runtime owners. [Guarantees](../language/contracts-and-trust/execution-guarantees.md).
- The compiler checks guarantees in trusted source bodies too. `uses` grants an exact internal capability. Caller requirements and live witness guarantees remain separate, and mutation or transfer can invalidate them. A typed witness owner can establish validity once for downstream use. [Witnesses](../language/contracts-and-trust/trusted-witness-values.md), [Trust](../language/contracts-and-trust/trusted-implementation-capabilities.md).
- Trait views retain the concrete subject's lifecycle. Traits can require finalization or destruction, but implementations cannot supply alternative bodies. Static cleanup follows dependency and domain precedence. [Selection](../language/lifecycle/lifecycle-selection.md), [Ordering](../language/lifecycle/lifecycle-ordering.md).

### Statics, memory and execution

- Static initialization fully materializes a constant value. Runtime acquisition belongs in explicit initialization or `Once`. Static identity includes the owning product instance and exact TLS attachment. Addresses, hashes and link order cannot define it. [Statics](../language/declarations/static-storage-declarations.md).
- Each static domain has one owner. Cleanup follows an acyclic dependency order with deterministic structural ties within each domain and precedence across domains. The current specification orders entry closure, external-root resolution, exact-thread cleanup, product cleanup, incident draining and infrastructure release. The WIP shutdown design replaces mandatory external-root waiting with cleanup eligibility. Static-owned edges inside the teardown set must not make it wait on itself. [Static cleanup](../language/declarations/static-storage-declarations.md#entry-closure-and-product-cleanup).
- Static access becomes unavailable before cleanup. Cleanup retains authority over declared dependencies but cannot publish new roots, initialize new statics or reopen entry. Foreign entry attaches before access, and nested entry reuses the attachment. Outer detach waits for pinned work and cleans up on the same thread.
- `Uninit<T>` has the exact layout of `T` without creating a value or lifecycle obligation. Each initialized epoch must end exactly once. `RawBuffer` owns its committed prefix. Anchored safe views need live authority for validity, initialization, aliasing, synchronization, stable movement and pending finalization. [Anchored memory](../language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md).
- Raw pointers grant neither ordinary ownership nor automatic safe access. Foreign-produced pointers can still carry provider dependencies. `DynamicSymbol<T>` retains its library, and callable conversion preserves ABI and provider roots. Code and data addresses may have different representations. [Foreign symbols](../language/targets-layout-abi-and-raw-memory/foreign-data-and-symbols.md).
- Synchronous roots establish blocking, compute and main execution authority. Async main cooperates with the scheduler and stays on main. Started children use compatible lanes. Observing identity grants no execution authority. Root and lane contracts establish ambient execution predicates. [Requirements](../language/async-and-concurrency/execution-requirements.md).
- Generated source frames resolve lexical owners before publishing terminal state. The host observes that record and resolves its payload. Product shutdown follows run termination. Services remain live through payload and static cleanup. [Roots](../language/async-and-concurrency/execution-roots-and-product-shutdown.md).
- Reachable task starts, async entries and host async cleanup require async runtime selection. Synchronous reports and root observation do not. Libraries select no runtime implementation. Workers and lanes may initialize lazily. Algorithm budgets grant no hard capacity authority. [Selection](../language/async-and-concurrency/entrypoints-and-runtime.md).
- Compiler-validated role metadata checks declarations, ABI, target and contracts. Function names and implementation language grant no authority. Bootstrap TLS uses four target key operations and clears state before an exact-thread destructor callback. Panic cannot cross the ABI. Callback outcomes preserve one report owner, and the mandatory incident sink must work without a scheduler. [Low-level runtime](../language/async-and-concurrency/low-level-runtime.md).

### Frames, admission and concurrency

- `Future<T>` and `Task<T>` are protected compiler-managed owners. Async invocation captures state without executing or scheduling it. Direct await stays in the same run and needs no task control block. `start` secures stable independent storage. The source model needs neither a public pin type nor hidden callable captures. [Frames](../language/async-and-concurrency/async-representation-and-storage.md).
- Each new local obligation must secure mandatory cleanup capacity before becoming live. That capacity survives moves, wrapping and completion proofs until its last activation or outcome owner resolves. Children retain their own allowances, so aggregates admit only additional local needs. Recursive and erased obligations need adequate backing for each live owner. Application finalizer allocations remain fallible. [Cleanup capacity](../language/async-and-concurrency/async-representation-and-storage.md#cleanup-capacity).
- Failed start acquisition panics before publication and cleans the inactive frame. Successful start returns its sole owner. Invoking join or cancel constructs an inactive future. The public task API has no detach, raw poll, raw wake or completion test. A fallible completed payload prevents normal implicit task cleanup. Abnormal cleanup records and abandons it explicitly. [Start](../language/async-and-concurrency/starting-tasks.md), [Task ownership](../language/async-and-concurrency/task-handles-and-obligations.md).
- Structured cleanup first visits every initialized nested task obligation to broadcast cancellation. This phase performs no resume, wait or destruction. Lifecycle resolution follows. Erased and recursive frames need separate visitors and initialized-state tracking. [Scope cleanup](../language/async-and-concurrency/structured-task-scope-exit.md).
- Cancellation is cooperative and idempotent, wakes eligible work and remains shielded during cleanup. A request does not establish completion. Finalizer errors become typed owned incidents after child work quiesces. Cancellation has no payload, so terminal observation transfers incidents to the mandatory sink. A panic report owns its suppressed entries. [Cancellation](../language/async-and-concurrency/cancellation.md).
- Start and observed terminal cleanup establish visibility edges. Moving a task or requesting cancellation does not. Affinity identifies an exact attachment or typed lane class. The implementation can track dependencies per control state or conservatively pin the whole task. [Memory model](../language/async-and-concurrency/cross-run-memory-model.md), [Transfers](../language/async-and-concurrency/capability-transfer-across-run-boundaries.md).

### Library types and native boundaries

- `box[S] T` uses an ordinary storage policy. Borrowing must be pure and total, so acquire locks before storage projection. The storage owner can keep backing stable. Trait views retain concrete lifecycle and witness dependencies. They cannot downcast or dispatch generic members. [Storage and view forms](../language/types/type-forms.md#owned-indirection-type-form).
- `RawAllocation` is linear authority. Copying its raw representation cannot duplicate ownership. `RawBuffer` tracks initialized prefixes and relocates ownership without copying elements. Allocation failure is a catchable panic, while invalid layout produces a typed error. [Allocation](../language/targets-layout-abi-and-raw-memory/raw-allocation-and-buffers.md).
- `Atomic<T>` is protected and target-dependent. It never falls back to hidden locks. Each operation has a closed set of orders. Notification is advisory, so waits recheck state. Library contracts specify ticket fairness, writer-priority `RwLock` and ordered channel and budget waiters. Internal runtime queues need their own policy decisions. [Atomics](../language/async-and-concurrency/atomic-operation-contracts.md), [Library concurrency](../language/async-and-concurrency/standard-library-concurrency.md).
- Public thread owners are ordinary Bray types with explicit state and structured borrows. Public `start` can fail on creation or capacity. Internal worker entry must establish attachment and run state before using public thread facilities. Public thread cleanup cannot own async-finalized payloads, while an async bridge can. OS waits are cancellation points only when their wrapper declares them.
- Contracts depend on control flow. Mutation, epoch changes and finalization can invalidate witnesses. Mathematical contract arithmetic never wraps. Every wrapper, lifecycle operation and ABI boundary must prove or propagate trusted obligations. [Reasoning](../language/contracts-and-trust/contract-reasoning.md), [Obligations](../language/contracts-and-trust/obligation-propagation.md).
- The compiler defines default layout and call ABI. Native boundaries require explicit layout and `c` or `system` ABI. Protected frames and reports, borrows, boxes and views need compiler-managed ABI lowering rather than representation casts. Target-gated Bray code can implement OS bindings directly. [ABI](../language/targets-layout-abi-and-raw-memory/callable-abi.md), [FFI](../language/targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md).
- Panic snapshots mutable or borrowed messages before raising. Failed snapshot allocation raises an allocation panic and never truncates the message. Forwarding transfers the same report owner. Synchronous report destruction owns disposal of suppressed payloads. [Panic](../language/expressions/panic-expressions.md), [Run outcomes](../language/types/union-types.md).
- Runtime callback state is explicit. A retained registration must stop new calls and drain entered calls before freeing context. Foreign trampolines acquire product entry and attachment before access. They contain abnormal outcomes and return ABI-zero. [Callbacks](../language/targets-layout-abi-and-raw-memory/extern-declarations-and-ffi.md#foreign-callbacks).
- Libraries expose portable contracts and choose no scheduler. Test roots share one activation's statics. Serial exclusion spans the command, and cleanup failure belongs to the product. Runtime and platform modules can remain internal ordinary Bray modules. [Products](../language/modules-and-packages/package-products-and-source-graphs.md), [Tests](../language/modules-and-packages/test-products-and-entries.md).
- Specified collections provide owned ordered sequences and maps. Structural traversal can define cleanup order, while hash iteration cannot. I/O uses typed failures and explicit guards. Process context is an immutable startup snapshot. Time and entropy expose nondeterminism. The temporal provider remains the permitted exception. [Core data](../language/core-data-standard-library.md), [Platform services](../language/io-and-platform-services.md).

## Appendix B. Implementation inventory

The [implementation inventory](bray-native-runtime-consumer-inventory.md) records existing callers, required behavior, source locations, fixtures and Rust implementations to replace. It is supporting evidence for the migration and distinguishes required behavior from current representations.

## Appendix C. Cost analysis

The [cost analysis](bray-native-runtime-cost-analysis.md) records execution, binary-size, memory and compiler tradeoffs. It supports representation choices and later measurements. It does not establish achieved performance.
