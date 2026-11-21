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

Pushed checkpoint `fb05ef10d` admits checked raw owner construction before
installation. The expanded candidate passed 1,708 of the 1,709 audited cases,
with an outcome for every case, and passed the 577 checker and 139 bound-tree
cases. All 32 expanded bootstrap bodies checked and lowered on Windows and
Linux for x64 and AArch64. Checkpoint `18ad9f166` binds entry witnesses to
their installed storage. All 1,709 fixed cases passed on its optimized test
binary. The timed-out case in the full run passed on the same binary in a
43.1 s isolated retry. The 31 focused checks, 577 checker cases, and 139
bound-tree cases also passed.

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

Read the same condition node directly through every handle that shares it.
One retirement uses the same dependency and value transformation across
current slots, witness conditions, admitted calls, and deferred frames.
It must leave unrelated nodes and captured lifetimes intact. A join uses
its checked selection guard when available. Only joins without a selector
derive path guards from both sets of ordinary premises. Reuse each joined
predicate observation across both predecessor lookups. Verify loop epochs,
saved referents, guarded facts, and deferred captures with the same native
and fixed-inventory checks. Compare planning before making further changes.

## Current verification batch

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
