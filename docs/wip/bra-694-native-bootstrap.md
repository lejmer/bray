# BRA-694 working record

Track [BRA-694](https://linear.app/bray-lang/issue/BRA-694/complete-native-bootstrap-owner-and-registration-contract-checking)
in [draft PR 717](https://github.com/lejmer/bray/pull/717). Follow
`StoragePlan` and `StorageFlowState` through the
[architecture map](bra-694-architecture.md).

## Refactor contract

The existing CFG determines evaluation order and phase boundaries. The plan
owns typed storage accesses, uses, definitions, and cleanup shapes. Backward
liveness reads those structural facts. Forward flow supplies the actual
evaluated operands, addresses, access permissions, and observation transitions.
The checked flow publishes them by CFG operation and reached outcome. Use the
same publication for proof, summaries, and lowering. Lowering consumes the
decisions needed for code generation.

| Boundary | Producer and consumers | Contract |
| --- | --- | --- |
| Input evaluation | Checked access routes and the paired current state select the value. Admission, effects, ownership, and proofs read that capture. | Preserve the evaluated value through argument conversion, defaults, parent-to-child frame capture, later operand evaluation, and replacement of its former slot. A route that crosses a descriptor must read the descriptor it follows. |
| Admission | The incoming composite state supplies actual arguments after the runtime guard. Consumption, requirements, terminal access checks, and certification use this admission. | Check requirements and conflicts before the operation invalidates their subject. Distinguish checked initialization from other requirements. Acknowledgement permits execution at its declared boundary. Certification still requires evidence. |
| Effects | The selected outcome and checked route resolve against current memory or an admitted snapshot. Physical memory and guarantees consume the same transition. | Keep actual addresses and conservative footprints separate. An escape contributes reachability. A write changes contents. Descriptor replacement, atomic payload updates, consumption, and backing retirement each revoke their own support. A finite bound requires complete captured dependencies. |
| Private output | The reached row produces a private result, writes, and transferred duties. Cleanup and the normal completion read those values. | Keep the result available to cleanup. Publish only after required cleanup. A whole-value move transfers its carried information, including opaque generic storage. A former slot retains no usable ownership. |
| Normal completion | Publish the surviving result, actual current writes, references, witness carriers, and guarantees from the same rows. | Retain value identity and guard when the data or layout is abstract. Keep code-generation activity decisions tied to that publication. Always distinguish explicit absence from unknown contents. |
| Cleanup and failure | A reached action uses its receiver, actual activity, and retained prefix. Ordered continuations retain their failure, effects, reports, and next action. | One owner owes each duty on each feasible path. Only performed effects and writes survive a failure. A conditional or possible address can bound invalidation. It cannot establish an exact cleanup target. |
| Deferred execution | Formation captures actual arguments and moves duties into the frame. Activation retains the resolved rows and cleanup. Observation and cancellation consume the selected producer and its support. | Pointees can change before activation. Captured descriptors stay the same. Keep competing producers separate even when they use the same row number. An inactive frame cleans up its capture. A running frame also cleans up obligations already acquired. |
| Joins and summaries | Existing current-value trees and fixed points merge reached outcomes. Export serializes the callable-observable terms, effects, and duties together. Import remaps identities and uses the same instantiator. | Possible information grows conservatively. Guarantees must hold on every path covered by their guard. Keep allocation recency and stable loop identities. An opaque region or shared source definition cannot establish unique allocation or authority. |

Scope retirement uses the prepared storage identities and the actual descriptor,
backing, and borrow ancestry. Ending a descriptor slot can leave the referent
alive. Retiring the backing ends all dependent access. Live stored loans,
cleanup, and reports can retain support. A raw address captures address bits
and bounds without forming another language loan. Escape checks read the
carried support of the published value even when its address is opaque.

Each transition determines which observations survive. Preserve independently
live sibling contents, pointers, and owners. The witness ledger retains the
logical subject, owning path, reached address support, and any associated
allocation. A live shared view can sustain an already supported clause.
Only ownership can transfer its cleanup duty. Retire support at the same
reached write or lifetime boundary in public proof and admitted frames.

Keep root, product-static, and thread-static identities explicit. A possible
target region cannot turn into an exact address, initialized cell, live
allocation, or ownership. Keep structured diagnostics when the program lacks
evidence. Impossible internal states fail at the violated contract.

### Required removals

- Shared value bindings replace separate reference and stored-value forms.
  Guarded outcome rows share ownership, reports, effects, references, and
  witnesses. The interface must encode these connected rows.
- Resolve actual input captures, effect targets, assignment destinations, and
  cleanup receivers at their owning phase. Remove later operand and address
  lookup from effect binding, output binding, and certification. Separate
  formal route selection from path traversal of the captured descriptor.
- Reuse checked per-operation mutation and retirement records. Remove source
  occurrence indexes that independently classify phases, inferred survival
  from declaration purity alone, and repeated settled-block transfers.
- Compare production and test code separately. Retain additional code when it
  represents a required invariant across phases. Remove superseded branches
  and conversion layers without using an arbitrary growth ceiling.

## Verification and current trace

- Checkpoint `51c1942c7` passed all 1,709 fixed comparison cases, including
  source and imported returns, nested cleanup, generator reference lifetime,
  and the production channel endpoint check. The comparison used one verified
  release test binary.
- The current Windows x64 standard library builds in 49 seconds, with planning
  in 21 seconds. Its interface decodes completely after normal and failure
  stored-value rows normalize against their portable destinations.
- Checkpoint `51c1942c7` checked and lowered all 43 selected Windows x64
  bootstrap bodies with zero diagnostics. The later full archive reached two
  additional report consumers. The current source candidate checks and lowers
  those consumers, both append contracts, and the metadata body without a
  diagnostic. The full archive and other targets still need verification.
- Normal calls and lifecycle cleanup publish exact reference targets in
  `StorageCompletion`. Trusted completion maps the reached actual input path to
  its formal `StoragePlan` path and reads the target's current value directly.
- The replaced `References` and `StoredValue` models, independent phase index,
  `prepare_call_references`, and `normal_storage_preserved` are absent. Lifecycle
  preparation uses the shared call-storage instantiator. Proof and lowering
  consume the published per-operation storage transitions.
- The optimized Windows x64 reference took 24.3 s to plan, 23.0 s to emit,
  and 53.0 s overall for the standard library. Native providers use Rust
  1.98.1 because the pinned LLVM cannot link Rust 1.99 provider bitcode.
- Packaged probes must open the target-qualified bundle. The parent directory
  contains an older manifest.

The timed numeric trace uses a 24-worker budget and a single root demand.
One `logarithm` storage solve took 80 s. The instrumented candidate with
parallel outgoing dependencies checked and lowered `square_root` in 65 s.
Its three body solves accounted for 53 s. Most slow operations are lifecycle
exits with multiple states, including resumed suffixes. A 3.7 s scope exit
visited cleanup 100 times. Applying cleanup and joining its continuations
accounted for most of the time. Preserve selected row contents through
physical effects and private publication. Rows and cleanup cursors with
different obligations require distinct state.

Cleanup selection and exit publication read current absence from the move
ledger and physical bindings. For each scheduled part, check its actual
path after the preceding action's invalidation and committed writes. Replay
reads the checked part decision for the operation and resumption cursor.
Keep explicit absence distinct from unknown contents. Test a finalizer that
installs a child in an initially empty slot and the control where the slot
stays empty. The suffix must destroy only the installed child. The old
operation-wide absence snapshot cannot describe both transitions.

Conditional cleanup must carry the taken state through the finalizer's
receiver write, destructor admission, and failure capture. Join the skipped
state only at the whole-part boundary. Compare the same finalizer and
destructor with definite and conditional initialization. For represented
children, check that an unused nullable formal supplies its entry
discriminator and that the physical join retains the receiver write for
the active payload. For nested guards, each payload exists only under its
enclosing discriminator prefix. The skipped state can prove a nested
payload absent without proving either discriminator absent on its own.
Keep initialization separate. A failed initialization check says nothing
about an otherwise feasible payload. Verify nested receivers in logical
proof, physical effects, and lowering alongside the unknown-initialization
control. A reached finalizer's trusted postcondition must keep the actual
receiver support used by the next part. The direct entry assumption serves
a definite continuation. A conditional join needs the supported clause to
retain it under the selected guard. Keep the authority's lifetime and raw
address bounds distinct from the selector. Verify that an omitted guarantee
or a write to its subject leaves the destructor requirement unsatisfied.
The receiver's updated reference descriptor owns its witness entry. A
conditional join reads that entry by its exact identity and verifies the
clause against the selected predecessor.

A lifecycle receiver may already be the storage reached through its
implicit borrow. Effect binding starts there and follows the remaining
checked route. A stored descriptor inside the receiver still needs its
own read. Retain a nullable or union tag when both inputs identify the
same tag snapshot through a payload write. An opaque effect retains a tag
when its overlapping reachable writes all select only the payload. An
unresolved alias that may reach the whole slot invalidates the tag.

Nested dependency batches stalled the 43-body probe at the sixth body with
all 25 threads waiting. Outgoing summary requests are sequential inside a
component query. Any parallel component batch must contain only ready
components with published prerequisites. Keep recursive approximations
inside the owning component and preserve deterministic diagnostics.

Memoize preparation of the exact ordinary and authoritative snapshots that
each proof path uses. Keep the existing equality basis, ordinary scope, and
established trusted premises for call, lifecycle, and postcondition proofs.
Test both a live borrowed-field fact and the same admission after that fact
or its authority changes. Reuse the resulting proof only inside its body.

### Current storage contract

The task admission output's checked result route ends in owning backing.
When its captured descriptor cannot supply an address, the route still
excludes invocation-owned inline storage. Carry that region through result
staging, ordinary forwarding, and package summaries, including descriptors
stored inside another allocation. The region may overlap other external
storage and needs no logical slot. Only a reached address provides a
definite mutation target. Unresolved ordinary pointer reads retain their
conservative fallback.

Complete borrowed alternatives keep actual addresses in the returned-input
record. Read each projected descriptor from its current, type-matched
target. A common descriptor can supply its address. Divergent reads keep
their regions. A missing alternative leaves the read opaque. A staged result
uses its own type, values, and liveness through the entire effect route.
An admitted operand keeps its committed incarnation while a repeated call
stages another producer at the same slot. Direct forwarding, projected reads,
effects, and commit must resolve the corresponding incarnation. A reached
physical destination can hold the backing region without a logical slot.
A write with several possible locations must retain the prior contents at
each location alongside the new alternative.
Keep exposure and lifetime effects on the private result until publication.
Writes through an input can reach a previous allocation at the same producer
slot. Retain their possible contents on the historical allocation without
letting their typed updates affect the new allocation.
Imported historical allocations use the dependency-bound channel. A bounded
ordinary borrow still carries its lifetime relation and cannot escape its
source storage.

Generic cleanup uses complete carried lineage through all feasible transfers.
Incomplete caller-visible sources retain conservative effects. Typed entry
preparation seeds inline buffer element paths only when they read no
descriptor and cross no backing or inactive payload. Unknown recursively
selected effects stay unknown, so expanding a generic cleanup cannot add
another layer of its own typed disposal on each fixed-point pass.

### Current regression contract

| Transition | Authoritative source and consumers | Required result and comparison oracle |
| --- | --- | --- |
| Cleanup escape | The checked effect route resolves an admitted receiver or capture. Physical escape, proof invalidation, deferred cleanup, and summary publication consume its region. | A resolved escape exposes precisely its reached roots. An unresolved escape cannot become an empty region. `retained_result` started then dropped must retire authority. A disjoint local owner must remain valid when a precise capture proves it unreachable. |
| Stored pointer observation | The reached memory value tree owns the exact field contents and returned-reference bounds. Raw read, effect targets, invalidation, and borrow summary consume its address. | Read exact current pointer contents directly when every possible container target has a compatible exact field. Otherwise retain every feasible input or ambient origin without inventing a single address. The recursive nullable result has two input origins and one empty origin. Opaque interference and raw reads after moves must reject unsupported authority. |
| Nullable payload | The planned discriminant and reached mutation determine whether a payload exists. Physical value merging, ownership transfer, postcondition proof, and call admission consume that result. | A promised mutation of `owner` must carry its optional-owner evidence through direct extraction. No promise or extraction from `other` must fail. The same rule applies to deferred and inactive cleanup routes. |
| Repeated producer site | Each reached frame and joined current value owns its actual producer and witness identity. Ordinary join facts, carrier publication, frame completion, and import consume those identities. | The first task from the loop can carry `held`. The independent second task cannot inherit it, in source or imported form. An index bound can survive only while its joined value represents the reached iterations. The loop exit at `index == 2` must remain reachable. |
| Repeated allocation site | The physical exit owns current and historical allocation incarnations. Summary export, package encoding, import staging, forwarding, and raw-memory checks consume their portable identities. | Outputs that reach different incarnations use different allocation subjects. A historical output carries only a region bound. Initializing the fresh allocation cannot initialize the historical allocation. Source and imported forwarding must retain the same distinction. |

Result effects read the staged output tree. Input effects read admitted
captures. Keep the same rule across deferred activation and cleanup.
Preserved discriminants belong to the reached value. For a borrowed value,
the reached referent determines whether its tag survives. A preserved
inline tag can coexist with a changed payload.

A joined slot takes the predecessor value at that named place. An older
capture inside another value keeps its own identity. At a loop header only
evidence that survives the fixed point can establish an invariant. At a
physical join a definitely absent parent makes its unlisted children absent.
An unknown parent supplies no such information.

A readable raw pointer prefix identifies a place where a later descriptor
can be read. The later descriptor carries its own origin bounds. This is
required in `admit_source_task`, where an opaque arena read can outlive the
caller-local `Current` view. A sound bound retains every feasible input and
ambient source. The fixed comparison and the bootstrap inventory exercise
both requirements. Reconsider the design if the reached values and effects
cannot distinguish those controls without maintaining another value tree.

### Current review batch

`StorageCompletion` and the current value tree supply the post-call owner for
each trusted witness subject. Normal call and lifecycle publication attach the
clause to that owner. Scope retirement and owned-result transfer consume the
same identity. They must not infer the owner again from the retired receiver
or its entry snapshot.

Retirement receives the values that end and the complete retained value tree.
It revokes a clause when its owner, observed contents, address support, or
backing expires. A retired container does not revoke a clause whose direct
owner and observed contents both survive as a caller-owned referent or a moved
child. The rule applies to the public witness ledger and admitted frames. A
new owner with equal contents gains no evidence.

The positive oracles are an updated referent that outlives a temporary
mutable-reference holder and an updated child moved out of its parent. The
negative oracles mutate the observed contents, retain an unrelated referent,
or move only the observed marker instead of its complete owner. Source and
imported lifecycle behavior must agree. The existing focused contract tests
cover these cases. Reconsider the design if retirement cannot decide from its
retained owner and contents indexes without another value tree or route
lookup. The repair should remove the temporary completion probes and add no
new persistent representation.

Lifecycle completion publishes all direct current writes before refreshing
reference aliases from their targets. Integrated flow and published replay use
the same order. The resulting clause and its retained owner therefore observe
one current snapshot when scope retirement evaluates them.

Returned-witness checking captures possible summary separation before
consumption, then reconciles it with the actual checked access. A noncopying
complete value that owns its witness clears that potential warning. Extracting
only a component does not.

The channel registration contract spans source and imported compilation. The
source `register_waiter` call establishes three trusted conditions on the returned
registration: its marker, the shared lock marker, and the shared channel
marker. `send` and `receive` keep that registration live across suspension.
`complete` makes cleanup unconditional on the markers. On an active exit,
cleanup must read the same reached registration and preserve each condition
until its destructor. The proof must never borrow a condition from an
unrelated registration or retain one after its owner or observed marker
changes. The source and imported standard-library paths, completion
publication, retirement, suspension, and cleanup proof are consumers.
The positive oracles are the `send` and `receive` bodies. A deliberately
absent or invalidated marker must remain a negative oracle. Source-mode proof
retains channel validity on the original `shared`
expression while cleanup reads `registration.shared.instance` through a loop
join at its actual storage place. Both cleanup receiver readers agree. The
join combines owned and scalar views of the same shared referent after a
partial write, then reanchors its data on the registration's loop placeholder.
Keep their common data under the existing referent identity without claiming
ownership. Different referent owners and changed marker contents cannot use
that path. Reuse the reached values and witness ledger; add no parallel state
or fallback.

The fixed comparison also checks two reference boundaries. A copied generator
element retains its original reference dependency even when its cursor and
backing support an opaque physical region. Escape checking reads established
borrow choices or a closed possible region, while the existing dependency and
loan checks reject a reference to a dying local. A cleanup finalizer can publish a symbolic
reference descriptor whose current bound names one exact referent. Completion
resolves that bound from the reached physical state before attaching the
destructor's witness to its owner.

## Current full-product report contract

`pop_record_value` publishes `retained_report_value` on its complete tuple
element. Tuple extraction and assignment to the loop's current owner must
retain it through the next `dispose_value` admission. Mutation or an unknown
write to that owner must revoke it. The input requirement supplies the first
iteration. During storage-flow construction, the `pop_record_value` completion
establishes both the condition and witness. The later `remaining = tail`
assignment removes both on some paths before the loop backedge. The earlier
call to the C-facing `report_records_append` exposes the address of `remaining`.
An independent returned report pointer can then conservatively name that
exposed storage, so the assignment correctly revokes the witness under the
current memory model. `ReportRecords` is copyable and its internal append
already operates on values. Use that direct value operation in `consume_values`
and retain the pointer wrapper for external callers. The C wrapper must keep
both incoming `valid_write` and alignment claims through the pool mutation
until its output writes. Calling the value operation from that wrapper loses
both `valid_write` claims because its effect can alias either pointer. Keep
the pointer-bearing contract at the C boundary and perform the pool link in
each contract context. Share the pure chain-metadata update so the append
policy has one owner. The positive oracle checks both append bodies and the
consumer. A pointer alias that may be invalidated remains a negative oracle.

`take_transfer` publishes `native_report_callbacks` on its nested report.
`write_result` borrows that complete owner through `clear_destination`,
`write_tag`, and `panic_destination`, then replaces the nested report through
`take_report(&mut owner)` before writing it to the raw destination. The returned
report owns the callback witness and the remaining owner retains its marker
and empty report. The focused packaged check now clears this diagnostic.

Positive oracles are the six affected packaged bodies and the full archive.
Negative oracles are changed report contents, an alias that invalidates a C
output pointer, a fabricated callback witness, and a partial extraction with
no independent result owner. Reuse the fixed 1,709-case comparison after any
compiler semantic repair. This source repair adds no parallel report value or
fallback proof.

## Acceptance

1. Finish replacing old paths and search for their callers and equivalents.
   Check value identity, retirement, and guarded transport as complete paths.
   Include round-trip import and source behavior, defaults, and cleanup.
2. Run the fixed inventory of 1,709 cases against the candidate. Use the same
   audited test inventory and reuse only exact-binary evidence.
3. Check and lower the actual bootstrap owners, registration, and dispatch
   on Windows and Linux for both x86-64 and AArch64. Build the complete
   native runtime archive and link an executable against it. Run native
   toolchain steps outside the sandbox. BRA-695 and BRA-696 cover execution
   and shutdown.
4. Measure an optimized build on 24 cores without tracing. Planning should
   take 17–25 s. The full build must produce a finished binary within 60 s.
5. Remove temporary probes and stale artifacts. Complete the repository
   audit, mutating style check, formatting, and independent debug review.
   Push verified checkpoints to the existing draft PR.
