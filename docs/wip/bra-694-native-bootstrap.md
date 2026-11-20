# BRA-694 refactor working record

Track [BRA-694](https://linear.app/bray-lang/issue/BRA-694) in
[draft PR 717](https://github.com/lejmer/bray/pull/717). Follow `StoragePlan`
and `StorageFlowState` across the [compiler architecture map](bra-694-architecture.md).

## Refactor contract

Keep the existing CFG, storage plan, current-value tree, proof indexes, and
demand-driven summary solver. Backward liveness reads structural uses and
definitions. Forward analysis admits actual inputs and supplies the reached
effects, outcomes, and cleanup decisions to proof and lowering.

Replace separate value, borrow, ownership, and carrier outputs with guarded
outcomes that share value identities. Ordinary calls, lifecycle calls, and
deferred execution use the same contract for source and imported bodies.
Remove duplicate preparation, effect classification, and scheduling from
downstream consumers. A fast path must derive its answer from that contract.

Storage planning attaches typed reference routes to the accesses that own their
logical paths. Body preparation can index those routes by their observation
footprint. Forward flow reads the same route for reached accesses and borrows.
Each borrow read retains its checked mode for permission and escape checks.
Backward liveness uses the existing structural paths. Check entry borrows,
reference fields, slice backing, and recovered accesses.

Bound reference outputs retain their typed input reads and the route after
the final descriptor. Forwarding keeps the actual evaluated descriptor.
Forming an address follows the prepared route through current backing.
An output with only bounds contributes a possible reference region. A
bound cannot stand in for the stored descriptor at a later read. Check
forwarded nested loans, backing transfers, and source and imported results.

Prepare owning buffer descriptors for nested types reached by the checked
input routes. Admission seeds the owning backing there, and later reads
and moves keep the actual descriptor. Ordinary calls and cleanup bind
those existing values. They cannot establish an address by scanning
formal reference subjects before an operation reads them. An actual
source read may observe an opaque descriptor with its known dependencies.
Check the queue compaction and enqueue chain, raw and nested borrow
effects, and cleanup with both bounded and genuinely unknown targets.

Read evaluated values and reached state from their producer. A possible region
bounds dependencies and invalidation. Exact addresses, initialized contents,
live allocations, and authority each require their own evidence. Keep empty
nullable payloads and unknown payloads distinct. Keep private results available
to cleanup and publish them only after normal cleanup completes.

Normal publication keeps a witness's predicate subject and its owning path.
The portable subject names the same logical observation checked in the body.
The carrier follows the actual ownership lineage, including a present payload
that carries a claim on its nullable value. Absence removes the payload and can
leave a witness on the whole object. Check nested outputs, inactive alternatives,
and witnesses on the payload itself in source and imported summaries.

Review production and test changes separately. Remove superseded and duplicate
mechanisms. Keep the code needed to preserve the phase contracts and supported
behavior.

Scope cleanup takes its feasible failure kinds from the planned action and the
reached receiver. Native releases use the outcomes their generated code can
propagate. A buffer with element cleanup retains the element outcomes. A raw
backing release can propagate a panic. Guarded calls and deferred cleanup retain
their selected abnormal rows. Continuations carry their performed effects,
primary failure, and next action into proof and lowering.

The checked cleanup part supplies the relative typed route from its reached
owner. Use that route for a scope exit and for a replacement whose destination
was captured before the right-hand side ran. The path used to select a guard
cannot establish an address. If the route reaches several backing locations,
keep the action conditional and its mutation conservative. Check represented
parts, nested owner backing, replacement, and cleanup after a failure.

Finite effects bracket observations by physical origin. Storage flow can omit
unrelated direct accesses when every target is a local storage slot or an
allocation. Keep stored loans, capabilities, and unresolved routes in the
observation walk. Check disjoint direct slots, aliased loans, and static or
returned origins against the fixed comparison and native bodies.

Cleanup invalidates the union of its resolved regions. It updates each
affected cell once, keeps independent ownership duties, and retires exact
backing separately. The checked reference-slot framing applies to the
complete cleanup transition.

An operation with no prior evaluation on its reached path needs no retirement
of old evaluation snapshots. Use the CFG's existing repeatable-site table and
the cleanup plan's repeated element ranges. A failure retains its own edge and
row identity. Check source loops, array repetition, and normal and abnormal
exits of the same inline scope. Preserve physical retirement and proof
invalidation on every reached mutation.

An escape publishes reachability in physical memory. Only effects that can
write or retire storage invalidate current contents. Classify this distinction
when resolving the observation transition so call and cleanup proof read it
directly. Ambient and unknown effects still use escaped roots, and atomic
writes still invalidate protected observations. Check both mutations against
the fixed comparison and take a separate 24-core planning measurement.

Observation certification needs the confinement of the admitted mutable
inputs. Record the widest scope while storage flow resolves those targets,
and join scopes by taking the wider one. Distinct local targets can both be
confined. Missing, previous, or unbounded targets cannot certify confinement.
Remove the second invalidation frame built solely to read its scope. Keep
the performed effects for ordinary purity, abnormal execution, and cleanup.
Check candidate purity through local, input, static, and unknown targets.
Once a resolved attempt has finished updating memory, proof, and
refinements, move its observation record into the checked publication.
The record belongs to the exact CFG operation. Capture retirements identify
admission, execution, or the end of the operation. Forward flow applies each
retirement and publishes it for certification at the same boundary. Cleanup
and failure records already identify their exact action or edge. Remove the
parallel phase classification derived from source occurrences. Guarded
retirements and invalidations combine possible changes, while completion
supplies only guarded facts covered by the reached admission. The comparison
includes repeated await, cleanup, panicking calls, and private result
production. Loop certification reads the combined operation frame.

Resolve move and assignment targets from the checked access and incoming
storage. Use the reached targets for physical application, observation
invalidation, and portable effects. Moving a loan descriptor retires its own
slot. Possible locations bound the effect, and an unresolved target supplies
an opaque footprint. Backward liveness retains structural accesses. Forward
proof and certification observe a move through its checked location. An
unresolved address can invalidate facts without establishing a transfer from
any particular slot.
Borrow conflict checks use the reached subject and each active loan's regions.
Disjoint physical bounds and checked capability paths can each establish
separation. An actual shared location conflicts unless a valid reborrow
authorizes access. A wider physical footprint alone supplies no evidence
against separation of the checked paths. The same rule applies to exclusive
requirements on deferred inputs. Check exact aliasing, independent input
loans, descriptor moves, protected payloads, and dynamic slice projections.
The reached access retains the loan carried by its evaluated descriptor.
Assignment retains that access through RHS evaluation and cleanup. Borrow
formation stores the admitted ancestry with the new loan's address. A join
keeps the ancestors common to its reached paths.
The scoped capability keeps its admitted receiver support through the body
and exit. The flow trace shows the returned descriptor carrying the enter
loan after liveness has ended it. For implicit calls that forward their
checked input loan, liveness must carry that same dependency from the result
contract. Independent results and possible storage regions cannot authorize
an expired capability. Check returned and independent references, copied
descriptors, and conflicting mutation while the retained loan is live.

Normal publication binds contracts to the actual returned reference fields.
Storage flow records the exact target reached by each committed descriptor
in the completion. After a row installs its writes, proof reads those
targets from current storage before binding the row's guarantees. The
completion applies only to the selected result paths. Bounds and expired
targets cannot supply contents. Keep the returned descriptor and nested
loan layers. Use the same completion in forward flow and replay for direct,
scoped, and deferred calls. Check live and expired scoped borrows, forwarded
fields, changed referents, and source and imported alternatives in the fixed
comparison. Reconsider the design if completion needs a second reference
table or can substitute a target from another result row.

Preserve normal call effects with their checked analysis operation and
source outcome row. Proof replay already selects these rows before execution.
It must apply the matching reached effects before publishing the row's
result or writes. An unresolved or widened admission contributes its
conservative effects to any row it could cover. Preserve the aggregate for
consumers without a selected outcome. Check explicitly closed resources,
source and imported guarded writes, and the opposite result where the
resource remains open. Direct await and running-task outcomes must retain
the actual producer where their rows name different callees.

Execution certification checks requirements against each reached admission.
Read the solved output when an operation needs no additional observations.
Otherwise, replay from its actual reached input states. Keep entry arguments
and proof facts paired across calls and lifecycle actions, then require each
applicable dependency to verify. Cleanup can omit a finalizer only when all
its reached receivers establish completion. An indirect const call still
requires one callable body, and loop proofs still require their rank or
cursor on every relevant path. Check native dispatch, guarded purity,
finalization, indirect calls, and termination. Keep the fixed comparison
and planning time as acceptance checks.

Retention queries index the maximal live roots from `CurrentValues` and any
private results that the transition retains. The index follows actual owned
components, including frame captures. Loans preserve reached observations
without adding referent ownership. Avoid flattening the same value trees
before constructing this index. One invalidation shares the extracted-input
index across all affected roots. Branch states share current roots and
admitted calls until the transition changes them. Check partially moved
aggregates, frame captures, and the cleanup tests that require a complete
surviving carrier.

An exact typed write or proved copy read records its cleanup duty on the
selected cell. A write can invalidate facts at possible aliases while
retaining their existing duties.
Both direct and summarized retirement still inspect overlapping cells. Moving
the written cell discharges that duty without clearing a separate cell's duty.
Keep conservative duties for writes with unresolved addresses.

Value transfers read the evaluated slot planned for the operand. Replacement
cleanup can update the old destination and reachable support while the operand
remains in its separate slot. Completion installs that reached value before
retiring the slot. Check transfers across a loop, a distinct frame formed at
the same site, and extraction across an imported call.

The checked return route distinguishes borrowed publication from ownership
transfer. Borrow publication preserves the loan without recording ownership
for its referent. A later checked extraction can transfer the reached
referent. A genuinely selected empty ownership layout still suppresses an
unavailable transfer across cleanup and joins.

When an outcome returns a raw address already present in an admitted input,
bind its access clause to the captured address dependencies. A portable
carrier's formal input identifies the route through the callee. The route
can go through a receiver whose lifetime differs from the address it
contains. Use formal support for an address without an admitted bound.
Keep the carrier's independent owner dependencies. Verify source and
imported free and receiver calls, live and expired backing, writes that
clear the original slot, and rejection after scope retirement.

A pending frame keeps the trusted truth captured for its admitted carriers
while those same carriers keep live clauses. Moving or replacing an unrelated
value may remove public facts without ending this private admission.
Writes to observed referents, expired addresses, evaluation reuse, and dropped
carriers still revoke the affected evidence. The private facts can resume only
for the selected frame and its actual result. Check the source and imported
loop and first-frame cases, the rejected second-frame cases, the separate
referent-write and discarded-carrier cases, and the fixed comparison.

Staged copies and nullable lifts publish the converted evaluated operand.
The checked input selection supplies one inline route to both storage and
proof. A materialization applies no callee effects or return-time refresh.
Physical reference observations can still supply an exact reached value.
When the existing input carries only referent information, stage the actual
descriptor from the checked capture. Proof and storage share the capture.
An actual selected conversion or default invocation keeps its call transition.
Established pure completion keeps the admitted referent when there is no
separately tracked current slot. It still reads an available current view.
Check borrowed argument forwarding, nullable lifts and extraction, and the
source and imported default and conversion cases. Native dispatch must
check its existing worker lease against the same referent after staging.

Call attempts derive input reachability from the admitted descriptor or
complete captured aggregate when available. Escape analysis, observation
invalidation, and returned-reference bounds share those reached regions.
Incomplete captures use the prepared access and known dependency bounds.
Keep address regions separate from ownership and initialization.

Frame guards read the admitted argument snapshots. No execution transition
populates the separate copy-origin table. Remove that path while keeping
the actual argument facts and the frame's existing selector invalidation.
A join reads each predecessor's own selector when comparing its copied
argument. It retains a Boolean established on both predecessors. Preserve
uncertainty when either predecessor gives no answer, keep borrows live
observations, and preserve paired storage.
Check both destinations of the same-site Boolean and numeric guard cases,
including source and imported calls and ordered and NaN real inputs.

The same-site loop reaches await with the known producer and its output
rows. The loop's current exit facts contradict the historical condition
that allowed an earlier iteration to form the frame. Selection wrongly
suppresses the known producer. The presence of an opaque alternative can
also come from joining a carried frame with an unwrapped view of that
evaluation. The joined slot is built before the predecessor frame rewrite
and installed afterward. Trace the lifetime of the selectors and owning
arguments through that installation. Simplifying the guards globally
made the active numeric case pass incorrectly and discarded real
input evidence. Preserve the facts that select a retained value or explain
its copied input. Verify active extraction rejections, complete transfers,
actual opaque producers, both loop destinations, and source and imported
cases from the fixed comparison.

The storage footprint records inline reference fields as descriptor slots.
An effect on a sibling can invalidate the enclosing value while the checked
physical frame still preserves the descriptor and supports its separate
referent observations. A nested load through another loan or an owning
allocation uses its actual reached route. Replacing the descriptor or
retiring its backing must still revoke the corresponding evidence. Verify
normal and abnormal registration and scoped cleanup, sibling mutation,
reference replacement, and retained shared and mutable borrow tests.

Entry proof uses the stored type of a selected field to distinguish a loan
from the data it reaches. Reachable-type queries still use the referent
type. Current values, framing, and source and imported outcome binding keep
this loan role through a write to a sibling. A real write to the reference
slot or its referent still invalidates the affected observations. The
`commit_start` trace found an owned output wrapper on the retained loan
before the rollback checked its entry obligation.

Published input writes name the exact committed path. Storage flow owns this
record when it resolves a write into an incoming mutable borrow. Summaries
encode the reached value at that path, and the caller uses the prepared
route and its admitted descriptor to install it. Ordinary, deferred, and
failed calls resolve the typed write route in the reached physical state.
The prepared route can cross inline fields and stored mutable loans.
Lifecycle writes start with their actual reached receiver. Replay uses the
bound write destinations and admitted descriptor coordinates. Distinct
possible writes survive as regions and cannot establish one current value.
Effects still invalidate
unresolved writes and competing aliases. A sibling's abstract contents do
not overwrite the reached state when a different field is written. A
whole-value write covers its children. A join keeps writes covered by every
predecessor. Check forward and replay publication for ordinary calls,
deferred calls, and abnormal prefixes, including overlapped writes and
source and imported address transfers.

A checked field write can attach physical metadata to a value already
represented by the current root's projection. `CurrentValues` retains that
metadata for subsequent storage operations. A proof that observes the value
after publication must still recognize the same logical snapshot. The async
composition trace shows `coherent` proved after the deferred write, then
rejected when the published field adds a redundant projection to the whole
object. Canonicalize that exact projection in the existing value constructor.
Keep explicit changed values, active payloads, array shape, and physical
snapshots. The positive async transport and negative moved, fabricated, and
conflicting-borrow cases cover the transition. The fixed comparison checks
source and imported outcomes, witness retirement, and finalization. If
canonicalization drops any physical binding or distinct data, keep that
component explicit. This repair needs no additional phase or representation.

The proof hot path reads live subtrees of the same current roots throughout
cleanup. Cache the maximal live parts on each shared root. Its contents,
observed parts, and availability determine those entries against the immutable
body shape. A write, scope retirement, or change to availability invalidates
the affected root's cached parts. A fork keeps the previous root's snapshot.
The same root caches its live scalar copies for proof invalidation.
Pending results and typed pointees keep their existing ownership rules.
Reuse the same referent revocation query across assumptions, witnesses, and
pending admissions within one resolved transition. Keep address-slot identity
separate from pointee contents. A condition outside the reached referents
needs no preservation query. Check partially moved and replaced owners,
borrow revocation, guarded outcomes, and the fixed source/import comparison.
Reconsider the cache if the same-candidate timing shows little reuse.

Observation survival for a returned input reads the admitted value. An
extracted input uses its existing checked extraction record. Other inputs
use the checked transfer route from the evaluated argument, including any
conversion already performed. Record only preserved inline observations
from that capture and use the same selector during replay. Changing an
old source slot still invalidates its address and contents observations.
Check source and imported extraction, mutated and fabricated owners,
referent writes, and consumption on an abnormal path. A missing capture
gives no preservation evidence.

An await of a direct frame must replay the checked normal invalidation of
that frame and its admitted rows. Distinct producers at one await can use the
same portable row number. Retain the existing frame producer identity with
the checked operation and source rows. Capture the key before the attempt
retires or changes inputs. A widened entry still contributes to every covered
selection. A missing checked row uses the existing aggregate. Task observation
keeps its separate completion transition, and failure prefixes remain on their
actual edges. Verify source and imported direct awaits with different writes,
shared continuation cases, cancellation, and the fixed comparison. Reuse the
guarded frame storage and keep the aggregate for refinement consumers.

A checked read through a reference derives its current data from the reached
referent. If that referent already carries an explicit storage observation,
keep its context through projections and a subsequent returned loan. The
const-getter trace reaches the original marker through the correct physical
address, but a second read replaces its scalar context with the first call's
result handle. Use the already reached context in the portable value binder.
The read still carries only a scalar observation. Independent copies and
unknown referents keep their existing behavior. Check chained getters,
separately stored loans, mutable field writes, scope retirement, and source
and imported outcomes. A copied data value alone cannot establish a borrow
origin or transfer ownership.

## Contracts to verify

Keep reached access targets by exact CFG operation. A plan describes the type
and purpose of an access. The composite flow resolves its address and validates
the action. Certification reads that operation's checked target for assignment,
move and raw memory access. A target common to all operations of a source site
is a query over the same records for exported templates and whole-body checks.
A divergent or unknown reach must never establish one exact target. The logical
raw write uses the operand captured by the invocation and the checked target
used for the physical write. MIR continues to use the structural plan and its
checked activity decisions.
Within an operation, select the target using the same retained rows as the
invalidation, retirements, and publication. Where no guard distinguishes
the reaches, only their common target is usable.

Review ordinary moves, copied cleanup occurrences, indirect replacement
destinations and exact versus uncertain raw accesses together. Use the existing
disjoint borrow, overlapping borrow, scoped exit, replaced destination and raw
memory cases in the fixed comparison. Replace the source-only target table.
Reconsider the publication shape if it requires copying storage state or a
second target index for each operation.

Cleanup certifies the state entering the reached phase or resumed suffix.
The composite solver already owns the absence map, chosen order, and
replacement address at that boundary. Retain those inputs with the exact
operation and continuation. An absence discovered during cleanup cannot skip
a preceding action. Scope and assignment summaries for MIR retain only the
activity decisions needed for runtime guards. Check scoped panic and
cancellation, cleanup suffixes, inactive represented parts, and replacement
through an evaluated descriptor in the fixed comparison.

Abnormal writes bind their destinations once against the admitted invocation
or selected deferred producer. A checked failure completion retains those
bindings with the physical snapshots that survived the performed prefix.
Certification binds its logical values from that record and the admitted
arguments. It may narrow feasible rows under its assumptions. It cannot
re-open a portable contract and resolve new destinations or borrow evidence.
Buffer cleanup reads its already admitted outputs through the same completion
boundary. Check failure before admission, a committed write before panic,
multiple possible failure rows, and different deferred producers on one edge.
Source and imported admission use the same record. Reuse the existing bound
value rows and completion record.

Normal deferred output rows bind at activation against the actual captured
producer. Refresh live contents through its retained descriptor. Keep its
address and ownership unchanged. The activation publishes its resolved rows
by CFG operation and producer alongside ordinary call rows. Certification
reads those rows and binds logical terms in the actual frame entry. It must
not consult another producer's entry, invent an outcome, or resolve write
destinations again. Check direct await, start and subsequent observation,
imported frames, repeated formation at one site, and unknown alternatives.
Extend the existing admission table without retaining a second copy of the
frame or its flow state.

Rows joined at one operation can bind the same formal write to different
addresses. Retain the admitted input addresses behind an exact write with
the row. Certification compares the evaluated descriptor it holds with
that record. A known mismatch excludes that reached row. A missing
descriptor keeps its possible result and effects without asserting a
current write. Validate both a surviving evaluated descriptor and a
different or unknown one after a join. This requires no route traversal.

Complete a local deferred body from its selected frame entry directly.
Formation identifies its callable and formal contract. Activation and
observation identify whose actual outputs, references, and retired owners
are published. A finishing frame cannot overwrite a different admission
stored at the same formation site. Keep ordinary call completion on its
own admitted record. Check both local and imported bodies and two live
frames formed at one occurrence.

Use a frame's admission facts for proof inside its selected completion.
Carry only the checked result and supported completion facts into caller
state. Row selection, witness separation, ownership layouts, and restored
witnesses need the same admission context. A saved ordinary guard cannot
assert a fact about a different evaluation in the caller. Reuse the shared
fact sets when the current proof already contains the admission facts.
Completion borrows the admitted argument map until a reached view changes.
When a guarantee depends on a borrowed input, admission retains local
lifetime support from the checked dependency and liveness data. Deferred
execution keeps that same admission. Completion uses the retained support
and the reached result, including when an imported witness binds its formal
support to a captured borrow. Remove the completion table that derived
support from the original expression or its current slot. Check owned,
entry-borrowed, and indexed inputs, defaults, and deferred observations.
Logical completion uses reference relations from the same selected row
as its returned value, carriers, and ownership. A widened row supplies
the merged relations it covers. Check a returned borrow or raw address
whose supporting input differs across outcomes.

At joins and witness refinement, update the actual frame where the value
tree retains it. Share each frame's update across all its holders. Apply
outer frame changes after nested changes so both survive. A join reads
the predecessor's current scalar values before installing any joined
values. Check nested frames with multiple holders and distinct frames
formed at the same site.

While staging a call's selected normal rows, record the exact reached
referents of fields that those rows return unchanged. Invalidation uses
the same resolved references when recording updated referent contents.
Abnormal prefixes use their own reached effects and carry no normal
reference observation. Check a returned reference with a mutated
pointee, a replaced reference slot, and a failure before a promised
reference is restored.

Inactive cleanup binds the capture contract against the held frame at
cleanup admission. Activation retains the resolved capture and result
cleanup effects for running-task outcomes. Running cleanup reads that
retained transition, since rebinding from later memory could identify
another object. A missing activated transition needs an opaque bound.
Check abandoned futures, failed scheduling, and a task whose captured
pointer survives a change to the slot that supplied it.

Apply the retirement list from a checked transition to each retained value
in one traversal. Keep the transition's ordering and reuse snapshots with
no affected descriptors. Clone an affected snapshot at most once and hash
its final state once. Use the same operation for current values, pending
writes, and lifecycle receiver writes. Check a batch with unrelated
descriptors, multiple affected backing lifetimes, and nested frames.

Witness copy and separation checks read the same reached accesses used by
the composite transition. Input transfers establish their addresses before
cleanup or mutation. Output transfers establish theirs when the producing
operation installs the output. The witness check reads the actual logical
value at the appropriate phase. It cannot resolve an address again after
effects have changed the descriptor. Review complete moves, partial
transfers, returned borrows, and cleanup of a displaced owner together.

Default providers publish their tracked scalar data in the same guarded value
rows as ordinary calls. Input completion binds those rows against the captured
earlier arguments and the checked generic and contextual substitutions.
Remove the separate default scalar summary and its package encoding. The
normal output supplies witnesses and ownership independently of scalar data.
Check positional and field defaults, nullable and union results, generic
defaults and import parity. Failure cannot publish a default argument.

Invocation effects and completion observe the admitted reference descriptor.
Follow its checked route into current memory for a nested referent. A moved
input needs the same record as a live one. A retired descriptor supplies no
new exact observation. Result and static routes read their actual storage.
Remove the separate input refresh that searches argument slots on completion.
Record surviving observations of consumed inputs by the admitted input
ordinal and its relative path. Proof reads that captured argument after the
slot loses authority. Keep borrowed and unresolved operand locations out of
this record. Check staged moves, inline inputs, memory reads that transfer a
payload, and ownership that survives only in a selected result.
Check a later argument that changes the original slot, a callee that replaces
a nested borrow, and a normal or abnormal lifetime retirement.

Normal publication belongs to the exact operation that completed, with its
reached producer and rows where those are available. Certification takes
that publication before reading result contents and installed references.
Lowering needs completion reachability by source site. Clause normalization
can use only references agreed by all reached completions of its source site.
Both queries use the same published records. Cleanup and abnormal outcomes
retain their existing action or failure identities.

A normal outcome join keeps the result and reached current writes under the
same guards. Each changed current slot has an identity at that publication.
Trusted clauses carry authority from each reached row. Ordinary postconditions
bind to the joined result and current contents using the admitted input
addresses. Check success and error rows, deferred completion, and the source
and imported counterparts.

Assignment evaluates its destination before the right-hand side. The checked
assignment must keep that address through right-hand side evaluation, through
replacement cleanup, and until installation. Storage flow captures an
indirect destination at expression completion, retires the address with its
reached lifetime, and uses it for conflict checks and the write. Replacement
cleanup projects its receiver from the same retained address. MIR
materializes dereferences and the compound left operand before the right
operand executes. A descriptor replacement cannot redirect the write.
Ending addressed backing still ends access.

Select old-value cleanup from the reached destination's availability after
the right-hand side. Publish the same decision for proof and MIR. Express
moved parts in the prepared cleanup table's coordinates, and keep observed
absence on the actual path used by proof. An unresolved destination keeps
cleanup conditional.

A selected compound operator has a private result of the operator's output
type. The public assignment expression produces unit. The call's abnormal
edge never installs the new value. Normal completion keeps the private result
alive through replacement cleanup, then transfers it to the admitted
destination. Represent that installation separately in the existing CFG.
Liveness ends the operator's input loans at call completion and retains the
private result to installation. Physical and logical publication use the
operator's actual bound result. Check built-in assignment, trait operators,
asynchronous replacement cleanup, and the operator and replacement failure
edges.

- Verify exact cell writes and consumption across guarded outcomes, including a
  separate helper that transfers a duty before a later retirement. An earlier
  retirement must see the duty when consumption is conditional or comes later.
- Check array cleanup failure before and after the current element. MIR retains
  the live cursor and the current element's remaining parts. Small known arrays
  have distinct checked actions per element. Verify the joined states for
  wider and generic arrays and the effect order after a failed element.
- Check failure publication across source and import. Distinct failure rows
  retain their own reports, performed effects, and nullable outcomes.
  Include fallible replacements that restore the destination before propagation.
- Check moved, nested, and imported frame captures against the backing retained
  by the actual owner. Require definite activity for authorizing loans and check
  all possible conflicts.
- Carry the actual witness support with the admitted owned value across
  formation, moves, running tasks, and completion. The public ledger supplies
  active facts. The entry captures the existing witness, its reached address
  support, and its allocation relation with the actual owning argument.
  Frame formation and nesting transfer that record directly. Refinement and
  completion read it through the selected frame. Retire clauses at the same
  writes, storage ends, scope exits, and abnormal outcomes as public support.
  Remove the lookup of a private witness in the public ledger by evaluation
  identity. Check partial extraction against the bound result row and the
  witness carried in its admitted entry. Keep conservative evaluation
  retirement while earlier and current owners share an evaluation identity.
  Before retaining earlier evidence across another evaluation, establish
  distinct identities for the held owner and the new value. Check independent
  frames formed at one site, payload guards, and source and imported calls.
- Trace trusted authority through admitted scoped exits. Keep protected cleanup
  tied to the actual backing effects and retire witnesses with their support.
  The last native comparison passed `prepare_start` on all four targets.
  `dispatch_ready`, `start_frame`, and `acquire_worker_ready` still fail.
  Trace the retained descriptor through registration and release, including
  the acquisition and status observations across `wait_execution` and
  `dispatch_task`.
- Check observation survival using the resolved normal effects and typed cell
  changes. Established pure admission can preserve inputs on normal completion.
  An unproved admission or an abnormal prefix still needs its reached effects.
  The Windows channel trace reaches a bounded escape of the generation field
  at await. Storage flow records the lock marker on its sibling field as
  surviving. The clause's marker observation survives, but the witness
  filter only accepts owning roots and drops carriers reached by live
  references. Retain the established clause when its carrier still has a
  live view and the checked invalidation keeps every required observation.
  Ownership transfers still require an owning path. Check shared-view
  survival, writes to the actual observed field, owner retirement, and
  rejection through a stale or unrelated view. Check the process
  completion contracts through the selected result, reached writes, and exit
  facts. A wrong owner or an error outcome must supply no success evidence.
- Select lifecycle outcome rows from the admitted receiver. Keep each row's
  effects, cell changes, reference outputs, and report together until both
  storage and proof have applied it. Join subsequent states at the action
  boundary. Check a definite row, ambiguous rows, and an abnormal suffix.
- Check `RawBuffer<T>` through generic stores, moves, loops, deferred captures,
  element extraction, and abandonment. Source and imported paths must agree on
  which dependencies transfer and which escapes fail.

## Verification and completion

1. Run the repository style and format commands and follow the end-to-end debug
   skill after replacing the old paths.
2. Compare the fixed inventory of 1,709 cases on the accepted candidate.
   Account for the test audit changes through `3afac09c5` and verify the
   native replacements. The comparison at `8d3f4a2c5` had 1,660 passes and
   49 failures, with every outcome matching the preceding frame capture build.
   Repeat the comparison after the remaining refactor and bootstrap repairs.
3. After the refactor, check the owner entry, dispatch, and registration paths,
   including `dispatch_ready` and `start_frame`. Trace the allocation reached
   through the task slot and the lifetime that supports the acquired `Current`
   view.
4. Verify source and imported contracts and native bootstrap checking and
   lowering on Windows and Linux for x86-64 and AArch64 on the same candidate.
   Run native workflows outside the sandbox.
5. Measure an optimized build on 24 cores. Acceptance requires planning in the
   prior 17–25 second range and a finished binary within 60 seconds. The latest
   profiled Windows standard-library build took 34.7 seconds to plan and 35.1
   seconds to emit. Track the result dependency solve and export proof scan
   alongside native reachability and emission.
6. Complete the repository audit and independent review. Push to the existing
   draft PR. BRA-695 and BRA-696 cover native execution and shutdown.
