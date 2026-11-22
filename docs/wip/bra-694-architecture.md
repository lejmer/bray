# Bray compiler architecture for BRA-694

This map follows the compiler from source and package inputs to checked bodies, imported contracts, MIR, and native
products. The [refactor record](bra-694-native-bootstrap.md) holds the agreed scope and verification results.
The [compiler design](../design/compiler-architecture.md) and [crate responsibilities](../contributing/crates.md)
describe the wider architecture.

## Compiler and product queries

```mermaid
flowchart TD
    Source["Project graph, source snapshots, selected target"] --> Syntax["Parser and declaration discovery"]
    Syntax --> Bind["Symbols, binder, and BoundUnit"]
    Interface["Dependency interfaces"] --> Import["Validated imported symbols and semantics"]
    Import --> Bind
    Import --> Contracts["Bound contracts for callees and lifecycle calls"]
    Bind --> Select["Types, selections, patterns, storage plan"]
    Select --> Check["CheckedBodySemantics"]
    Contracts --> Check
    Check --> Certify["Execution certification"]
    Check --> Lower["Lowering to MIR"]
    Select --> Lower
    Check --> Summary["Interned result dependency summary"]
    Summary --> Contracts
    Summary --> Export["Interface surface and selected bodies"]
    Lower --> Native["Product realization and LLVM code generation"]
    Native --> Emit["Emission and native linking"]
    Export --> Emit
    Runtime["Selected bootstrap and runtime artifacts"] --> Emit
```

- **Source and package selection.** `bray-project` owns the package-product graph. Compilation selects sources, import
  inputs, and target settings. Parsing supplies immutable syntax to declaration discovery. The symbol snapshot and binder
  give the selected source body a `BoundUnitKey` and source-correlated HIR. Imported signatures and relationships join
  the same symbol and semantic-value system. [Compilation construction](../../crates/bray-compilation/src/compilation/state/construction.rs)
  · [Bound-unit query](../../crates/bray-compilation/src/compilation/unit/query/evaluation.rs)
- **Semantic queries.** Type checking and call or operator selection establish declarations, receiver conventions,
  substitutions, implementations, witnesses, and conversions. Pattern checking, memory-operation checking, storage
  planning, and prepared body analysis use those selections. The fact cache shares immutable publications and records
  dependencies under the owning query. A body prerequisite can change without changing its declaration identity.
  [Body query inputs](../../crates/bray-compilation/src/compilation/unit/storage.rs)
  · [Fact cache](../../crates/bray-compilation/src/fact/cache.rs)
- **Body and summary.** For source callables, anonymous bodies, and source runtime defaults, the demanded result-dependency
  component supplies both the checked body and its summary. Recursive members use the current interned approximations of
  members of their strongly connected component. The component settles those approximations before publication. The arrow
  from summaries to bound contracts represents a caller reading its callees' contracts. Imported summaries and checked
  leaf contracts enter through the same callable result-dependency query.
  [Result dependency component](../../crates/bray-compilation/src/compilation/binder/symbol/result_dependencies.rs)
- **Downstream queries.** `execution_properties` uses the checked body, prepared graph, and certified dependencies to
  check execution promises. Its proof flow applies the body's checked storage records. A candidate query shares the
  solved flow across obligations with the same entry assumptions and enabled edges. The settled transfer records
  actual call admissions, cleanup actions, and arithmetic and index proofs. Each obligation certifies that evidence
  and records its own dependencies and diagnostics. `lowered_unit` assembles
  checked inputs for `bray-lowering`, which builds MIR. Compilation collects structured diagnostics from the prerequisites
  and returns no lowered unit on errors. `bray-messages` renders diagnostic text.
  [Execution certification query](../../crates/bray-compilation/src/compilation/execution_guarantees/query.rs)
  · [Lowering query](../../crates/bray-compilation/src/compilation/lowering/query.rs)
- **Products and runtime.** Product realization selects concrete instances and ABI mappings. LLVM translates and verifies
  MIR, including generated host, callback, and async bodies. `bray-emitter` stages outputs and `bray-linker` links selected
  objects and runtime artifacts. `bray-runtime-interface` describes compiler-facing roles and compatibility, while
  `bray-runtime-abi` defines the native records and symbols. The bootstrap package supplies production Bray bodies.
  Rust runtime adapters expose the stable ABI of their selected components. [Product emission](../../crates/bray-compilation/src/compilation/product_emission/execution.rs)
  · [LLVM design](../design/codegen.md) · [Runtime integration](../design/async-runtime.md)

## Data owners and identities

| Concept | Producer and storage | Meaning at the next boundary |
| --- | --- | --- |
| Operation, slot, and route | The checker produces `StoragePlan` and the CFG. `BoundExecutionSite` identifies the source operation or implicit call. `AnalysisOperationId` identifies an exact CFG operation, including each copy of a scoped exit. `StorageIdentityId` and the path within it identify planned storage. | `StorageReferencePath` combines inline projections with typed `Read` and `Backing` steps. Liveness uses structural uses and definitions. Forward analysis resolves reads against actual state. |
| Current value and availability | `ExecutionState.current` holds one `CurrentValues` tree per root. Evaluation and assignment change the tree. An `ExecutionCondition` can carry both logical data and `StorageCapturedValue`. | Availability and contents are separate. A moved value can retain its terminal data while its old slot has ceased to supply ownership. A capture can survive replacement of its source slot. |
| Descriptor and possible regions | The physical memory state holds `StorageValueBinding` per target. `StorageAddressValue` distinguishes actual alternatives, region bounds, displaced offsets, and unknown addresses. Whole-value dependency bounds have their own completeness. | A finite region bounds what may change. An exact write, initialized payload, or ownership transfer needs separate evidence. Inactive payload, actual null, and unknown contents each have a distinct meaning. |
| Allocation and access authority | Physical state tracks reached `StorageTarget` origins, initialization, and borrow capabilities. Allocation recency distinguishes a current allocation from older objects formed at the same site. | Pointer equality can establish data equality. Lifetime, initialized contents, access permission, and the cleanup duty each come from their own checked relation. |
| Mutation and observation survival | `StorageMutationTarget` holds the resolved address and conservative footprint. Effect kind and access type accompany it. The storage collector publishes changed logical paths, changes within exact typed pointees, and phase-specific retirements. | A reached component mutation updates that part of the current value tree. Whole or unresolved overlap frames the enclosing value. Descriptor replacement, atomic payload updates, consumption, and retirement each revoke their own support. An inactive payload contributes no target. A broad footprint grants no exact write or initialization. |
| Raw retirement | The raw pointer or the allocation record's pointer field comes from the admitted argument capture. Each preceding typed discharge has its own exact reached cell. | Lifetime invalidation uses the retirement address. Outstanding-duty diagnostics compare each pending cell with the proved discharge targets. A guarded outcome publishes exact cell changes after its performed effects. |
| Witness and report support | `ExecutionState` has ordinary facts, trusted facts, a witness ledger, and allocation-owner relations. Result dependency state tracks report values and their retained storage dependencies. | A witness belongs to an actual owner and can depend on contents, a descriptor, or a reached lifetime. An allocation-owner association restricts the use of proof to the transferred owner. A missing association at a guarded join supplies no new evidence. A report has its own ownership and may outlive a scope. |
| Portable identity | `DependencySubject` and captured terms name formal entry values, results, captures, and statics. Packages remap those identities through checked interface records. Unary signs use the scalar term encoder. Binary captured storage arithmetic retains both evaluated operands. | Receiver-first argument ordinals, result selectors, and selected root, product-static, and thread-static identities must retain their distinct meanings after substitution and import. Scalar arithmetic supplies no ownership evidence. |

Data definitions and producers. [Storage plan](../../crates/bray-bound-tree/src/storage/plan/model.rs)
· [Typed routes](../../crates/bray-bound-tree/src/storage/reference.rs)
· [Current values](../../crates/bray-checker/src/analysis/guarantee/current.rs)
· [Physical state](../../crates/bray-checker/src/analysis/storage_flow/memory/model.rs)
· [Value binding and target precision](../../crates/bray-bound-tree/src/storage/value.rs)
· [Proof state and retirement](../../crates/bray-checker/src/analysis/guarantee/retirement.rs)

A returned address keeps its own result identity and its direct stored input
binding. When that binding is one exact current operand, physical retirement
and effect publication follow it directly and retain ordered discharges. Region
bounds and alternative origins remain conservative footprints. Copyable access
promises with no tracked storage owner survive through their live address
value without gaining allocation ownership.

### Storage plan and flow state

| Boundary | `StoragePlan` | `StorageFlowState` or its published record |
| --- | --- | --- |
| Body preparation | The compilation fact cache owns one plan per bound unit. Each planned access retains its typed read and backing steps with its logical path. CFG, dependency binding, and refinement preparation borrow the plan. Preparation records the result-producing edges and shares them with storage flow and certification. Prepared cleanup requirements and lowering type shapes are shared across summary iterations. | The entry state adds actual inputs to the planned identities. Observation preparation indexes the routes by their footprint. |
| Backward liveness | Planned accesses, definitions, transfers, and lexical ownership supply structural uses and definitions. Prepared cleanup requirements identify slots eligible to retire at their last use. | Reached flow uses those requirements for entry state, last-use and edge retirement, and private storage alias bounds. |
| Forward operation | The collector reads the checked access, type and use from the plan. | The current state owns logical values, physical contents, and retained duties. Admission captures evaluated inputs once. Reached storage effects supply the observation frame and portable effect summary. Normal and abnormal successors use the admitted transition or its performed outcome. |
| Scope cleanup | Prepared steps name owners, parts, and receiver types. The scope owns the checked destination of any pending transfer. | Reached flow selects actions from live, moved, and absent storage. Proof reads each operation's reached order and failure suffix. MIR reads the published scope schedule by scope and exit. |
| Replacement cleanup | The plan names the destination access evaluated before the incoming value. Prepared cleanup classifies its type once. A selected compound operator has its own typed result slot and a separate installation operation. | Flow holds the evaluated indirect address through input evaluation, cleanup, and installation. The same address supplies conflict checks, old-value availability, the cleanup receiver, and the write. Proof, suspension analysis, and MIR read the reached cleanup decision. |
| Suspension and frame execution | The plan names the evaluated await operand or the staged receiver of `start()`. | Storage flow checks the selected frame's requirements against the reached state. `StorageSuspensionState` publishes retention and rejected requirements for await. Async behavior reads the selected calls at await and start. |
| Summary and proof | Exact plan identities bind contract subjects and observation sites within this unit. | `StorageFlow` publishes invalidation, capture retirement and completion by exact CFG operation and admitted outcome. Cleanup actions and failure edges retain their own transition identities. Exit rows abstract the actual value, effects and report into portable summaries for the next caller. |
| Lowering | `LoweringInput` borrows the same plan for runtime places, use kinds and checked transfers. | Lowering reads published storage and async decisions. It realizes guards and preserves pending values while cleanup can still change the outcome. |

[Plan and body fact queries](../../crates/bray-compilation/src/compilation/unit/storage.rs)
· [Flow state](../../crates/bray-checker/src/analysis/storage_flow/model/state.rs)
· [Body publication](../../crates/bray-checker/src/body_semantics.rs)
· [Lowering input](../../crates/bray-lowering/src/input/model.rs)

Scope retirement uses the prepared storage identities in both physical cleanup
and proof. Proof reads surviving caller-owned referents from the current value
tree using the installed input loans. A replacement descriptor supplies its
own reached observation. A cleanup action's effects, report and completion
belong to its actual receiver and the operation that reached it. A lexical
exit names the source boundary for lowering. Several graph operations can run
cleanup at that boundary, including a continuation after a failure.
When lexical cleanup finishes, whole expired private roots leave the proof
state. Prepared analysis shares the plan's initially absent root identities
with both storage solving and proof replay. An expired or skipped private
root stays absent at a join without a materialized value. A live predecessor
can still make its availability conditional. An unobserved physical pointee
has unknown contents and availability. Terminal formal values remain
readable for completion clauses. Surviving transfers keep their own captured
or pending values. Call setup uses the same planned absence when it vacates
a whole slot before a staged input or result arrives.

A scoped entry can return a fallible wrapper. Its successful payload becomes the scoped capability.
The exit's checked access names that current capability for both physical and proof admission.

The write record names committed inline paths into admitted mutable inputs.
The summary reads the final value of each written path. Call binding projects the prepared path from the admitted
descriptor to find the live destination. Effects still frame the full area the call may change.
Lifecycle completion publishes the reached receiver and its selected write destinations for proof replay.
Reference observations retain the reached target and the checked pointee type.
Proof uses that observation to read updated contents through the existing descriptor.
Normal completion publishes the reached borrow's known identity, or the
evaluated producer's logical identity when a portable row leaves its origin
opaque. The checked storage transition supplies address and lifetime support.
Input materialization captures the published descriptor.
Predicates read the reached value without duplicating the reference's own context.
Ownership and cleanup continue to read the original current value tree.
Evaluating the source resolves overlapping selectors once. Staging copies the
captured inline fields into their destination without reading the old source
route or weakening the reached address because of its original coordinate.
Final call admission refreshes its contents through that same loan.
Intervening argument evaluation can change the contents or end its authority.
Finite mutations publish which typed pointees can change using the reached
memory's alias relation. Proof materializes retained observations on demand.
Current storage applies the published masks to the affected places.
Framing and proof retirement resolve each checked preservation path once.
After opaque invalidation frames the current values, surviving scalar copies
still supply their data to proof. Ownership still follows the live owner tree.
Both read the actual current roots. A type-directed storage read also keeps
the physical path when the path has no inline execution place.
For an observed effect, the changed places identify which roots need masks.
Proof still reads surviving observations on independent roots.
Consecutive unbounded effects with the same physical scope share one weakening
of current memory. The checked effects retain their access types for proof
framing, portable summaries, and diagnostics.
An admitted extraction can retain its complete private snapshot through a write to
the vacated slot. The selected outcome must preserve the captured value and its
reached effects must leave the capture's carried regions untouched. The slot itself
loses the transferred owner. The result receives its witness only on completion.
Return publication uses one mapping from the actual result's logical observations
to portable result paths for guards, formal transfers, and witness clauses.
Physical snapshots still supply ownership and reachable support. Their capture
coordinates do not form part of a portable predicate subject.
An immutable scalar returned by a const call can publish its equality to the
selected call term under the reached outcome. The checked reference capture
continues to supply access authority for reference-valued calls.
A known-present nullable can carry the existing owner of its payload. The
whole nullable observation and the payload observation keep distinct paths.
Rebase each exact observation from the returned value. An absent alternative
has no payload carrier.
Direct calls and deferred bodies share the prepared input route and output type
for each owning transfer. A deferred body reads its current typed input
referent at activation through its retained descriptor. The operation and
producer identify that capture in `StorageFlow` for later proof checks.
Invalidation and completion read the extracted value from the same admitted
body entry.

The physical state owns the input-region bounds of returned addresses by producer and incarnation. A first publication has no prior incarnation to retire. Retiring one projection keeps the current bounds for sibling views and retains the original bounds for historical aliases. A later full publication can supply different current bounds at the same producer.

At a call attempt, escape analysis, observation invalidation, and returned-reference bounds share the reached input
regions. The admitted operands supply possible reach through their captured descriptors and carried dependencies.
An unresolved structured escape retains the identity of the value whose carried
reference escapes. It can expose unknown external storage, but it does not
publish unrelated invocation operands. Only an unscoped escape summary widens
to every admitted input region.
Result staging keeps an already published exact descriptor. It reads
the input regions on demand when a remaining opaque output needs a bound.
Entry preparation includes the backing descriptor reads named by prepared
result routes, including forwarded views of nested buffers.
An exact result relation can select a field behind an admitted address.
Output staging reads that address binding and follows the checked suffix in
current memory. A later consumer reads the published descriptor and any
additional descriptors along its own route. A projected region bounds a
possible effect. It cannot stand in for a subsequent descriptor read.
Calls that materialize their arguments already carry the formal wrapper.
When an operation uses an unwrapped admitted argument, effect and output
binding both translate the formal route to that actual argument before
reading its descriptors.
Await also reads the arguments in the retained frame. A finite bound requires complete inputs. A typed descriptor
can identify the storage it reaches after its source slot changes. Raw address formation publishes the descriptor's
address and bounds. It retains the ordinary loan only on the input. Output dependency fallback uses the prepared
value shape and leaves inferred operand loans off values with no borrow fields.

The physical memory intrinsics check initialization on the admitted operands before applying effects.
A protected read can discharge its initialization requirement from the actual typed cell. Its admission records
that decision for completion and keeps the remaining trusted requirements independent.
A staged read or copy uses that admission at completion. An assignment captures the evaluated right-hand side
before invalidating the destination and installs that same value. A compound assignment uses the completed
operator's private result.
Normal slice publication records the extent of the evaluated descriptor. Later indexing reads that extent through the
descriptor, including after the original bound variables change. If a resolved write leaves the checked reference
slot in place, proof framing retains its descriptor identity and replaces only the changed referent observations.
The checked entry publishes each installed, unconditionally present inline
input descriptor. A formal access that already dereferences its input holds
the reached observation. Reading a typed pointee removes an enclosing access
layer and retains any descriptor stored in the selected cell.
An input loan or active payload supplies the reached references after access or selection establishes them.
Call output staging records a common inactive payload as a physical no-payload
binding. Completion installs that binding with the returned value. The current
`StorageFlowState` reads those bindings together with its logical move and tag
absence facts when selecting cleanup, retaining owners, and publishing exit or
replacement decisions. Reference staging reads only no-payload facts. A later
write or join updates the physical binding before the next query, so the
caller does not copy the call's absence into a second persistent map.
An imported or source result reference follows its prepared typed route
through the admitted input capture. If a later descriptor is opaque, its
possible pointees come from the reached pointer value and previously exposed
storage. A borrow that keeps the enclosing owner alive does not by itself
make that owner a possible pointee of the returned reference. The symbolic
result identity retains those bounds across transfer and later escape.
Opaque observation preparation includes each exact nullable or union tag, not
only its enclosing root. An unexposed tag can stay stable even when a sibling
field is reached through a borrow. Physical no-payload bindings and logical
absence facts use that same preserved tag. If the tag can change, the physical
binding loses its absence rather than keeping an inactive value through a
possible write.
An address returned from the slice retains the
containing storage lifetime through its portable reference relationship. The fixed element selector and its backing
lifetime remain distinct from an exact cell address in the caller.
An evaluated integer index can select an exact cell in reached inline storage
when its value is a literal or an immutable entry input. A portable effect
binds the selector to the admitted call or frame's actual argument. Different
symbolic values can alias. An unresolved value supplies only a region.
Protected memory operations use the same admitted descriptor or staged
owning coordinate for mutation, initialization, and result publication.
An unresolved raw deallocation cannot retire an invocation-owned inline slot. Physical invalidation and
observation framing use the checked pointer type while still invalidating possible heap aliases. Buffer
cleanup disposes initialized elements and retires their backing. The backing comes from the admitted
owner's current descriptor. Reached allocation targets retire through their actual captured addresses
and generations. A retirement carries the exact cells whose typed duties had
already transferred at that point. The retired address and the consumed cells
can come from different captured descriptors. The summary publishes each as
its own typed route. Call binding resolves those routes against the admitted
operands. The reached cell state drops discharge evidence when a joined path
leaves the cell initialized or a later effect may reinitialize the cell. The
caller checks a routed typed discharge directly, including descendants in
the same allocation. It does not reread the source cell's type or initialized
state after that cell was retired. The unordered effect envelope only bounds
the mutations that can occur.
Unresolved cleanup supplies the same unknown effect to the local state and
the portable summary. Buffer cleanup prepares the descriptor routes for the
retirement and each carried discharge.

A static effect can name an owned backing region without supplying its address. Bind the checked route to the
current backing when the descriptor is available. Otherwise keep the static identity and its owning path as
a write bound. Reading a raw pointer from a static requires its own reached region.
A returned view of exact product-static storage carries its actual selection and typed contents. Opaque
transitions can preserve eligible inline observations through that view even when the caller has no direct
access to the declaration. A surviving view supports established content clauses about the referent it actually
reaches. Thread-local storage also requires its attachment context.

## Composite flow from admission to publication

```mermaid
flowchart TD
    Plan["Checked selections, CFG, StoragePlan"] --> Live["Reachability and backward liveness"]
    Live --> State["Incoming StorageFlowState"]
    Plan --> State
    Contracts["Bound call and lifecycle contracts"] --> Inputs
    State --> Inputs["Capture actual operands and check admission"]
    Inputs --> Apply["Resolve effects and apply the attempt"]
    Apply --> Private["Produce the private value"]
    Apply --> Failure["Panic or cancellation with performed effects"]
    Private --> Cleanup["Required cleanup"]
    Failure --> Cleanup
    Cleanup --> Exit{"Reached outcome"}
    Exit -->|Normal| Publish["Publish actual result and supported guarantees"]
    Exit -->|Abnormal| Report["Retain effects, report, and remaining duties"]
    Inputs --> Checked["Checked inputs, attempts, decisions and completions"]
    Apply --> Checked
    Publish --> Checked
    Report --> Checked
    Checked --> Replay["Execution proof replay and MIR lowering"]
```

The diagram shows the semantic order across calls, result regions, and cleanup. Each has its own CFG operations.
Ordinary and implicit calls use `Input`, `Attempt`, and `Completion`. Result regions use `Begin`, `Production`, and
`Completion`. Yield or break transfers can install the private value. Suspension and scope exit have separate
operations. [CFG operation kinds and phases](../../crates/bray-checker/src/analysis/model.rs)
· [Result region construction](../../crates/bray-checker/src/analysis/result.rs)
· [Call and await construction](../../crates/bray-checker/src/analysis/execution.rs)

Staged input preparation consumes the operation's resolved accesses. It reads
evaluated selectors from the logical state owned by that operation. The
completed capture returns to the composite state for admission and effects.
Trusted proof, the runtime precondition guard, and normal completion bind
entry conditions through the admitted arguments and their recorded equalities.
Witness transfers bind the appropriate result through the same argument lookup.

| Boundary | What the implementation passes forward |
| --- | --- |
| Prepare and solve | `check_body_semantics` prepares refinements, call contracts, and storage inputs once. It computes reachable edges and backward liveness, then solves forward `StorageFlowState`. Structural reachability and liveness feed one forward solve. The reached action determines the feasible cleanup outcome edges, and the solved flow supplies async publication. `StorageFlowState` keeps logical proof, physical memory, moves, borrows, effects, and report dependencies together. [Body checking](../../crates/bray-checker/src/body_semantics.rs) · [Flow domain](../../crates/bray-checker/src/analysis/storage_flow/model/domain.rs) |
| Evaluation and admission | `resolve_value_captures` and `capture_entry_values` retain the evaluated input, any typed current referent, and possible element dependencies. `ExecutionCallEvidence` holds these actual arguments, entry facts, admitted carriers, extracted values, and bound outputs. The checked phase determines when ownership is consumed. A false entry requirement can fail before the body runs. [Physical capture](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/references/captured.rs) · [Entry evidence](../../crates/bray-checker/src/execution_guarantees/evidence.rs) |
| Body entry | `StoragePlan` identifies the formal loan slots. `StorageFlowState` initializes the reached input reference targets. Entry proof binds clauses once against the actual initialized value tree. The declared carrier route selects the actual carrier and its ownership role in that same tree. Raw-memory requirements bind to address targets separately from the formal descriptor slots. `StorageFlow` passes the reached targets to execution certification. Returned carriers identify incoming backing through a typed address read and bind its lifetime against the caller's admitted support. [Entry state](../../crates/bray-checker/src/analysis/storage_flow/model/state.rs) · [Entry proof](../../crates/bray-checker/src/analysis/guarantee/flow/model.rs) · [Carrier binding](../../crates/bray-checker/src/analysis/guarantee/trusted/completion.rs) |
| Attempt and invalidation | `apply_operation_effects` binds value rows and resolves selected call, intrinsic, and assignment effects. Effects, reports, and completion use the producer's admitted rows. The physical memory update and proof invalidation consume the resolved observations. Typed routes keep evaluated locations distinct from conservative region bounds in calls, frames, and cleanup. Failure effects capture addresses before argument transfer. Execution retirements reach the proof state before completion. Last-use and consumed-handle retirements follow publication. [Operation driver](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/core.rs) · [Effect binding](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/effects.rs) · [Proof invalidation](../../crates/bray-checker/src/analysis/guarantee/flow/invalidation.rs) |
| Private result and cleanup | The private result can be present while caller publication is pending. Storage checking selects active obligations, and proof replay reads their checked schedule. Both execute the same cancellation and lifecycle phases and read absent paths directly from the reached or published storage state. Lifecycle invocation uses the resolved receiver and its checked contract. Cleanup can change contents, retire support, retain dependencies for a report, or take an abnormal edge. [Private result](../../crates/bray-checker/src/analysis/guarantee/flow/value.rs) · [Storage cleanup](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/cleanup.rs) · [Lifecycle transition](../../crates/bray-checker/src/analysis/guarantee/cleanup/transition.rs) |
| Normal completion | `complete_entry_outputs` selects feasible value rows and completes ownership and carriers against each actual result. The join publishes the result and current writes together. Ordinary postconditions and authored result-guarded value guarantees observe that publication. Compilation retains every declared guarantee and its result witness subject separately from the subset a safe body must prove. One completion instantiator binds the clause and subject to the reached entry snapshot and actual result. A safe body publishes only a clause proved by one live source witness and retains that source support. A trusted declaration supplies the clause without a body proof obligation. When a carried clause lacks checked portable support, the caller binds the declared support from the admitted arguments and reached result. In both cases, the declared subject supplies the portable carrier path. `publish_captured_outputs` pairs the surviving logical value with its physical snapshot and records it for replay. Exact current destinations and possible region writes have different publication paths. `StorageFlow` also publishes reachability, storage decisions, replacements, suspension, and scope exits. [Output binding](../../crates/bray-checker/src/analysis/guarantee/flow/captured.rs) · [Completion and carriers](../../crates/bray-checker/src/analysis/guarantee/trusted/completion.rs) · [Checked storage flow](../../crates/bray-bound-tree/src/storage/flow/model.rs) |
| Failure, catch, and cancellation | Calls and frames bind the selected normal and abnormal effect rows together against their admitted inputs. A failure edge applies its captured prefix and carries its report. Its completion passes the reached write bindings and surviving snapshots to certification. An opaque prefix uses the checked effect envelope. Result dependency analysis carries reports through catch boundaries. Cancellation after a child completes can own that child's already transferred result and require its cleanup. [Failure propagation](../../crates/bray-checker/src/analysis/storage_flow/model/domain.rs) · [Failure writes](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/failures.rs) · [Report ownership](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/reports.rs) · [Await edges](../../crates/bray-checker/src/analysis/execution.rs) |
| Deferred execution | Formation captures the selected callable, receiver type, witnesses, evaluated arguments, and owned inputs. Immutable value nodes record whether their complete structure contains an actual frame, including retained origins and aggregate fields. Discovery and frame-only rewrites can skip subtrees without frames. Start and direct await refresh reached contents while retaining the captured address descriptors. Activation publishes bound value rows by operation and producer. `StorageDeferredTransition` keeps latent effects, typed failure prefixes, and cleanup. Scheduling accounts for possible interference. Observation selects the task's performed effects from its captured frame, then publishes its actual result and any acquired report. An outer failure while suspended has its own admitted state and possible performed prefix. [Frame discovery](../../crates/bray-checker/src/execution_guarantees/condition/traversal.rs) · [Deferred proof flow](../../crates/bray-checker/src/analysis/guarantee/flow/deferred/execution.rs) · [Deferred physical effects](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/deferred.rs) · [Captured transition](../../crates/bray-bound-tree/src/storage/capture/transfer.rs) |
| Replay and lowering | Certification reads the actual targets, invalidation, completions, and retirements of the reached CFG operation. Obligation selection borrows the admitted logical arguments, assumptions, and implementation choices. Templates and whole-body checks use a source target only when its reached operations agree. Lowering uses the plan, checked activity decisions, scope exits, suspensions, and completion presence to emit MIR. MIR carries explicit initialization guards, result and report transfers, cancellation, and cleanup edges to code generation. [Proof inputs](../../crates/bray-checker/src/execution_guarantees/call_proof.rs) · [Proof replay](../../crates/bray-checker/src/analysis/guarantee/flow/operation.rs) · [Lowerer](../../crates/bray-lowering/src/lowering/lowerer.rs) · [MIR cleanup](../../crates/bray-lowering/src/lowering/cleanup/control.rs) |

Each prepared publication retains its source outcome row when several reached destinations join. Proof and storage flow use the same frame-alternative selection. Nullable and union refinements consume the checked discriminant observations, including tags whose payloads changed while the tag stayed live.
The template's possible-effect envelope covers execution before an outcome
is known. Each normal row supplies its own performed effects. Substitution
and interface binding preserve those effects with the rest of the row.
An outcome join receives both actual result values and their current states.
When a result already occupies its planned slot, the slot and expression
share one joined value and owner.
The shared join uses the result selector when preserving conditional facts
about current writes, ownership, and witnesses.
Publication installs the assembled root with its captured components.
Component records publish their initialization and use the root for data.

At a storage join, each held frame binds its moved inputs and copied
observations to its reached owner. It retains the copied data and its facts
separately from the staging slot and from the selectors used to choose the
producer. A write can invalidate a selector or live referent while the
frame still retains its immutable data.

Cleanup liveness reads the dependency support carried by the current value
in each live cleanup owner. The plan identifies those owners. The same
storage binding carries support through result publication, moves, and
replacement. A possible dependency bound can accompany an opaque output.
Pending and carried reports retain their own support until their disposition.

Storage flow publishes normal observation frames by exact CFG operation. Call and await attempts retain the set
of portable rows that reached each frame. Certification selects the covered rows, or uses the combined frame
for that operation. Loop certification reads that same combined frame. Checked refinements retain the facts
before each operation. Several executions of the same row may still require a conservative join.

Inline argument materialization publishes the already evaluated operand into the planned input slot. Storage and proof read the inline route from the checked input selection. Proof consumes any exact reached reference observation without running callee completion. A later call reads that slot's captured descriptor with the same origin and capability. Default providers and other calls bind their checked outcome rows separately.

A reference input carries its reached snapshot. An exact typed cell can also
identify the enclosing view when the checked projection reaches that same type
within the same inline storage. Ordinary admission retains that relationship
with its captures. Completion reads the already updated value tree through
the same relationship. Writes and retirement use physical overlap with the
enclosing storage. Deferred activation reads the live view
without promoting a saved frame snapshot into current storage.

A checked extraction admits the selected value and its witness before its
source is cleared. Finite invalidation revokes the public source observation
while the private admission retains clauses that observe the extracted
value's inline contents. Live address supports are subject to the physical
effect and lifetime checks. Only the selected returned owner can resume
the admitted witness.

An implicit receiver loan has the checked passed type. That type records whether the captured descriptor can bind a current write to its referent.

The bound dependency table prepares the rest of an indirect effect's route from the type reached by its first descriptor. Calls, deferred execution, and cleanup follow the same typed route against the admitted targets. A projection into owning storage reads the backing from its retained descriptor. When the descriptor cannot be read, the owning region bounds its possible backing without supplying an address.

A known inactive inline alternative has no descriptor to read. Route
resolution checks that alternative before following a stored reference or
owning backing. Missing descriptor knowledge still leaves the effect
unresolved.

Checked result views and effect continuations contribute their selected
paths to entry preparation. Type-level demand propagates across inline
components and through prepared raw-address reads. The finite closure
stops recursive type expansion. The physical state resolves the resulting
routes from its admitted entry values, including backing selection for
owned buffers.

Storage flow records the scope of mutable input targets as it admits a call.
Certification reads that scope when checking a callee under its Observation
obligation. The executed transition separately records which observations can
change, including effects that reach beyond those inputs.

### Joins and loops

Cleanup preparation registers each storage identity's ownership, transfer, and represented parts once.
Forward flow and async checking read those entries by storage ordinal. `StorageFlow` owns the reached
exit decisions and schedules. `CheckedAsync` supplies suspension facts and registered cleanup types.

Scoped selection, checked cleanup preparation, and MIR use the same compiler-known Result payload
types and field identities. On the normal error arm of a fallible scoped exit, MIR commits the Error
and then broadcasts cancellation for the displaced body before resolving that owner's lifecycle.
The storage plan retains the raw invocation result until its reached tag transfers the payload into
the capability, private region result, or report. Checked scope suspension includes the displaced
body and the suppressed error.
[Result layout](../../crates/bray-symbols/src/compiler_known/availability.rs)
· [Scoped cleanup preparation](../../crates/bray-checker/src/analysis/guarantee/cleanup/inputs.rs)
· [Scoped MIR](../../crates/bray-lowering/src/lowering/scoped.rs)

The existing fixed-point solver schedules blocks and edges. Edge propagation refines feasible paths, applies failure
effects when appropriate, transfers report ownership, and ends lifetimes before the merge. At a merge, may-effects and
possible dependencies grow. Definite initialization and unconditional proof require agreement on all covered paths.
Predecessors share immutable ownership layouts until a reached value changes its layout.
Availability, tag tests, and ordinary predecessor facts can guard more precise observations. Logical joins use stable
block, expression, and place identities. Physical merges combine targets and contents separately and require both
temporary evaluation storage and queued retirements to have been processed. A new loop iteration can retire a prior
evaluation at the same source site. Edges carrying explicit yielded or broken results and selected call rows can retain
distinct composite states through these joins. Loop headers and the finite alternative budget use the existing
conservative merge. Each transfer produces its block's reached storage decisions and condition facts. Acyclic blocks
consume their inputs. Cyclic blocks keep the input needed for convergence and replace their publication when they run
again. The final publication combines the last records from each reached block.
Current-value joins publish the assembled root once. Component records supply their own initialization.
Logical ownership records connect a retained snapshot to the input it actually transports.
They keep explicit absence of input lineage and conditional transfers distinct.
A wholly empty layout has one representation. A mixed layout keeps the guards
on both its transfer and no-transfer alternatives. When the predecessors agree
on a current value and its layout, the join keeps that record through renaming.
Physical addresses and cleanup duties remain part of the reached storage state.
The incoming predecessor supplies its original current storage for deferred frame guards and contributes
remapped proof, captures, and pending results. Only the joined state retains current storage and ownership.
Stable observations read the live entries of the retained roots. Predicates use the logical observation
of each reached borrowed value. Joins use that same observation when remapping and checking clauses
against their predecessors. Storage transfer keeps the complete physical capture.
Bounded condition substitution visits the retained children in the original order. If the evaluated
aggregate or reference observation still has the same canonical form, it keeps the original node.
An aggregate reuses its field list until a field changes.
An inline field retains the data and physical capture of the selected payload. Its nullable extraction view
can still name the containing snapshot for checks at the read site. Storing that view in the field would
retain an obsolete parent on every loop pass.
Ownership and retirement traversals follow the payload in that view. The containing snapshot supplies
read-site provenance.
An unresolved element or range changes the enclosing logical observation.
The checked transition keeps the physical bound and publishes the enclosing
observation for proof. A range cannot become a new logical value component on
every loop pass. A selected exact element keeps its own address and value.
An enabled edge can take a settled predecessor output when it is the only enabled successor and the source
has no terminal publication. Branches with multiple enabled successors, cyclic consumers, and terminal
publication retain the outputs they read.
Published snapshots and diagnostics retain graph order.
[Worklist](../../crates/bray-checker/src/analysis/fixed_point.rs)
· [Guarded composite states](../../crates/bray-checker/src/analysis/storage_flow/model/outcomes.rs)
· [Composite merge](../../crates/bray-checker/src/analysis/storage_flow/model/state.rs)
· [Logical joins](../../crates/bray-checker/src/analysis/guarantee/flow/merge.rs)
· [Value join](../../crates/bray-checker/src/execution_guarantees/snapshot.rs)

## Summary, import, and call binding

1. At each reached normal exit after cleanup, the summary builder reads the solved `StorageFlowState` directly. It
   borrows the physical bindings and uses the same state's result, proved postconditions, effects, and report dependencies.
   The summary packages those observations with the guard, captured value graph, ownership layout, inactive payloads,
   and witness carriers in `CapturedValueOutcome`. `CapturedValueTemplate` retains portable inputs and operations
   with explicit unknown nodes.
   Reached failures capture their performed prefix, writes to caller storage, and report dependencies in
   `CapturedFailureOutcome`. A known empty prefix and an opaque prefix have distinct representations.
   [Exit producer](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/core.rs)
   · [Normal summary rows](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/references/publication.rs)
   · [Portable model](../../crates/bray-symbols/src/value/captured.rs)
2. The source component combines observations across recursive rounds. Generic substitution remaps the same portable
   graph. Package export maps it to interface symbol, type, term, and projection identities. The interface encodes the
   captured values, typed cell changes, and report support for each outcome. Panic and cancellation retain their
   own failure writes. Decoding, remapping, validation, and interning use the importing semantic store's identities.
   Specialization identity also depends on the portable contract.
   [Substitution](../../crates/bray-symbols/src/value/substitution_apply/dependency.rs)
   · [Export conversion](../../crates/bray-compilation/src/compilation/export/semantic/dependencies.rs)
   · [Capture codec](../../crates/bray-package-interface/src/semantic/codec/captured.rs)
   · [Validation](../../crates/bray-package-interface/src/semantic/validation/captured.rs)
   · [Imported interning](../../crates/bray-package-interface/src/semantic/interning/dependency.rs)
   · [Specialization identity](../../crates/bray-compilation/src/compilation/product/specialization_identity/dependency.rs)
3. Source and imported `CallableResultDependenciesQuery` results feed the same checked call contract. `BoundCallStorage`
   supplies typed subject and endpoint routes. At admission, the instantiator binds entry guards and destinations against
   the evaluated input. Completion binds residual result guards against the produced result. `ExecutionOutputs` keeps
   portable row indices so physical references, effects, reports, and logical publications can share admission.
   [Imported result query](../../crates/bray-compilation/src/compilation/binder/symbol/imported.rs)
   · [Call instantiation](../../crates/bray-checker/src/dependency/call/contract.rs)
   · [Bound routes](../../crates/bray-bound-tree/src/dependency_table/storage.rs)
   · [Output rows](../../crates/bray-checker/src/execution_guarantees/outputs.rs)

Ordinary call attempts can carry one selected row through physical effects, private production, and publication.
Physical call effects adapt the formal route to the admitted input's representation before traversing
its captured descriptor. A staged nullable input already holds the wrapper. An unstaged present
conversion uses the original value and drops the corresponding formal projection before traversal.
For a rowless callable, a selected purity promise bounds the normal fallback only after the
input checks at admission. The effect row selector retains the conservative envelope and abnormal
prefixes. Storage flow publishes the premise by CFG operation. Certification uses the entry evidence
at that operation to verify it. Explicit effects in checked rows remain part of the transition.
Reference staging passes unchanged descriptors' actual referents to normal invalidation. An ordinary call's
failure edge uses the admitted state and captured failure prefix. Deferred observation uses the same
composite flow for selected frame identities and completed or abnormal alternatives. The flow retains the
captured inputs, actual payload, effect prefix, and reports until variant refinement. For lifecycle calls, the
reached receiver admits each row. Storage and proof apply the row's effects, cell changes, reference outputs,
receiver publication, and report before joining at the next action. Declared guarantees use the admitted
observations and the reached post-state. In a normal completion, result reference fields read their current
contents through the exact targets recorded by storage flow before proof binds the row's guarantees.
Inactive cleanup binds the retained capture environment at cleanup admission. Running-task cleanup
reads the effects retained by activation.
[Invocation outputs](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/references/outputs.rs)
· [Explicit guarantee binding](../../crates/bray-checker/src/analysis/guarantee/trusted/dependencies.rs)

Scope entry and ordinary callable factories reserve backing after requirements and before transferring their owning
inputs. Their reservation failure retains the invocation identity for liveness and proof replay. The caller owns
the initialized inputs and the report. Factory construction and deferred execution remain separate transitions.
On async scoped entry, commit the child outcome into the capability or Error owner before polling cancellation.
The effective cleanup scope follows the acquired owner.
[Call graph and factory admission](../../crates/bray-checker/src/analysis/execution.rs)
· [Scoped lowering](../../crates/bray-lowering/src/lowering/scoped.rs)

Direct calls and awaits retire consumed input slots at admission. Normal and abnormal outcomes start from the same
admitted state. Storage flow applies the matching performed prefix on the abnormal edge and publishes any reached
writes to caller storage. Proof replay uses the invalidation, capture retirements, and write snapshots from that edge.
Report retention uses the admitted failure rows for the edge's kind, and summary export retains that kind.
`RunResult.Cancelled` is payload-free. On a terminal cancellation, the host owns its cleanup
incidents. A cleanup panic can instead transfer them with a `PanicReport`.
The checked catch edge transfers the reached report to the Error payload. The successful edge names
the evaluated operand that it transfers into Ok, and liveness and both flow consumers read that operand
from the edge. Report dependencies move with these payloads through memory and joins.
Reads in a value recipe use the referent captured in its admitted environment. Starting a deferred body
refreshes that environment's reached contents once. For lifetime checking, a storage requirement names
the required access while a copied value retains
its own loan dependencies. The surviving value or report boundary owns those dependencies.
An ordinary scoped-exit incident also transfers the actual borrow origins of its Error payload.
A direct await over one known body uses that body's selected panic row for its prefix, report, and
edge guard. When failure rows widen, the transition joins the already captured prefixes within the
same failure kind. A widened task observation retains the selected frame's possible panic payload
at each reachable result depth alongside the normal value.
[Invocation and await CFG](../../crates/bray-checker/src/analysis/execution.rs)
· [Dependency failure CFG](../../crates/bray-checker/src/analysis/cleanup.rs)
· [Failure application](../../crates/bray-checker/src/analysis/storage_flow/model/domain.rs)
· [Cancellation and incidents](../language/async-and-concurrency/cancellation.md)

### Cleanup and resumption

The storage schedule owns the acquired values and cleanup order. The invocation owns
its selected effect prefix and report. The enclosing exit owns the primary reason
and already retained incidents. The checked graph and MIR broadcast task
cancellation for all exited scopes before resolving their lifecycle operations
in reverse lexical order. A selected scoped exit runs during lifecycle
resolution before destruction of its local values. The storage solver
publishes the accesses it selected for broadcast and their absence facts at
that phase. Lifecycle resolution has its own later entry state. Replay reads
the applicable state, and lowering derives storage guards from both ordered
schedules. A represented cleanup part keeps the nearest inline owner and
the known tag restrictions. Ownership or absence of that slot can exclude the
part. Addressing a child across that boundary still requires its typed
projection. Certification observes reached cleanup during proof replay.
`StorageExitDecision` carries the reached cancellation and lifecycle orders,
absence facts, and the selected protocol exit with its recovery status.
The prepared cleanup requirements supply the owner's shape and phase needs.
The scope exit publishes one fixed lifecycle order when all reached selections agree.
For differing selections, the checked dependencies and reverse declaration
priority govern runtime choice among present owners. Proof reads the reached
orders for each operation. A cleanup fault carries the performed effects and
the remaining schedule after that action. The continuation retains the
broadcast and resolved prefix, then broadcasts newly exited scopes and the
abandoned pending value. The checked entry of each cleanup phase or suffix
retains its order, incoming absence facts, and any displaced destination.
Within an operation, continuations waiting at the same checked cursor join
their current states before running the suffix. The cursor keeps the selected
schedule, action, primary failure, and enclosing continuation. Storage flow
and certification share the worklist. Each action that failed already
attempted its own slot. Resumption starts with the following action or the
next array iteration.
A two-way cleanup join uses the checked selection guard. A join without a
selector derives alternative guards from the retained predecessor premises.
Admission rejection of a scoped invocation uses
the caller's input state. A failure after admission uses the performed state.
An abandoned transfer names its actual produced result slot. Its failure
continuation broadcasts cancellation for that owner, broadcasts newly exited
scopes, then disposes the pending value before continuing lexical lifecycle
resolution. The same prepared cleanup parts are available to storage flow and
proof replay. Normal delivery leaves this owner available to the destination.
The producer supplies the slot for returns, yields, breaks, and iteration
results. A `try` failure installs the selected error and its dependencies in
the destination before running cleanup. Abandonment retires the stored value
and its dependencies even when the value has no lifecycle action.
The reservation guard remains a separate duty after an attempted exit.
The failure continuation can release that backing without invoking the
protocol again. A scoped result replacement stages the new value before
its displaced owner begins cleanup. The displaced owner records its own
failure point while the enclosing exit owns the resulting transfer.
For a partial assignment, the replacement decision selects a range within the
owner's checked part partition. Certification reads those parts and MIR uses
the same range of initialization guards. A failure during old-value cleanup
retains the effects and remaining actions of that attempt. The assignment
finishes the suffix and installs the already evaluated replacement before
the selected failure reaches the enclosing scope or catch.
A generator accumulates elements in its planned result slot. Each admitted
yield transfers the evaluated element into that slot before retiring its
source. An early exit resolves the prefix through the checked disposal plan.
Rejected buffer growth leaves the yielded element and the prefix owned by
the caller. Completion publishes the accumulation slot as the result.
Replay reads each cleanup mutation by operation, access, part, and lifecycle slot.
MIR retains the failed finalizer's place for destruction. Represented
cleanup retains the current part range and the projected place at each
failure edge. An array continuation finishes the current element and uses
the live loop cursor as the end of the remaining range. Other owners remain
in the checked scope schedule. The checker uses exact element paths when the
extent and shape fit the observation work limit. Otherwise it joins reached
states at the start of the repeated element region and carries those states
into the remaining cleanup. A region supplies possible effects and dependencies.

| Cleanup input | Producer and consumers |
| --- | --- |
| Selected owner | `StoragePlan` supplies the produced slot. Scope resolution selects live accesses. Each selected cleanup part supplies its own typed receiver path to proof, effect binding, and report binding. An unresolved represented layout keeps the owning access as a lifetime bound. |
| Pending result | Return, yield, break, and propagation write the checked destination before cleanup. Successful delivery publishes that slot. Abandonment applies the slot's actual cleanup and ends its storage and dependencies. |
| Performed effects and report | Each reached lifecycle attempt uses its captured receiver and selects contract rows at admission. Effects account for changes to precisely resolved targets and possible regions. A report retains its requirements before the action changes the receiver. Scope escape, catch publication, and package export read the retained report. |
| Resumption | The ordered cleanup schedule comes from the reached owners. A failing action must retain the performed prefix and next position so the failure continuation can abandon the pending result and run the remaining actions. Lowering must preserve the same continuation through the finalizer, destructor, and represented parts. |

[Scope exit assembly](../../crates/bray-checker/src/analysis/assembly.rs)
· [Lifecycle transition and replay](../../crates/bray-checker/src/analysis/guarantee/cleanup/transition.rs)
· [Mutation identities](../../crates/bray-bound-tree/src/storage/observation.rs)

## Cross-layer compositions

| Composition | End-to-end relationship |
| --- | --- |
| Generic value, buffer, frame, and escape | A value containing a reference moves into generic `RawBuffer<T>` storage. The captured graph retains the whole appended or popped value. Admission captures the backing's reached element bounds. Publication binds new elements against those bounds. Branches and loops affect which prefix and owning transfer survive. Moving the buffer into a deferred frame carries the backing's dependencies and cleanup. Pop transfers the selected element and leaves the remaining dependencies with the buffer. Scope exit or escape checks referent lifetimes against the actual surviving owner. Source and imported callers instantiate the same rows. [Buffer source](../../standard-library/std/src/memory/buffer.bray) · [Checked buffer operations](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/memory/buffer.rs) |
| Nominal `Self` in a generic member | The selected method has bindings for its enclosing declaration and its own generic parameters. The named `Self` type uses the parameters of its declaring struct or union. Normalization and product mapping get that open type from the source or imported declaration before applying the selected bindings. Nested results and receiver types carry the same nominal identity. [Type normalization](../../crates/bray-checker/src/type_normalization.rs) · [Declaration lookup](../../crates/bray-compilation/src/compilation/substitution.rs) · [Constant import control](../../crates/bray-compilation/src/compilation/export/build/tests/constants.rs) |
| Saved pointer, escape and retirement | Selecting a field, reading its descriptor, and following it are separate operations. A later write to the field changes the current descriptor. Retirement through the earlier evaluated value applies to that reached allocation and revokes its dependent witnesses. An incomplete escape route below a reached raw address starts its conservative reach at that address. Extend it through known carried data and aliases. The effect footprint bounds invalidation. A precise retirement still requires the captured operand. [Typed route execution](../../crates/bray-checker/src/analysis/storage_flow/memory/references/route.rs) · [Effect resolution](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/effects.rs) |
| Native dispatch and start | `dispatch_ready` holds `Current` while retaining a resident task reference. The same context view supplies the pending read, worker attachment, and recovery thread. Recovery takes the scheduler reference reached from the execution owner. The C adapter reads its admitted native slots. Notification, frame retirement, and advancement keep the caller's dispatch claim until explicit release. `start_frame` moves the prepared owner from its selected result into a scoped entry. Its service acquisition supplies lane selection and registration. After lane admission, start moves the token out of its slot and authorizes it for the inspected thread origin. Commit consumes that token and receives the task borrow, selected registration, and inspected metadata. The scope keeps the view and service acquisitions and clears its registration before wake once the task cell owns it. An early exit withdraws the reservation if present before closing the service owner. [Dispatch](../../runtime/bootstrap/src/execution/driving.bray) · [Current and context](../../runtime/bootstrap/src/execution/context.bray) · [Recovery](../../runtime/bootstrap/src/scheduler/lane.bray) · [Start](../../runtime/bootstrap/src/execution/start.bray) · [Commit and rollback](../../runtime/bootstrap/src/execution/prepared_start.bray) |
| Wait links and event counts | A selected neighbor access carries the slot address, the host lease, and the acquisition established under that lease. Withdrawal keeps both selected neighbors through collection updates. The same borrow supplies the link write. Event creation and retention issue an acquisition with the represented reference after establishing its count. Wait registration moves that reference with the duplicated waiter. Access uses the actual addresses and the acquisitions live with their owners. [Neighbor access](../../runtime/bootstrap/src/task/waiting/links.bray) · [Withdrawal](../../runtime/bootstrap/src/task/waiting/withdrawal.bray) · [Event ownership](../../runtime/bootstrap/src/event/storage.bray) · [Registration](../../runtime/bootstrap/src/event/registration.bray) |
| Activation inspection and begin | Both operations use the checked slot selected under the report pool guard. The lookup validates layout and the lease's shape, epoch, address, and registered descriptor. Inspection reads the address of an inactive or retained frame. Begin accepts only the inactive state, consumes it under the guard, and then opens the header. The metadata caller's effect summary comes from the inspection path. The dispatch transition publishes the slot and header writes. [Resident selection](../../runtime/bootstrap/src/storage/activation.bray) · [Inspection](../../runtime/bootstrap/src/activation/storage.bray) · [Begin](../../runtime/bootstrap/src/activation/transition.bray) |
| Native provider, worker and retirement | Provider construction returns a present or absent provider under `Ok`. The consumed payload survives binding installation and supplies provider installation. Worker locking binds the selected pool marker and the guard's actual acquisition before accessing protected state. Driver retirement withdraws the active context and moves its initialized packet after its views and service slots are closed. The complete packet retains the lock witness and backing allocation through attachment cleanup and predecessor restoration. Field cleanup releases the backing last and ends dependencies on that exact allocation. [Entry](../../runtime/bootstrap/src/execution/native_owner.bray) · [Provider](../../runtime/bootstrap/src/execution/native_provider.bray) · [Worker locking](../../runtime/bootstrap/src/worker/storage.bray) · [Driver retirement](../../runtime/bootstrap/src/execution/retirement.bray) |
| Windows environment read | `native_environment_entry` produces the Boolean and span together and declares initialized-read guarantees under the true result selector. `environment_storage` selects the true branch and passes the actual address field to `byte_buffer_read`. The source returns distinct tuples. Loss can occur after result production, during guarantee instantiation, publication, branch refinement, or proof retention. [Windows implementation](../../standard-library/std/src/platform/windows.bray) · [Production check and lowering](../../crates/bray-compilation/src/compilation/trusted_contracts/contract_tests/standard_storage.rs) |
| Scoped unit completion | Scope enter, body, and exit can complete with a unit result and no recorded observations. The collector publishes a completion for a reached normal phase even when its payload is empty. `invocation_completes` uses that record in lowering to preserve the following return. [Completion collection](../../crates/bray-checker/src/analysis/storage_flow/check/analysis/core.rs) · [Lowering consumer](../../crates/bray-lowering/src/lowering/expression/core/operations.rs) · [Scoped result control](../../crates/bray-compilation/src/compilation/lowering/tests/lifecycle.rs) |
| Deferred task observation | `start` holds the selected body's evaluated inputs. The completed `join` or `cancel` outcome produces the body's value and resumes its witnesses. The abnormal outcome has its own prefix and cannot use that value. `try` selects `Completed` before transferring the payload. [Deferred frame and logical completion](../../crates/bray-checker/src/analysis/guarantee/flow/deferred/execution.rs) · [Composite outcome propagation](../../crates/bray-checker/src/analysis/storage_flow/model/outcomes.rs) · [Source and imported controls](../../crates/bray-compilation/src/compilation/export/build/tests/deferred_completion.rs) |

The bootstrap proof entry points request both `execution_properties` and `lowered_unit` for each listed body.
The [verification record](bra-694-native-bootstrap.md#acceptance) tracks the comparison and native targets.
[Bootstrap proof entry points](../../crates/bray-compilation/src/compilation/trusted_contracts/contract_tests/bootstrap.rs)
· [Native target definitions](../../crates/bray-target/src/native.rs)
