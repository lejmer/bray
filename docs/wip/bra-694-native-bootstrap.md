# Compiler boundary refactor

## Scope and ordering

Complete the full captured-value, typed-transition and guarded-outcome refactor on
feature/bra-694-native-bootstrap-contracts, then fix remaining native failures. The user's full plan is in the
pasted attachment from this chat. Start was05d2dc1b6. No baseline test run was requested at the start.

The user explicitly forbids repairing or retrying dispatch_ready and start_frame until every agreed refactor boundary
is complete. Correct regressions introduced by each slice, then continue the refactor. No native retry or native source
repair is authorized now. Keep PR717 draft and BRA-694 In Progress until the full contract and review are complete.
Do not end the task at a partial checkpoint, commit, push or clean unit-test run.

Latest user steering is mandatory. Stop treating intermediate local failures as gates for each separate boundary. Complete
the cross-dependent model, source/import, deferred, cleanup and abnormal migration as a larger connected batch, remove
the competing paths, then judge convergence. Use intermediate checks only for blockers that prevent further implementation.
Reconstruction is a liability. Access authoritative captured values and resolved transitions directly. When data is missing,
carry it from its producer. Do not infer it later from types, source slots, declaration identities or old summary fields.
Existing reconstruction is code to replace, never a constraint requiring another reconstruction layer.

Keep CFG, composite flow, CurrentValues, proof, SCC and demand-driven summaries. Possible regions establish no equality,
actual location, allocation identity, initialization, lifetime or capability. Pointer equality grants no permission.
Compiler definitions and older allocation classes do not establish one unique runtime object. Unknown never becomes
empty. Nullable inactive alternatives and actual null addresses remain distinct. Root, product-static and exact-thread
identities remain explicit. Lowering consumes checked decisions and does not repeat ownership reasoning.

## Pushed checkpoints

The remote checkpoint is 1c5770b2c. The captured graph, guarded ownership and paired publication are
committed locally at 11b860fa2. Branch-local replay, deferred publication, and frame refinement follow
in the working tree.

- d087dde8f replaces independent normal/deferred preservation booleans with immutable DependencyEffectOutcomes.
  Admission captures possible failure effects before consumption. Deferred formation keeps body effects latent.
- 551d12bb1 replaces References/StoredValue and their staged reconciliation with AddressValue/ValueBinding.
  Actual address knowledge, dependency bounds, completeness, null and inactive slots remain separate. Interface revision2.
- b27260893 separates private production from completion. take_outputs drains output fields only. produce_transition
  retains input captures and abnormal/deferred metadata. Normal completion cannot replay pre-cleanup values.
- 8be430 migrates typed Read/Backing StorageReferencePath interpretation and removes implicit address scans.
- 338483 migrates shared StorageObservationTransition classification and removes independent guarantee reclassification.
- 584e0e2666ac93fd332902c85ecb1f4b9d08b9f0 retains captured actual effect addresses separately from footprints.
  Shared MutationTarget resolution serves calls, lifecycle and deferred cleanup. Bound-only Dispose becomes Cleanup.
  Layout and definite initialization require an actual current address. Retirement updates both address and footprint.
- 1c5770b2 removes BoundReturnedValue.observations and both substituted-return-type field inventories. Existing ownership
  masks frame actual captured inline subtrees. Reference descriptors survive separately from refreshed reached data.
  Nullable wrappers retain a tag and actual payload, without fabricating a wrapper owner.

Earlier evidence is version-specific. Latest full frozen comparison was551d12bb1,1714/1714 pass in303.88s, zero missing.
Checkpoint584e0e passed564 checker,368 composition and36 raw/owner controls with3 existing ignored tests. Checkpoint1c577
passed564 checker,222 symbols,136 bound-tree,368 compositions and36 raw/owner controls. Its added generic probe was red.
Old native18 atd087dde8f failed only dispatch_ready/start_frame. None of that is current native or frozen evidence.

## Current graph implementation

Pure Contents, Effect, Transfer, ValueBinding, deferred joins, selection and physical snapshot capture moved from checker
memory into bray-bound-tree/storage/capture and storage/value. Checker aliases these records. may_retain_address stays
checker policy. Split storage/flow is a thin root with model and phase-specific retirement metadata in child modules.

StorageCapturedValue shares an immutable physical transition and a projection coordinate. The coordinate is not runtime
allocation identity. CapturedValue pairs that snapshot with ExecutionCondition data in CurrentValues. Component selection
and replacement transport the corresponding snapshot. Metadata participates in current-value fixed-point equality but
must be transparent to proof data. No separate current-data registry and no caller-side substituted-T field inventory.

CapturedValueTemplate in bray-symbols is a portable immutable graph. Nodes distinguish actual Input from scalar Data and
explicit Unknown. Operations include Select, Read, Reference, Owned, Scalar, Inline, Present, OwningStorage(pointer field),
EmptyPrefix, Append, Last, Remainder and count arithmetic. Arc trees share expression structure. Guarded bindings retain
ordinary entry/result-shape conditions. Codec, import/export, substitution, selection, validation and hashing have initial
wiring. The compiler SCC combiner and operator-call binder now preserve these bindings too.

Input-phase StorageValueCapture pairs receiver-first evaluated inputs with inline snapshots and typed referent snapshots.
StorageCompletion retains captures and StorageValueTransition definitions for replay. At Attempt the graph binds once
against entry evidence before invalidation. ExecutionCallEvidence.outputs retains those bound values. Normal completion
reuses outputs. Guaranteed Current writes require resolved normal write effects. Loss of abstract knowledge is not a store.
Produced observation preservation uses the actual bound graph. Failures receive no normal produced preservation.

Checked buffer allocation creates an explicit owning-prefix edge and records zero initialized count and capacity.
Push/pop update count and prefix together, moving complete opaque T values through the graph. Count supplies no permission.
Logical owned-components traversal follows explicit prefix children. Physical publication adds conservative AnyElement
bounds from actual captured values. It never reads an AnyElement bound as actual T. Unknown prefix remains unknown.
Zero-sized T still obeys element duties. Capacity zero requires no allocated resource. Do not invent zero-byte policy.

Physical State queues producer/lifetime retirements. Operation/cleanup boundaries drain them to captured metadata and
publish them by StorageMutation and StorageValuePhase. Replay consumes the same events. State.merge asserts the queue
is empty. Four direct physical fixtures now explicitly drain their events before joins. No production invariant weakened.

Actual nodes can have multiple observation roles. StorageValueBinding.observations is now a set. Capture groups all roles
per path and fills only missing actual role sets. An opaque receiving view cannot replace producing roles. Joins intersect
roles and demotion to dependency bounds clears them. The former single Option dropped Marker behind Identity and ordinary
InlineBits behind Mutable. Inline preservation now uses actual roles, without reasserting live or promoting alias bounds.

Normal exits carry ownership and current-value bindings together. Semantic and bound contracts derive their
aggregate ownership layout from these rows. Completion applies the admitted row before joining alternatives.
The separate buffer staging path is gone. Push, pop and prefix counts now use captured input values.
Unknown elements retain dependency bounds and give no exact address or initialization. Private witness
support, per-exit effects and reports, cleanup, abnormal publication and static instances remain open.

Deferred frames carry the selected instance, signature, dispatch, witnesses and receiver convention from call
selection. The interface transports that record directly. Frame binding no longer invents call terms or
ordinal arguments. The latest release panel passed 568 checker, 223 symbol, 137 bound-tree and
218 interface tests. The same candidate passed 318 of 368 compositions and 34 of 36 active
owner controls, with three ignored. The two failing owner controls are nullable extraction and
guarded extraction of one of two owned sibling fields. The native and frozen gates remain pending.

Normal-exit collection pairs the actual physical references and their inactive slots with the same
exit's logical returns and witnesses. The remaining unconditional physical consumers still require a join.
Both physical and logical publication use the produced snapshot, with installation and consumption
retirements applied at the existing retirement boundary. Replay receives that same snapshot.

A reference effect starts from the descriptor captured at admission. Direct and deferred
root-indirect effects use the same physical descriptor lookup by the checked receiver-first ordinal.
Observe explicit call descriptors before capture. Implicit calls without a captured input retain their
existing attempt boundary.
If a captured descriptor lacks a precise address, keep its bound and leave unresolved routes
conservative. Uncaptured implicit invocations and typed routes that reach further storage still
have existing physical producers to migrate. Each subsequent descriptor read uses
the location reached by the preceding steps in the same incoming memory. Only an actual descriptor location
permits a read. A region alone leaves its pointee unresolved. An unrepresentable terminal selection widens
within its region, but an unresolved intermediate read can reach beyond that region. Cleanup composition
retains the outer scope and the callee's ordered reads. Carry the same steps through import, export, generic
substitution, selected-record reachability and specialization. Nested saved-pointer, successive borrow and
cleanup effects are positive controls. An ambiguous or retired descriptor must never establish a layout.
Use the existing resolved physical operands and per-phase storage. Remove the single-indirection limitation.

One immutable evaluated condition may have several handles. A condition held as a guard or an evidence-map key
is the same condition after its handle is copied. Structural traversal must visit and transform the original
children, and memoize by the shared node. Mapping shared frames through current values, expressions, private
outputs and admitted arguments must return the same changed record to each holder. Distinct records with equal
payloads remain independent. Owner retirement can drop a particular holder. It cannot confer runtime aliasing
or permission from a compiler allocation address. Positive controls exercise moved frames and private captures,
and the negative controls cover nested same-site frames and expired owners. Remove per-holder metadata
reconstruction and duplicate retirement traversal. Reconsider if the pass needs global interning or authority.

A yield, valued break or propagation to a result region supplies its evaluated operand to the same private
result until cleanup completes. A join merges those operands, including their actual frame alternatives.
Only result completion publishes the destination. Clear the private result on region entry. Source and
imported active_partial must reject a missing witness, and captured_field_fact must retain its captured
field witness. A trace confirmed both branch futures form but await reads Unknown in both source controls.
The private result reaches the binding and the evaluated await operand. Suspension splits the producers
and rejoins their entries. Completion, cancellation, reports and witness diagnostics must consume
argument zero of the admitted await. The pre-admission split and physical attempt read the evaluated
operand directly. Never extend the source slot lifetime or look up a frame by its site.

Both immediate and deferred binders use the admitted receiver-first descriptor for a current write.
The fixed-point rule distinguishes an absent write from an unknown value written to a known destination.
Exit collection must consume assignment addresses and actual current publications from that path.
Track writes to an entry borrow through its established referent. Intersect at joins, and include only
those destinations when exporting current values. A write on another return, a possible effect of a call
or a region reached by a nested pointer cannot prove that this path writes the containing borrow.
The guarded extraction, unsafe partial transfers and conditional write controls exercise this distinction.

A physical mutation retains the addressed operand separately from the invalidation footprint. Exporting a
returned region, wildcard or truncated static selector can name the bounding route only. The semantic
effect, codec, remapper, specialization hash and cleanup expansion carry route precision. An unrepresentable
selector cannot dispose the enclosing layout. Owned nested pointers still need ordered typed reads.
Round-trip the bounded route, keep exact explicit disposal, and use the existing physical bounds negative.

Bind entry-only guards at admission. Keep the checked predicate for a result-dependent guard until
normal completion supplies its result. Evaluate it with that frame's admitted arguments and actual
result for reachability, publication and ownership diagnostics. Joining different admissions must
never substitute caller inputs a second time or let a different exit supply a witness.

Capture one complete normal outcome at each checked exit before aggregation. Rows with the same
portable guard can have different values. SCC iteration keeps identical sets of rows. If successive
approximations for the same guard disagree, retain only common guaranteed bindings and record changed
values as unknown. Never grow recursive identities to distinguish dynamic executions. Remapping, interface
encoding, validation, specialization hashing, immediate calls and deferred execution must consume the same
row. Concrete aliasing may bind two formal destinations to one actual location: differing predictions widen
to unknown. Never retain the last enumerated prediction. Existing conditional owner, raw extraction, and
source/import fixtures cover positive and negative routes. Each exit publishes the witness clauses supported by its own returned owners alongside its guard,
values and duties. Guarded positive and fabrication-negative controls must agree through a package
round trip. SCC widening intersects promises across approximations of the same guard. Read-only
consumers needing unconditional facts intersect all reachable rows. Remove the independent carrier
table in the model, interface, SCC combiner and binders. Effects, reports and cleanup duties remain open.

The physical body runs against activation memory. Bind its effects to the actual frame when direct await
or Future.start starts it. Local frames carry private result coordinates. Imported frames can carry
the same effects without private coordinates. Join, cancellation and cleanup read the stored effects.
A nested task already started when its observation future forms. Its observer cannot refresh its body
effects. Unknown physical captures use conservative effects.

Normal return ownership belongs to the actual exit. Use the value's existing ownership record at that
exit, qualify transfers by the active payload on that path and publish the row. Remove the second pass
that adds a common transfer to empty alternatives. Returning a nullable owner, moving a generic
field, returning an unrelated value and returning an inactive payload are the distinguishing cases.
A caller must apply the selected row's ownership to the produced value once. During completion,
candidate transfer values can be inspected for guard checking, but their lineage and preservation
masks are installed only in the selected layout branch. A result payload whose activity is unknown
publishes separate active and inactive ownership alternatives with explicit complementary conditions.
The inactive alternative owns no payload. The nested union sibling and raw nullable extraction
tests exercise these rules.

For each boundary, actual checked operands remain authoritative. A possible effect overapproximates changed
storage. An outcome publishes only established values, duties and initialized shape under its own guard.
Keep the existing solver and reverse indexes. No new allocation identity from a code site or pointer comparison.
Earlier fixed evidence and growth constraints apply: target at most 1000 net production lines excluding moves
and tests. Source/import compositions, guarded extraction, pointer replacement, static domains, deferred
start/observe/cancel, normal cleanup and abnormal duties are the integrated controls.

## Remaining full-refactor boundaries

Await, scheduling, and opaque cleanup must consume the selected frame's transition directly. A capture can itself
hold an unrelated frame formed at the same source site. Searching nested values by site can execute that other
frame's effects. Only task observation follows receiver capture zero into another frame. Capture the private result
coordinate with the prepared transition at formation and relocate only at execution. Preserve explicit unknown
alternatives. Joining unknown with a known transition must reach a fixed point. The moved-frame/saved-pointer
tests cover relocation and retirement. A nested same-site frame with a different captured address must stay latent.

The portable frame binder receives the actual argument graphs and an optional semantic call in capture scope.
Both logical and physical frame activation use the receiver convention recorded with that call.
An opaque callable value has only explicit arguments. Keep returned frames and their captures even
when the body cannot be resolved. Local source
provenance is optional. An imported alternative has an instantiation point and an alternative position for
fixed-point joins, never a local deferred callee site. Callee metadata must retain formal capture positions
through substitution. Private admission requires explicit exported evidence and must never inherit unrelated
ambient caller facts. Source-site consumers cannot claim completion facts for an imported body.

FutureStart transfers the receiver's actual environment to the produced Task. At start or direct await,
refresh reached values in that environment before body effects. Observing a Task follows its retained
body environment and must not refresh the inner body's entry a second time. Repeating a program point
expires dependent logical terms in admitted arguments and outputs through the same rule as current values.
Keep each formal slot, carried physical information, independent siblings, and captured duties.

Execution start consumes the latent body's result expression and its admitted argument record. A local
selected call already has its checked body template. A returned named frame identifies the body by its
semantic callee and selected witnesses. Bind receiver and parameter roots using the producer's input mapping.
Prepare checked operations for replay without another query or another read of the old source slot. Body
normal-completion facts apply to the body alone. Opaque completions remain feasible without guaranteed
outputs. Conditional completion, reference overwrite, returned futures and imported generic extraction are
the positive and negative controls for this path.

Bound CFG replay records finite checked rows whose destinations can differ with the incoming address.
Retain distinct rows and their weak region writes at that boundary. An unrecorded value contributes an
opaque row. Resolve input captures and output rows from the operation's incoming branch, bind them on
that branch, and record joins for replay only. Branch observation maps cannot feed back into local checking.
The selected memory transition belongs to the invocation expression itself. A staged input shares
its syntax expression but executes only its own adaptation. Treating its CallInput occurrence as the
enclosing RawBufferPop tried to read an operand before argument completion. The copied-buffer and
recursive cleanup tests exercise this distinction.
A borrower with several possible destinations publishes dependency bounds to each and
establishes a current write only with one actual address. Replaying an exact write alongside a weak
alternative cannot establish exactness unconditionally. An opaque incoming completion cannot support
a common witness clause. Keep widening for recursive portable summaries
whose expressions can grow. Exercise the weak-region and alternative-return cases in the same panel.

Await admission captures the evaluated frame before report preparation, physical effects and
logical execution. Effects, reachability, reports and completion take that same receiver.
A report borrowing from an opaque await retains the admitted frame's captured dependencies. The pre-admission split alone may inspect
the evaluated operand to select a feasible producer. An await completion publishes resolved writes
to the observing operation's actual destinations. Flow selects the same frame alternative for both
physical and logical completion of ordinary awaits and Task observations. Both paths merge only
after that alternative has published its own output.

A lazy call's completed-result dependencies belong to the evaluated frame with the same capture
as its latent reports. Formation binds the selected result and report contracts using the staged
inputs on that branch. Copies, forwarding, and suspension retain that snapshot. At completion,
select it from the admitted frame, through Task observation if present. No dependency lookup
uses the producer's syntax occurrence at observation time. An unavailable completion keeps
the evaluated frame's dependency bounds and cannot establish independence. Imports with a
serialized frame still need output dependencies bound from their transported captures. Static
producer analysis before forward flow is used for conservative liveness only. Keep the known
independent-result/opaque-report pair, forwarded child cases, and ownership controls in the same
release panel. Remove the completion dependency table keyed by source expression.
Local completion also retains the selected body row on the body's admission for diagnostics.
An imported body has no local admission key. Its formal ownership, carriers and guards stay in the
body entry and never replace the observer's entry. A portable frame that captures a forwarded owner
can retain the enclosing admission's existing ledger entry. Take the intersection of the admitted
owners and the complete owned captures, and retain only owning clauses proved in the original entry.
An unrelated owner, a captured loan, or an unproved clause supplies no private authority. Positive
forwarding and fabricated-marker controls exercise the same rule through source and package imports.
Proof refinement traverses the actual retained frame graph. Each selected producer contributes
new private clauses and immutable captured facts. Apply all of those updates in the same bottom-up
rewrite so a changed nested frame cannot invalidate another update's parent handle. No locality or
matching syntax site can justify moving support to a different frame. The existing shared-frame
and suspended-carrier controls cover the same graph and proof semantics.
Both predecessors of distinct_sites return a complete live witness. The call trace confirmed
the join carries that witness, but its predicate references a zero-field aggregate overlay while
the requirement references the same base directly. Empty overlays add no values or identity.
Canonicalize them at construction so both operations use the same logical value.
Witness substitution compares logical proof values independently of physical capture coordinates.
A returned witness can be installed from a selected completion only when the producer exported
its actual support. The first portable support case is a live owning clause on a complete
result carrier with no external storage or address dependencies. Use the retained witness
ledger to establish this at exit, including every proved clause used to support a declared
guarantee. Keep it on the same guarded result row through substitution, binding and package
encoding. Bind it against the selected frame's admitted arguments and the actual result.
If the required support is unavailable, imported completion cannot create witness authority.
External lifetime and tracked address dependencies still require explicit portable identities.
Existing forwarded-input, fresh result, fabricated marker and cancellation cases cover the
ownership and phase constraints together.
The release candidate with portable support and static overlap passed 568 checker, 223 symbol,
137 bound-tree, and 218 interface unit tests. All six focused controls passed, including both
fresh-frame controls and the generic static relay through package import. The fixed composition
and owner panels retained the same 50 and 2 failing identities. Cached aggregate ownership was
then removed from the semantic and bound tables. That removal passed compilation, tooling and
style checks. An updated release panel is pending.

For result paths such as a Task outcome payload,
publish the complete outer result after the body finishes. Check ordinary, direct await, task
observation and package-import compositions together.

- Finish conservative joins of unknown-prefix dependencies and possible duties across guarded outcomes.
- Connect guards, values, effects, reports, initialized shape and obligations from the same exit.
  The outcome row owns the transfer layout. Pre-selection consumers can derive their aggregate
  view from the rows. Typed transfer preparation visits the source rows directly. Delete redundant
  cached aggregate fields and any duplicate independent transfer collection. Check the two
  conditional extraction failures in the same widened comparison panel.
- Preserve typed address-of-slot versus reading a stored pointer and every nested loan layer.
- A selected static binding belongs to the complete semantic selection. Storage planning must
  intern that selection directly from the checked reference and key the slot by it. Lowering and
  logical place lookup consume that planned key. Delete the lowering search through child syntax
  and the extra access-to-static cache. No unmatched declaration-only dependency can name a
  selected address. A declaration-level write retains region effects across its feasible slots.
  Fixture controls include two instances of one declaration, distinct product and thread storage,
  and source and imported invalidation of only the selected instance. Portable selected effect
  roots reuse the existing interface static selection encoding and substitution. Interface revision 4
  carries external selected storage separately from declaration regions. All four focused static
  regressions passed, including a package call that exposes only one of two specializations.
  The comparison remains 318 of 368 compositions and 34 of 36 owner controls. The bound-tree
  footprint test now asserts that declaration regions cannot supply exact addresses.
  Distinct open selections of the same declaration may resolve to the same closed instance.
  Structural conflict checking, mutation invalidation and output binding must share that overlap
  rule. Compare the selected descriptors from the plan and from portable resolved origins.
  Closed distinct selections are disjoint. Possible overlap supplies no definite current slot.
- Capture complete deferred environments with stable callable/substitution/witness identities. Refresh formation descriptors at execution
  start once. Await/join must reuse that outcome. Inactive and partial cancellation must retain only acquired duties.
- Use the same phase-aware instantiator for ordinary, deferred, lifecycle and abnormal paths. Remove competing returned/reference/stored
  preparation, special cleanup/deferred binders, and independent borrows/owned_result/stored_values/carriers/effects template fields.
- Publish observable abnormal writes, reports and acquired duties without a normal result.
- Move privately produced control-transfer results into their actual planned current-value slots
  before cleanup. A positional saved expression is not the result's live owner. Yield, break,
  propagation and catch must preserve the same ownership and carried dependencies during scope
  cleanup, before source evaluation can observe their result. Backward liveness consumes the
  planned private slot at completion and stops that demand at region entry. Production on the
  actual transfer or propagation edge alone initializes the slot. Check normal and abnormal cleanup
  against the live slot without forwarding the operand through a pending-expression map.
  The liveness migration restores nullable propagation origins, generic projections, finite call
  effect selection and guarded union ownership. The full release composition panel passes 319 of
  368, one more than the preceding checkpoint. The owner panel passes 34 of 36 active tests and
  has three ignored. All 568 checker, 223 symbol, 137 bound-tree and 218 interface tests passed.
  A trace confirms the actual initialized nullable result and inactive alternative both reach
  completion through the slot. Keep these checks while removing duplicated transfer destinations
  and sharing the output contract across normal, deferred and abnormal paths. The storage plan
  already identifies a control transfer's Initialize access. Physical transfer reads that slot
  and its declared type after the access initializes it. Logical publication takes the live
  private slot before scalar branch operands. A retired or uninitialized slot supplies no value.
- Join conditional witness evidence from the two actual produced values. Ordinary call completion,
  task observation and branch return already pass both values to merge_returned. Pass those values
  through to the same conditional proof engine that joins current slots. Remove its synthetic
  pending-result entries. Scalar branch expressions can still use the checked pending operation
  values at this stage. The proof engine must keep a live unknown payload distinct from an
  inactive alternative, and a borrowed referent supplies no owning witness. Compare the nullable
  and guarded union owner controls, including the negative proof tests, against the 319 composition
  and 34 owner passes. The change should eliminate writes and avoid another stored representation.
- Stabilize loops and recursive summaries with existing joins, recency and dependency indexes. No arbitrary cutoff loses dependencies.
- Finish typed external graph validation, immutable interning/sharing, public API documentation and structural/style audit.

Build coherent completed batches. An unresolved contract blocks speculative expansion. Read-only source tracing and focused experiments
remain allowed. Do not repeatedly guess a causal fix and rebuild for each helper. Mutating cargo xtask style is mandatory at batch
finalization. Run cargo xtask format before final handoff. Unrun checks stay unverified. No native retry while any boundary is incomplete.

## Tools, resources and final gate

Use the shared Cargo target from .cargo/config.toml, target/cargo. Before builds target/cargo/tooling measured232.83GiB
and the drive had99GiB free. Never clean shared outputs. Build checker and compilation together to retain feature reuse.
Release binaries under target/cargo/release/deps:
- bray_checker-05848d70c4b314f7.exe
- bray_compilation-4af58abd509cab18.exe
- bray_bound_tree-c8ccf88d46ce9e45.exe
- bray_symbols-04553e9aef447f44.exe
- bray_package_interface-4ceed9dd563cf950.exe
Use3 libtest threads. Builds and semantic tests have used approved require_escalated. Native LLVM/linker/executable work
must run outside sandbox. No native child or agent is running. Observe the one active build before launching another.

The frozen comparison-baseline.json has1718 identities. Four removed provider IDs are OPEN native replacements, never
passes. Keep the fixed1714 remaining identities and counts. Existing comparison-run.py uses8 workers,3 libtest threads,
isolates slow cases and runs attachment/admission tail separately. Refresh source fingerprints including untracked files.
The final joint gate is actual Linux/Windows bootstrap checking AND lowering on x86_64/AArch64, plus all1714 comparison
identities on the SAME candidate. Only after every refactor boundary is done may remaining native failures be repaired.
BRA-695 execution and BRA-696 shutdown remain separate. No new issues.

Re-read AGENTS.md before the final mechanical audit. Audit every changed behavior and equivalent repo paths, ownership,
architecture, DRY, sizes, public API/docs, errors, numerical policy, clones, invariants and tests. Run style and format,
required behavioral checks, update draft PR, commit/push, complete fresh review, fix valid findings and make ready only
when full implementation, acceptance and review pass. Follow the work-on-bray-issues contract gate after compaction.

## Repository and review

Issue https://linear.app/bray-lang/issue/BRA-694. Draft PR https://github.com/lejmer/bray/pull/717.
origin fetch/push is git@github.com:lejmer/bray.git. Verified public repository, owner lejmer, authenticated GitHub and SSH
account lejmer, admin/push permission. Reuse that evidence unless destination or authentication changes.
Automatic approval review rejected the non-forced checkpoint push because it did not recognize the
external publication authorization from the attachment. The explicit confirmation request is pending.
Continue local work. Do not retry the push or use another publishing route without explicit authorization.
PR title/body still describe older native framing and must be rewritten for final scope. The retained review through
4f767fbb2 has no findings at review5922465574. Current code needs a fresh full PR review at finalization. The skill explicitly
requires a reviewer subagent for that stage. No other subagents are authorized or running.
