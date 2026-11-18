# Compiler boundary refactor

## Scope and ordering

The [compiler architecture map](bra-694-architecture.md) records the current producers, consumers, and
cross-layer dependencies. Keep it current when a refactor changes those relationships.

Complete the full captured-value, typed-transition and guarded-outcome refactor on
feature/bra-694-native-bootstrap-contracts, then fix remaining native failures. The user's full plan is in the
pasted attachment from this chat. Start was05d2dc1b6. No baseline test run was requested at the start.

Defer repairs to `dispatch_ready` and `start_frame` until every agreed refactor boundary
is complete. Correct regressions introduced by each slice, then continue the refactor.
Verify the audit's native fixtures at the appropriate product boundary. Keep PR 717 draft
and BRA-694 in progress until the full contract and review are complete.
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

## Branch checkpoints

This checkpoint builds on 63b675072, which merges origin/develop at
1f6a3da96 without conflicts. Earlier checkpoints retain the captured graph and paired
publication at 11b860fa2, selected deferred frames and statics at 675d89c21, and produced
region results at d08975a30. Checkpoint 594180b13 admits implicit loans and joins actual
produced results directly. Checkpoint 3c9b4845a shares checked implicit input captures.
Checkpoint 570915f20 carries normal reports with guarded outcomes and preserves symbolic
report equations during recursive checking. Checkpoint 15abf8f8a uses admitted raw read
descriptors, one resolved cleanup receiver, and stable predecessor conditions on joined frames.
Checkpoint 33fd987bb shares typed subject and endpoint routes between calls and scheduled
cleanup. Checkpoint 18eac85b0 carries the selected receiver type through interface revision 7
and uses the same typed reader for captured and current memory.
Checkpoint fb61a7397 carries performed effects on normal outcome rows. Checkpoint 730dc963e
joins private Task results before normal publication. Checkpoints 8215a5920 and f043cd412
retain portable witness support and index admitted extraction snapshots by their input terms.
Checkpoint 21324247c moves physical references, stored dependencies and inactive payloads
onto the producing exit and carries actual union tags through interface revision 5. Call admission
uses the same feasibility decision for physical and logical outputs. Established physical descriptors
survive logical joins, and ownership transport reads the checked routes directly.

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
218 interface tests. The same candidate passed 321 of 368 compositions and 34 of 36 active
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

- Bind typed subject routes and stored endpoints once for ordinary invocations and scheduled
  lifecycle calls. The selected ordinary argument or the scheduler's actual cleanup receiver
  supplies the root. A prepared route identifies descriptor loads and owning backing selections.
  The same routes feed effect binding, staged reference transfers, carried dependencies and
  borrow observations. Resolve source descriptors against entry memory before invalidation.
  A region or an unavailable descriptor cannot establish an exact location. A normal cleanup
  completes its prepared writes before the next part observes them. Retain conservative unions
  when cleanup has no evidence to exclude an exit. Abnormal execution applies possible effects
  without normal writes or reports. Keep explicit, implicit, imported and deferred invocation
  consumers on the common route and address resolution. Replace the three parallel lifecycle
  indexes and the independent reference type binder. Keep ordering in the scheduler. Initial
  verification uses the borrowed receiver, finalizer-to-destructor reference replacement,
  memory-evidence cleanup, generic and imported cleanup, and the 368 composition and 36
  active owner panels against checkpoint 15abf8f8a. Existing negative variants must still fail
  with the same diagnostic families. The frozen 1714 and native gates remain separate.
  The route consolidation should fit within the production growth limit, with duplicate
  resolution removed. Reconsider if a common binding must fabricate an invocation result,
  reconstruct the receiver or use a post-effect memory read to retrieve an input descriptor.
  Scheduled cleanup now uses the admitted receiver to exclude impossible normal rows
  before staging references, stored dependencies and report equations. Actual cleanup
  results and per-exit effects remain open. The shared route candidate compiled and
  matched the 46 failing identities in the four unit and two comparison panels.
  A conditional finalizer probe with both branches joining at one exit still loses
  the selected reference. Existing unconditional reference replacement remains an
  acceptance control. Retain the probe when branch-local effect and output values can
  survive that join. The extra cleanup panel also failed scoped exit authority and
  conditional payload cleanup. Neither is in the preceding panel, so compare to the
  exact checkpoint before attributing those failures.

- Prepare local and imported deferred effect routes from the selected call's input types.
  A method frame must carry the prepared receiver type with the selected signature.
  Publish that type from the forming selection through the package and substitutions.
  Its parameters and completion type come from the existing selected signature. An
  explicit argument value starts from its admitted inline snapshot. Its first descriptor
  read uses that snapshot. All subsequent reads use actual reached addresses in the
  activation memory, even when an address leads back into the original source slot.
  Direct writes to an internal by-value parameter produce no external location. A
  descriptor with only a region cannot supply a subsequent actual read. A captured
  owner with unavailable backing retains a conservative effect. Static routes retain
  the selected static identity and address domain. Local and imported frames, ordinary
  calls and cleanup must use the same typed reader. The observer of an already-started
  task consumes its saved body effects. Remove the deferred flat projection check and
  type-free target projection. Preserve existing source/import, saved-pointer, moved
  frames and allocation-bound negatives. Use the current panel and the deferred and
  lifecycle controls. Reconsider if routing demands materializing the captured snapshot
  into current memory or fabricating a location for an inline value. Keep production
  growth within the existing target and replace duplicate descriptor interpretation.
  The nested backing regression passes, the four unit suites are clean, and the source,
  import and owner panels retain the same 46 failing identities. The cleanup reference
  replacement control passes with its original fixture. The extra cleanup panel passes 18
  of 20 active cases with two ignored. Its two open failures are the missing scoped exit
  authority diagnostic and conditional payload cleanup.


- Record portable effects as a prefix of the reached composite state. The same operation
  already resolves its address, possible bounds and mutation kind before it modifies
  memory. Export that descriptor there. Success adds normal effects to that successor.
  The captured failure transition adds performed or possible prefix effects on the
  abnormal successor, including on a later caught path. Hypothetical failure capture
  must not contaminate the successor that returned normally. At a join, union may-effects.
  Never infer a definite write, layout, ownership or lifetime from that union.
  At a reached normal exit after cleanup, attach its prefix to the same output row.
  Keep a separate conservative possible envelope for failure and unselected alternatives.
  Bind possible effects once while entry descriptors are live. Select the subset on
  admitted normal rows using the same row identity as reference and report outputs.
  Widen SCC approximations by unioning possible effects within a guard while keeping
  guaranteed writes as intersections. Preserve each effect's actual route and bound
  across generic substitution, codec validation/remapping and specialization identity.
  Remove the independent normal classification accumulated across the CFG. An aggregate
  normal slice is only a derived shortcut for phases with no selection evidence.
  The conditional finalizer, borrowed cleanup and captured saved-pointer controls cover
  these directions. The panel must demonstrate no extra retirement on a disjoint exit
  and no fabricated preservation on a feasible writing path. Keep caught abnormal and
  inactive cancellation effects separate from normal outputs. The fixed comparison and
  bootstrap gates remain pending. Reconsider if this requires replaying effects from
  original slots, a separate CFG traversal, or adding branch histories to loop identity.
  Internal joins may still need to carry guarded stores and prefixes from their
  predecessor snapshots before a single exit can publish branch-specific outputs.
  Source flow records portable may-effects in the reached state and publishes them on
  each exit. Calls, cleanup and activation now select the feasible normal effects from
  those rows. The package codec, record selection, interning, type substitution and
  specialization use interface revision 8. A checked envelope covers every row.
  The release candidate kept all 46 failing identities in the two behavioral panels.
  Checker, symbols and bound-tree tests pass. Four handwritten wire fixtures still
  had the old aggregate normal field, and one encoded a normal row for a noncompleting
  contract. After updating those fixtures, all 218 interface tests pass. Style and
  maintained-source formatting also completed. Keep the conditional finalizer case
  open while internal joins still drop its selected reference.
  The first Task trace finds the actual future body and its private witness at start.
  The wrapper later reaches observation without that body in capture zero. Direct await
  reaches the body and resumes the same witness. Preserving result metadata in normal
  output selection did not change the failures. The split attempts each produce the
  actual Task into pending_results. StorageFlowCollector rejoins those branches with
  ExecutionJoin::Boundary, which drops differing pending results. Join the actual
  pending value at this operation's existing expression before normal completion.
  Retain each producer, predecessor selection and opaque alternative. Keep this value
  private until the normal completion boundary. The opaque and opposite-branch task
  cases must still reject fabrication. The private result join now keeps both actual
  Task alternatives. All ten focused positive and rejection cases pass. The wider
  panel gained twelve source and imported Task cases with no new failures. It now
  passes 335 of 368 compositions and 35 of 36 active owner controls. All four unit
  suites pass. The remaining Task cases carry nullable or nested formal witnesses,
  imported produced results, same-site iterations and an effectful owned body.
  The raw trace finds the joined nullable value in admission's extracted set and the
  checked owning transfer from the same formal term at completion. The borrowed source
  in the completed transfer still lacks extraction. Selecting the checked output kind
  and stripping the output loan for an admitted extraction did not change the result.
  Admission currently stores extracted values in an unkeyed set. Physical capture can
  subsequently add metadata to the logical operand. Completion binds the same term
  again and requires equality with the earlier whole snapshot. Store each admitted
  extraction under the checked transfer's input term. Completion reads that snapshot
  directly and uses the prepared output kind to distinguish an owning transfer from
  a borrow. Deferred custody already visits actual nested captures. Remove the second
  enumeration that copies their extraction entries into the observer. At a join,
  retain a keyed extraction only when the actual admitted snapshots agree. Borrowed
  outputs, inactive payloads, fabricated owners and a second raw read cannot inherit
  the admitted witness. Verify the unchanged sibling extraction and existing deferred
  cases alongside the nullable positive and rejection cases.
  Both repairs must reuse captured handles. Reconsider any approach that allocates a
  new frame or owner identity or revisits a pointer's source after admission.
  Completion now looks up the admitted snapshot by the formal transfer input.
  The recursive custody traversal replaces the observer's flattened extraction set.
  All four unit suites pass, and the same 34 behavioral controls fail. The nullable
  exchange still rejects the live complete owner. Further work must trace the
  caller's selected storage publication and witness invalidation on the same path.

- Carry external support for a returned witness from its owning ledger entry.
  ExecutionWitness.dependencies and each selected ExecutionGuarantee.addresses
  are authoritative. Export only support whose storage owner is reachable through
  a checked entry, result or exact static route. Each address requirement retains
  its evaluated descriptor separately from the owner that bounds its lifetime.
  Distinguish missing support from an empty, independent support set. Remap these
  identities with the witness and guarded row. If every alternative establishes the
  same guarantee, a conservative join retains the union of possible supporting
  owners. Any unsupported alternative removes the claim to portable completeness.
  At admission bind formal support directly from the captured descriptor and its
  admitted storage. At completion install a clause only if its complete carrier and
  all required owners and descriptor observations are still present. Reuse the
  existing witness dependency indexes for invalidation, including on cancellation.
  Never recover an owner from the predicate name, naked value equality or a
  substitute type. Declared result clauses can use multiple support clauses of
  the same concrete carrier, but unrelated carriers cannot jointly create one
  transferable authority. Use the guarded producer, nullable extraction and
  borrowed reference controls through source and package import. A source positive
  must stay valid while its actual owner is alive, and the retired-owner and
  fabricated-owner negatives must continue to fail. Replace the independent-support
  bit when the checked and portable forms can both represent these relations.
  Reconsider if the design requires storing duplicate logical ownership snapshots
  or evaluating the original source operand after its consumption.
  Producer support now survives package encoding, selection, interning and
  specialization. Deferred completion binds it from the admitted arguments and
  the actual result place. All four unit suites pass, including the extended
  interface round trip. The two behavioral panels retain the same 34 failures.
  Ordinary completion still uses its existing observation binding. The focused
  nullable and nested formal controls still fail.
  Ordinary and deferred completion must install inferred carriers from the same
  selected row, actual published value and portable support. The producer ledger
  owns the support. Remove the inferred-carrier path that rediscovers supporting
  lifetimes from declaration inputs and return origins. An unavailable owner or
  descriptor withholds that inferred authority in both paths. Explicit declaration
  clauses still use their own checked observation and entry binding until the
  producer carries those identities directly. Avoid reinstalling declared authority
  when a complete row already supports the clause. Keep first-entry observations
  distinct from refreshed current borrowed contents. Compare the source/import
  fresh owner, borrowed result, invalidation and fabricated-capability controls on
  the same candidate. Reconsider if sharing discards an actual current referent.
  The fresh-carrier and post-completion fabrication cases cover the same producer
  through both source and import. The source borrowed-result and mutation controls
  exercise lifetime retention and invalidation. Preserve only an exact selected
  static instance or current pointee as external support. A declaration or earlier
  allocation can bound a footprint without establishing the current owner.
  Shared publication now passes the six live-carrier, fabrication, borrowed-result
  and mutation controls and all four unit suites. The wider panels retain the same
  34 failing identities. The conditional cleanup and scoped-exit authority controls
  remain open. Interface carrier validation now lives with captured-value validation.

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
  The cleanup scheduler selects one lifecycle step and obtains its reached receiver before
  any effects. Effect binding, physical outputs and observation publication consume that
  target. They can read current contents at the appropriate phase without following the
  source slot again. Use the prepared output path to publish logical coordinates. A broad or
  unknown receiver cannot establish exact outputs. A skipped conditional cleanup cannot
  publish a completed output or retire its receiver. Check scoped cleanup, stored reference
  replacement and deferred cleanup in the existing source/import and ownership panels.
  Continue toward one instantiator. Reconsider if a consumer still needs the original
  receiver access to find its physical target after effects.
- Publish observable abnormal writes, reports and acquired duties without a normal result.
  The composite state at the reached panic or cancellation exit owns the performed
  effect prefix and the report that survives cleanup. Today `collect_completion`
  reads that state only for normal exits. Abnormal collection separately unions
  reports, and invocation captures the all-path effect envelope for the caught
  failure edge. The envelope is needed to check entry conflicts. It can include
  effects that occur only on successful execution.
  Reuse the captured outcome shape for abnormal exits. A reached row retains its
  entry guard, performed effects and report. Its destinations and duties refer to
  the surviving external state. It has no normal result, result carrier or result
  ownership. Absence of checked abnormal rows represents unavailable evidence for
  opaque and provisional bodies. Known absence of an abnormal path is distinct.
  Source calculation, recursive summary widening, specializing and package import
  must preserve that difference. Resolve selected normal and abnormal effects
  from the same admitted environment. Apply the abnormal prefix and its external
  outputs before caller cleanup, with original addresses and without reenacting
  operand evaluation. Retain the possible envelope solely as the upper bound for
  admission and unavailable bodies. The existing catch fixture covers an actual
  write before panic. Extend it with a write confined to the normal branch and a
  caught panic before that write. Both controls must share the same consumers
  through source and package import. Reconsider a design that duplicates result
  preparation, loses the distinction between absent and opaque, or tracks effects
  independently of the row that supplies its writes and report.
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
- Route moving memory reads through their actual explicit attempt in both checking and replay.
  Their selected operation supplies the evaluated address, and the reaching typed value moves
  before retirement. Staged input adapts one operand and must not execute the enclosing read.
  Remove the duplicate checks that treat the enclosing syntax expression as the active site.
  Preserve target and initialization diagnostics on the true read. Use the raw nullable
  extraction, double read, generic storage, and buffer move controls to distinguish phase errors
  from missing captured ownership.
- Implicit scope and iteration calls must admit the actual typed receiver value. The checked
  storage input plan selects Borrow for the receiver, and the physical admission uses its
  capability. Logical admission currently copies the raw source condition for both moves and
  borrows. Captured Read then receives an owned resource or an undecorated formal value and
  panics during source summary binding. Apply the checked borrow at admission and retain its
  reached value. Keep Read's invariant and the owned transfer path intact. Check the existing
  sync and async cleanup-report tests, borrowed reports with an independent explicit result,
  and iterator and scoped lifecycle controls. Both existing report tests now pass. The former
  opaque fixture forwarded a known producer that emitted no report. Its replacement produces a
  suppressed incident carrying the scoped loan and must diagnose an escaping dependency in
  source and package checking. Both variants pass. The full panel now passes 321 of 368
  compositions and 34 of 36 owner controls with three ignored. Moving-read phase classification
  leaves the existing two extraction failures, where the generic result lacks ownership lineage.
- Extend receiver-first captured inputs to implicit scope and iteration calls. The checked access
  and its passing mode already define the input type, and the existing capability or evaluated
  slot supplies its physical descriptor. Keep the capture and the reached contents as separate
  nodes at admission. Replay, effect binding, and result publication should consume the same
  record. Do not inherit owning inline observations from a receiver's storage into its borrow
  descriptor. Keep the scoped report, borrowing an inline receiver, consuming iteration and
  latent scoped execution controls. Reconsider if the protocol needs a separate capture table.
  The final capture implementation passes eight focused report, propagation, cleanup and
  deferred controls. All four compiler unit suites pass and the broad panel has the same 321
  composition and 34 owner passes. The capture loop shares a receiver-first iterator.
- Keep physical output origins and stored dependencies on the ordinary guarded exit that
  produced them. The collector already pairs the resolved ReferenceExit with its actual
  symbolic return. Convert that physical snapshot before merging exit rows. Use row-local
  allocation anchors and explicit inactive payload information. Before selecting a row,
  exactness requires the same established origin on all feasible active exits. An unknown
  active exit cannot act like an absent payload. The common view is derived from the rows.
  Package remapping and SCC widening preserve the row and its conditions. Replace the
  unconditional borrows and stored-values fields as the checked consumers migrate. Use
  the forwarded nullable origins, distinct branch escape roots, generic stored dependencies
  and guarded-owner negatives through the same source and import path. Reconsider if
  conversion must look back into caller syntax or needs a second independently joined table.
  Source conversion and package row encoding are in place, including specialization hashing,
  substitution and selected graph remapping. Interface revision 5 includes the new row data.
  The semantic and interface contract-level fields are gone. The 368-case composition and
  owner panels retain the same failures as 3c9b4845a. The additional query panel exposes a
  recursive forwarding precision loss and a loss of inactive variant information on a
  fallible result. An explicit null is now exported separately from the live address.
- Consume guarded physical rows with the admitted call. The typed output paths are indexed by
  the actual summary relations and stored endpoints. A provably false input guard must leave
  neither an output nor an authority bound. Unresolved completion guards remain feasible.
  Use the same guard binding as logical output instantiation. Retain the actual inactive
  payload after production and through moves. No second unconditional table of origins or
  stored dependencies is retained. Check mutual recursion, fallible nullable wrappers, and
  exact-versus-null through source and import, including the existing negative cases.
  A result guard cannot be tested against the slot's old value. Until its private result
  exists, use a conservative join. Determine inactivity from the actual produced tag and
  physical output coordinates already observed on reachable exits. This also handles result
  paths absent from the local access plan. Do not enumerate the result type to reconstruct
  its carried dependencies. The fallible-wrapper control passes. Do not weaken validation
  or treat missing data as absence.
  Encode the actual constructed union tag in the captured value itself. The payload and tag
  share one producer and survive source and imported moves. Preserve variant identity through
  specialization, package validation and interning. A guard may select a value, but cannot
  substitute for the tag of a produced value.
  The mutual-recursion trace shows both current normal rows still return the exact same
  borrow before the forwarding caller loses its origin. Prefer the descriptor already
  installed by normal physical production. A joined logical value can be less precise.
  It must never overwrite an established address with bounds or install that payload
  a second time. Opaque returned footprints cannot stand in for a produced address.
  The mutual-recursion and imported tag controls pass. The extra query panel passes
  58 of 59 tests. Its transitive witness diagnostic also fails in the earlier tooling
  binary. The full 368-case composition and owner panels still match 3c9b4845a.
- Carry the checked moving-read target into ownership and replay. A typed current physical
  target already establishes the view for that load. The logical consumer may consult its
  current value to transfer lineage and vacate the source. It must not separately reject
  the target because no syntax access expresses that same projection. Keep raw-read,
  double-read, fabricated witness, nullable and deferred extraction cases together.
  The move currently reads the descriptor separately for effect resolution, logical
  observation, physical transfer and ordinary initialized-state tracking. Resolve the
  admitted pointer descriptor once. Use its precise typed view for the logical snapshot,
  its full address knowledge for the physical move, and its conservative footprint for
  conflicts and partial effects. Read the current pointee before vacating it. Replay keeps
  the checked target and no second source path. Check where the two extraction fixtures
  lose the formal lineage before changing ownership conditions. A conservative region or
  scalar observation cannot supply missing ownership. Reconsider if the physical and
  logical paths still need separate resolution of the pointer operand.
  Captured raw-read operands now supply mutation effects, the logical move and the physical
  read and initialization update. The 368 composition and 36 active owner panel retains its
  49 failures. The focused raw buffer reachability control also fails. Its result on the
  preceding report checkpoint was not measured. Both direct extraction failures persist.
  The moving-read trace shows the exact typed borrowed input is reached and its live
  formal value is captured. Its ownership layout is still empty after the move. The
  move must record original-input lineage for that same live snapshot even if earlier
  observation recorded a non-owning layout. Existing equality to the actual current
  formal value and successful move into a non-borrow type bound this authority.
  The guarded sibling extraction now passes. Nullable extraction still rejects the case
  that observes the moved active payload. Traversing initialized components at admission
  instead of checking the aggregate root did not restore the witness. Locate the
  prepared transfer and the actual payload's evidence before changing admission.
  Keep the double-read and fabricated-owner negatives.
  Keep the checked input root and typed route with each owning transfer. Admission and
  survival consume that route directly. Identify the route by its input and output, which
  remain the same when different feasible exits intersect their preservation masks. Staging
  shares only transfers established on every admitted normal row, including nested guards.
- Normal report dependencies belong to the same exit row. The result-dependency solver
  already has the report requirements and definition bindings at each reached exit after
  cleanup. Close those actual equations at the exit. Absence of a report and a report with
  no dependencies remain distinct. Across feasible alternatives, reports and their
  dependencies are possible outputs. At invocation capture the admitted report dependencies
  before consumption, then publish only the applicable completed report. Abnormal edges
  keep their own report and possible effects. A caught or suppressed report must retain
  its actual owner and borrowed dependencies. Replace the independent normal report field
  through SCC widening, substitution, the interface and checked calls. The synchronous
  cleanup reporters and both borrowed report controls exercise this through source and
  import. Bind each report's storage obligations once alongside its portable requirements.
  Use the existing selected input-site map to translate the portable equations while the
  inputs are still available. Carry row identity through execution joins so consumers can
  filter the frozen report snapshot using the same admission as physical outputs. Retaining
  a report from an excluded row or dropping one after a conservative join both violate the
  contract. A normal report with an unresolved result condition remains possible until the
  produced result is available. Reconsider if binding needs a second query of the callee
  summary, a second guard interpreter, or a later read of the original operands.
  Report rows now cross interface revision 6. The bound report retains both checked
  storage obligations and the portable equations. Recursive calls keep their existing
  symbolic equations, with concrete obligations checked against the SCC approximation.
  Ignore a row when the admitted input disproves its compound guard even if the result
  clause remains unresolved. The true and false report calls pass through immediate
  execution and await. Source and import borrowing, callback report channels and the
  abstract recursive query pass. All four unit suites pass. The broad panels match the
  49 existing failures on 21324247c. The returned-value and recursion panel passes 59 of
  60 tests. Its one failure also occurs on 21324247c and in the earlier tooling binary.
- Preserve the actual predecessor selection when joining previously formed frames. The
  result and current-value joins receive both incoming proof contexts. Their opposing
  stable branch facts distinguish which existing frame reaches the joined slot. Attach
  those facts to the existing producer and an opaque alternative, when present, without
  replacing either frame. Only actual opposite ordinary facts can distinguish
  predecessors. Mutable observations and trusted claims cannot select a frame. When
  either predecessor remains feasible, effects and dependencies still cover it. A
  join at the same producer may conservatively widen without making another frame
  identity. Use preformed future and task selection and fabrication, the opaque
  alternatives and same-site loop cases through source and import as controls.
  Preformed future selection now passes through source and import. The broad panel passes
  323 of 368 compositions and 35 of 36 active owner controls with three ignored.
  Preserving the Task wrapper result after inner completion did not change the selected
  Task cases. The next trace must show whether the selected body reaches observation
  and whether its captured proof is available there. Preserve the inactive panic
  and cancellation outcomes and keep the false-branch fabrication cases in the panel.
- Stabilize loops and recursive summaries with existing joins, recency and dependency indexes. No arbitrary cutoff loses dependencies.
- Finish typed external graph validation, immutable interning/sharing, public API documentation and structural/style audit.

Build coherent completed batches. An unresolved contract blocks speculative expansion. Read-only source tracing and focused experiments
remain allowed. Do not repeatedly guess a causal fix and rebuild for each helper. Mutating cargo xtask style is mandatory at batch
finalization. Run cargo xtask format before final handoff. Unrun checks stay unverified. No native retry while any boundary is incomplete.

## Active contract for failure prefixes

The trace at 63b675072 follows checked producer rows, local and imported invocation
admission, the failure edge, the catch boundary, and the caller's exported effects.
Panicking before a write currently publishes that write on the catch path. The
failure edge applies the all-path effect list. It has no captured prefix from the
callee's abnormal exit. Both source and import reproduce the error. The catch
also disposes a report. Preserve that cleanup effect.

The reached abnormal exit after cleanup supplies the entry guard, performed
effect prefix, and report dependencies. Those values share the same portable row.
A known empty prefix can still produce a report. An opaque abnormal exit retains
the conservative all-path bound. A failed admission can panic without running the
body. A failure row never produces a normal value, a normal write guarantee, or a
returned carrier. Cancellation after a completed await retains the completed
child value's duty in the existing CFG.

Collect the existing effect prefix and closed report equations at the same exit.
Carry the row through component widening, substitutions, validation, export,
interning, and bound calls. Resolve addresses from admission once. Let call,
deferred, and cleanup paths use the selected failure prefix and its report.
Keep the all-path envelope for queries that genuinely cover every outcome,
including execution certification and unknown summaries. Remove the separate
abnormal report field and all-path failure capture for known exits. Reuse the
existing row and effect binding. Reconsider if the change needs new runtime
identities, a second guard interpreter, or repeated reads of input slots.

Extend the existing catch control to check panic before and after a write
through source and import. Test for the particular caller-cell mutation so
ordinary report cleanup remains visible. Also retain normal preservation, borrowed
reports, suspension and cleanup controls. The frozen comparison and production
checking and lowering still qualify the whole refactor together.

The release executable with SHA-256 6A9B6E4CAA1748D2A6ECADA9C99FA7007D1F037A0EA79BA1AA3830649A05AE4F
passed the caught writes, empty scoped completion and opaque future controls. The normal
owner and Windows environment controls still fail. The new imported owner panic came
from an export path that retained structural ordering for failure effects after remapping
to interface identities. It now canonicalizes the mapped effects and reports. This correction needs a rebuilt run.

## Active contract for guarded result production

The checked result slot and the state that produced it are authoritative. Both arms of a
structured result already install the actual value in the same planned slot before cleanup.
The current join stores a common placeholder. It retains formal ownership in a separate
choice table, while the captured-value graph reads the placeholder. The selected generic
payload, its lifetime, the performed effects, and the dependent report can therefore come
from different paths. Substitution at import cannot repair a value that was never exported.

Carry reached result alternatives through the existing composite solver, through the
result's completion and subsequent operations, and to real function exits. Use the
existing producing edges and slots for identity. Retain physical storage, logical facts,
cleanup and effect state together. Material alternatives share immutable substructure.
Widen with the existing conservative merge at cyclic boundaries and when the number of
alternatives exceeds the analysis budget. Never recover a payload from an ownership
description, create ownership from a possible region, or retain an effect from an
alternative that cannot reach the observed exit.

The published checks, scope transitions and operation decisions must join evidence from
each reachable alternative under their current must/may rules. Export the actual exit
records with the same outcome guards used for admission and publication. This slice must
still address how callers retain semantically distinct admitted rows through ordinary,
deferred and cleanup operations. The Windows producer already has exact alternatives, so
the address and its witness must also survive selecting a consumer row and any later
effect. Finite reuse of existing block and value IDs is necessary. Reconsider if the design
requires a second solver, independently interpreted output lists, cloned plan graphs, or a
worklist state that grows with loop iterations.

The first guarded build restored interface import. It left the three ownership controls and
Windows environment red. Completion had already been applying individual rows to temporary
execution states, then merging before physical storage could read the selected row. The next
cut passes selected call rows through the same composite state as resolved physical effects and
retains the selection during normal publication. The first build also split a unit-valued
conditional and the diagnostic retained one of two deallocation sites. Preserve ordinary
control joins and use the actual yielding or breaking edge for value production.

The build with SHA-256 760F2DB2DD615D53980B04456550C0A01E2E9C647FE18D299DC84347FDC89AA9
passes all fifteen focused tests. This includes both source and imported normal owners,
both caller-cell write timing controls, scoped MIR, production Windows environment, and
both deallocation origins. The pinned diagnostic comparison runs 1712 fixed identities
and records two exact replacements retained in the suite. Four native replacements
remain open. Some test-only files changed during compilation, so this binary qualifies
diagnosis and does not qualify the final source state.

The comparison recorded 1,168 passes and 390 failures. A further 154 cases did not
finish before their shards timed out. Of the failures, 294 shared the deferred provider
failure. The probe build captured and removed temporary instrumentation for this case.

The current contract is selection of a deferred outcome through observation and propagation.
In `formal_array`, `start` retains the admitted array and selected producer. Both `join`
and `cancel` produce a `Completed` payload bound to the original array and restore its
element witness. Their abnormal sibling cannot publish that payload or witness. The trace
shows both states before `merge_completed`, then an opaque expression and no witness after
`try` selects `Completed`. Keep these states distinct through the existing composite flow
so variant refinement selects the actual payload, effects, cleanup, and report. Use the
same selection for analysis and publication. Bound the alternatives with the existing
join budget. Widening conservatively joins the reached states. Use stable producer and
observation identities and keep loop headers as widening points.

Check the existing source and imported deferred matrix cases for a valid completed
array owner and rejection of independent or fabricated owners, alongside panic,
cancellation, captured cleanup, and the previously passing ordinary call and scope
panel. Compare the frozen cohort on the resulting build. Refactor the existing merge
path in this scope, retaining only conservative widening. Reconsider if ordinary
unrelated branches multiply states or the producer's actual capture needs rebinding.

Native address escape must use the normalized footprint of the resolved raw address
in current normal and failure observation reachability. Existing raw reachability can
span the backing object even when the pointer designates a single field. The failure
snapshot must come from the actual admitted state. Disjoint storage and ordinary
loan reachability still protect independent witnesses. Check all address-authority
and disjoint opaque controls on the same build.

The selected deferred build, SHA-256 54C1A640FAA7AF5CFFA3BC923185EBDDC2C37C6690ED4D4273E836063554B341,
checks the provider and passes 288 of 302 deferred controls. Ten of the remaining
failures also failed at E7. The four new failures affect the fresh nullable permit
and the discarded task permit through source and import. Compare both before changing
selection, since the latter accepts an unavailable permit. The same build passes native
pointer reachability and the previous owner, scoped, catch, and Windows environment
controls. The ordinary result-completion and borrowed propagation controls remain red.
The frozen comparison on the pinned executable finished with 1,534 passes and 117
failures. Sixty cases have no result after bounded shard timeouts. Against E7, 102
previous failures recovered and 26 previous passes failed. The prior partition build
reported 390 failures and 154 unrecorded cases. The smaller shard run recovered many
cases that had stopped on the deferred provider and recorded more independent controls.
Separate bounded runs recorded 38 additional passes and five additional failures
from those sixty cases. The combined receipt is 1,572 passes, 122 failures, and 17
unfinished cases on the same executable.

The two new deferred negatives and several owned-transfer negatives protect one
rule. A copied scalar cannot keep the original whole permit alive. Both deferred
negatives reach their actual selected output. The nullable replacement has its own
producer identity, and the discard result has no permit. Retirement still finds the
original owner in its outside values and keeps the supplied permit witness available.
The next trace records the current storage provenance of those outside values and
the admission and completion retirement paths. Both positive retained-owner controls
and the exact negative inspections stay in the panel. Also trace the ordinary
borrowed result postcondition to its prepared observation, published condition, and
consumer result refinement. Preserve failure-side mutation and the existing Windows
and catch controls while checking any change to ordinary completion.

The storage-slot trace identifies the same live local binding in direct consumed
permission, deferred nullable replacement, and task discard. The storage collector
moved the input check and transfer before the normal/failure fork, but temporarily
took its execution state out of the storage state. The shared move transition
therefore retired the physical path without retiring the current logical owner.
Apply the existing checked move with both halves of the admitted state present.
Snapshot the failure only after that move. No separate witness or path inference is
needed. Check direct, nullable, and task rejection alongside successful transfer,
unrelated owners, both catch write timings, and raw address authority.
The admission build `DFECC9ED3BA87B24CAFF52E5B8B1134144ED95DC2CA2CA5F74BB90F39F39751F`
ran both matrices and the coupled controls. It passed 783 of 816 checks. Twenty
former failures recovered, including the four deferred permit negatives and the
related direct and deferred ownership cases in both modes. All 761 passing controls
with a prior comparison receipt remained green. The remaining 33 failures were
already red. Both catch timings, raw address authority, scoped MIR, and Windows
environment still pass on the same binary.

The ordinary postcondition trace shows the selected successful row has the actual
`open = false` referent and its completion predicate simplifies to true. The later
ordinary verification replay merges output rows before the consuming branch, while
the selected storage flow still sees them. Trace the published row and verification
replay through its return and propagation edges. The fix must carry selection from
the checked operation into this existing replay. It must preserve the guard and
referent under the certification's specific entry assumptions and reject the later
overwrite control. Do not add a second ordinary postcondition implementation.
The replay now consumes the published transition values to choose a feasible
normal row and keeps it through CFG refinement. Composite flow and replay share
the bounded outcome join, production-edge selection, and call-row splitting.
Postcondition verification and finite correspondences read the reached states
directly. Existing analyses that need their common premises use a conservative
joined view. Replay still uses the common physical observation table where a
selected physical effect or a separate cleanup phase has not migrated. The
ordinary contracts, both matrices, and the array-copy audit controls are on the
release build panel. SHA-256 E6172A609F47E6D9C0128F05719E3BBF3E7F828AD813B8F05C947EF0CF859828
passed both ordinary postcondition controls and all 783 previously passing
checks. The enlarged panel records 830 passes and 32 failures. The extra
failure in that panel, direct copied-field authority, also failed the frozen
comparison. The array repeat now treats one element as one copy. Zero evaluates
the producer and creates no copied element. The checked one-element list moves
the owner without duplicating its witness.

The remaining deferred cases cover inactive capture, returned dormant frames,
multiple feasible body producers, and repeated captures at a loop site. The
spec preserves an intact witness in an unstarted Future, including one returned
inside a selected alternative. A copied scalar can match the live witness but
cannot carry it. Starting the body makes private authority unavailable until a
selected completion actually returns its owner. The active trace follows admission,
frame formation, and the body outputs through these positive cases and their
discarded or inactive siblings. The array and selected nullable cases trace actual
normal output bindings and their witness-bearing components.
The frame trace confirms that a live permit is suspended at lazy admission and
remains suspended in the unstarted frame. The real producer and its private
capture both retain the original owner. Move the suspension to the existing
actual execution boundary for this case. Synchronous execution, start, and
await still suspend the admitted ownership before any effects. Verify that
dormant authority stays available while a started or destroyed capture does
not supply it. The trace also shows a separate deferred body completion that
admits two concrete rows and joins them before the active payload is tested.
Carry row choice through the frame's actual operands, physical execution, and
publication. Selecting a logical row after applying every physical branch's
effects would leave the underlying contract split.

The dormant-admission build `026A7BCC1C94F592C7AE66445779BC538ED8FBB099C978878B91FA5730AD8DCD`
restored all four dormant and conditional capture cases through source and import.
All 830 previous passes stayed green. Two newly exercised audit cases failed.
The nested conversion used a declaration predicate as a value in a constant
function. The scoped exit control had previously been absent from this panel and
still needs a provenance trace. The resulting panel has 842 passes and 30 failures.

The next connected boundary is execution of an admitted frame. At direct await,
`start_frame_effects` reads the original entry before `set_awaited_execution`
refreshes the pointees and binds result rows. Both must use the refreshed entry
instance while retaining each original address descriptor. Once a direct await
has an admitted producer, its feasible normal rows own the selected effects,
private value, carrier publication, and normal report. The cancellation and panic
edges both inherit the consumed frame's admitted state and can apply only their
performed prefix. For a running task the already-resolved body addresses are
authoritative. Observation may select its outcome but cannot bind fresh body-entry
addresses. Opaque or widened frames conservatively retain possible effects and
provide only common guarantees.

Share the entry-preparation phase with proof replay and reuse the existing bounded
producer and row selection. The ordinary failure snapshot already implements the
required admission-to-abnormal boundary. Extend that boundary to the suspension
edge before adding another outcome container. Reconsider if the change reruns
admission after suspension, independently interprets guard order, or clones
unbounded frame state. Task outcome and cleanup activation still require their
own ordered transition work. The new trace follows formal frame output encoding,
actual execution arguments, and the supported witness after completion, including
both the independent owner and the fabricated sibling of a returned Future.

The frame-attempt build `2D57680D8FE39BD77110D7306CC636808504FAE2ACE7BD10CAF492F40ED3293A`
uses the same admitted frame before binding effects, selecting a normal row and
restoring the suspended state on abnormal edges. Both conditional deferred and
suspending owner controls recover through source and import. The corrected nested
conversion passes. The broad panel records 828 passes and 27 failures, and a
supplemental run passed all 35 earlier controls it exercised. The two additional
failures also reproduce on the pinned prior executable. Task observation of an
already-running body still needs selection of its originally resolved effect rows.

The source trace for `choose_pending` shows a real formal Future owner and its
unchanged opaque frame record after dropping the other input. Physical flow can
retain a descriptor even when reached data changes. Framing currently rewrites
the Future value to a generic cleanup post-state and then unconditionally
reattaches its old frame, including in cases where that frame did not survive.
Retain the actual handle and latent frame graph when the checked descriptor is
retained. A replaced or consumed handle cannot retain that graph. Export the
original formal frame expression only when it is still the actual value.
Verify passthrough, fabricated and independently selected Future rows, unexposed
descriptor controls, and started and discarded authority. The same trace shows
preservation materializing a payload for a known absent alternate. Share the
existing tag-based inactive-component rule with transport and publication. No
unknown owner or unproven payload should acquire complete contents from this
change. The copied borrowed field and the scoped witness are separate ownership
support problems and keep their exact source and import failure controls.

The descriptor build `DD7FE932BEFA35B3520B0D49C0C9DC4891EB07128676EE455DDE692EFD27C831`
restored both `choose_pending` cases. Both fabricated rows still reject. Every
earlier passing case rerun in the 895-case panel stayed green. Two additional
audit checks were red. The native Test host reached an unbound runtime mapping
on both this build and the pinned dormant-frame build. The unknown-length
generator fixture's concrete cursor used its associated type's own name,
producing a type-member cycle. The fixture now names its cursor separately.

The next abnormal transition uses the actual exit kind. The explicit `await`
currently publishes separate panic and cancellation edges with the same generic
failure refinement. Dependency calls route cancellation through the panic edge.
Storage-effects replay, liveness and report retention therefore cannot select the
same failure prefix once panic and cancellation have different guards. Use sibling
outcomes from the admitted attempt and carry the exit kind through the row table,
binding, package interface and actual report handling. Opaque effects may conservatively
cover both kinds. A source-visible cancellation has no report payload, though
host cleanup must still own any retained incidents. Running-task observation
selects the already resolved body rows against its actual RunResult variant.
Keep the existing single graph, solver, ordinals, capture and retirement rules.
Reconsider if `ReportDependencies` becomes a substitute for the performed prefix,
or if a second graph or row numbering is needed. Confirm independent panic and
cancellation with catch, scoped cleanup, dormant disposal, running tasks and
both source and imported reports before replacing the combined envelope.

The typed change is in the current build. It selects normal and abnormal
prefixes before resolving their shared address expressions and captures failure
effects and report channels by the selected kind. Checked exits and package
interface revision 11 preserve that kind. The test control changes distinct
caller-owned cells on each failure channel and checks both direct and imported
dependency summaries. Opaque rows keep their common conservative envelope.
Running-task and lifecycle selection are still open. Proof replay still needs
to read the resolved operation and failure observations rather than the
structural effect map and the ordinary storage-preservation shortcut.

The first typed build `B289D735D8BCD9AB03680ACA478D00111F4D694D2A16110511E762537B4A33AC`
preserved both prepare and apply prefixes for a known cancellation. The new
source control still recorded a panic prefix with both writes. Its `try` of a
known `Cancelled` value had a reachable panic successor because both abnormal
edges used the same `Completed == false` refinement. The checked propagation
now owns all three selected variants. The checker selects actual tags and MIR
uses those same tag and payload identities. The previous panel was interrupted
by the custom-iterator cardinality fixture exhausting the query worker stack.
The unknown-count oracle now uses a runtime range, while the divergent-yield
oracle still uses a statically bounded source. Keep the distinct precise reason
for each rejection.

The propagation build `4C103F827E352B0DF10A7380939F7CB485A5A08D18F8AD88F11EC979B31C0734`
passes the distinct source and import failure-prefix control. The 895-case panel
records 869 passes and 26 failures, with no regression in a previously passing
case rerun. The runtime-range test found its expected cardinality diagnostic
alongside a duplicate source binding. Renaming the callable removes that other
diagnostic. The native synchronous finalizer emission failure also reproduces
on the pinned earlier executable before the typed change.

The copy trace shows the borrowed value requirement at the final call bound to
the same `&owner.tag` that was observed before a by-value transfer. Argument
materialization is planned at `CallInput`. The execution proof currently creates
the copy context during completion, then a bound conversion row can publish
the earlier input again. Materialize the checked copy in the captured input
before binding and applying conversion. The conversion consumes the copy's
actual data and its distinct storage observation. Remove the completion-time
copy pass when the first capture owns it. Verify copied scalar equality and
permitted matching against a live witness, alongside rejection of rebinding a
borrow, duplicate owners and retired authority. Check staged defaults, ordinary
and user conversions, and source/import captured outputs under the same rule.
Reconsider if the result needs a nominal type exception instead of its existing
checked copy operation and destination slot.

The next observation transition should carry the resolved failure observation
under the same mutation identity as the physical effect and report. The checker's
published ordinary `CheckedRefinements` already contains resolved normal
observations. `StorageFlow` publishes separate observations for ordered cleanup,
and also has a conditional map specifically for the Observation proof premise.
That conditional map must stay distinct from unconditional physical effects.
Publish typed call failures with the existing checked cleanup mutation records.
Replay must take the admitted state before applying that failure's exact frame.
No absent invalidation or storage-only path can manufacture a stable pointee.
After the authoritative route reaches replay, remove the source-summary normal
storage shortcut. Preserve empty resolved transitions explicitly so a harmless
call retains every current observation without iterating unrelated pointees.
Check source and imported borrowed postconditions, both catch-write timings,
the typed failure control, raw retirement, zero extent, and all deferred cases.

The pattern fixture traces an acknowledged call requirement into the checked
entry and then into `TrustedChecks::after_operation`. That consumer reports the
unproven requirement even though the same entry records explicit acknowledgement.
Keep the proof result available for memory initialization and other semantic
consumers. Suppress only the caller-obligation diagnostic using the admitted
entry. The pattern test must lower one call in every valid wrapper. Unwrapped
requirements and authority used after a trust boundary must still be checked.

The outcome-observation build `1D6F57FE4E60E1BF1CD287726E04614C87B951C7411AF6D9588858C5DEA4ECF8`
passed 937 of 972 tests. Both corrected fixtures pass and all earlier staged-copy
passes repeated in this run stayed green. Comparing the 35 failures against the
prior typed-failure executable found 29 matching failures and 6 tests that depend
on the old trust diagnostic. Those six fixtures now distinguish unacknowledged
proof requirements from real storage-domain errors and are pending rerun. The
typed prefix, two caught-write timings, and Windows environment storage controls
passed on the observation build. No native bootstrap acceptance was inferred.

The next connected deferred boundary is observation of an already-running task.
Today the suspension operation applies the body-normal envelope, and the
abnormal observation then appends an aggregate failure prefix. A cancellation or
panic result can therefore inherit a sibling's writes. Start owns the body-entry
captures and binds the effect addresses there. Retain each admitted normal row's
resolved effects alongside the typed abnormal prefixes. Observation may select
from those records using the actual RunResult tag and selected body row. It
cannot read a fresh value out of the original body inputs to resolve an address.
Keep externally interrupted observation distinct from a returned Cancelled tag.
Only the all-Completed chain owns the body value and its witness publication.
A returned Panicked tag retains its selected report, while Cancelled has no
source payload. Scheduling effects and cleanup still use the actual task's
captured state and existing obligations.

Apply the selected child effect prefix at completion, before publishing the
outcome value or its report. During suspension, retain effects that may have
occurred on a genuine outer-abnormal edge and preserve the existing liveness
dependencies. Reuse the bounded outcome solver at the existing completion
operation. Remove the early body-normal application and the later combined
abnormal overlay once both flow and replay consume the same observation record.
Widening preserves possible effects and only common guarantees. Reconsider if
task observation performs the start-time address binding again, introduces a
second scheduler, or grows per loop iteration. Verify source and imported join
and cancel with distinguishable completed, panic and cancellation writes, plus
captured pointer replacement before observation, an opaque task control, and
report/payload survival through catch and cleanup.

The first task-observation build `312483F9017BCFA29FBA8402E466012B61AD8C557844C9F72139027E118A7AE8`
ran 1,005 cases. All 937 earlier observation-build passes stayed green. The six
trust-boundary fixtures also passed after correction. Four separately exercised
async cases still fail and reproduce on the earlier executable. The new task
test reached a `Completed` row with its actual payload and both sibling writes.
Its effect prefix began at `Future.start()`. That invocation applies the child's
interference conservatively, but currently also puts the whole normal envelope
in the parent run's performed effects. Preserve the scheduled interference for
storage and liveness while leaving the task's actual write prefix with the task.
Before task creation succeeds, a startup failure may resolve abandoned captures
but has no performed body prefix. Keep these phases distinct when exporting
effects or replaying observations. The new exact result test remains the oracle.

The schedule-prefix executable
`D9E39AF44E158DB1C4B430C9D521B8D0DCD8DDB3A6DD97A8D78CC198CC5B355F`
passed the paired task result and write-prefix regression on all four source,
import, join and cancel paths. The expanded panel listed 1,144 tests and
reported 1,083 passes and 58 failures before a query worker overflowed its
stack. Three host and report cases had no completion receipt. Run those
individually on the checkpoint candidate so a process crash identifies its
case. Several of the completed failures had obsolete explicit acknowledgement
oracles. The audit repairs those cases and retains the valid source and
imported negatives and positives.

The next lifecycle contract spans entry, cleanup sequencing, and the enclosing
exit. `dispose_part` admits the actual receiver after the preceding cleanup step.
`lifecycle_effects`, `prepare_cleanup_references`, and `collect_lifecycle_reports`
currently select different views of that invocation. The normal rows are filtered
against its logical entry, the abnormal report is pulled from the whole summary,
and the physical fallback takes all effects if completion was unproved. Bind each
applicable effect and report row from that same receiver entry. A normal cleanup
step can succeed on an already-abnormal enclosing exit, and a cleanup panic can
replace cancellation. The run's exit kind therefore supplies neither the child's
admission nor its result. A failed cleanup step must run only its acquired suffix
and cannot establish its normal postcondition or republish its result.

The current CFG forks synthetic cleanup-required paths before scope resolution.
They share one failure branch for panic and cancellation, and all scopes on that
branch run with an abnormal disposition. Follow the ordered prefix through the
existing `StorageScope` schedule, materialize the performed first failure at its
own identity, and carry the current exit reason through the remaining cleanup.
The scheduler owns order and shielded finalization. The bound invocation owns
the reached receiver, its outcomes, and the memory snapshot. Reuse the ordinary
effect resolver, typed prefix and report structures, and checked mutation records.
Check source and imported normal/failing receiver effects, scoped result
continuation, panic superseding cancellation, and fallible finalization with an
already-retained report. A branch-specific reference may never be attached to
an unexecuted cleanup result. Reconsider if the design duplicates the cleanup
schedule or computes child failure feasibility from the enclosing outcome.

Keep the enclosing control outcome available as cleanup proceeds. A finalizer
that returns `Error` has completed normally and can append a retained incident.
It does not cancel the run or replace a primary panic. A panic before the
formerly pending result is published drops that result and preserves the
already-performed prefix. Later steps run shielded on their admitted receivers;
each sees the writes of earlier steps and owns its own completion or report.
`StorageMutation` identifies the actual cleanup invocation, but replay must
also select the reached outcome for that invocation. A widened within-outcome
state must retain the enclosing panic or cancellation reason. Verify the
explicit scoped exit before implicit cleanup, early break and return, and a
cleanup report that itself owns a pending frame.

The first lifecycle slice shares the admitted receiver and selected effect and
report rows across checking and physical publication. Output capture reads the
installed destination. The published snapshot, current proof value and replay
must refer to that same value. Unresolved parts stay unknown and possible-region
publications stay weak. The release test executable
`34AA066A5EEE88EE9785299B1BF1535C1080294B14079B5A6B93C93FD0A2364E`
passed the paired task regression, the async cleanup and callback tests, the
scoped result continuation, and Windows environment storage. The scoped
authority transfer and conditional cleanup failures reproduced. The last
failure in that panel used a zero-length negative prefix. The audit now uses a
nonzero prefix and requires the missing predicate at its call.

Seventeen of eighteen activation, host admission and report tests passed on
the same executable. The report-pool test and the independently run native
exit authority test overflowed a query worker's stack. The latter stopped
while checking the explicit scope without admission. Check the exact semantic
recursion on those paths after the checkpoint. A native archive build also
exposed a toolchain mismatch. The default Rust 1.99.0 LLVM 23 bitcode was
rejected by the pinned LLVM 22.1.8 tools. Use the installed Rust 1.98.1, which
also uses LLVM 22.1.8, for the native artifact builds. The ordered cleanup
failure and suffix still require the scheduler change above.

Use the fixed source and imported two-owner and nullable or union selection controls, their
fabricated and absent counterparts, address invalidation after real retirement, and
the scoped continuation and Windows environment controls. Check correlated writes and
borrowed reports across catch and cleanup in the same candidate. The historical two native
failures come after the refactor's acceptance.

## Captured scalar operations

Windows standard-library planning on the current executable fails while binding
callee result rows. The backtrace reaches `bind_entry_outputs`, recursive
`bind_captured_value`, and the storage-operation arity assertion. The older
executable in Cargo's intermediate build directory plans and emits successfully.
Use the current final Cargo artifact in `target/release` for comparison.

`captured_data` serializes the evaluated condition for returned values, mutated
parameters, and deferred arguments. `BoundOperator` is shared across arities.
Only the two-operand forms use the storage arithmetic operations, which are
also produced by initialized-prefix append and removal. The portable scalar
encoder already preserves unary identity and negation with one argument.
Type-checked source and decoded package contracts both substitute this term
through `bind_captured_value`. Unary arithmetic carries scalar data. It cannot
create a storage owner, replace the source descriptor, or bind an unknown
operand as a concrete value. An unencodable scalar expression stays unknown.

Restrict captured storage Add and Subtract to their actual binary shape. Reuse
portable scalar terms for unary signs, with no new operation, schema or
consumer fallback. Compare the exact unary and binary result rows, local and
imported forwarding, and the Windows standard-library plan. The existing raw
buffer prefix controls protect both storage arithmetic operations. Reconsider
if a reachable captured unary value needs an operand the portable term cannot
represent to establish a valid contract.

The repaired native candidate has SHA-256
`1500B24DCDB63BD6C4D7D168E5397771A3CBBBA298E5F36AE1F21E048030D788`.
Its platform build completed. It passed the point at which the previous
executable asserted, but still spent more than ten minutes in planning.
The run was stopped after observing 1,031 CPU seconds. Retain the pinned
binary and use a bounded query trace before another full native attempt.
The focused unary and binary contract regression passed in source and import
on test binary `8e5d830bfc6bcf579f253e257ce3a45e6bcca033c6f4eec1166ec270a7008861`.
All 46 MIR and all 38 lowering unit tests passed. The focused compiler panel
passed 38 of 46. The catch-success owner evidence failure is a valid oracle
for the pending publication and cleanup work.

The second focused compiler executable
`4ffa658e79e15f7fc81e8fda625c1e88e3ed0d0a5cbe39531b93d2fdbd142935`
passed 45 of 48 cases, including forwarding the arithmetic through source and
imported summaries. The other failures were the catch-success witness, the
generic const default fixture with an owned unconstrained `T`, and evidence on
a local atomic after a task that cannot reach it. The task fixture now uses an
immutable owner for the independent control and retains the actual atomic
write case.

Clean test executable
`a3aa789c2d90513adf02ba63f8a6653dfca083dac3141cfa9df06ca386569a68`
passed 46 of 48, including the owned inactive capture and both task cleanup
controls. It also passed the exact unary and ordered binary operands through
local and imported wrappers. The remaining default fixture passed after
removing the unused value parameter and keeping the explicit specialization.
A standalone run of that exact edited fixture exported the provider, imported
its implementation and evaluated the defaulted result to 14 and the explicit
result to 4. The valid catch-success witness case still fails on the first
matched success arm. The updated fixture and compiler failures remain distinct
in checkpoint verification.

The planning slowdown is part of verification. The profiled reference
`C4BB26AAD9C9BB5BB3A7F8DD16F79CFC13A3307D5A68DE345A3C93B4AB851479`
completed the same Windows build in 43.7 seconds, including 17.6 seconds of
planning and 21.9 seconds of emission. It completed 1,698 result-dependency
components in 60 aggregate seconds of same-thread evaluation time. The
current probe `A9BFE3286561B4AE40AC4D750D6FED60711AA996EF1E5032579BC3659427AC8F`
reached 1,113 components and spent 205 aggregate seconds in that query by
the 90-second planning cutoff. Both reached 24 active worker slots. The
current run nearly stopped publishing completed components between 40 and
70 seconds and opened another work wave around 80 seconds. The recursive
summary probe observed no fourth round.

The body trace `D993649F4C428B666C14E38E055C1A3E36EF6ED42B3F8F8D2E82E353488E9E47`
recorded 1,155 completed body evaluations. Storage flow accounted for 171
aggregate seconds. `std.process.platform_child_spawn` spent 34.27 seconds in
storage flow across two rounds. The wide UTF-16 string conversion spent 16.75
seconds, and `std.format.write_complex_bits` spent 13.00 seconds. Cleanup-edge
exclusion triggered the second round in both child spawn and complex format.
The next trace must distinguish fixed-point transfer, checked publication,
and nested query work within storage flow. Use those source bodies and an
independent synchronous control. The measured critical path is already much
longer than the reference plan, even before native emission.

The isolated reproduction uses the maintained project graph, the Windows
target and `bound_unit_at` on the actual `platform_child_spawn` body. A direct
`storage_flow` request took 45.209 seconds and produced zero diagnostics.
Reuse that source selection to test internal phase costs and proposed
changes before repeating full standard-library planning.

Both temporary probes have been removed from source. The pinned binaries and
the source map retain the evidence for the measured runs.

## Test audit

The two fixed identities ending in `deferred_completion::{source,imported}::same_site_fabrication`
retire in favor of the same source cases ending in `same_site_independent_second`.
The retained cases check the diagnostic kind and the failing expression. Count the retired
identities separately in replay. The opaque future test outside the fixed cohort now has
one valid return fixture and asserts both the suspension shape and the dependency contract.
The next build also retires `constants::generic_constant_type_members_can_call_generic_constant_helpers`.
Its equivalent `constants::generic_constant_type_members_export_forwarded_self_results`
checks the actual imported ordinary and nullable values.

The redundant `owner_transport::ownership_composition_preserves_trusted_result_transport`
retires in favor of `owner_transport::ownership_composition_accepts_complete_transport`.
The deleted variant added a trust boundary to a producer with only a
postcondition. The underlying native execution fixture remains in the suite.
The standalone trusted disjunction negative is also retired. Its source, premise,
and failure are the first row of `mixed_boolean_requirements_preserve_trusted_occurrences_and_ordinary_guards`.
The independent audit completed all 6,226 Rust test bodies, all 1,714 frozen
comparison identities, 254 standard-library tests, and 67 native execution
sources. Its scratch report records confirmed fixture defects, stronger
surviving checks, and valid oracles that expose compiler defects.

The frozen executable manifest includes 1,718 identities before the four
native or profiling replacements. The latest listing retains 1,709 after
those four and the five recorded equivalence retirements. Each removed
identity is accounted for. Per-fixture reconciliation and native execution
verification continue from the audit report after this checkpoint.

The independently retired phase profiler's exact reuse check now survives in
`lowering_results_are_computed_lazily_and_published_once` and passed on the
admitted-lifecycle executable. The three native product replacements
still require their own artifact and execution receipts.

## Tools, resources and final gate

The narrow panels are no longer the comparison baseline. Keep the exact frozen
manifest and the all-green receipt in comparison-d087dde8f.json. The prior task record
also records an all-green replay at 551d12bb1. The clean candidate 806fe36ca was
built and checked against the retained manifest before more source changes. After
isolating unfinished cases on the same binary, 1508 passed, 202 failed assertions,
one crashed with a query-worker stack overflow, and three timed out. The exact
results and binary identity are retained under comparison-806fe36ca in the audit
directory. On e7eb0f901, the same 1714 cases produced 1510 passes, 200 failures
and four timeouts after reruns. Two source/import lifetime cases recovered.
The merge updates diagnostic rendering and does not qualify as a fresh frozen
comparison run.

The largest observed cluster is 45 ownership compositions failing in both source
and import. The address controls include both unnecessary rejection of live
authority and incorrect acceptance after lifetime expiry. Five lowering controls
also fail. Four removed native identities remain separate from that count.

Use the fixed results to identify the first lost producer fact. Pair valid and
invalid ownership with borrowed descriptor mutation and lifetime retirement.
Trace actual checked output, selected guard, active cleanup and package/query
binding before changing their shared mechanism. The historical native diagnostics in
bootstrap-historical-diagnostics.log
locates earlier failures at `context(selected)` after the task-failure update and
at `commit_start` after `register_task`. Those records name where proof was
required. They do not establish that a specific allocation was incorrectly
retired. An actual pair of reached allocation and acquisition identities remains
necessary to make that determination.

The coupled probe traces generic selected ownership, raw-address support through
mutation and scope exit, and a scoped result lowering control from the verified
candidate. It records the emitted summary rows, their admitted operands, the
private result, the retained witness ledger, and the owners attached to the
published descriptors. The paired abnormal-exit fixture is saved outside the
tracked tree so it cannot change the fixed comparison. Immediate failure now carries
its admitted entry and captured effects. Scoped lifecycle and separate cancellation
outcomes still need migration.

The address trace shows the reached owner at admission and at the input
postcondition. A physical capture wrapper around a scalar observation makes
`descriptor_identity` read the enclosing observation's owner instead of its
copied descriptor. The normalized predicate instead reads the copied operand.
The ledger therefore records the clause with no matching address support, and
scope exit cannot revoke it. A physical capture must preserve the exact logical
descriptor identity already used by the predicate. Keep reference observations
as descriptor boundaries. Use the same selection for admission, explicit and
inferred publication, witness support binding and invalidation. Check live and
expired fields and mutations through source and import before accepting the
change, including the Windows environment storage boundary on the fixed suite.
The descriptor change restores the independent-field lifetime tests through
source and import. The mutation tests now reach their last method-return case
before they fail. Their direct call, returned free-function and mutable receiver
cases all pass. The borrowed-result control passes. The forwarder, conditional
record, imported address retirement and Windows environment failures remain.
The generic join trace records both actual tagged values at production. The
inactive branch carries no transfer. The active branch carries the complete
formal value into the same private slot. The emitted normal exit instead
contains a placeholder payload and a separate ownership choice. Admission
holds the source witness, but publication cannot attach it to that placeholder.
The source and imported consumers must receive the selected value and transfer
together.

The scoped lowering trace records a reached completion for both scoped enter
and exit. The storage collector discards the completion when it has no
observations. Lowering uses the presence of that completion to decide whether
code can follow the scope. Reachability belongs to the reached phase, including
when that phase publishes no values. Entry failure, panic and cancellation
instead carry their performed effects and reports and cannot publish the normal
result. Both checking and replay must use the phase that produced the record.

The Windows environment trace shows distinct actual false and true result
tuples at the callee exits, each with its own declared clauses. The caller
fails to prove an initialized read under the true selector. This is a separate
loss downstream of producer capture. A join change alone cannot qualify the
contract. Correlate the actual producer snapshot, phase, cleanup prefix and
admitted portable row through publication, and use all three traces to verify
the consumer paths.

Use the shared Cargo target from .cargo/config.toml, target/cargo. Before builds target/cargo/tooling measured232.83GiB
and the drive had99GiB free. Never clean shared outputs. Build checker and compilation together to retain feature reuse.
Release binaries under target/cargo/release/deps:
- bray_checker-05848d70c4b314f7.exe
- bray_compilation-4af58abd509cab18.exe
- bray_bound_tree-c8ccf88d46ce9e45.exe
- bray_symbols-04553e9aef447f44.exe
- bray_package_interface-4ceed9dd563cf950.exe
Use3 libtest threads. Builds and semantic tests have used approved require_escalated. Native LLVM/linker/executable work
must run outside sandbox. Observe the active build and test-audit agent before launching another.

The frozen comparison-baseline.json has 1718 identities. Four removed provider IDs remain open native replacements.
Keep the fixed 1714 remaining identities and counts. The saved comparison-e7eb0f901-run.py replays the pinned executable
and writes separate replay results. A fresh candidate requires a fresh binary and source fingerprint, including untracked files.
The final joint gate is actual Linux and Windows bootstrap checking and lowering on
x86-64 and AArch64, plus the fixed comparison and its recorded equivalent retirements
on the same candidate. The test harness has entry points for all four targets. Only
after every refactor boundary is done may remaining native failures be repaired.
BRA-695 execution and BRA-696 shutdown remain separate. No new issues.

Re-read AGENTS.md before the final mechanical audit. Audit every changed behavior and equivalent repo paths, ownership,
architecture, DRY, sizes, public API/docs, errors, numerical policy, clones, invariants and tests. Run style and format,
required behavioral checks, update draft PR, commit/push, complete fresh review, fix valid findings and make ready only
when full implementation, acceptance and review pass. Follow the work-on-bray-issues contract gate after compaction.

## Repository and review

Issue https://linear.app/bray-lang/issue/BRA-694. Draft PR https://github.com/lejmer/bray/pull/717.
origin fetch/push is git@github.com:lejmer/bray.git. Verified public repository, owner lejmer, authenticated GitHub and SSH
account lejmer, admin/push permission. Reuse that evidence unless destination or authentication changes.
The user explicitly authorized committing and pushing the test-audit checkpoint
to the existing PR branch. Resume the compiler refactor on that branch after
the push.
PR title/body still describe older native framing and must be rewritten for final scope. The retained review through
4f767fbb2 has no findings at review5922465574. Current code needs a fresh full PR review at finalization. The skill explicitly
requires a reviewer subagent for that stage. The source audit is complete and
its findings are being checked against the working tree.
