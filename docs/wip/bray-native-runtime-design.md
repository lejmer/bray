# Bray-native runtime design proposal

Status: research proposal, not an accepted language change or implementation. Baseline `a65cc2f0` on `docs/bray-native-runtime-design-research`. Read [language notes](bray-runtime-language-notes.md) and [consumer notes](bray-runtime-consumer-notes.md) first. This records step 2 of the [delivery strategy](https://linear.app/bray-lang/document/bray-native-runtime-and-platform-delivery-plan-46623e2d308d); it does not replace that strategy or rewrite Linear yet.

## Recommendation

Build the runtime around Bray's ordinary ownership, dependency, lifecycle, storage and capability contracts. Product hosts, attachments, admission backing, activations, tasks and reporting policy should be Bray implementations. The compiler supplies checked cleanup plans, protected-representation adapters and immutable descriptors; explicit target ABI bindings supply OS mechanisms. Rust can remain in compiler/build/test coordination outside produced programs. Project-owned linked Rust and custom non-Bray runtime/platform shims are absent from the destination; the pinned temporal provider remains the stated exception.

Bray can express the required data structures today at the specification level. Existing source implements owners, `Uninit<T>`, anchored access, atomics, synchronization, native thread creation and TLS keys. Implementation gaps must be demonstrated by small native consumers, then fixed in compiler lowering or private contracts alongside those consumers. They do not justify a preliminary Rust runtime expansion.

The proposal retains necessary shared identity and synchronization. It does not translate the Rust object graph, `Arc`/`Weak` families, `Pin`, `Any`, descriptor reconstruction, poisoned-lock errors, duplicated registries or eager pools. A reference count at an erased/foreign lifetime boundary is justified by independently surviving owners; ordinary source borrowing alone cannot reclaim a provider behind a C caller.

## Ownership boundaries

Names below describe private concepts, not a proposed new public API or compiler-known source types.

| Owner/capability | Obligation it enforces | Expression in Bray |
| --- | --- | --- |
| Product host | One activation's static domains, service binding and terminal cleanup | Noncopy ordinary owner; explicit fallible/shielded terminal operation; synchronous backing disposal |
| Attachment | Exact native-thread identity, entered products and thread-static cleanup | Stable owned TLS state; exact-thread dependency; nested entry capability |
| Provider lease | Keep originating product/code/storage available through final callback return | Noncopy retained owner; explicit duplication; borrowed internal uses |
| Cleanup allowance | Capacity for the concrete owner's additional local mandatory work | Compiler-managed ownership association with concrete backing; moves preserve it |
| Activation | Stable active frame, initialized parts, continuation and result storage | Owned storage + checked descriptor; anchored views for each scoped access |
| Task | Sole source resolution duty; internal dispatch/wake reclamation duties | Protected source handle backed by one Bray control owner and limited retained internal references |
| Wait registration | Withdrawal before commit or ownership of one committed wake | Ordinary noncopy guard with explicit state and cancellation-safe disposal |
| Incident/report | Quiesced typed payload, backing, origin and disposal dependencies | Move-only owner + generated concrete disposal adapter; ordered owned chains |

Use direct typed borrows within a host/attachment/activation whenever its enclosing lifetime is provable. Shared storage is governed by short lock/atomic capabilities with anchored views. Runtime synchronization guards cannot survive arbitrary callbacks or suspensions. Ordinary library guards already show how these contracts work; no new public lifetime parameters or `Send`/`Sync` marker hierarchy is needed.

Erasure is limited to heterogeneous frames/payloads and native transfers. A descriptor contains exact layout, initialized-state operations, dependencies and callbacks; it is immutable and remains provider-rooted. Do not reconstruct compiler model objects, symbol strings or vectors at activation. Trait views are appropriate where their ordinary surface suffices; generated concrete descriptor operations are required where protected representation or heterogeneous disposal exceeds that surface.

## Host formation and provider lifetime

Immutable product metadata supplies the already-closed static order, demanded services and dependency templates. A separately mutable host instance contains only live state. Final-linked code, static archives and imported MIR share the consumer's host; separately loaded images get distinct activation identities even if their artifact bytes or descriptor address match.

Formation is transactional: validate the external binding and transitive provider requirements; secure host/static cleanup backing; construct the complete host; publish entry availability. Rollback owns every accepted input and provider dependency. Static materialization remains constant initialization; formation does not run a user's lazy initializer. Trusted generated inputs rely on compiler validation rather than repeating a second runtime planner; genuinely external bindings are checked at entry.

Keep one resident Bray service owner for the admitted host/admission domain. It lives outside unloadable images whose callbacks it protects. Independent schedulers borrow that domain but retain separate workers, budgets and execution authority. The domain is not an async runtime and a synchronous product can use it alone. An embedding host supplies an explicit binding; no cached image-local fallback may replace it.

Foreign handles require a small validated handle table with non-reused load/epoch identity and terminal invalidation. Internal paths borrow the stable host directly instead of resolving an integer repeatedly through process maps. A release or unload callback cannot execute in code it just unmapped: the enclosing resident caller retains the last lease until image code and all its local destruction have returned. Bray `with` exits before body-local destruction, so the final residency lease must be an enclosing owner or explicitly released after an inner scope has finished; an entry capability alone cannot supply that ordering.

**Choose the specification's product-retention rule.** An external owner capable of reaching provider code or storage delays that provider's static cleanup as well as unload. An escaped panic report therefore keeps its product available until the report is disposed. BRA-501's code-only cleanup expectation and `docs/design/cleanup-storage-and-reports.md` conflict with the normative static/root chapters; update that expectation when this design is accepted. Do not introduce a code-only exception without a separately checked contract proving all disposal/message operations independent of provider statics.

Tradeoff: retaining a small report can retain product resources for longer. The benefit is one dependency model for symbols, callbacks, reports and storage, with no unproven claim that cleanup code cannot touch statics. New source entry still closes immediately. Existing owners can complete disposal; closure must not reject the operations needed to release them.

Freeze a teardown set before closing admission. External roots block reached domains; static-owned edges inside the set determine consumer-before-provider order rather than blocking cleanup. Dynamically installed provider edges must pass cycle/admission checks before publication. Incident owners created by teardown are internal terminal work, drained before any dependency they use becomes unavailable; they cannot be treated as external roots that make teardown wait on itself. No new escaping roots are published during cleanup.

Within a domain, use the compiler's dependency-topological order and structural tie key. Across domains retain precedence and exact identities without inventing a universal completion order. The spec's bootstrap reverse-registration wording should be clarified to match these static rules. Exact-thread cleanup never moves to another worker.

## Admission: ownership first, pooling second

One compiler cleanup description drives legality, effects, symbolic storage needs and generated actions. Concrete specialization adds layout without changing action selection. Each new local obligation admits its **additional** allowance before publication; represented children already carry theirs. Construction failure leaves initialized inputs and their allowances owned. A current completion proof does not release capacity that later mutation may need again.

The allowance covers the actual mandatory path: activation/control storage, concrete finalizer frame and result, typed incident backing, report/wait links and bounded bridge outcomes. It also covers host detachment acknowledgements and other terminal infrastructure that must run when allocation is denied. Arbitrary allocations inside application finalizers remain fallible application operations. Unlimited explicit finalizer retries are not covered.

Use caller/enclosing storage where the live shape is closed; use separately owned backing for erased/recursive state and escaping payloads. Non-overlapping lifetimes may share backing, but a result or report still owning a region prevents reuse. Recursive ownership carries the child's local allowance; it does not reserve an infinitely expanded type graph. Every new dynamic obligation secures its own additional needs before becoming live.

Activation consumes a reserved region into one active owner; rejection leaves the reservation intact. Source discharge releases unused allowance and logical accounting, never backing that transferred into an activation or report. Reclaim physical storage only when its last typed/erased owner resolves. Epoch identity prevents address reuse from authorizing stale activation/disposal.

Pooling is an optional backing strategy inside this ownership contract. Start with typed owned allocations and fixed local regions where they fit; reuse existing slab behavior only where measured costs justify it. A process-wide scalar credit counter or emergency pool is insufficient proof of physical capacity for live frames and typed errors. Admission domain binding stays stable across ordinary moves; public values need no new domain parameter or fallible rebinding.

At admission failure, return the role's prepublication failure outcome through caller-owned storage. The allocation-failure report can use an inline header/static message without another allocation. Bootstrap leaves use explicit target allocation, raw layout and atomic operations whose terminal cleanup needs are proven empty or already supplied. Ordinary runtime source receives the same capacity/lifecycle checks as other Bray source; avoid a blanket exemption that makes the runtime's own allocation guarantee circular.

## Runs, frames and scheduling

Generated code owns source control flow, initialized-state tracking, two-phase task broadcast/resolution, direct-await composition and protected frame operations. Bray runtime code owns admission, stable activation backing, dispatch, waits, cancellation state, terminal publication, observation and infrastructure. A generated ABI adapter establishes role-provided execution authority; source cannot assert itself onto an execution lane.

A task has one control record rather than parallel task/run/scheduler descriptor graphs. Short synchronization owns transitions among ready, running, waiting and terminal. Dispatch borrows/detaches execution authority under the transition lock, invokes source outside it, then commits suspension/terminal state. Queue entries retain only enough storage authority for safe dispatch, never a second source resolution owner.

Cancellation request and cleanup shielding are explicit scoped state. Request wakes eligible suspended work but does not permit storage reclamation. Publication of the final terminal record occurs after generated lexical cleanup; observation supplies the completion visibility edge and transfers one result. Host payload/static cleanup follows it. Main root stays on main; origin-thread dependencies pin to the exact attachment; other state may migrate between compatible lanes.

Waits use observe/register/recheck with a commit point. Uncommitted cancellation withdraws the registration; a committed notification retains its target until callback completion. Coalesce repeated wakes and preserve a wake arriving while the task runs. Reserve any mandatory waiter/queue link before it can be needed; callbacks and disposal run outside collection locks. Safe source task handles still expose no raw wake/poll/detach API.

Use one initial scheduler implementation with per-compatible-lane ready queues, ordered deadlines and explicit worker owners. Public channel/budget FIFO rules remain intact; do not inherit every internal Rust queue/worker heuristic as semantics. Separate task storage failure from the hard quota on simultaneous execution: excess ready tasks queue. Workers and reactor resources initialize only when reachable work needs them, and worker creation/limits remain explicit policy.

Bootstrap worker entry uses target thread/TLS operations below public `std.thread`, avoiding a cycle through run/event services it is establishing. For synchronous roots the same host/attachment/report owners work with no scheduler. Shutdown preserves allocator, sink, lanes, loaders and providers through static cleanup; attachment acknowledgements, worker join and final release happen in that order without self-join or holding host locks.

## Reports, platform and test products

Retain the existing owned report/message and iterative suppression behavior, but represent ownership through move-only Bray owners internally. Copyable ABI transport records are trusted encodings, not duplicate resource owners. Generate adapters for protected `PanicReport` rather than exposing its fields to ordinary source. Preserve full message snapshots and same-owner forwarding.

A typed finalizer error moves into already admitted payload backing after quiescing child runs. Its incident retains concrete type, origin, ordinal and provider/attachment dependencies, plus synchronous concrete destruction. No Rust `Any`, `Box<dyn ...>`, native unwinding or Rust payload registry remains linked. Destructor-generated reports use secured outgoing slots and preserve primary/suppressed ordering. Report disposal can happen after producer-thread exit only when its dependency contract permits it.

Use Bray target-gated bindings for thread/TLS, allocation, atomic wait, clocks, loader, I/O and reactor mechanisms. Share existing internal target leaves below standard-library policy; public thread/run/I/O wrappers must not bootstrap their own prerequisites. Macro constants and target representations can be generated from pinned SDK inputs; they do not require custom runtime C/Rust shims. A genuinely unrepresentable ABI is a compiler/target capability gap to solve, not an open-ended shim allowance.

The test coordinator may remain host-side Rust. Everything linked into a test program—root boundaries, session protocol, capture, timeout cancellation and incident disposal—must be Bray. Test execution uses the same host/admission core. Command-wide serial exclusion and shared product statics remain explicit coordinator/product contracts.

## Concrete proof sequence and removals

These are bounded acceptance cuts, not a claim that each entire row fits one PR. Split a row only while preserving its concrete consumer and deletion target.

1. **Settle lifetime semantics and bootstrap role lowering.** Preserve spec-level product retention; align conflicting design text/fixtures; prove a typed role-bound Bray owner and TLS destructor entry with no recursive trampoline/admission dependency. This is the first implementation prerequisite, not another Rust conformance expansion.
2. **Replace the synchronous host boundary.** A produced executable exercises resident product formation, ordinary owner cleanup, panic/failure reporting, exact-thread statics and shutdown through Bray, including the mandatory backing those owners need. Delete linked Rust product/sync-root/attachment/report-rendering paths for it; prove this synchronous consumer links no project-owned Rust through map/member provenance. Do not wait for full async scheduler migration to establish this cut.
3. **Replace formation/admission across a real loaded provider.** Two independent loads, static/archive/imported-MIR contributions, escaped report/typed error, last callback return and reload; run terminal disposal with allocation denied. Remove Rust product registry/retention/static-admission consumers in that path. Extend compiler metadata for physical cleanup backing as required.
4. **Replace frame/task execution.** Known direct await adds no task; erased/recursive activations, start/join/cancel and abnormal payload cleanup use secured backing, one terminal owner and separate broadcast/resolution. Remove Rust `NativeFrame`/`NativeRun`/TCB/Any bridge from the selected path.
5. **Replace lane/event/timer/worker services.** Prove lost-wake races, cancellation withdrawal, affinity, queued execution limits, main-thread progress, and worker-thread static cleanup. Remove linked Rust scheduler/cancellation/event/platform consumers as each complete service switches.
6. **Replace test-host policy and finish packaging.** Protocol/capture/serial/timeout behavior uses Bray; delete adapter partitioning, Rust common support and runtime platform artifacts once no produced consumer requires them. Audit all six target artifacts and execute native fixtures on available target hosts; do not equate cross-compilation with cross-target runtime proof.

Every implementation cut should name its real consumer, owner invariants, rejection/terminal cases, eliminated linked Rust functions/objects, and bounded compiler gaps. Compatibility facades or parallel production runtimes are not the destination. Existing Rust tests can remain temporary behavioral oracles outside produced programs while Bray fixtures replace their consumer coverage.

## Decisions and limits

- Recommend whole-product retention for external report/code owners; this changes BRA-501 fixture expectations and can increase retention duration. It needs architectural acceptance before code/spec-design alignment.
- Recommend preserving one shared admission service domain independently of scheduler identity; multiple loaded products require explicit binding rather than per-image fallback.
- Exact backing sizes, handle-table representation, lock choice and worker counts remain implementation decisions constrained by consumers; no generic reference-counted container framework is proposed.
- Source research supports feasibility, not a completed native prototype. Missing compiler support must remain visible in the proof sequence. No tests, performance measurements, Linear edits or runtime code changes were performed in this research task.
