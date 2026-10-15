# Runtime stack implementation and completion plan

Current execution is recorded in the [Linear execution guide](runtime-stack-linear-execution.md).
The user accepted the replacement groundwork and requested issue preparation on 2026-10-12.
Pause and authorization statements in the dated assessment below describe that earlier task, not a new implementation gate.

## Superseded design proposal

The user rejected this proposal as insufficiently complete on 2026-10-12. Its acquisition representation and
implementation order are provisional. The [rule coverage and replacement plan](runtime-stack-rule-coverage.md)
supersedes its design and sequencing decisions. Keep this document for earlier issue, cost and ancestry evidence.
Implementation remains paused pending discussion of the replacement plan.

Historical proposal from 2026-10-12, superseded as described above. It followed the
[architectural assessment](stack-architecture-assessment.md). It prematurely treated the assessment's design choices as resolved by deduction
from the existing contracts. Implementation remains paused until discussion of the replacement plan.

## Outcome and scope

Finish the current authority/bootstrap stack as one reviewed integration into develop, then complete the remaining
native runtime and lifecycle consumers through the existing open issues. The destination includes correct source and
imported behavior, zero project-owned non-Bray runtime implementation in produced programs apart from the documented
temporal provider exception, and the execution, footprint and complete-build requirements below.

Finishing the current stack is an intermediate landing point. It does not complete the whole runtime migration.
Conversely, later workers, timers and test-host delivery must not keep five old PR branches in active circulation.
Every remaining obligation below has an open owner. No closed, completed, canceled or duplicate issue is assigned work
or proposed for reopening. No new language feature is proposed.

This plan is based on source inspection, the language rules and live issue/PR metadata. It contains no fresh compiler,
native or benchmark results. Earlier experiments are evidence about implementation gaps, not the basis for deciding
language semantics. No additional experiment was necessary for the deductions below.

## Governing contracts

| Authority | Consequence for implementation |
| --- | --- |
| [Axioms](../axioms/core-axiom-set.md), especially 1-6, 9, 11 and 12 | Explicit mutation and aliases, one ownership story, deterministic cleanup, a small language core, bounded local trust and structured execution. |
| [Compiler architecture](../design/compiler-architecture.md) and [checker](../design/checker.md) | Immutable demand-driven results. One composite domain for mutually dependent storage, capability and lifecycle facts. No late recovery pass that rechecks an established contract. |
| [Witnesses](../language/contracts-and-trust/trusted-witness-values.md), [reasoning](../language/contracts-and-trust/contract-reasoning.md) and [dependencies](../language/ownership-and-borrowing/dependency-contracts.md) | Provenance, observation dependencies, physical references and lifetime requirements remain distinct. Existing inferred metadata carries them across calls and interfaces. |
| [Product fields](../language/types/product-types.md), [assignment](../language/expressions/assignment-expressions.md) and [partial moves](../language/ownership-and-borrowing/partial-moves.md) | Mutable bindings cannot rewrite immutable fields. Whole-value lifecycle prevents an extracted component from escaping an unrepaired owner. |
| [Uninit and anchored borrows](../language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md) | Keep existing protected storage. Initialization, alias authority, epoch, actual referent and lifetime anchor all need their own evidence. |
| [Execution guarantees](../language/contracts-and-trust/execution-guarantees.md) and [finalization](../language/lifecycle/finalization.md) | Non-unwinding, totality, purity and completed cleanup are different properties. No use of normal postconditions on panic or cancellation edges. |
| [Cleanup design](../design/cleanup-storage-and-reports.md), [lowering](../design/lowering.md) and [codegen](../design/codegen.md) | Select once in checking. Expand execution and symbolic backing together in lowering. Specialize layout afterward. LLVM consumes completed facts. |
| [Interfaces](../design/compiled-package-interfaces.md) | Export compact checked relationships with structural identities, exact dependencies and provenance. No source rechecking at each caller. |
| [Runtime design](../wip/bray-native-runtime-design.md) and [cost analysis](../wip/bray-native-runtime-cost-analysis.md) | Reuse the host, admission service, report pool and task/activation owners. Switch consumers and remove replaced policy at each migration. |

Language chapters take precedence over the WIP and historical handoffs. Correct stale wording alongside its active
consumer implementation, rather than using a documentation disagreement to relax the language or reopen a closed issue.
Plans and verification history stay in handoffs. Goal-state design chapters should describe the resulting architecture.

## Selected compiler design

### One owner for each fact

Keep the existing representations and their owners. Extend their meaning only where a consumer needs an absent fact.

| Result | Owning component | Published meaning |
| --- | --- | --- |
| Selected calls, conversions and lifecycle targets | Existing selection and representation analysis in bray-checker | Exact selected operation and substitutions, including scoped enter/exit. |
| Storage/access plan | Existing StoragePlan and checker planner | Occurrence paths, storage roots, reference operations and potential cleanup sites. Planning does not decide flow legality. |
| Composite storage flow | Existing checker body stage | Initialization, moves, live capabilities, trusted provenance, reference targets, mutation invalidation and applicable lifecycle transitions at each reachable operation. |
| Callable dependency and behavior summaries | Checker inference, coordinated by bray-compilation | Formal lifetime dependencies, reference relations, observable effects and normal/abnormal outcomes established by the owning analysis. |
| Portable contracts | bray-symbols meaning and bray-package-interface encoding | Only facts needed by callers, export, specialization and invalidation. No solver state or local graph IDs. |
| Selected cleanup and symbolic execution | Checking decisions consumed by bray-lowering | One selected lifecycle description, then one execution expansion with symbolic local backing requirements. |
| Concrete realization | Existing specialization/layout/codegen preparation | Target size, alignment, multiplicity and executable descriptors. No repeated semantic selection. |
| LLVM and runtime | Existing backend and ordinary Bray owners | Translate or execute the decided operations. Validate real foreign identities at their boundary, not trusted internal compiler facts again. |

The semantic unit remains the body or other existing declaration-owned unit. A type query must not force native layout,
whole-product reachability or unrelated body diagnostics. Correlated narrow accessors can share the body's immutable
owning result without introducing another checked tree.

### References, effects and witness invalidation

Use a physical reference relation separate from a lifetime dependency. For example, borrow_from(owner, pointer) has a
lifetime/capability dependency on owner and reaches the storage described by pointer. Neither implies the other.

Reference transfer belongs in the composite flow. Address formation, copies, moves, assignment, projection, nullable
presence and selected call results update it in evaluation order. A join unions possible targets for mutation safety and
retains an exact target only when every relevant incoming path agrees. An absent branch contributes no live payload.
Unknown targets remain unknown. Distinct occurrence IDs are never evidence of disjoint memory.

Reuse formal dependency paths and the existing Borrow/Address distinction. Stop inferring physical identity from a
single lifetime anchor in storage planning. Replace the independent syntax walk for exact returned origins with a
projection of the owning reference analysis as that analysis takes over. Preserve the current portable relation and
codec work where its meaning remains correct.

A condition needs two separate views of its dependencies:

- What it can observe, so a write can make its truth unavailable.
- What must stay live and valid, so destruction, movement, epoch replacement or capability expiry can revoke its use.

These are meanings within existing proof/dependency data, not a request for a new universal dependency framework.
An empty acquisition witness can depend on an enclosing lifetime without observing every byte protected by that owner.

Classify compiler-known operations once. Distinguish value-content writes, initialization transitions, moves, allocation
retirement, borrow-slot reassignment and pure address formation. Apply the classification to proven targets. Keep
state-dependent extent and initialization reasoning in the flow domain that knows those facts.

Ordinary checked assignment of a live T leaves its destination initialized on normal completion. It can invalidate
predicates about T's value without destroying the initialization of an enclosing Uninit<State<T>>. Taking T out,
destroying it or retiring its epoch has different effects. A raw noncopy read is not treated as a harmless scalar load.
A potentially failing replacement has the actual intermediate state on its exceptional edge.

For ordinary calls, infer conservative effects over formal reachable storage and relevant static roots. Include callback
and selected lifecycle effects. Distinguish normal from abnormal continuation where the distinction affects cleanup.
An opaque foreign or indirect operation without sufficient evidence retains conservative effects. A shared borrow alone
does not prove purity, because it may reach interior-mutable state.

Derive these summaries through the existing behavior/dependency owners. Do not add a later preservation validator,
per-library exceptions or a general source effect-annotation language. A declared normal ensures clause establishes no
abnormal-exit fact. For the RwLock overflow case, the proof must use operations actually reached before the panic and
their cleanup. Later operations cannot pollute that edge, and the real overflow failure remains.

### One composite forward transition, with explicit fixed points

Replace the sequence of preliminary trusted-memory flow, storage flow and final trusted lifecycle flow with one
cooperating forward domain over the existing shared graph. Keep ordinary propositions and trusted evidence distinct
inside it. Keep backward liveness separate because it computes a different fact.

Prepare selected operations, formal call mappings, literals, reference paths and structural lifecycle choices once.
At each operation the composite domain:
1. Reads the current initialization, capability, reference and proof state.
2. Checks the operation's requirements from that state.
3. Applies the operation's actual normal and abnormal transfers.
4. Resolves applicable selected cleanup from the same state, including partial availability.
5. Establishes permitted postconditions and consumes or transfers witness ownership.
6. Projects changed facts to successor states.

These are responsibilities of one transfer implementation, not six whole-body passes. Use the existing cleanup
transition for replacement and scope exit. Diagnostic publication may replay converged entry states through that
transfer to identify errors. It must not solve the body again or maintain a different lifecycle interpretation.

Structural lifecycle selection can be prepared before availability is known. Conditional invocation, partial
initialization and current completion are resolved by flow. This breaks the current need to first solve a body with
empty cleanup and later solve it again with selected cleanup.

Body-local mutually dependent facts converge together. Interprocedural summaries use the existing query/dependency
coordination and explicit strongly connected components where recursion requires it. Collect selected edges once.
Keep provisional states task-local. Do not implement recursion as queries for each other's supposedly completed bodies.

May-effects accumulate conservatively. Must-identities and guarantees retain only supported common facts.
No recursive cycle may establish totality or authority merely by assuming its own result. Preserve the specified
well-founded totality proof. Known declared contracts and validated foreign/role assertions retain their distinct
provenance and certification obligations.

A body may legitimately need analysis under different declared guard domains. Reuse identical preparation and identical
input results, but do not merge different assumptions or promise one solve for all contracts. Remove duplicate solves
for the same inputs. Early unwind requirements must consume early-available selected structure or the same owning
cooperation, never query a late body result in a dependency cycle.

Use finite roots/projections and the existing bounded proof model. Intern repeated terms and index conditions by affected
roots so one write does not scan all proofs. Requeue only changed dependents. Recursive path growth needs an explicit
conservative summary, not unbounded unfolding. Do not claim general linear complexity: alias joins can require sets and
fixed points. Avoid path enumeration and per-caller transitive graph walks.

### One lifecycle description and one execution expansion

Checking selects lifecycle targets and owns legality, completion and symbolic effects. It exposes structural choices
for flow and execution-property reasoning without generating machine frames.

Lowering expands those selected actions into execution and symbolic additional storage together, following the existing
cleanup design. Specialization substitutes and realizes concrete layout. Consumers of existing selected parts must stop
recursively selecting the same lifecycle again. This is reuse across representations, not moving lowering into checking.

Preserve separate cancellation broadcast and lifecycle resolution. Preserve exit-before-body-local-destruction where
specified, whole-value versus represented-part obligations, normal versus abnormal outcomes, and separate typed error
destruction and backing release. Sharing transitions does not mean making these events interchangeable.

Uniform local capacity is a type-level requirement. A current-value completion witness can skip an invocation without
releasing capacity needed after later mutation. Retain the approved unconditional-total capacity reduction only for the
failure contribution it actually eliminates on the valid domain. It does not remove child obligations, fallible Error
results, backing release or unrelated authority checks.

### Separate compilation and downstream trust

Export formal reference relations, presence guards where required, effect facts and proof provenance once. Substitute
them for source and imported callers through the same path. Generic calls retain open dependencies until instantiation.
A reference result's exact target does not itself authorize dereferencing it.

Use existing predicate-valid const-call contracts to describe the optional witness observation below. Publish checked
presence and result-reference relationships. At a present branch, instantiate those relationships and the existing
postconditions. Do not repeatedly symbolically execute helper bodies or infer equality merely from matching function
names. No general theorem prover or new const interpreter is needed.

Validate required schema and external records when decoding. Bump semantic/schema identities when their meaning changes.
Invalidate callers for changed contracts, selected witnesses or relevant effects, and concrete realization for changed
layout/target inputs. Downstream stages consume established facts and assert internal invariants. Preserve specific
structured diagnostics at the phase that owns user errors.

## Acquisition and initialization with existing language features

### Chosen representation

Use an ordinary empty witness field and an immutable owner reference inside each acquisition guard. Give the guard
whole-value lifecycle behavior. Its acquisition predicate observes the witness field, not the guard's mutable payload
or everything reachable through its lock reference. The predicate supplies semantic authority, not a runtime Boolean.

This is a library representation using existing witnesses, field mutability, dependencies and lifecycle. It is not a
new compiler-protected type. The compiler must not recognize the guard or token by source name. Use submodules and the
repository's naming rules for ordinary implementation types.

The acquisition producer performs the real synchronization transition before publishing the witness-bearing guard.
There is no safe standalone witness factory. Its declaration and every trusted helper are part of the authority audit.
The witness is not explicitly copyable with a contract that duplicates acquisition.

The lock reference and witness field stay immutable even in a mutable guard binding. The guard may provide a mutable
payload borrow whose capability lifetime is anchored in the guard, while its physical target is the lock's payload.
Exclusive dependent borrowing prevents unlocking while that borrow is live.

Use distinct read/write acquisition contracts for RwLock, while sharing reference/effect machinery. Condition waiting
releases and reacquires through real protocol transitions. It cannot retain exclusive payload authority while waiting.
The wait operation's normal result restores the acquisition, and abnormal cleanup follows its declared/native behavior.

### Deductive safety argument

Assume correct implementations of the compiler rules and the explicitly trusted acquisition/release operations.
The plan repairs enforcement of those assumptions. It does not claim a proof of today's implementation.

| Attempt or transition | Why it does or does not confer authority |
| --- | --- |
| Construct an empty token and a guard aggregate | Ordinary construction supplies fields, not the trusted held condition. Access and lifecycle requirements remain unsatisfied. |
| Copy the owner reference or token-shaped bytes | Ordinary field copies do not copy witness provenance. Witness copying needs an explicit valid copy contract. |
| Move the complete guard | Transfers the same immutable pairing and its dependencies. The old path loses authority. |
| Move only the witness into a different guard | Separates a lifecycle-bearing owner. Safe code must repair it before whole use or ownership end. Assignment cannot repair its immutable field, and fabrication cannot supply the old authority. A witness-separating partial move is independently forbidden. |
| Replace the original guard to evade that obligation | Replacement must resolve the old owner first. It cannot destroy an incomplete guard as though it were complete or silently drop acquisition. |
| Rebind the lock field through a mutable guard | The field declaration is immutable. Mutable local binding does not override it. |
| Write through get's payload borrow | Changes the exact payload target, not the witness field or immutable owner-reference slot. The live acquisition remains. Payload predicates are still invalidated as appropriate. |
| Destroy/reinitialize the lock or replace an owner epoch | Live borrow/dependency rules forbid the conflicting operation or invalidate the dependent witness. Address equality cannot restore authority. |
| Consume/unlock/drop the guard | Applies release once, then ends the witness lifetime. A later path cannot reuse it. |
| Use an explicit trusted fabrication | Outside the safe-code guarantee. Trust remains explicit and local. Internal-use acknowledgement alone is not such a boundary. |

Zero-sized values still have distinct semantic ownership identities. Never identify witnesses by numerical addresses,
which may coincide for empty fields. The physical absence of bytes does not make an empty fabricated value authoritative. Equal field contents cannot substitute one borrowed witness identity for another or transfer trusted provenance.

For try-lock, the conceptual postcondition is: absent result owns no acquisition; present result carries the exact
witness in that present guard. Express it through existing nullable observation and a predicate-valid const accessor
where needed. Check/publish the accessor's presence relationship and exact returned reference. Pattern matching then
transfers the contained witness normally. This is ordinary path, equality and dependency reasoning already required by
the specification, not a new preservation clause.

### Protocol storage and helper boundaries

An anti-forgery design must cover safe callers of every internally accessible helper, not only the public lock method.
A broad pointer helper must not let a safe caller obtain protocol-state write authority merely from owning a guard or
knowing that storage is initialized. Nor should a shared view expose control atomics for arbitrary safe stores.

Keep protocol state in existing Uninit storage. Expose only the payload views authorized by the current acquisition.
Implement complete control-state transitions inside trusted functions, with their exact uses and caller requirements.
Address-only helpers can share mechanics without exporting initialization/alias guarantees for arbitrary protocol
mutation. Helpers that grant unrestricted raw access require an explicit trusted obligation unavailable from mere
internal acknowledgement. Do not publish a reusable safe control-state borrow.

For Once/Lazy, initialized storage alone is insufficient evidence of a valid protocol. A public State aggregate could
claim ready while its payload is absent. Establish a whole-instance protocol witness through the real constructor,
paired with immutable protected inline storage and whole-value lifecycle. Use a separate attempt witness for the
initializer claim. Keep both fields immutable to safe field assignment and partial extraction.

Interior state transitions and terminal destruction use existing bounded trusted raw/anchored operations. Immutability
does not authorize arbitrary writes: those operations still need the exact storage, epoch, alias and lifecycle proof.
No new protected storage type, heap box per Once or mutable public active flag substitutes for that proof.

The Once constructor can establish a fresh empty protocol in constant initialization when all its operations are
constant-valid. It acquires no runtime service and copies no existing acquisition. Static initialization must call that
checked constructor rather than imitate a runtime witness with an aggregate literal.

Publish ready only after the payload write commits. A failed or recursively entered initializer rolls back accepted
state once. Read access requires a live valid instance and the real publication transition. An attempt's release or
publication consumes its acquisition. Payload observation must not expose writable protocol state. Apply the same
audit to Lazy delegation, bounded channels and thread wrappers that consume these primitives.

This adds library witness fields only where they enforce an actual protocol invariant. Do not add a token to every
ordinary value. The host's resident storage anchor and live lease remain distinct where they serve different lifetimes.

### Why this does not require a language extension

The needed rules already exist: trusted provenance, immutable fields, whole-value lifecycle, dependency-anchored
borrowing, nullable observation, const contracts, and separately compiled inferred dependencies. The failures in the
current investigation concern enforcing and preserving those rules consistently.

Do not introduce private fields, a new initialized-storage type, public lifetime parameters, preservation syntax, runtime
witness tables or a runtime lock identity comparison. A genuinely missing expression rule must be demonstrated against
the completed semantic model before proposing a language change. A current diagnostic or a difficult implementation is
not that demonstration.

### Cost argument

The empty witness needs semantic identity in checking, not a unique runtime address, allocation, atomic, lookup or branch.
Erase contract-only observation and zero-byte movement before emitting native work. Keep the existing owner reference
and required synchronization. Existing runtime flags needed for actual protocol transitions are not erased proof state.

Default layout remains a compiler/target decision, so unchanged production layout is an implementation gate rather than
a promise derived from a synthetic guard test. Empty witness machinery must not emit out-of-line helpers or mandatory
descriptors. Prefer a nongeneric token when it carries no representation-dependent state.

Compiler work adds a bounded observation and reference relation, while removing repeated global invalidation and
re-analysis. That is a reasoned cost expectation, not a measured speedup. No unrestricted symbolic execution, all-path
enumeration or per-call body loading is part of this design.

## What to do with the current stack code

Preserve the combined tree before any restructuring. Keep useful tests even when the tested implementation is replaced.
Do not replay the stack or an abandoned donor branch.

| Existing work | Disposition |
| --- | --- |
| BRA-609 caller obligations, witness provenance, imported contracts and diagnostics | Retain. Move state transfer into the composite domain and remove competing authority flow. |
| BRA-619 selected enter/exit occurrences, receivers, activity guards and pending-outcome lowering | Retain. Complete native typed incident transfer through shared admission/report ownership. |
| BRA-620 memory/foreign contracts and current Uninit-based synchronization | Retain valid contracts. Replace whole-guard opaque acquisition with the chosen witness representation and audit protocol helper authority. |
| BRA-621 slab/record ownership, message backing and callback identity | Retain the existing pool. Repair its stale SpinLock aggregate via the real constant-valid constructor. |
| BRA-553 host owner, exact-thread identity, TLS callback and rollback | Retain behavior and evidence. Reverify against consolidated flow. Do not repeat host migration or collapse anchor and lease. |
| Shared cleanup transition and control-flow/reachability work | Retain and incorporate. Remove separate cleanup selection/preparation and the preliminary/final proof split. |
| Exact reference metadata, Borrow/Address classification, codecs, substitution and hashing | Retain the distinction and portable plumbing. Replace the parallel source-origin proof with the owning flow projection. |
| Checked non-unwind versus total separation | Retain. Share operation edges and preparation, with distinct certification rules. |
| Liveness ordering, diagnostic preservation, interface normalization and bounded inliner fixes | Retain demonstrated corrections. Review their interactions and ownership rather than reverting them because they crossed issue files. |
| Synthetic permit tests and saved failed probes | Keep useful regressions, then cover actual declarations/imports/native consumers. Do not substitute synthetic success for production completion. |
| Remaining Rust runtime components | Keep only for explicitly unfinished consumers listed below. Switch real callers and delete the replaced owner/policy at each migration, not at an indefinite final cleanup. |
| Numeric fixture source-ID subsets and count-only coverage | Replace local subsets under BRA-620. Use BRA-552 for the broader independently verified native test inventory. |
| Handoffs and multiple proposed sequences | Keep dated evidence. This plan becomes the proposed execution sequence, linked from stack-completion.md. |

Consolidation is complete only when superseded mechanisms are removed. More files, a forwarding wrapper or a new cache
over duplicated analysis do not count as simplification. Report production and test growth separately. Every new
production representation needs an invariant, a current/planned consumer and a stated cost.

## Implementation sequence and deliverables

### C1. Preserve the baseline and prepare shared semantic inputs

Owner: BRA-620, with BRA-609's authority acceptance retained.

Inventory every changed/untracked file, preserve the complete repair as a recoverable checkpoint, and record all live PR
heads. Keep the original issue branches intact. Build a behavior/owner/removal matrix from the affected code.

Prepare the existing graph, selected occurrences, structural lifecycle targets and formal contract mappings once.
Introduce no parallel long-lived plan. Establish finite reference/effect domains and their merge rules before editing
their consumers. Add focused rule and summary tests during implementation, not exploratory parser/Bray probes now.

Exit: the ownership map and transfer inputs are explicit, imported meanings are defined, and every proposed replacement
has an identified existing mechanism to remove.

### C2. Consolidate storage, proof and cleanup flow

Owner: BRA-620.

Integrate physical reference transfer, initialization and trusted evidence into the existing composite flow. Replace
global invalidation for proven local targets, share closed operation classification and model abnormal edges precisely.
Move final dependency/reference publication to that owning result.

Use prepared lifecycle structure to apply cleanup in the same flow. Remove the early empty-cleanup authority solve,
late independent authority solve and repeated structural property expansion. Preserve required distinct guard domains.
Resolve recursive summaries within their explicit owning cooperation. Never publish speculative must-facts.

Exit: valid payload mutation, guard destruction, owner destruction, RwLock failure cleanup and imported nullable
propagation follow the same transitions. Forged, copied, stale and opaque-dependent authority still rejects.
Downstream consumers use published facts without semantic reconstruction.

### C3. Complete actual synchronization and bootstrap integration

Owner: BRA-620. Preserve BRA-621 and BRA-553 acceptance in the combined tree.

Apply the acquisition/protocol witness design to SpinLock, Mutex, read/write RwLock, ConditionVariable and Once/Lazy.
Audit bounded channels, thread consumers, raw-memory wrappers and all internally accessible helpers. Keep public fairness,
blocking/lane, recursion and overflow semantics.

Update the report-pool initializer through the real constructor. Check production declaration bodies and callers,
encoded interfaces and normal native products. Stop maintaining hand-selected numeric source subsets. Review all
uncommitted repairs against the issue contract and repository rules, including module naming and thin roots.

Exit: no known BRA-620 authority/compiler finding remains. Host formation, nested entry, recursive failure, TLS clearing
and exactly-once disposal still satisfy their contracts. Run one complete production consumer cost checkpoint before
replicating the representation further.

### N1. Complete physical backing and entry/report ownership

Owners in order: BRA-502, BRA-554, BRA-558.

BRA-502 extends the existing cleanup description and realization. Compute additional local storage from selected actions,
actual simultaneous lifetimes, type-wide impossibility and target layout. Prefer enclosing storage for proven local
lifetimes, coallocation for compatible surviving owners, and separately retained backing only where needed. Do not
reserve a global maximum or allocate separately per local value by default.
Admission secures bytes and required terminal services before ownership commits. Rejection preserves inputs. Move and
wrapping transfer existing allowance. Activation consumes one matching reservation. Source discharge releases only unused
parts. Incidents/results retain transferred regions through their last use. Keep the existing shared service/pool rather
than introducing another arena or public domain field.

BRA-554 builds synchronous and foreign entry on the resident host and admitted outcomes. Acquire before source access,
restore nested context, close admission atomically with entry, contain abnormal outcomes at the ABI and retain providers
through callback return and local destruction. Release synchronization before callbacks.

BRA-558 consumes the same typed incident owner. Preserve original and ordered suppressed failures. Quiesce required child
work, destroy each concrete error, then release its backing even if destruction reports another failure. Drain chains
iteratively from secured storage. Reporting has no scheduler dependency or mandatory diagnostic-history copy.

Exit: generated native consumers preserve a large/over-aligned typed error under denied post-admission allocation.
No scalar credits stand in for bytes, no transferred backing is reclaimed early, and each migrated consumer has switched
to Bray with its replaced Rust policy removed.

### N2. Close BRA-619 and retire the current branch stack

Owner: BRA-619, consuming N1 rather than implementing another incident transport.

Bind TransferCleanupIncident through the shared typed admission and report route. Cover successful/failed entry,
fallible exit on ordinary completion and pending return/break/continue/propagation/panic/cancellation, and async scoped
forms. Exit runs once. Preserve the pending outcome, lifetime dependencies and eventual exactly-once error disposal.

This is complete compiler scoped-use acceptance. The later replacement of the general task/activation engine has its
own open owners. It does not permit skipping async scoped native behavior now. Any prerequisite defect exposed by this
matrix must be fixed under its listed owner before claiming N2 complete. If that requires a later implementation item,
bring that item forward with an explicit dependency, not a duplicate bridge or deferred failing test.

After C3, create the agreed feature/trusted-storage-integration branch from the checkpointed repaired tip in the same
worktree. N1/N2 work can be reviewed in bounded commits or temporary PRs against that integration branch. Keep one final
integration PR into develop for the existing stack. Do not merge the five original PRs separately into develop.

Exit: all requirements assigned to the current five issues are complete, all handoff findings have a verified resolution,
combined native checks and published review pass, and the single integration PR is ready. User merge remains separate.
After it lands, retire superseded PRs and continue the remaining milestones from the landed history.

### N3. Deliver the complete synchronous product, then loaded providers

Owners: BRA-556 -> BRA-557 -> BRA-608 -> BRA-555.

BRA-623 is an additional BRA-608 prerequisite. The entry audit found that ordinary runtime requirements
are retained in checked contracts but not emitted before calls. Implement those checks in lowering before
invocation and ownership transfer, using established proofs and the existing panic/cleanup path. Keep
opaque authority requirements compile-time obligations. BRA-554 separately preserves native versus Bray
invocation conventions. See [BRA-623](https://linear.app/bray-lang/issue/BRA-623) for the acceptance matrix.

Use compiler-provided static dependencies and one terminal host state for graceful shutdown and resident fallback.
A blocked shutdown preserves the caller owner and returns. It must not wait for roots held by that caller. Drain
teardown incidents before dependency release, with internal static edges ordering cleanup rather than blocking it.

Finish exact-thread membership and dependency-ordered TLS cleanup. Explicit detach and native TLS exit share one path.
Keep attachment/backing live through callback return and do not move exact-thread duties onto a helper thread.

BRA-608 switches synchronous root orchestration and proves the entire direct/synchronous executable closure has no
project-owned Rust implementation. It consumes entry, reports and static owners rather than recreating them.

BRA-555 then adds independent load identity, whole-product provider retention, external handle validation, reload and
resident last-lease release after image callbacks return. Internal borrows use established lifetime instead of repeated
handle-table lookup. Optional independent diagnostic history permits release only when its consumer requests it.

Exit: the documented early synchronous milestone works before worker/test-host migration. Dynamic providers retain code
and statics through all dependent users. Existing donor code is behavioral evidence only and is not replayed wholesale.

### N4. Deliver admitted activation, ordinary cleanup and represented owners

Owners: BRA-561 -> BRA-505 -> BRA-506. BRA-561 can start after BRA-502/BRA-553 and need not wait for the full host umbrella.

Generate immutable frame descriptors from checked/lowered state. Bray owns one activation and an iterative direct-await
driver. Known child frames use enclosing storage when lifetimes permit. Recursive and erased frames retain adequate
backing. Direct await adds no task, scheduler registration, transition allocation or enqueue.

BRA-505 applies the same admitted lifecycle to ordinary owned scopes. Preserve completion-aware legality, shielded
abnormal finalization, typed incidents and synchronous destruction fallback. BRA-506 applies it to fields, active
nullable/union payloads, replacement and initialized array/buffer prefixes. Emit iteration where element-wise code
duplication is unnecessary.

BRA-508 updates actual resource APIs to distinguish retained/retryable ownership from terminal completion. Independent
families can proceed once their prerequisites hold. Host-dependent families also consume BRA-507.
BRA-484 completes general generator construction over BRA-502/BRA-506 with one initialized-prefix ownership path.

Exit: ordinary, represented, generic and imported cleanup uses the same plan and retained backing. Remove Rust
descriptor reconstruction and the replaced NativeFrame/NativeRun ownership. No second aggregate cleanup engine,
fixed alignment/slot ceiling or unbounded native recursion appears.

### N5. Complete started tasks, waits, scheduling and suspending hosts

Owners: BRA-560 -> BRA-562 -> BRA-563 -> BRA-564 -> BRA-565.
BRA-566 follows BRA-564 plus BRA-555/BRA-557. BRA-567 follows BRA-566.

Start adds one TCB and one source resolution owner around the established activation. Internal queue and delivery
references retain backing without gaining another right to consume the result. Rejected start preserves low-level
inputs and resolves the consumed inactive source frame according to the source contract.

Use one observe/register/recheck/commit/withdraw protocol for wait ownership. Events and deadlines specialize its
collection and readiness condition, not its lifetime rules. Preserve wakes arriving during execution. Callbacks run
outside collection locks. Cancellation requests are cooperative and never establish completion.

Count running work separately from admitted/queued storage. Suspension releases the execution slot. Use compatible-lane
queues and demand-created worker owners, not a global lock for every transition or an eager fixed worker set.
Main-thread and exact-thread requirements remain explicit. Worker detach acknowledges on the owning thread before join.

BRA-507 consumes BRA-555/BRA-557/BRA-506/BRA-567 to complete suspending static/foreign/returned ownership and resident
fallback. Admission must retain compatible execution as well as bytes. Use the same host state and dependency-release
trigger, not a polling service, helper thread per owner or second fallback engine.

Exit: source lexical cleanup precedes terminal publication, results transfer once, no wake or retained user sees freed
storage, and services remain live through admitted fallback. Remove migrated Rust task, scheduler, cancellation, event,
timer, worker and async-root policy as each real consumer switches.

### N6. Complete test products, packaging and target conformance

BRA-552 supplies independently verified test/catalog completeness using existing discovery facts. It can proceed early
and does not wait for the child runtime migration.

BRA-559 reuses the completed host, deadline, cancellation and report owners for child session protocol, capture, serial
coordination, timeouts and terminal reporting. External coordination may remain Rust. No Rust codec or runtime policy
remains linked in the child as a shortcut.

BRA-568 removes obsolete runtime archive/adapter build routes and publishes through ordinary package-native demand.
Optional components must be absent from code and data when unused. Remove unconditional registration and descriptor
references that retain unrelated services. Preserve permitted OS/system ABIs and the documented temporal exception.

BRA-192 supplies the six-target native conformance proof. Compile all six targets and execute the required matrix on
actual supported hosts: aarch64 and x86-64 on Linux, macOS and Windows. Unavailable native evidence stays outstanding, not counted as a pass.

Exit: direct, synchronous, async, test and supported shared-library products all use the intended ownership architecture.
No completed consumer remains Rust-backed. Remove superseded WIP inventory/appendix material only after enduring design
and source contracts are correctly documented.

## Linear plan: use active owners, create no duplicate repair issues

Issue state was read live on 2026-10-12. This table is a proposed update ledger, not changes already sent to Linear.

| Open owner | Current state | Proposed action |
| --- | --- | --- |
| [BRA-620](https://linear.app/bray-lang/issue/BRA-620) | In Progress | Put C1-C3, the chosen authority proof, source/import/native matrix and removal list in its acceptance. Keep the authorized compiler repair here. |
| [BRA-609](https://linear.app/bray-lang/issue/BRA-609) | In Progress | Reconcile its enforcement acceptance with the shared flow and integration PR. Do not build a parallel repair on its original branch. |
| [BRA-621](https://linear.app/bray-lang/issue/BRA-621) | In Review | Preserve its repaired transport and update integration evidence after report-pool construction changes. No second pool. |
| [BRA-553](https://linear.app/bray-lang/issue/BRA-553) | In Review | Preserve completed host behavior and revalidate material interactions. Link integration readiness without declaring the whole stack complete. |
| [BRA-619](https://linear.app/bray-lang/issue/BRA-619) | In Progress | Keep native transfer explicitly incomplete until N2. Preserve blockers BRA-502/BRA-558. |
| [BRA-502](https://linear.app/bray-lang/issue/BRA-502), [BRA-554](https://linear.app/bray-lang/issue/BRA-554), [BRA-558](https://linear.app/bray-lang/issue/BRA-558) | Backlog | Execute N1 in that order. Retain existing physical-storage, entry and typed-disposal ownership. |
| [BRA-556](https://linear.app/bray-lang/issue/BRA-556), [BRA-557](https://linear.app/bray-lang/issue/BRA-557), [BRA-608](https://linear.app/bray-lang/issue/BRA-608), [BRA-555](https://linear.app/bray-lang/issue/BRA-555) | Backlog | Execute N3 with synchronous independence before loaded-provider completion. |
| [BRA-561](https://linear.app/bray-lang/issue/BRA-561), [BRA-505](https://linear.app/bray-lang/issue/BRA-505), [BRA-506](https://linear.app/bray-lang/issue/BRA-506) | Backlog | Execute N4. Add the BRA-558 -> BRA-505 incident-consumption blocker. Preserve existing BRA-561/BRA-502 -> BRA-505 and BRA-505 -> BRA-506 prerequisites. |
| [BRA-484](https://linear.app/bray-lang/issue/BRA-484) | Backlog | Add BRA-502/BRA-506 blockers and implement generator construction over their shared initialized ownership. No generator-specific cleanup service. |
| [BRA-508](https://linear.app/bray-lang/issue/BRA-508) | Backlog | Keep BRA-506 dependency. Record BRA-507 as a dependency of host-dependent families without unnecessarily blocking independent families. |
| [BRA-560](https://linear.app/bray-lang/issue/BRA-560) through [BRA-565](https://linear.app/bray-lang/issue/BRA-565) | Backlog | Preserve the activation -> task -> cancellation -> lanes -> events -> deadlines dependency order. Separate issues remain owners of complete consumers. |
| [BRA-566](https://linear.app/bray-lang/issue/BRA-566), [BRA-567](https://linear.app/bray-lang/issue/BRA-567), [BRA-507](https://linear.app/bray-lang/issue/BRA-507) | Backlog | Finish workers, async roots and then admitted suspending host fallback. Keep existing concrete prerequisites. |
| [BRA-552](https://linear.app/bray-lang/issue/BRA-552) | Backlog | Reuse for broad test inventory/completeness. Add it as a verification dependency of BRA-192, without making it a runtime dependency. |
| [BRA-559](https://linear.app/bray-lang/issue/BRA-559), [BRA-568](https://linear.app/bray-lang/issue/BRA-568), [BRA-192](https://linear.app/bray-lang/issue/BRA-192) | Backlog, Backlog, Todo | Finish child host, ordinary artifact cutover and native independence in that order. |
| [BRA-281](https://linear.app/bray-lang/issue/BRA-281), [BRA-482](https://linear.app/bray-lang/issue/BRA-482), [BRA-547](https://linear.app/bray-lang/issue/BRA-547), [BRA-548](https://linear.app/bray-lang/issue/BRA-548) | In Progress, In Progress, Backlog, Backlog | Keep as outcome trackers. Update checklists to this active-only sequence. Do not make implementation wait on completion of its own umbrella. |
| [BRA-456](https://linear.app/bray-lang/issue/BRA-456) | Backlog | Complete regression policy early using the existing runner and synthetic policy inputs. Keep absolute competitiveness separate from merge-base regression. |
| [BRA-402](https://linear.app/bray-lang/issue/BRA-402), [BRA-404](https://linear.app/bray-lang/issue/BRA-404), [BRA-406](https://linear.app/bray-lang/issue/BRA-406), [BRA-429](https://linear.app/bray-lang/issue/BRA-429) | Backlog | Own bounded execution, footprint, compilation and bulk-transfer corrections as consumers become available. No new competing optimization project. |
| [BRA-540](https://linear.app/bray-lang/issue/BRA-540), [BRA-541](https://linear.app/bray-lang/issue/BRA-541), [BRA-542](https://linear.app/bray-lang/issue/BRA-542) | Backlog | Keep final migrated-runtime gates after BRA-568. Earlier cost checkpoints belong to the implementation/parent issues and do not wait for these gates. |
| [BRA-588](https://linear.app/bray-lang/issue/BRA-588), [BRA-574](https://linear.app/bray-lang/issue/BRA-574), [BRA-428](https://linear.app/bray-lang/issue/BRA-428), [BRA-409](https://linear.app/bray-lang/issue/BRA-409) | Backlog, In Progress, Backlog, In Progress | Reconcile final optimized-demand corpus, broader competitiveness and product-independence evidence after their active prerequisites. Preserve completed foundation implementations. |

Use actual blocking relations for prerequisites, with descriptions explaining the consumed contract. Do not add edges
from an early implementation to its own final cost gate. The final native and performance gates can proceed independently
once their artifacts are complete. Broader umbrellas cannot close until their required outcomes are demonstrated.

No new issue is justified by the current findings: authority consolidation, native ownership, test completeness,
performance and final delivery all have active owners. Do not create one issue per helper, test, codec or symptom.
If implementation reveals an independently substantial missing subsystem, first search active owners, then propose one
labeled issue with acceptance, dependencies and removal obligations. Never reopen a closed issue or use its title as an
active assignment. New issues must not become a way to defer a failing current acceptance criterion.

Move an issue to In Progress when implementation starts, and to In Review only after its complete acceptance and review.
Do not mark it Done manually. Verify integration issue linkage with the merge workflow so superseding a PR does not
close an incomplete issue or leave a completed issue attached only to an abandoned PR.

## Git and review mechanics

Live GitHub heads on 2026-10-12 were checked against the current repair HEAD:

| PR | Live head | Included as ancestor |
| --- | --- | --- |
| #601 | f244c10c2dda07ca6fc9bb69a5483215e5feb724 | Yes |
| #602 | e3dbc6c5baca462ba185d902649534cb86a8cc4f | Yes |
| #604 | 22473ff6384db081ad3e131e0726e16e7ff9c682 | Yes |
| #605 | fb69346e31b225c39105d095e9f900c9cebad97c | Yes |
| #606 | cca4bd4429d0648cae0493db72045b7d4a7a3e73 | Yes |

The repair branch is feature/bra-620-repair-synchronization-authority. Its HEAD is the #606 head plus the uncommitted
repair in the existing isolated worktree. The original BRA-620 work is present. The earlier assessment measured about
34.3k net lines across the combined stack, including tests and untracked repair. That is not a new measurement here.

Before implementation, checkpoint tracked and untracked work without dropping primary-checkout handoffs. Do not reset,
rebase published heads or force-push as a consolidation shortcut. Recheck live heads before integration, incorporate only
absent commits, and verify ancestry plus the resulting behavior. Ancestry alone does not detect an accidental later
revert. Compare contract-bearing changes and native consumers too.

Keep original PRs as review history until the integration lands. Mark their descriptions as superseded by the integration
when published and prevent separate merges. Carry unresolved review findings into the combined review matrix.
After the user merges the integration, reconcile old PRs and issue links. Subsequent milestones use bounded consumer PRs
from the landed base, with temporary stacking only for real dependencies.

Follow the repository workflow for exact-destination verification, publication and fresh review on the published diff.
An earlier clean review does not cover material later changes. This planning task authorizes no publication or merge.

## Performance constraints throughout implementation

Deduction chooses the design now. Later measurements verify the finished mechanisms rather than searching randomly for
a language rule that happens to pass the current compiler.

| Dimension | Structural requirement during implementation | Final acceptance |
| --- | --- | --- |
| Compilation | Share body preparation and selected edges. No duplicate lifecycle transfer, per-call source analysis, uncontrolled path splitting, speculative cache hierarchy or universal pass. Reuse published libraries and demand only required native instances. | BRA-406/BRA-542 require each comparable row and aggregate at most 1.25 times the faster Rust/C++ peer for full process and reported work, plus full-process time no slower than current rustc. Include LLVM, publication and linking. |
| Execution | Erase proof-only data. Use direct known calls, existing atomic operations and real synchronization. No token lookup, extra direct-await task, transition allocation/enqueue or unconditional provider lease for an internal borrow. | BRA-540/BRA-428 require matching or beating the faster equivalent Rust/C++ peer outside measured variability. Intermediate 1.25 thresholds are not final success. |
| Binary size | Demand-select code and data. Avoid universal descriptor tables, unnecessary generic adapters, per-element unrolling and optional service references in common sinks. Share generated operations only when ABI and ownership agree. | Keep empty products at or below matched C++. Preserve the non-async bound of 1.25 times Rust and BRA-428's broader 1.25 times smaller-peer ceiling. Satisfy both where applicable and remove avoidable cost even below the ceiling. |
| Memory and retained resources | Account for admitted bytes, frame/TCB/wait state and retained providers. Prefer enclosing/coallocated regions when lifetime permits. Retained reports prevent reuse of their region, not necessarily a whole frame. | Report concrete reserved/live/retained memory and constrained-resource behavior. No fixed-slot truncation, premature release or concealed whole-product retention. |

Use equivalent behavior and failure contracts, the repository's runtime linkage policy, pinned/recorded LLVM and peer
toolchains, matched optimization and worker settings, and current sources. Distinguish cold preparation, published
consumer reuse and the immediate library-to-consumer pipeline. No front-end-only victory that increases LLVM/link work.

Use the existing xtask/performance and profiling facilities. Keep timing artifacts free of profiling and observation
side effects. Obtain code shape, layout, allocation and link-map evidence in separate untimed checks. Report unavailable
observations as unavailable. Compare every workload, not only an average. Report non-comparable rows with reasons.
The performance reference currently excludes Mach-O peer comparison until equivalent runtime linkage is available.
Resolve that comparison prerequisite under BRA-541/BRA-428 before claiming comparable macOS performance.

At C3, N1/N2 and each later first complete consumer, inspect the predicted costs before broad replication. Final corpus
runs belong to combined gates, not every edit. Noise invokes BRA-456's evidence policy, not repeated experiments until a
passing number appears. An intermediate cost requires its cause, owner, removal dependency and reevaluation milestone.
No temporary waiver can satisfy the final destination.

LLVM alias/noalias, lifetime and unwind attributes must follow proven Bray facts. In particular, lifetime anchoring is
not disjointness, and shared interior-mutable storage is not globally immutable. Performance tuning cannot strengthen
backend assumptions beyond the language.

## Validation and completion gates

Planning has not run the compiler. During implementation, derive tests from the contracts after the design is selected:

- Authority: genuine producers, safe fabricated aggregates, copied fields, whole moves, partial extraction, attempted
  re-pairing, whole replacement, stale epochs and safe internal helper access.
- Flow: direct and returned aliases, reassignment, joins, nullable/union presence, generic/imported calls, independent
  lifetimes and referents, payload mutation, initialized-state changes, raw noncopy reads and opaque calls.
- Lifecycle: ordinary calls, scope enter/exit, failed entry, explicit consume, replacement, normal/abnormal destruction,
  cancellation shielding, broadcast-before-resolution and conditional versus unconditional completion.
- Runtime: initial/reentrant/failed host formation, callback closure races, TLS clear-before-callback, retained provider
  return, denied allocation, over-aligned/large typed errors, separate destruction and backing release, iterative chains.
- Architecture: one producer for each fact, no surviving duplicate transfer/selection, finite deterministic convergence,
  cancellation publishing nothing, source/import equivalence and precise invalidation.
- Costs: guard/protocol layouts and emitted operations, cleanup/frame/descriptor bytes, allocations/atomics/TLS accesses,
  selected native units, complete compilation and retained component subtraction.

Use actual production declarations and consumers. A fake-owner negative must fail for the intended authority/lifecycle
reason, not fixture syntax. Failed valid cases remain blockers. Inspect the full source/module inventory so a passing
subset cannot hide an unexamined Once, thread or bootstrap sibling.

Start with smallest meaningful checks, then run repository-required verification and native API/outcome coverage at the
combined milestone. Native toolchain processes run outside the sandbox. Reuse shared build storage and observe long
operations. No repeated broad run is needed without a relevant change, unresolved failure or new concern.

Before each delivery, reread AGENTS.md and mechanically audit every changed file and added behavior category. Search
repo-wide for equivalent behavior and remove duplication. Enforce crate/module ownership, names, thin roots, error
contracts, numerical policy and public API shape. Run mutating cargo xtask style after Rust changes and cargo xtask format
for changed Bray work, plus required compilation/tests. Do not use formatting-driven line movement as a code-size gain.

The end goal is reached only when the current stack and all included later consumer contracts are complete, the active
final conformance/performance gates pass, all required platforms have honest evidence, migrated implementations have been
removed from Rust-linked paths, and the design has no unresolved authority or lifecycle workaround. A new tracking issue
can record genuinely new work, but cannot make a known missing requirement count as implemented.

## Discussion recommendation

Approve this direction as one coherent repair and delivery sequence: the composite owning analysis, precise reference
and effect facts, ordinary acquisition/protocol witnesses, one cleanup/admission/incident path, and the active-issue
milestones above. Keep the existing language. Keep the current combined code as the baseline and remove superseded
mechanisms as their replacements become complete.

The largest risks are implementation complexity in the coupled flow, physical cleanup backing cost and whole-product
retention. They are explicit design obligations with named owners and cost gates. They do not justify more runtime
frameworks or weakened contracts. If deduction against a concrete consumer contradicts this design, resolve that
contradiction before expanding implementation.

Documentation audit: all 45 referenced Linear issues were verified open. The active dependency graph including proposed edges is acyclic. Relative links resolve and whitespace checks pass. All 2,940 baseline non-documentation files retain their hashes, with no new implementation files. No compiler experiments, builds, native runs, issue mutations, commits, pushes or branch changes were performed.
