# Runtime stack rule coverage and replacement plan

## Agreed semantics and active implementation, 2026-10-13

The user accepted the closed observation dependencies and explicit trusted establishment/preservation boundary in the
[semantic proof](runtime-stack-semantic-proof.md), then authorized completing steps 1 and 2. The normative contract is
in [contract reasoning](../language/contracts-and-trust/contract-reasoning.md),
[trusted callers](../language/contracts-and-trust/trusted-caller-obligations.md) and
[witnesses](../language/contracts-and-trust/trusted-witness-values.md).

Continue on the existing repair branch. The implementation baseline is `e3f56424`, with proof checkpoint `7f300ef2`.
Step 2 must complete shared authority analysis and remove its superseded mechanisms. Production API migration is step 3
and is not part of this authorization. Historical test results below remain evidence, not current acceptance.

Prepared on 2026-10-12. The user accepted the groundwork and requested actionable Linear issues.
See the [Linear execution guide](runtime-stack-linear-execution.md) for the recoverable starting state and publication order.
This update prepares the issues and preserves existing code. It does not start implementation.

This replaces the design choices and implementation sequence in the [earlier proposal](runtime-stack-implementation-plan.md).
The [assessment](stack-architecture-assessment.md) retains the measured growth and earlier source findings.
This document is an implementation contract for the affected ownership, authority and runtime domains. It is not a
claim that the current compiler implements them, a formal verification of Bray, or fresh performance evidence.

## Decision

Complete the shared semantic implementation before migrating another runtime consumer. Organize the work around the
rules below, with library protocols as applications of those rules. Do not add a special case whenever a lock, host or
task exposes an omitted rule.

The required change is broader than preserving one acquisition predicate. The current tree has multiple representations
of authority and reference state, independent traversals of the same operations, and incomplete native consumers of
already specified lifecycle behavior. Move the correlated state into the existing composite storage analysis. Publish
its conclusions once. Preserve the distinctions that express different obligations.

No new language feature is proposed. This conclusion rests on the explicit carrier, observation and protocol arguments
below, not on assuming that inferred dependencies will somehow repair every missing relationship.

## Scope, evidence and boundaries

The inspected tree is the existing BRA-620 repair worktree, based on BRA-553 commit
`cca4bd4429d0648cae0493db72045b7d4a7a3e73`, including its inherited uncommitted repair.
The source findings below are inspection results. Prior passing tests remain historical evidence with their original
limitations. No Bray compiler experiment, build, native run or benchmark was used to choose this design.

Coverage means every operation category and exit in the affected domains has a rule, owner, representation and consumer.
It does not mean every implementation line has been proved correct. The closed inventories in
[the coverage appendix](runtime-stack-coverage-inventory.md) make omissions in this map detectable. Parsing, unrelated
numeric algorithms and ordinary overload resolution retain their existing owners. Their selected operations, conversions,
defaults and failures enter this model through the same call and storage rules.

The language specification governs observable behavior. Design documents govern phase ownership and representation.
The WIP runtime design supplies the selected runtime architecture where consistent with those documents.
The principal sources are:

- [Axioms](../axioms/core-axiom-set.md), [checker](../design/checker.md) and [compiler architecture](../design/compiler-architecture.md).
- [Witnesses](../language/contracts-and-trust/trusted-witness-values.md), [contract reasoning](../language/contracts-and-trust/contract-reasoning.md), [dependencies](../language/ownership-and-borrowing/dependency-contracts.md) and [ownership boundaries](../language/ownership-and-borrowing/scope-exits-and-ownership-boundaries.md).
- [Uninit](../language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md), [partial moves](../language/ownership-and-borrowing/partial-moves.md), [replacement](../language/lifecycle/partial-values-and-replacement.md), [destruction](../language/lifecycle/destruction.md) and [finalization](../language/lifecycle/finalization.md).
- [Scoped exits](../language/lifecycle/with-expressions.md), [cancellation](../language/async-and-concurrency/cancellation.md), [task cleanup](../language/async-and-concurrency/structured-task-scope-exit.md), [frame storage](../language/async-and-concurrency/async-representation-and-storage.md) and [shutdown](../language/async-and-concurrency/execution-roots-and-product-shutdown.md).
- [Execution guarantees](../language/contracts-and-trust/execution-guarantees.md), [predicate calls](../language/contracts-and-trust/predicate-expression-callable-calls.md) and [statics](../language/declarations/static-storage-declarations.md).
- [Cleanup storage](../design/cleanup-storage-and-reports.md), [composed execution](../design/composed-async-execution.md), [interfaces](../design/compiled-package-interfaces.md), [lowering](../design/lowering.md) and [codegen](../design/codegen.md).
- [Runtime architecture](../wip/bray-native-runtime-design.md), [consumer inventory](../wip/bray-native-runtime-consumer-inventory.md) and [cost constraints](../wip/bray-native-runtime-cost-analysis.md).

## Facts and their owners

Use the existing identities, places, dependency subjects and checked operation records. The following are semantic
components of the existing analysis, not seven new public APIs or independent passes.

| Fact | Meaning and producer | Transfer and consumer |
| --- | --- | --- |
| Storage and initialization | Storage plan supplies locations and potential operations. Composite flow establishes live, complete, partial, absent and consumed states, including protected contents. | Moves, writes, replacement and cleanup update it. Access legality and selected cleanup consume it. |
| Physical referent | Composite flow relates borrow/address values to possible storage paths. | Projection, assignment, call substitution and joins update it. Overlap and mutation effects consume it. |
| Lifetime/capability dependency | Existing dependency rules identify what must survive, including exact product, attachment, provider and scoped roots. | Whole moves preserve it. Boundaries check it. Borrow expiry and owner retirement revoke it. |
| Trusted provenance and carrier | Checked trusted producer identifies which live value carries a condition. A condition observing a field is not automatically carried independently by that field. | Whole-owner transfer moves authority. Copy, extraction and retirement use the witness rules. |
| Observation dependency | Predicate semantics and checked reference relations identify what can change a condition's truth. | A write revokes affected observations. It does not automatically retire the carrier or its lifetime roots. |
| Lifecycle obligation | Selected lifecycle and current flow establish pending, completed, transferred or abnormally abandoned steps. | Cleanup consumes this decision. Completion of one step does not discharge represented parts or backing. |
| Execution and capacity | Checked calls and cleanup supply effects, possible outcomes and verified execution properties. Lowering supplies symbolic local storage, realization supplies layout. | Callers consume summaries. Admission acquires actual backing. LLVM and the runtime do not re-prove these facts. |

Two examples prevent the current conflations. An anchored payload borrow can depend on a guard while physically pointing
inside a lock. A guard can carry a predicate that observes its empty marker while depending on the lock staying alive.
Neither relationship licenses treating all three locations as the same storage.

## Rule coverage map

Status describes this tree, not a test verdict. "Present" means an implementation owner exists and the inspected structure
matches the rule. "Split" means overlapping mechanisms implement parts of it. "Gap" means a required capability is absent
or the inspected representation cannot express it. None of these labels waives implementation acceptance.

Paths in this table are relative to the repository. The source-owner index below provides links.

| Rule | Required transition and boundary | Current implementation coverage | Replacement owner |
| --- | --- | --- | --- |
| R01 | Construction initializes only committed components. Failed construction resolves committed parts and retains untransferred inputs. | Present across storage flow, raw-memory flow and lowering initialization. Their authority evidence is separate. | Composite flow plus existing lowering initialization. |
| R02 | Uninit allocation creates no T. Write/from commit one T. Move/assume/destroy consume that exact initialized epoch once. | Split between memory classification, storage_flow raw state and guarantee/trusted proof. | One memory-operation transfer in composite flow. |
| R03 | Read/copy/move follow selected copy and ownership contracts. Raw noncopy reads consume. Ordinary byte or field copying cannot duplicate witness authority. | Split between effective storage purpose, memory read kind and late witness-transfer checks. | Composite transfer consumes already selected copy/read kind. |
| R04 | Partial move updates initialized parts. It cannot separate a carried guarantee or violate whole-value lifecycle. Fresh results obey the same rule. | Late witness checking skips first destinations of temporary/result/allocation ValueTransfer. Guard extraction therefore needs coverage independent of lifecycle repair. | Carrier-aware transfer at the actual ownership operation, including fresh-result destructuring. |
| R05 | Whole move transfers initialized parts, dependencies, authority and existing allowance. Old path loses them. | Existing move and dependency machinery, with separate proof reconstruction. | One transfer with durable dependency projection. |
| R06 | Projection and observation create no ownership. Nullable/union facts apply only to the active payload. | Present, but nullable reference/proof propagation is split. | Shared projected subjects and presence guards. |
| R07 | Borrow/reborrow checks initialization, overlap and capability. A mutable dependency prevents conflicting owner use. | Existing borrow capability and liveness machinery. Returned aliases use differing origin rules. | Composite references and capabilities, with existing backward liveness. |
| R08 | Physical target is distinct from lifetime anchor. Unknown aliases remain conservative. Distinct access IDs do not prove separation. | Gap/split. storage/expression/result.rs derives a returned access from a sole direct lifetime source, while guarantee flow uses exact borrows metadata. | Storage planning records a symbolic returned access. Composite reference transfer resolves it. |
| R09 | Address formation preserves dependencies without ordinary borrow protection. Address integers convey no authority. Reinterpretation preserves valid roots, not invented initialization. | Existing checked memory variants and portable Borrow/Address kind. | Retain kind and checked memory semantics. Share transfer with R08. |
| R10 | Mutation revokes facts whose observations overlap. Initialization-preserving replacement differs from move-out, retirement and borrow-slot reassignment. | Gap/split. storage_invalidation globalizes local-borrow writes. guarantee/flow/invalidation adds hook-specific recovery. | One operation effect applied to resolved targets and state. |
| R11 | Replacement evaluates destination once and RHS before old cleanup. After cleanup starts, install the RHS before abnormal propagation. | Present in lowering/replacement.rs. Analysis cleanup does not expose the same explicit restoration stages. | Shared semantic replacement stages, consumed by existing lowering. |
| R12 | Calls check input authority before transfer, apply effects on each outcome, and publish normal ensures only on completion. | Present with separate execution evidence and storage stages. | Composite call attempt/completion transitions. |
| R13 | Defaults, conversions, operators, custom indexing, iteration and lifecycle calls follow R12. An indirect call consumes its checked callable contract. | Selection and graph already model many implicit sites. Unwind adds a separate traversal and cleanup walk. | One selected occurrence inventory, exhaustive adapters into R12. |
| R14 | Conditions survive joins only on all reachable relevant paths. Dependencies and possible targets retain all possible obligations. | Existing independent flow merges. Exact return origins use another syntax walk. | One composite merge with separate may/must components. |
| R15 | Loops invalidate iteration-local observations and converge without equating fresh owners from different iterations. | Existing worklists and expression invalidation, separate domains and coarse bounds. | Finite abstract roots and terms, explicit joins and conservative cycle widening. |
| R16 | Return/yield/store/trait view/box/argument transfer must preserve every carried dependency. | Existing dependency templates and boundary checks. | Project from finalized composite facts, no re-derivation of movement. |
| R17 | Whole-value finalization leaves a fully initialized owner. Error alone does not decide completion. | Selected completion machinery exists. Native admitted execution remains incomplete. | Composite lifecycle state, shared lowering expansion, BRA-505/506 consumers. |
| R18 | Destruction consumes self, cannot leak derived authority and destroys remaining initialized parts once. | Existing lifecycle and storage checks. Trusted completion repeats state transfer. | Same call/consume/part transitions as ordinary operations. |
| R19 | with entry failure runs no exit. Successful entry creates a capability. Exit runs before body-local destruction and handles pending outcomes. | BRA-619 selection/lowering exists. Native typed incident transfer is incomplete. | Preserve selected enter/exit sites, use the same call and outcome rules. |
| R20 | Scope exit first broadcasts task cancellation, then resolves lifecycle in dependency order. All transfer forms use the same exit path. | Existing graph and async plans. Guarantee cleanup can erase all witness state. | One selected cleanup description and composite transfer. |
| R21 | Abnormal cleanup shields cancellation, attempts finalization, retains errors, performs abandonment, destroys once and preserves primary/suppressed order. | Compiler/Rust paths exist. Arbitrary typed native incident ownership is missing. | Shared cleanup outcome lowering plus BRA-502/558 backing and sink. |
| R22 | Deferred Future invocation owns captures but grants no execution postconditions. Await completion, start and terminal observation have distinct transitions. | Existing async/dependency/MIR semantics. Rust still owns activation/task policy. | Existing checked phase contracts plus BRA-561/560 owners. |
| R23 | Suspension retains all live dependencies and required stable storage. Cancellation request is not completion. Exact-thread roots constrain the entire retained chain. | Existing frame/liveness contracts and Rust runtime. Bray consumers incomplete. | Composite suspension projection plus BRA-561 through BRA-567. |
| R24 | Admission precedes each new obligation. Failure retains inputs. Moves do not readmit. Active/transferred backing survives source discharge. | Outgoing records implemented. General physical activation/result/typed-error backing is incomplete. | BRA-502, one local allowance and release owner. |
| R25 | Current completion may omit invocation, but only type-wide impossibility removes its capacity. total, pure and non-unwind are distinct. | Retain approved total capacity change and checked non-unwind distinction. Early/final traversals duplicate work. | One selected call inventory, separate property judgments. |
| R26 | Static identity includes substitution, witnesses, target, product and exact attachment. Constant initialization grants one fresh static owner, not a copyable runtime witness. | Existing static templates and realization. Native lifecycle policy remains partly Rust. | Existing static owners plus BRA-556/557. |
| R27 | Foreign outputs establish initialization only after native success. ABI/callback identity checks do not grant unrelated authority. | BRA-620/621 repairs exist. Once helpers still rely on forgeable fields. | Explicit trusted producer contracts and protected protocol storage. |
| R28 | Provider dependencies retain code and statics through final callback return. Closing entry preserves existing disposal authority. | Existing semantic roots. Whole-product runtime retention needs consolidation. | BRA-555 and resident caller release. |
| R29 | Source/imported/generic/dynamic contracts mean the same thing. Interface decoding validates external records once. | Dependency codecs and Borrow/Address plumbing exist. Exact reference/observation/carrier projections are incomplete. | bray-symbols meaning, package-interface encoding, body owner publication. |
| R30 | Recovery never creates proof. Invalid source gets structured diagnostics at its owning operation. Broken established invariants expose compiler bugs. | Existing result categories and diagnostics. Preserve them through consolidation. | Each owning rule, no downstream semantic validator. |
| R31 | Component demand follows actual retained behavior. Optional runtime code stays absent. Compiler and runtime share typed role contracts. | Role catalog and optimized demand exist. Runtime artifacts still contain Rust consumers. | BRA-568/588, retain existing demand and artifact owners. |
| R32 | Infallible mandatory cleanup covers all live owners even if new allocation fails. No fixed incident count substitutes for unbounded ownership composition. | Present bootstrap record mechanisms, incomplete general native backing. | R24 plus each protocol's terminal path below. |


## Concrete shared transition design

### Storage planning and the operation inventory

Keep the immutable checker graph. Extend its existing operation payloads only where they currently hide an observable
intermediate state. Do not introduce another checked tree, universal IR or a second graph for trusted execution.

Storage planning assigns symbolic locations and selected accesses. It must stop deciding that a sole lifetime anchor
is the physical referent of a returned borrow. A returned reference gets its own symbolic slot and the selected callable's
formal relation. Composite flow substitutes actual input roots at that occurrence.

Prepare once per demanded body:

- Selected ordinary and implicit calls, argument mappings, conversions, defaults and execution phases.
- Potential lifecycle targets and represented-part paths, independent of whether the current state will require them.
- Checked memory operation classification and symbolic effects.
- Scope boundaries, all control exits and immutable entry snapshots for declared guard domains.

Use `CheckedMemoryOperationKind` for raw-memory behavior. The existing compiler-known catalog supplies operation identity.
The appendix assigns every current hook to its owner. Classification must be exhaustive at that boundary. A recognized
operation cannot silently enter a catch-all "harmless" branch. Unknown foreign behavior is a deliberate conservative
contract case, not the default for forgotten compiler-known operations.

The graph may retain potential abnormal edges while analysis determines applicability. Every edge starts at the state
actually reached by its operation. No normal postcondition is visible on a call's panic edge.

### State, transfer and merge

Fold the storage-relevant parts of `ExecutionState` into `StorageFlowState` under the existing composite domain.
Keep predicate evaluation helpers and proof terms in their owning modules. Sharing state does not mean putting all
algorithms in one large file. Mutation-sensitive refinement availability belongs in this same state, because its
invalidation depends on references and authority. Retain structural branch/pattern seeds as inputs. Publish
CheckedRefinements as a projection instead of running an independent mutation-sensitive refinement solve.

Use the following transfer order for one operation:

1. Resolve the source/destination paths and active variants from the incoming state.
2. Check the operation's initialization, ownership, capability, witness and declared requirements against that state.
3. Capture entry facts needed by the selected call or guarded guarantee.
4. Apply consumption, writes, epoch changes and dependency transfers in language evaluation order.
5. Split normal and abnormal successors where execution can differ.
6. On each successor, apply only the guarantees and completed lifecycle steps established on that successor.

A rejected operation produces its specific structured diagnostic and conservative recovery. It never publishes the
authority that successful execution would have produced. Diagnostics come from stabilized deciding states. A solver
may replay local transfer to emit diagnostics or project results without performing another independent proof.

For joins, use component-specific rules:

| Component | Join |
| --- | --- |
| Reachability | Ignore unreachable predecessors. Retain any possible reachable execution. |
| Live owners, loans that may conflict, dependencies, possible effects and possible referents | Union. |
| Definite initialization, definite absence, definite active capability, available ordinary/trusted condition | Intersection, under the same presence/variant guard. |
| Exact reference or unchanged entry-slot relation | Retain only if every relevant incoming execution agrees. Otherwise keep a conservative target set. |
| Carrier/provenance | Retain a guarantee only with a valid carrier on every relevant incoming path. Preserve branch-dependent carrier alternatives rather than equating identities. |
| Lifecycle progress | Retain possible pending work. Mark a step discharged only where every relevant path has resolved it. |
| Recovery | Remains explicit and cannot strengthen proof. |

Paths through an absent nullable or inactive union alternative do not supply a fake contained owner. They do not erase
a guarded fact about the present alternative either. The existing dependency guard representation should carry this
distinction through storage, returned contracts and interfaces.

Repeated execution of an allocation or construction site is not the same runtime epoch. Use entry identities and
flow-local creation identities with strong invalidation on overwrite. At loop joins, lose a must-identity when different
incarnations can meet. A syntactic allocation ID alone must never prove that an older pointer reaches the current object.

### Calls, effects and finite summaries

The summary is a projection of checked behavior over formal subjects. It contains only caller-required facts:

- Lifetime/capability and provider/attachment requirements.
- Guarded possible reference targets and exact entry-to-exit reference relations where proved.
- Reads relevant to condition observations, writes, initialization changes, retirement and escaping dependencies.
- Invocation versus deferred effects, possible panic/cancellation, and normal result/receiver guarantees.
- Witness carrier/provenance relationships needed to preserve existing trusted contracts.

Keep runtime outcome cases distinct from analysis uncertainty. A conservative effect on a known reachable input does
not imply arbitrary mutation of an unrelated local marker. Conversely, a shared reference can reach interior-mutability
operations, so shared input is not a purity certificate.

For unknown or erased callees, use the callable's checked contract. If it supplies no narrower effect, conservatively
include the storage reachable through its inputs and admitted ambient roots, with the possible operations its authority
permits. A trusted foreign boundary may assert a narrower contract. Ordinary source may not obtain that assertion by
renaming a function or placing it in a runtime package.

For compiler-known operations, instantiate their closed effect directly. For example:

- Address/offset/layout operations do not write or consume contents.
- Copy reads preserve initialization. Noncopy reads consume it.
- A zero-length copy performs no element transition.
- A spare-buffer write changes the described allocation range, not the buffer header merely because it is the anchor.
- Moving from Uninit clears its tracked initialized contents.
- Replacement restores initialization before propagating old-cleanup failure.
- Atomics and volatile operations preserve their actual synchronization/observation effects even when no ordinary
  ownership changes. An atomic load is not pure observation of an immutable value.
- Deallocation retires allocation-dependent evidence. Equal addresses cannot restore it.

For recursive source summaries, use an explicit demanded call component, not nested queries that request their own
published result. Reuse the existing fixed-point worklist and compilation dependency tracking.

Possible effects and dependency requirements form finite may sets. Exact relations and guarantees form finite must
candidates drawn from the body's selected subjects and declared terms. Within a recursive component, solve the
correlated equations before publishing any member. A recursive assumption does not introduce a trusted condition.
Authority must originate in a declared input, a valid transfer or an explicit trusted producer. Total certification
additionally requires the existing well-founded proof and rejects live recursive proof cycles. A non-unwinding loop
does not thereby become total.

Finite paths come from selected accesses and formal projections. Recursive projection growth uses a conservative
reachable-suffix summary at the repeated type/path cycle. It does not enumerate arbitrary heap paths. Terms at joins
use stable graph identities, not a new nested expression per solver round. Existing deterministic proof limits remain
explicit. Failure to prove a relation yields unknown, never an invented equality or authority.

This bounds the abstract domain for a demanded component. Each worklist update must move monotonically in its
component's information order. May components use subset inclusion and union. Must components use reverse inclusion
and intersection over the fixed candidate universe. Unreachable is a separate bottom state. Incompatible exact values
join to a target set or unknown. Epoch freshness is never inferred from equality of a repeated allocation site.
Do not use an arbitrary iteration counter as the explanation for convergence.

Backwards liveness remains a separate directional analysis over the shared operations. Where finalized feasibility
changes its input, invalidate that dependent result through the owning stage. Reusing a graph does not justify using
stale liveness, and "one analysis" does not mean one traversal regardless of data dependencies.

### Cleanup selection and execution

Select structural lifecycle targets before flow, without assuming that every potential step runs. Flow decides which
initialized parts and obligations apply at each site. This removes the current dependency on a completed async cleanup
plan merely to interpret a destructor's effects.

Use the same transition routine for explicit lifecycle calls, implicit scope exit, replacement and represented parts.
The inputs vary by exit reason, initialization state and selected receiver. Those are genuine semantic distinctions.

Lowering consumes that completed selection and expands execution together with symbolic backing once. Source lowering
adds continuations and initialization guards. Specialization supplies concrete layout and descriptors. Property queries
read the selected actions instead of recursively rediscovering finalizers and destructors.

The replacement subgraph is fixed:

`destination -> RHS owner -> old cleanup under shield -> install RHS -> propagate selected outcome`

Failure during destination or RHS evaluation uses the partial ownership state reached before replacement begins.
Failure during old cleanup cannot bypass installation. A pending RHS is an owner with its own dependencies and allowance.
The same applies through a mutable borrow, a projection, a nullable destination or a union.

The scope subgraph is fixed:

`selected with exit, if entered -> task broadcast -> dependency-ordered lifecycle -> pending transfer`

This notation does not combine nested lexical scopes. Each scope resolves its own owners in the specified order.
A finalizer's normal Error remains an ordinary result and is handled according to the surrounding cleanup context.
Destruction, release and payload quiescence retain distinct steps.

### Interfaces and phase trust

Publish formal relationships, not local solver maps or graph IDs. Use structural identities, exact generic substitutions
and selected witnesses. Guarded presence/variant relations travel with the same dependency template in source and imports.

A predicate-valid const accessor returning a field view needs a checked relation such as "on present input, present result
refers to this field". Infer it from selected return/projection operations and their guards. Instantiating that relation
is sufficient for nullable acquisition reasoning. It requires neither arbitrary const-body evaluation at each call nor
a library-name exception. Arbitrary helpers with no provable relation keep their conservative contract.

Validate schema, references and provenance at external decoding. Update semantic/schema identity when meaning changes.
Within the compiler, downstream consumers rely on the completed result and assert impossible states. MIR validation
checks MIR's own structural contract. It does not repeat source authority analysis.


## Acquisition and protocol authority without a new language feature

### Carrier and observation are separate

The complete guard returned by the acquisition operation is the witness carrier. An immutable empty marker is only the
argument observed by the acquisition predicate. The marker is not a separately minted transferable acquisition token.

This distinction corrects the earlier proposal. Merely putting a noncopy empty token next to a lock reference leaves
its pairing argument underspecified. The producer's checked result contract must retain the complete carrier, its field
projection and its dependencies. Do not reduce provenance to the smallest observed field.

The existing witness specification states that guarantees can belong to a returned value while referring to its fields,
that ordinary field observation/copy does not copy authority, and that a partial move separating a guarantee from its
carrier is rejected. This design uses those rules. It does not add a rule that every aggregate containing an independent
witness becomes permanently indivisible. A pre-existing independently owned witness retains its own transfer contract.
Here the acquisition producer introduces the guarantee on the returned complete guard. A tuple producer such as host
preparation can instead return several carriers and explicit relationships between them. Destructuring preserves those
relationships only when all their carrier/dependency requirements survive. Do not make every returned aggregate one
indivisible carrier merely because a guard requires that treatment.

Choose an immutable lock reference, an immutable owned marker and whole-value lifecycle on that guard. There is no safe
factory that returns a held marker alone. The predicate takes an observational borrow of the marker, preserving its semantic identity separately from constant value equality. The opaque predicate observes the marker, so a payload-content change cannot
change its observed state. The complete carrier separately retains the lock/capability dependency.

This permits payload mutation while preserving acquisition. It does not preserve an arbitrary predicate over the whole
guard, and does not weaken invalidation of payload-dependent predicates.

| Operation attempted by safe code | Deduction |
| --- | --- |
| Construct matching fields, including a new empty marker | No trusted producer established the complete carrier's guarantee. Field equality is insufficient. |
| Copy the marker or reconstruct it as a constant | Ordinary field copying carries no authority. Zero-sized layout does not equate semantic owners. |
| Extract the marker, including directly from a fresh acquisition result | Extraction separates the newly established complete carrier from its guarantee. R04 applies before the transfer, not only at eventual destruction. |
| Partially move the lock field and substitute another | Whole carrier use and lifecycle need complete initialization. Immutable field assignment cannot repair it. More directly, the authority's dependency/pairing cannot be transferred by field reconstruction. |
| Diverge after extracting a marker | Does not help. Witness separation is rejected at extraction, independently of whether an eventual destructor is reachable. |
| Move the entire guard into another binding, wrapper, return or frame | Transfer the same carrier and dependencies. Old path loses authority. |
| Borrow the marker and fabricate another guard around that borrow | A marker borrow is not the guard's owned marker and does not copy its carrier provenance. |
| Mutate the payload through get | Physical target is the payload. Marker observation is unchanged. Guard and lock dependencies remain live. |
| Release while a payload borrow remains active | Exclusive dependency conflict rejects it. |
| Reassign or replace the complete guard | Resolve the old acquisition, then install the newly produced owner's contract. Equal addresses do not reuse authority. |
| Release and reacquire the same lock | Consume the old acquisition. Only the new successful acquisition establishes the new guard's authority. |
| Copy or split a read guard | Requires an explicit checked copy/split contract and real reader acquisition, not byte copying. Otherwise reject. |
| Pass through a generic helper or encoded interface | Same carrier/projection/dependency relation after substitution. No source-name recognition. |
| Forge through an internal helper | Internal acknowledgement grants access only. Every helper that creates a usable view must require the corresponding protocol authority. |
| Use an explicit trusted boundary to assert missing obligations | Remains an explicit trusted action. Safe-code guarantees do not erase Bray's intentional trust boundary. |

For try-acquisition, the guarantee is guarded by result presence. The present payload is the complete guard carrier.
Unwrapping that nullable wrapper transfers the complete contained carrier. Extracting the marker remains a partial move.
The const accessor's checked guarded projection relates the predicate argument to the present guard's marker. Nullable
absence creates no owner and requires no release.

### Protected protocol state

Put mutable protocol representation in existing Uninit storage. Safe accessors return only the authorized payload view.
A public or internally acknowledged helper must not expose control atomics through an unrestricted shared State borrow.
Atomic interior mutability makes that an authority leak even when the borrow is shared.

Trusted operations may share address calculation, but usable read/write guarantees require the protocol witness that
authorizes the exact transition. Constructors establish a valid whole-instance protocol carrier. Merely fabricating a
ready flag plus uninitialized storage cannot establish that carrier.

For Once, use an immutable instance marker and immutable protected state field in the complete lifecycle-bearing owner.
The constructor establishes an empty valid instance without acquiring a runtime service. A claim operation creates an
attempt carrier bound to that instance and exact thread. Runtime state still records empty, initializing, reentered and
ready. These are real concurrent states, not erased witnesses.

Publication writes and commits T before releasing ready. Reentry invalidates the active attempt even when the nested
panic is caught. Rollback consumes the attempt and returns the cell to its specified retry state. A ready observation
requires both a valid instance and the actual acquire publication transition. Destruction has exclusive instance
authority, selects its actual state and destroys T only if initialized.

The compiler does not prove the concurrent atomic protocol from an empty marker. That bounded trusted implementation
must obey the protocol table. Ordinary callers cannot fabricate its input authority. Both sides are necessary.

Static constructors materialize fresh instance provenance at the exact static destination. They cannot copy an already
live acquisition or obtain runtime resources during const evaluation. Generic constant reuse must distinguish a recipe
for a new owner from duplication of a live witness. Use the actual constructor for the bootstrap report pool.

## Complete runtime protocol transitions

The following table identifies the publication point, intermediate failure owner, terminal owner and live prerequisites.
Compiler rules above apply to every row. Atomic and foreign transitions remain explicit trusted implementation duties.
"Commit" means the point where ownership actually changes, not a later flag written after publication.

| Protocol | States and commit | Failure, reentry and terminal transitions | Existing code and open owner |
| --- | --- | --- | --- |
| SpinLock / Mutex | Valid instance -> acquiring -> held guard -> released. Only successful ticket/lock acquisition publishes the guard. | Failed try-acquire publishes no guard. Live payload borrow excludes release. Release runs once before instance destruction. No allocation or callbacks inside the transition lock. | std/sync spin_lock and mutex. BRA-620. |
| RwLock | Valid -> reader guards or one writer -> released. Overflow rejection occurs before reader-count commit. | Failed/overflow path owns no new acquisition. Existing readers retain authority. Writer and reader release contracts differ. Counter atomics cannot escape through safe state views. | std/sync rw_lock. BRA-620. |
| ConditionVariable | Consumed held guard -> released lock + retained owner borrow -> generation wait -> newly acquired returned guard. | The current API consumes its guard. Failure after release owns no acquisition to restore or unlock. Keep the mutex alive and initialized. Successful reacquisition alone mints the result carrier. A future borrowing wait API would need a different restoration contract. Do not add that unused protocol now. | std/sync/condition.bray. BRA-620. |
| Once / Lazy | Empty valid instance -> exact-thread attempt -> committed T -> ready publication. | Ordinary error/panic/cancellation consumes attempt and rolls back. Same-cell reentry marks attempt invalid before panic. Catching nested panic cannot publish. Ready readers retain instance and T. Destroy initialized T once. Lazy delegates rather than adding another initialization protocol. | std/sync/once.bray. BRA-620. |
| Channels and structured thread owners | Transfer elements/results only into initialized, admitted storage. Sender/receiver closure and child terminal publication have explicit owners. | Failed send/start preserves caller ownership. Cancellation is not child completion. Unobserved terminal payload follows its lifecycle. No raw control-state access through sync helpers. | std/sync and std/thread consumers. BRA-620 authority audit, BRA-608 native completion. |
| Host bootstrap / resident storage | Unformed -> forming -> ready -> closing -> terminal. Reserve startup backing before publishing host. Resident anchor retains storage, lease authorizes current synchronized access. | Formation failure disposes accepted parts. Reentry must see a defined forming/ready state. Allocation occurs outside host lock. Anchor cannot substitute for lease. Final callback and local cleanup return before last backing release. | runtime/bootstrap/src/host and heap. BRA-553 retained, BRA-620 contract recheck. |
| Exact-thread attachment | Unattached -> prepared record -> TLS-published attachment -> nested depth -> closing -> detached. | TLS store failure retracts record/count exactly once. Nested entry validates exact lifetime identity. TLS destructor clears TLS before callback. A reused native thread number grants no old authority. | host/attachment, thread/destruction. BRA-553 retained, BRA-557 complete TLS domain. |
| Synchronous/foreign entry | Valid product/context -> admitted boundary -> active entry -> outcome delivered -> restored outer context. | Admission fails before accepting inputs. All exits restore complete outer run/cancellation/output/lane context. No language unwind crosses native ABI. Callback references retain provider through resident return. | Rust native/callback, native/export and generated host adapters. BRA-554. |
| Cleanup allowance | No obligation -> secured local backing -> published owner -> active cleanup or transferred incident/result -> final release. | Admission/activation rejection preserves previous owner. Partial construction retains children. Source discharge cannot release transferred backing. Provider release follows last storage user. | bootstrap outgoing/pool, realization/outgoing, LLVM outgoing. BRA-502. |
| Reports and typed incidents | Caller destination -> owned primary/typed payload -> suppressed chain or sink -> quiescent payload -> synchronous disposal. | No new allocation needed for mandatory transfer. Error payload children quiesce before incident retention. Detach collection before callbacks. Preserve reports raised during disposal. Release payload backing before final provider lease. | bootstrap reports, Rust incident/frame, static finalization lowering. BRA-558 with BRA-502. |
| Product statics | Validated formation graph -> admitted static owners -> closed entry -> eligible teardown set -> consumer-before-provider cleanup -> terminal. | Failed formation keeps cleanup services for accepted state. External roots block eligibility. Internal static edges order teardown rather than blocking it. Drain teardown incidents before provider loss. | Rust product/host and product/cleanup, existing compiled static graph. BRA-556. |
| Thread statics | Product formed + exact attachment -> admitted TLS domain -> closing -> exact-thread cleanup -> acknowledgement -> detach. | Failed admission accepts no TLS owner. Product closure cannot migrate cleanup. Worker join follows acknowledgement and occurs outside host locks. | Rust product/attachment, native/static_finalizer and Bray attachment. BRA-557. |
| Loaded provider | Validated image/service binding -> formed product -> retained entries/values -> closed admission -> eligible cleanup -> final resident release -> unload. | Retained values keep code and statics. Failed formation preserves services for rollback. Reject stale load epochs. Last image callback returns before unload. Independent loaded images do not share identity accidentally. | Rust product/host, product dependency graph and platform loader. BRA-555. |
| Inactive and direct-await frames | Admitted inactive owner -> stable active frame -> composed child -> result moved -> remaining cleanup -> release. | Failed activation retains inactive owner. Active parent and child share one run. Retired backing survives result/internal users. Recursive or erased layouts use owned indirect storage. Iterative driver avoids native recursion by async depth. | Rust native/frame and native/run, generated frame descriptors. BRA-561. |
| Started task / observation | Admitted task -> queued/running/waiting -> lexical cleanup complete -> terminal -> one result observation -> resolved. | Start rejection preserves frame. One dispatcher and one source resolution owner. Observation registration does not duplicate result ownership. Queue/wait users retain storage independently until they finish. | Rust task and scheduler. BRA-560. |
| Wait / wake / cancellation | Observe -> register -> recheck -> wait or withdraw -> committed delivery -> callback returned. Cancellation request sets state and wakes eligible work. | Signal racing registration is caught by recheck. Signal racing dispatch release sets pending wake. Withdraw only uncommitted registration. Committed target lives until callback returns. Shield masks delivery, not request observation. | Rust event, cancellation, scheduler/wait. BRA-562/564. |
| Lanes / deadlines / workers | Ready work acquires compatible lane slot -> dispatch -> suspension/terminal releases slot. Deadline owns ordered registration. Worker owns attachment through acknowledgement. | Admission limits backing separately from running quota. Deadline cancellation withdraws or completes delivery. Retained frame dependencies constrain migration. No self-join, callback under queue lock or worker bootstrap through its own high-level service. | Rust lane, timeout, scheduler, native/workers. BRA-563/565/566. |
| Root / shutdown / fallback | Root cleanup -> terminal publication -> host observation -> close entry -> eligible domain cleanup -> reports drained -> services released. | Blocked graceful attempt returns retained owner. Abnormal abandonment releases caller control but resident fallback retains storage AND execution/affinity/reporting. One terminal state prevents duplicate cleanup. Dependency release schedules newly eligible fallback. | Rust root/shutdown and product host. BRA-567/507. |
| Child test host | Session setup -> selected test run with shared product statics -> captured outcome -> protocol delivery -> product cleanup -> finish. | Timeout requests cancellation and waits for specified resolution. Product cleanup failure is product-owned. Coordinator serial exclusion remains external tooling policy. | bray-runtime-adapter test_host and test_entry. BRA-559. |
| Packaging / target delivery | Checked semantic demand -> concrete retained code -> selected artifact units -> linked product. | Invalid external ABI/schema fails at import/formation. Missing required native implementation is a build failure. No Rust fallback silently satisfies a migrated role. | compilation product planner, runtime catalog, xtask runtime_artifact. BRA-568/192/588. |

The actual condition-wait API consumes its input guard before waiting. Its exceptional path therefore differs from
replacement, which must restore a borrowed destination. Reusing ownership rules does not mean imposing a generic
transaction wrapper on both. Once the consumed guard releases, failure cleanup preserves the borrowed lock and has no
remaining acquisition to discharge. This removes the need for the restoration mechanism considered in the earlier
protocol draft.

For any operation that does retain an owner across a temporary invalid state, every modeled source-visible exit must
restore or transfer that owner's obligations. A return code or active flag alone cannot establish the invariant.
## Outcome coverage and composition

These are the cases each semantic operation must distinguish. Do not flatten Error, panic, cancellation and divergence
into one "does not complete" bit.

| Boundary | Ordinary result or transfer | Panic/cancellation | Partial or rejected operation |
| --- | --- | --- | --- |
| Ordinary call | Apply checked normal postconditions, including Error results. Transfer the returned owner. | Retain only facts proved for the reached abnormal exit. No ordinary result and no normal ensures. | Argument evaluation resolves its own committed ownership. Uncalled callee guarantees are unavailable. |
| Constructor/admission | Publish complete owner only after its local needs are secured. | Resolve committed parts and preserved inputs using their actual states. | Failed admission publishes no new obligation. |
| Replacement | Resolve old state, install already evaluated new owner. | If old cleanup began, finish restoration then propagate. Panic takes precedence over cancellation. | Before replacement begins, retain moves already performed while evaluating inputs. |
| Finalizer | unit/Ok resolves its selected graceful step. Error uses its checked completion postconditions. | Attempt under shield, retain incident, abandon graceful step where the language permits it. | Retryable Error retains initialized owner and unresolved obligation. |
| Destructor | Consume self and destroy remaining initialized parts once. | Preserve infallible result contract and the specified handling of cleanup panic. Never use panic as an excuse to repeat resolved parts. | Partial values skip whole-value lifecycle and resolve only initialized parts. |
| with enter | On successful entry create capability and execute body. | No matching exit if entry did not succeed. | Preserve resource/input ownership according to actual entry receiver mode. |
| with exit | Normal/yield result contributes to expression failure contract. | Pending return/break/continue/propagation continues while exit Error becomes an incident. Cleanup panic follows primary/suppressed precedence. | Capability is live during exit and expires afterward unless explicitly transferred. |
| Await | Completion publishes deferred ensures and result once. | Commit cancellation/panic cleanup without a normal result. | Inactive frame retains captures and obligations if never driven. |
| Join/cancel observation | Transfer terminal payload once with completion visibility. | Cancelled remains payload-free. Route incidents to host or report according to terminal outcome. | A cancellation request does not consume the terminal ownership duty. |
| Static/host terminal cleanup | Drain eligible domains and incidents before service/provider release. | Abandon failed graceful work, destroy once, continue independent nodes. | Blocked graceful shutdown retains caller owner and resident fallback. |

Divergence has no successor to which a postcondition can be applied. It does not discharge a live owner merely because
the analysis cannot see termination. Catastrophic abort/hardware termination are outside the source cleanup guarantee
and must remain explicitly distinguished from recoverable runtime failures.

The obligations compose without adding a special algorithm per consumer:

- A guard stored in a nullable, tuple, box, returned product or inactive frame remains the same owned carrier under
  R05/R06/R16/R22. Each wrapper adds only its own real lifecycle work.
- A payload borrow retains the guard's capability while pointing into the lock. R07/R08 prevent conflicting unlock.
- A typed assignment through that borrow preserves the initialized payload on every exit after cleanup begins under
  R11. The surrounding initialized State therefore remains initialized unless another operation actually removes a
  part. Payload-value predicates may still be invalidated.
- An incident outliving its frame transfers backing under R24 and provider/attachment roots under R28. Frame completion
  alone cannot release either.
- Task cleanup inside an error, report-construction input or inactive join frame uses the same initialized ownership
  traversal. Cancellation broadcast precedes any child wait.
- Product closure excludes new acquisitions but does not invalidate the existing operations required to dispose of
  already accepted values. Eligibility and authority are separate decisions.

The whole-state safety argument is inductive. Entry has only declared input authority and freshly established local
state. Each listed transition preserves unique ownership, correct initialization and valid dependencies or rejects.
Joins cannot strengthen those facts. A trusted producer is the explicit proof boundary for its physical protocol.
Lowering preserves the selected order and layouts. This argument does not assume any particular lock type name.

## Source owner and removal ledger

Delete a replaced mechanism in the same coherent change that switches all its consumers. Retain tests for its required
behavior. Do not preserve an old path as a fallback when the new path cannot prove something.

| Current source | Keep | Replace/remove |
| --- | --- | --- |
| [body_semantics.rs](../../crates/bray-checker/src/body_semantics.rs) | Unit-owned orchestration and immutable result publication. | Early trusted-memory solve followed by storage flow followed by a separate final trusted-completion flow. |
| [analysis/model.rs](../../crates/bray-checker/src/analysis/model.rs) and [storage plan](../../crates/bray-bound-tree/src/storage/plan/model.rs) | Shared graph, typed exits, occurrence IDs and planned accesses. | Independent rebuilding for property-specific operation discovery. Add only missing semantic stages such as replacement restoration. |
| [storage_flow/model.rs](../../crates/bray-checker/src/analysis/storage_flow/model.rs) and [guarantee/state.rs](../../crates/bray-checker/src/analysis/guarantee/state.rs) | Initialization, loans, provenance, conditions and correct existing transfer logic. | Parallel live-owner/authority state. Fold correlated facts into the composite owner. Keep distinct resource epoch and carrier identities. |
| [refinement/check.rs](../../crates/bray-checker/src/analysis/refinement/check.rs) | Structural condition seeds and CheckedRefinements output. | Independent mutation-sensitive availability transfer. Project it from the composite state that owns mutation and alias effects. |
| [storage/expression/access.rs](../../crates/bray-checker/src/storage/expression/access.rs) | Selected result type and symbolic returned access construction. | Using a single lifetime source as physical referent proof. |
| [checked reference publication](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/references/publication.rs) | Portable formal Borrow/Address relationships. | Independent syntax traversal for exact origin and unchanged slots, whole-body exclusions for anonymous callable presence or implicit Error. Project flow results instead. |
| [storage_invalidation.rs](../../crates/bray-checker/src/analysis/storage_invalidation.rs), [flow/invalidation.rs](../../crates/bray-checker/src/analysis/guarantee/flow/invalidation.rs), [memory/classification.rs](../../crates/bray-checker/src/memory/classification.rs) | Closed checked memory operation meaning and state-sensitive extents. | Repeated hook lists and downstream restoration after global invalidation. Classify once, apply to actual targets. |
| [trusted/completion.rs](../../crates/bray-checker/src/analysis/guarantee/trusted/completion.rs) and [trusted/check.rs](../../crates/bray-checker/src/analysis/guarantee/trusted/check.rs) | Trusted producer/caller obligations and structured diagnostics. | Inferring transferable authority merely from observed-field overlap. Fresh-result bypass must not skip witness-separation checks. |
| [cleanup/transition.rs](../../crates/bray-checker/src/analysis/guarantee/cleanup/transition.rs), [plan.rs](../../crates/bray-checker/src/analysis/guarantee/cleanup/plan.rs), [check.rs](../../crates/bray-checker/src/analysis/guarantee/cleanup/check.rs), [asynchronous/execution.rs](../../crates/bray-checker/src/asynchronous/execution.rs) | Already shared cleanup transition and genuine lifecycle distinctions. | Multiple structural expansions and all-witness erasure followed by re-establishment. Use selected calls and actual effects. |
| [checker unwind](../../crates/bray-checker/src/analysis/guarantee/unwind.rs), [compilation unwind](../../crates/bray-compilation/src/compilation/execution_guarantees/unwind.rs), [behavior.rs](../../crates/bray-compilation/src/compilation/behavior.rs) | Non-unwind/termination distinction, imported contracts and property-specific proof rules. | Separate body/cleanup preparation and per-root closure scans where the demanded component already supplies the same edges. |
| [replacement.rs](../../crates/bray-lowering/src/lowering/replacement.rs), [scoped.rs](../../crates/bray-lowering/src/lowering/scoped.rs), [synthetic lifecycle](../../crates/bray-lowering/src/synthetic/lifecycle.rs) | Restoration, pending outcomes, initialized-part cleanup and common generated lifecycle. | Re-selection of semantics or a second capacity-only expansion. |
| [outgoing realization](../../crates/bray-compilation/src/compilation/product/realization/outgoing.rs) | Approved unconditional-total reasoning and uniform type-level allowance. | Scalar records as a substitute for general physical backing. Extend the existing realization owner. |
| [static finalization](../../crates/bray-codegen-llvm/src/mapping/static_storage/finalization.rs), [native/frame.rs](../../crates/bray-runtime/src/native/frame.rs) | Observable failure/payload semantics and useful fixtures. | Allocation during mandatory typed-error transfer and u128-limited separately allocated frame/result backing. Use admitted concrete layout. |
| [sync](../../standard-library/std/src/sync), [host](../../runtime/bootstrap/src/host), [record pool](../../runtime/bootstrap/src/record_pool.bray) | Actual synchronization algorithms, attachment ownership, slab/record transport and platform bindings. | Forgeable protocol control access, whole-guard opaque acquisition dependency, stale report-pool aggregate initialization. Do not create another pool. |
| [runtime role catalog](../../crates/bray-runtime-abi/src/catalog/execution.rs), [platform catalog](../../crates/bray-runtime-abi/src/catalog/platform.rs), [runtime role interface](../../crates/bray-runtime-interface/src/role.rs) | Closed typed roles and demanded service distinction. | Source spelling as authority or a parallel role registry. |
| [runtime consumers](../../crates/bray-runtime/src), [runtime adapter](../../crates/bray-runtime-adapter/src), [runtime artifact builder](../../xtask/src/runtime_artifact) | Existing behavior until its consumer has a completed Bray replacement. | Rust implementation of each migrated consumer, duplicate task/activation descriptions and final archive partitioning support once unused. |

Keep the distinction between input snapshots and current state, borrow and address, provenance and ordinary truth,
resident anchor and live lease, task cancellation broadcast and lifecycle resolution, result ownership and storage
retention. These are required concepts, not duplication.

Keep code organization conventional. Use existing domain modules, thin roots when splitting, submodule names instead
of prefixed siblings, and structured diagnostics. No new crate, general protocol framework, effect-annotation language,
runtime witness registry or public request/context wrapper is justified by this design.


## Implementation order

Each unit below replaces a complete mechanism and includes all applicable rules from the map. The list is not an
instruction to implement only the example consumer mentioned beside it.

### 1. Preserve the stack and fix the shared compiler owner

Keep the existing repair worktree. Save a recoverable checkpoint of all tracked and untracked work before restructuring.
Retain the original issue branches and their PR history. The current combined tip already includes the five previously
verified PR heads. Refresh live ancestry immediately before publication.

Under BRA-620, with BRA-609's obligations preserved:

1. Make the selected operation/exit inventory exhaustive and prepare its shared graph, call mappings, memory operations
   and structural lifecycle targets once.
2. Integrate reference transfer, initialization, provenance/carriers, observation invalidation and lifecycle progress in
   composite storage flow. Cover every R01-R23 and R25-R30 case relevant to checking, including fresh-result extraction.
3. Publish guarded reference/effect/carrier summaries through the existing dependency/interface owners. Close the
   recursive component's equations before publishing. Remove the independent origin walk and preliminary/final
   authority solves.
4. Switch execution-property checking and cleanup consumers to those completed facts. Remove redundant structural
   traversal. Keep distinct guard domains, backwards liveness and concrete layout demands.
5. Complete source/imported/generic/indirect coverage as part of this same change. Do not postpone imported semantics
   until after production synchronization happens to work.

Finish this unit only when every affected rule has one state owner, all its variants are routed there, and the removal
ledger has no surviving competing implementation. A compile success or one repaired lock test is insufficient.

### 2. Apply the authority representation to all current consumers

Still under BRA-620, apply the complete-carrier/marker representation and protected protocol boundaries to SpinLock,
Mutex, RwLock and Once/Lazy. Preserve the consuming ConditionVariable API and its simpler released-failure path.

Audit sibling events, semaphores, barriers and blocking-word helpers. They need their declared value and concurrency
invariants, but no acquisition witness unless they grant access to owned protected data. In particular, an observational
thread ID or a bare atomic-word lock operation must not be accepted as storage authority.

Cover memory Output/InPlace/anchored views, buffers, channels, thread wrappers, all foreign outputs, and every internal
helper capable of returning a usable protected view. Recheck the retained BRA-621 and BRA-553 consumers using these
same contracts. Repair the bootstrap report-pool initializer through its actual constant-valid constructor.

This unit completes the currently known authority repair. It must not introduce library-name recognition, additional
protected types, per-lock identity checks or standalone token factories.

### 3. Finish backing and current-stack native acceptance

Implement BRA-502's physical allowance service and concrete requirement projection over the selected cleanup expansion.
Then BRA-554's entry owner and BRA-558's incident/sink path consume that same service. Complete BRA-619's pending native
scope-exit acceptance using those consumers.

This includes synchronous and asynchronous scoped forms, entry failure, every pending transfer, and typed exit errors
under denied allocation. Existing activation/runtime components may remain only for the explicitly unfinished runtime
migration issues. Their use is not evidence that the new Bray incident or admission consumer is independent of Rust.
Audit the migrated consumers separately and retain the remaining consumers' open owners.

Merge the current stack into the agreed feature integration branch, not develop issue by issue. In practice, branch
from the checkpointed combined repair and incorporate only genuinely missing commits. Do not backmerge descendants into
old BRA-620, replay already contained changes or rewrite the old branch stack.

Open one integration PR to develop once the five current stack contracts and their required native acceptance are
complete. Retain review history and reconcile their Linear/PR links. Retire the superseded PRs only with the final
integration accounted for. This is a proposed publication sequence, not authorization to merge.

### 4. Complete runtime protocols using the established contracts

Proceed in dependency order from the landed integration. The protocol table determines implementation, not a Rust-file
translation. Every migrated consumer switches its real callers and removes its replaced Rust policy in that unit.

| Delivery | Existing open owners | Complete mechanism |
| --- | --- | --- |
| Synchronous products | BRA-556, BRA-557, BRA-608 | Product teardown, exact-thread TLS cleanup and zero linked Rust for the complete synchronous product. |
| Loaded providers | BRA-555 | Formation, whole-product retention, closure and resident final release. Use the same host terminal state. |
| Activations and ordinary cleanup | BRA-561, BRA-505 | Immutable generated frame descriptions, admitted activation, iterative same-run composition and ordinary async/fallible lifecycle. BRA-561 can proceed after backing/host prerequisites without waiting for all provider work. |
| Represented owners and APIs | BRA-506, BRA-508, BRA-484 | Partial/array/erased ownership, storage policies, generator construction and resource APIs using the same completion and admission rules. |
| Tasks and dispatch | BRA-560, BRA-562, BRA-563, BRA-564, BRA-565 | One source resolution owner, registration/delivery ownership, cancellation, lane quotas, event readiness and deadlines. |
| Worker/root/fallback | BRA-566, BRA-567, BRA-507 | Demanded workers, attachment acknowledgement, main-lane root and retained suspending fallback. Reuse synchronous host state. |
| Test products and artifacts | BRA-552, BRA-559, BRA-568 | Complete test catalog, child session/capture, and demanded Bray artifacts with replaced Rust archive support removed. |
| Final native and cost acceptance | BRA-192, BRA-540, BRA-541, BRA-542, BRA-588 | Six-target native provenance/conformance, execution, memory/footprint, full compilation and optimized-demand evidence. |

The intermediate integration does not promise that all later runtime migration is already implemented. It does promise
that remaining work is these named complete protocols, using the agreed semantic foundation. Discovery of a requirement
outside this model suspends the affected implementation for a model correction and affected-rule audit.

## Linear reconciliation

All 45 issues below were read and confirmed open on 2026-10-12. Closed/completed/cancelled/duplicate issues are excluded
from the active plan. None will be reopened. The follow-up issue preparation records this contract in those open issues.

Retain the existing issue identities. Update their descriptions and acceptance links to this agreed plan, rather than
create one issue for every helper, missed branch or test. The user explicitly authorized associated compiler repairs
inside BRA-620. No additional independent subsystem was identified that lacks an existing open owner.

| Issues | Change to their plan |
| --- | --- |
| BRA-609, BRA-620 | Link R01-R18 and R25-R30 plus the compiler removal ledger. BRA-620 owns consolidation and production authority repair. BRA-609 retains the authority acceptance contract, without a second implementation effort. |
| BRA-621, BRA-553 | Preserve delivered implementation and evidence. Recheck their real declarations on the consolidated tree. Do not redo the bootstrap migration or infer completion of the whole stack from their In Review status. |
| BRA-619 | Preserve selected scope invocation. Retain BRA-502/558 native blockers. Acceptance includes async forms and the complete outcome table. |
| BRA-502, BRA-554, BRA-558 | Link allowance, entry and incident protocols. Share backing and report service. No independent capacity walker, report pool or fallback admission route. |
| BRA-556, BRA-557, BRA-608, BRA-555 | Link one product/attachment terminal model and whole-product retention. BRA-608 is the complete synchronous consumer checkpoint. |
| BRA-561, BRA-505, BRA-506, BRA-508, BRA-484 | Link the same activation/cleanup and represented-owner rules. No second generator/array initialization or cleanup framework. |
| BRA-560, BRA-562, BRA-563, BRA-564, BRA-565, BRA-566, BRA-567, BRA-507 | Link the explicit task/wait/lane/worker/root/fallback transitions. Preserve current dependency order and one terminal owner. |
| BRA-552, BRA-559, BRA-568, BRA-192 | Catalog completeness can proceed early. Final native completion requires the normal shipped product pipeline and actual retained implementation provenance. |
| BRA-402, BRA-404, BRA-406, BRA-429, BRA-456 | Keep independent performance attribution and bounded improvements available during migration. Apply regression policy at the first meaningful consumer checkpoint. These do not become another runtime implementation architecture. |
| BRA-540, BRA-541, BRA-542, BRA-428 | Final comparative gates retain the numerical requirements below. A failing row stays open with its owning correction. |
| BRA-574, BRA-588 | Preserve optimized-demand implementation. Complete corpus evidence over the resulting native products, with no duplicate native selector. |
| BRA-281, BRA-547, BRA-548, BRA-482, BRA-409 | Parent outcomes roll up their actual consumers. Close through completed work and normal merge automation, not a planning-document assertion. |

Required relation changes during issue preparation:

- Add BRA-558 as a prerequisite for BRA-505's retained typed-incident acceptance. BRA-561 is already its activation prerequisite.
- Add BRA-502 and BRA-506 as prerequisites for BRA-484's admitted generator/array ownership completion.
- Add BRA-552 as an acceptance prerequisite for BRA-192's complete native test inventory.
- Keep implementation prerequisites separate from final cost evidence. Do not make every functional issue wait for final
  BRA-540/541/542, whose completed-runtime inputs depend on those implementations.

Recheck relations before writing them and reject dependency cycles. Status stays unchanged during issue preparation.
The execution guide records the recovery checkpoint and issue rollout. The original assessment made no external changes.


## Cost model and limits

Correctness does not establish competitive cost. The design below avoids known unnecessary work and states the remaining
costs explicitly. Absolute Rust/C++ comparisons require measurements after implementation. No timing result is claimed
from this assessment.

| Concern | Design choice | Required evidence and rejection condition |
| --- | --- | --- |
| Compiler traversal | One prepared selected-operation graph and one correlated storage/authority lifecycle state owner per demanded body. Property domains share inputs. | Count graph builds, operation preparation, solved states and repeated requests. Reject an added parallel body model or a whole-program scan for a narrow query. |
| Solver complexity | Finite roots/projections/terms. Worklist schedules changed dependents. Recursive call components solve together, with no unbounded symbolic path expansion. | State height and each transfer's monotonicity must follow the join rules. Profile actual queue updates and peak state size. No "retry until it works" pass schedule. |
| Imported compilation | Compact formal summaries with selective decoding. No caller loads a body solely to rediscover a checked origin, effect or carrier relation. | Unchanged imported consumers reuse summaries. Changed contracts/layout/witness dependencies invalidate the exact affected results. |
| Generated guard code | Empty observed marker, complete semantic carrier, existing owner pointer and synchronization. Proof-only operations erase. | No new runtime identity field, allocation, hash table, atomic or helper call for witness reasoning. Check production layout and emitted native path, not only a synthetic empty struct. |
| Mutation and borrowing | Exact referents and disjointness are compile-time facts. Preserve proven alias information into existing lowering/backend contracts. | No runtime checks added to ordinary borrowed access. Do not invent LLVM noalias from lifetime anchoring alone. |
| Cleanup backing | Existing/enclosing storage first where lifetimes allow. Coallocate compatible owners. Retain separate backing only when it must survive. | Record bytes, alignment, multiplicity and last user for each region. Do not reserve global maxima, duplicate child allowances or allocate one heap object per compiler helper. |
| Async execution | One run and iterative activation chain. Known ready direct awaits use enclosing storage and avoid queue/TCB creation. | Zero extra allocation/enqueue per immediately-ready transition. Preserve real suspension, fairness and exact-thread constraints. |
| Synchronization | Locks protect ownership transitions. Execute callbacks, destruction, allocation and worker join outside collection locks. Thread-confined state uses its confinement proof. | No process-wide lock on ordinary moves or payload access. Foreign wake concurrency must be included before removing synchronization. |
| Binary size | Existing optimized demand and artifact granularity select only reachable services and descriptors. Share equivalent generated cleanup where ABI and ownership permit it. | Synchronous consumers retain no scheduler through generic reporting. No unconditional registration retaining optional services. |
| Runtime retention | One provider dependency retains code and statics through last use. One backing owner retains live storage. | Measure retained resources as well as executable bytes. Optional independent diagnostic copies remain explicit, never the ordinary reporting path. |

Let N be selected operations, E graph edges, P observed storage paths and F the finite fact universe demanded by a body
and its imported contracts. A worklist processes affected edges as facts change. A conservative bound is proportional to
E times the domain height times transfer cost, not to the number of concrete execution paths. Set operations can still
be costly in P and F. Use existing indexed identities, sharing and sparse demanded facts, and avoid copying whole
unrelated state per property. Do not promise linear time for arbitrary alias or predicate reasoning.

Declared guard domains may legitimately require distinct executions of the same transfer rules. Backwards liveness is
also distinct. Reusing immutable preparation and rule implementation is required. Caching another copy of a redundant
analysis is not the planned optimization.

The largest unavoidable cost risk remains physical backing and whole-product retention. Those are explicit current
language/design requirements. If measured costs prevent the target performance, bring the specific rule and cost tradeoff
back for discussion. Do not reduce incident fidelity, release live providers, remove target support or weaken authority
to reach a number.

The existing open cost contracts require:

- BRA-540 final matched execution no slower than the faster Rust/C++ peer outside measured variability. The old 1.25
  threshold is intermediate, not final parity.
- BRA-542 each comparable workload and aggregate at most 1.25 times the faster peer for complete process and reported
  work, with complete-process performance also no slower than current rustc.
- BRA-541 retains at most 1.25 times matched Rust for comparable non-async products and empty-product size at or below
  matched C++. BRA-428's broader size comparison uses its smaller-peer requirement. Apply the stricter overlapping rule.
- Peak compiler memory, reserved/runtime live memory, linked code/data, setup and published-consumer costs remain visible.
  A faster front end cannot hide additional LLVM, linker or bundle preparation work.

Use the repository's matched runtime linkage and target policy. In particular, an unavailable comparable target row
cannot establish parity. Regression checks and final absolute comparisons are separate. Start with the first complete
consumer, reuse relevant controls, and run the full final corpus at the completed-runtime gate.

## Acceptance derived from the rules

The implementation test inventory must be derived from R01-R32 and the closed categories, not from past bug reports.
Existing regressions are retained evidence within this larger inventory.

For each rule, cover each applicable representation and boundary:

| Axis | Cases |
| --- | --- |
| Storage | Local, temporary, owned parameter/receiver, protected contents, heap/box/buffer, product static, TLS, foreign/mapped, erased and generated frame storage. |
| Shape | Whole and partial product, tuple, fixed/repeated/generated array, prefix buffer, nullable present/absent, active union, owned indirection and trait view. |
| Reference | Direct, projected, copied borrow, reassigned borrow slot, anchored borrow with different physical target, returned reference, raw address, ambiguous alias and stale epoch. |
| Transfer | Whole move, explicit copy contract, partial extraction, fresh-result destructuring, return/yield, field store, argument, generic wrapper, inactive frame, task and native boundary. |
| Exit | Normal, Error, return/break/continue, nullable/result/run-result propagation, panic/catch, cancellation, failed entry, cleanup failure and divergence. |
| Publication | Source body, encoded interface-only caller, generic substitution, selected trait/dynamic callable, debug/optimized native product. |
| Concurrency | Acquisition/release, publication/read, register/signal/recheck, withdraw/commit, wake/dispatch release, closure/acquisition, last callback/provider release and exact-thread detach. |

Do not blindly multiply all axes into thousands of redundant fixtures. The rule inventory must state which combinations
exercise a distinct transfer and which are equivalent after selected semantics. Test each distinct transfer and its
critical compositions. "Not applicable" needs a rule-based reason, such as a forbidden ownership operation, rather than
absence of a current consumer.

Required composition cases include:

1. Real production mutable guard access followed by guard and lock destruction, for each guard family.
2. Present/absent acquisition through a const accessor, imported generic wrapper and branch join.
3. Fresh-result marker extraction, field reconstruction, copied ordinary fields, whole move, divergence after attempted
   extraction, and pairing with another lock.
4. Payload replacement whose old destructor panics, with a caller catch observing initialized replacement.
5. RwLock overflow before commit, Once nested reentry caught by the initializer, and host TLS publication rollback.
6. Failed with entry, every pending exit form, scope-exit Error and cleanup panic under denied allocation.
7. Noncopy raw read, zero-length copy, initialized-prefix rollback and a spare-region write preserving the owner header.
8. A typed incident retaining over-aligned backing and provider state after its producing frame or permitted thread ends.
9. Recursive/erased frames with task broadcast before any waits, single result movement and no allocation after admission.
10. Closed provider with an external report, internal static dependencies and final release after callback return.
11. Registration withdrawal racing committed delivery, wake during dispatch, cancellation under shields and exact-thread
    cleanup before worker join.
12. Unchanged imported reuse, changed-summary invalidation and narrow parallel queries producing deterministic results.

These are implementation acceptance tests, not another production semantic-validation phase. The owning analysis proves
the contracts. Tests check that its implementation enforces them and lowering/runtime preserve them.

At each mechanism replacement, complete the repository audit and required formatting/tests. For Rust changes run the
mutating `cargo xtask style`. Format Bray with `cargo xtask format`. Native toolchain commands run outside the sandbox.
Use the established shared target directory and do not clean it. Run broader checks once the mechanism is coherent,
then repeat only affected checks when changes justify it.

## Stop rules and completion

A discovered defect belongs to one of these categories:

- A listed rule was implemented incorrectly or missed a listed variant. Correct its owning transfer and audit every
  mapped equivalent path. Do not add a consumer-specific bypass.
- Two implementations disagree on the same rule. Remove the superseded state/algorithm while preserving behavior.
- A required behavior cannot be expressed by the selected representation or rules. Stop the affected implementation,
  update this model and its cost consequences, and discuss a material design change before adding a mechanism.

The third category must not be relabeled a small bug to keep implementation moving. Conversely, a normal coding error
does not require reopening the entire architecture. The map identifies the affected owner and consumers.

The current stack is ready to land only when its issue contracts, source/imported authority, actual std/bootstrap
consumers, required native scope outcomes, removal ledger and review are complete. The full destination is complete only
when all later protocol owners, native provenance and final cost gates are complete as well. No closed issue is used as
a place to hide remaining work.

The old proposal is explicitly superseded. Follow the execution guide and current Linear issue contracts.
The assessment itself made no implementation changes. The subsequent issue preparation preserves the inherited repair
as an incomplete checkpoint and publishes the plan without beginning the replacement implementation.


## Assessment verification

Re-read AGENTS.md and audited the changed planning documents for scope, conflicting instructions, rule ownership,
source links and duplicate active plans. Relative links resolve and whitespace checks pass. The operation appendix
accounts for all 461 entries in its 12 source inventories. All 45 listed issues were open when read, and their active
prerequisites plus the proposed edges contain no cycle.

SHA-256 comparison confirms that all 2,940 files in the non-documentation assessment baseline are unchanged. No new
implementation file was added. No compiler/native experiment, build, test, formatter, branch change, commit, push,
Linear write or PR write occurred. Documentation-only verification does not establish implementation acceptance or
performance. No task-owned process remains running. The existing shared Cargo target was not resized or cleaned.
