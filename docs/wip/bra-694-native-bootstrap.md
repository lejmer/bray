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

## Verified checkpoint

Pushed checkpoint `b02202c62` checks and lowers all 25 original bootstrap
bodies and four direct task dispatcher helpers on Windows and Linux, for both
x64 and AArch64. The checked acquisition follows its owner across the waiter,
dispatch, retirement, and start transitions.

Checkpoint `ce33eb970` passed all 1,709 fixed cases, 578 checker tests,
and 139 bound-tree tests. It checked and lowered all 32 selected bootstrap
bodies on Windows and Linux for x64 and AArch64. The full runtime archive
and executable link remain open.

The optimized 24-worker Windows x64 standard-library build planned in
28.7 s and emitted in 26.9 s. Runtime preparation compiled the same
standard-library package in 62.4 s with platform providers available.
Executable link timing remains open. The same 1,698 result-dependency
component evaluations took 151.8 worker seconds, up from 60.0 in the
reference profile. A sample of 202 expensive body checks repeated one
unit. A sample of 3,354 cleanup actions spent 2.5 s on proof invalidation
and 0.06 s on state copies, with most time in referent checks and framing
changed roots.

The Windows x64 runtime archive had produced no archive after 23 minutes.
Resumed cleanup accounted for about two-thirds of a sample of active threads.
One runtime scope exit resumed 2,047 times at 11 distinct cursors for a
single incoming state, taking 56 s. Standard-library operations also revisit
the same cursors in storage flow and certification.

Join pending continuations at the same admitted action before visiting the
remaining suffix. Use the existing flow joins and visit cursors in schedule
order. The cursor retains the ordered `StoragePlan` accesses, current part,
lifecycle slot, primary failure, and enclosing continuation. Separate source
paths, replacement storage, panic and cancellation, and distinct schedules
keep their own frames. The joined `StorageFlowState` carries all possible
effects and reports and only supported definite observations. Certification
uses the same worklist with its current proof state. An abstract array still
iterates to convergence. A failed destructor has already attempted its slot.
Resumption can still reach the next array iteration.

Check represented-child guarantees, exact receiver writes, conditional
cleanup rejection, replacement failure, pending-result abandonment, and
retained report dependencies against the same fixed inventory. Use the full
native archive and standard-library timing to verify that duplicate suffix
work decreases. If merging at an equal cursor loses a guard needed later,
retain that guarded input under the cursor rather than joining other
continuations or treating a possible address as exact.

The shared-worklist compiler planned the standard library in 27.0 s and
emitted it in 24.3 s. The warm-provider package took 59.65 s wall time.
The result-dependency component still accounts for 151.3 worker seconds.
The current runtime sample spends 7.4 of 18.1 sampled worker seconds in
state joins and 1.7 s scanning conditions for retired evaluations.

The shared-worklist runtime archive still had no output when interrupted at
25 minutes. The body trace separates two costs. Complex division visits each
block once, with one scope exit taking 85.6 s. In `arctangent`, the same loop
keeps 37 work items pending and repeats its hottest block over 280 times.
Compile the source body against the archive's standard-library bundle and
query its storage flow directly. A certificate query can complete without
checking the body when it declares no certified properties. The package-backed
trace shows that successive header states differ only in `owned_results`
after the first few visits. Current values, memory, and proof assumptions
already agree. One key alternates between absent and an empty layout with
three guards. Another empty layout accumulated 40 guards. A layout with no
transfers has one canonical form in the retained ledger. A joined slot that
keeps the same value and layout keeps the original record through renaming.
An explicit no-transfer mapping still suppresses inferred input lineage.
A possible empty arm still constrains a mixed layout with a real transfer.
Guarded input moves, exact transfer origin and preservation masks still follow
the reached snapshot through framing, return publication, and imported calls.
The change removes redundant guard work and reuses an agreed record directly.
If a key still alternates, identify the reached reader of that key before
changing its lifetime. The body and archive are the convergence oracles.
Use loop-carried references, conditional guarantees, and repeated cleanup
to check whether the repair keeps or discards the wrong information.
The instrumented candidate checked `arctangent` without diagnostics. The
storage solver visited 1,245 blocks counting repeats, with at most four
visits to any block. It took 49.9 s. The package-backed query took 124.7 s
with its dependencies. Rerun the clean compiler before measuring throughput.

The probe-free loop repair planned in 25.4 s and emitted in 23.6 s.
Result-dependency components still consumed 152.3 worker seconds.

Dependency queries can visit a shared condition node once. Observation
iteration preserves separate handle slots. The checker suite exposed that
distinction in the first clean candidate. A join uses its checked selection
guard when available. Only joins without that guard need
to derive alternatives from both sets of ordinary premises. Reuse each
joined predicate observation across predecessor lookups. The added retirement
memo did not improve planning in paired runs. Framing and retirement can
share the mapping of checked preservation paths into retained current roots.
Join assembly can defer shared content identities until a changed output
actually needs the identity of a common owner. The index must still read
both original predecessors and choose the same owner for its aliases.
Preserve the physical path for storage that needs its type-directed read.
Definite absence still suppresses a change. An unknown component of an
existing root is still a possible write target. There is no additional cache
or retained table. If a reuse would require combining outcomes or replacing
a reached address with a bound, keep those reads separate. Verify saved
referents, copied values, represented cleanup, and deferred captures with the
same native and fixed-inventory checks.

For an observed effect, only the roots in the reached changed-place set are
framed. Build masks for those roots. Proof retirement still reads the full
set of surviving observations because an independent alias can sustain a
fact. An opaque effect frames every potentially initialized root and uses
the complete mask.
Compare represented sibling fields, raw address descriptors, cleanup and
copied referents. The native sample after the loop repair still spends most
active time applying and merging cleanup continuations.

At completed lexical and iteration cleanup, the storage plan names the
retiring owners and storage flow ends their physical lifetime. The proof
state can discard whole private locals and evaluated slots at that boundary.
This includes staged call inputs and implicit call results whose scope ended.
Keep terminal formal data for guarantees and committed writes. Keep partial
retirement, abandonment, and replacement records where another reached path
can still use that slot. A surviving transfer reads its admitted, pending, or
published value. It cannot recover a value from an expired private slot.
Read the expiring values and external loans before removing the roots. The
existing witness retirement then uses the current retained tree and those
actual snapshots. Compare normal and failing scope exits, scoped mutable
postconditions, copied scalars, result moves, and source/imported witnesses.
If a downstream reader needs a removed private slot, trace its owned result
or capture before changing the lifetime rule.

With private-root expiration, the optimized 24-worker standard-library build
planned in 23.7 s and emitted in 22.5 s. The package took 54.4 s wall time.
Runtime preparation measured 23.3 s planning and 22.1 s emission. The archive
run still spent most sampled compiler time in resumed cleanup and state
joins. It was interrupted before the bundle published. Use the package-backed
storage solver trace to distinguish repeated visits from expensive individual
cleanup operations before changing either mechanism.

The package trace shows expensive acyclic cleanup. Complex division still
visits each block once while some individual scope exits take over 20 s.
A reached cleanup state held 216 roots, 148 absent. The initial proof state
materializes absent slots even for blocks that never run. Deleting an expired
root on one branch then joining with such a bypass can reintroduce the dead
slot as unknown, making opaque effects frame it.

Keep the checked plan's initially absent root identities once in prepared
analysis. Proof states store materialized values and availability changes.
A missing planned root means absent, including on a bypass or after completed
lexical cleanup. Unknown unplanned observations, such as a pointee without
a published snapshot, still mean unknown. A join with one live predecessor
still derives conditional availability from both reached states. A new
definition can install that slot again. Keep retained formal data, live or
partially retired children, and required pending values. Check a lexical exit
and bypass join in each predecessor order, then check redefinition and
conditional initialization. Compare the runtime's retained root counts and
the same fixed inventory. Stop and trace the reader if any phase needs the
contents of an unmaterialized root.

The same default applies when call setup vacates an entire planned slot
before publishing a staged argument or result. If that call fails, cleanup
reads exact absence directly. Whole replacement can discard an empty
private record. A projected replacement or an unplanned pointee still
needs its reached availability. Preserve retained component observations.
Exercise actual entry and lexical retirement before the bypass join, and
check vacancy both before a first publication and after reusing a slot.

## Current verification batch

The fixed inventory passed on `ce33eb970`. The activation probes in its
slowest group fork the production fixture once per caller variant. Load each
test's independent caller sources in one revision so their bound units use
the same runtime and standard-library facts. Keep each source separate for
its diagnostic offset and keep each function independently named. Check all
cases for the same acceptance, rejection kind, predicate, and call site.
Compare the same activation inventory and compiler binary configuration before
changing the group further.

Checkpoint `96fe2d669` shares fixture revisions for the migrated activation
groups. The validation candidate passed all 1,709 fixed cases, including
40 activation controls, and all four selected native bootstrap checks.
The next batch shares one revision for the remaining activation variants,
including the second-invocation rejection. It retains the diagnostic
subjects, call sites, and successful lowering checks. The same 40 controls,
579 checker tests, 139 bound-tree tests, 1,709 fixed cases, and all four
selected native bootstrap checks passed with sparse planned absence.

An isolated 24-worker standard-library build planned in 29.5 s and emitted
in 28.8 s. Building platform providers took 16.2 s, and the tool invocation
took 80.9 s wall time. Planning still exceeds the 17–25 s target. The
runtime archive and executable link remain open. The concurrent verification
runs establish correctness. Measure each workload in isolation.

The physical transition for a call binds the formal route to the actual
admitted operand before traversing its descriptor. Staged conversions
already hold their formal wrapper. Unstaged present conversions remove the
formal wrapper's path prefix. All three consumers keep reached cells,
explicit absence, and possible regions distinct. Verify this across
reference outputs, effects, failure rows, and deferred frames.

Join assembly publishes each completed value into its retained root before
mapping the surviving witnesses to its predicate observation. Present and
absent predecessors, moved owners, independent views, and frame variants
keep their own guarded alternatives. Stable observation lookup reads
matching live entries without copying the predecessor tree.

The referent preservation query reads actual retained snapshots. Exact
known roots can answer directly. Membership and overlap build a path index
on demand and refresh it after insertion. The check must distinguish
retention of a path, equal contents, and ownership of a referent. The
comparison retains the same optimized standard-library inputs and the
same fixed test inventory.

An opaque normal effect bound comes from an applicable callable contract
and the reached input checks. Apply that bound during effect row selection
and publish any purity premise with the operation that relied on it.
Certification discharges the recorded premise at the admitted call.
Checked rows retain explicit effects. The conservative envelope,
abnormal prefixes, and transfer of owned inputs keep their own semantics.
Verify guarded and opaque calls alongside checked source and imported
effects, including paths that bypass a requirement or replace a descriptor.

The formation contract for a recognized storage type is a checked admission
even when its literal invokes no callable. It must check the actual field
operands before installing the owner. Cleanup and subsequent callers must
observe ownership only if the formation proves it. The construction oracle
checks a real ownership transfer, forged null fields, and copied owner fields.

Bind entry clauses once against the value tree initialized from the plan and
the flow's installed inline descriptor observations.
Admission, cleanup, and certification must observe the same entry facts.
Use the formal path to select each actual witness carrier in that tree.
Its ownership role determines whether it can transfer authority. Formal
auto-dereferenced storage holds the
reached observation. Admission uses the typed pointee and strips inherited
access layers without unwrapping a stored nested descriptor. Sequence
extent uses the same reference conversion as a selected sequence call.
Preserve the bound facts only while their original subjects survive. Verify
the protected-write, copied data, returned loan, and loop controls alongside
the host lifecycle cases. A write to an element can retain the descriptor.
Selecting a different descriptor loses its prior bounds.

When a checked result row produces a borrow whose portable owner is opaque,
use the retained producer's logical identity. Preserve a row's known
identity. Exact current storage supplies the established owner when the
provisional producer names the same reached object. Physical address,
reachability, and lifetime remain the completed storage transition's
responsibility. The opaque row must never create an
exact address, transfer ownership, or revive evidence from another
evaluation. Check the two ABI callers and the existing positive and
negative controls for returned references, stale predicates, and exposed
storage. Reuse the current value tree and the existing producer
substitution. Another observation table or a special rule at any caller
would require reconsidering the design.

Scope retirement uses the actual input loans published by storage flow.
Retain the final reached observation of a still-caller-owned referent when
its descriptor goes out of scope. A replacement can refer to a different
object and cannot carry the original subject's witness. Remove the
lifecycle plan's inferred surviving-input list. Check that a witness
attached to an external loan survives without transferring ownership from
its expired container. Verify independent external referents, descriptor
replacement, and destruction of embedded owned backing.

The bootstrap dispatch claim stays owned by the caller until explicit
release. Wait-link access preserves the original host lease and slot
address. The event marker represents a count already acquired by the same
complete reference. Start commits the admitted token, selected registration,
and actual metadata. Publication transfers rollback responsibility to the
cell before wake. The runtime packager and linker still need verification
with the resulting compiler.

## Acceptance

1. Finish replacing old paths and search for their callers and equivalents.
   Check value identity, retirement, and guarded transport as complete paths.
   Include round-trip import and source behavior, defaults, and cleanup.
2. Run the fixed inventory of 1,709 cases against the candidate. Use the same
   audited test inventory and reuse only exact-binary evidence.
3. Check and lower the actual bootstrap owners, registration, and dispatch
   on Windows and Linux for both x86-64 and AArch64. Run native toolchain
   steps outside the sandbox. BRA-695 and BRA-696 cover execution and shutdown.
4. Measure an optimized build on 24 cores without tracing. Planning should
   take 17–25 s. The full build must produce a finished binary within 60 s.
5. Remove temporary probes and stale artifacts. Complete the repository
   audit, mutating style check, formatting, and independent debug review.
   Push verified checkpoints to the existing draft PR.
