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

The guarded owner publication and typed pointee changes pass the audited
inventory of 1,709 cases, 582 checker tests, and 139 bound-tree tests.
The package-backed check and lowering attempt covers 41 bodies and
leaves 22 distinct diagnostics from the 43 in the baseline. The four
native targets check the 32 selected owner, registration and dispatch
bodies, including `dispatch_ready` and `start_frame`. A complete runtime
archive and executable link must check the remaining bodies and backend.

In an isolated optimized run with 24 workers, the Windows x64 standard
library took 28.3 s to plan and 26.9 s to emit. Platform providers took
18.3 s, and the full package invocation took 80.3 s. The earlier
private-root candidate planned in 23.7 s and emitted in 22.5 s. Compare
pinned binaries with the same prepared providers and one workload at a
time before attributing the difference to a compiler change.

## Runtime archive and cleanup

The exact checkpoint reused its standard-library bundle and had no
archive at the first seven-minute cutoff. The follow-up trace recorded
progress through numeric bodies at the nine-minute cutoff. Complex
division visited 609 of its 616 CFG blocks at most once. One scope exit
took 102.7 s, spending 67.3 s in 8,439 observation joins and leaving eight
outgoing states. Only 82 joins met the whole-state reuse check. The
`arctangent` loop separately converges after canonicalizing empty
ownership layouts and keeping agreed layouts when values are rekeyed.

The complete Windows x64 archive run used the pinned checkpoint compiler
and its prepared standard-library bundle, with Rust 1.98.1 and LLVM 22.1.8
for native providers. Runtime checking returned 54 diagnostics after 18
minutes. It reported nine capability declarations, a name collision,
40 unmet requirements, one raw deallocation with initialized storage,
and three unverified postconditions. The archive and link remain open.

The selected bootstrap fixture compiles runtime and standard-library
sources together with standard-library source authority. The archive
uses the ordinary runtime package with the prepared standard-library
interface. Reproduce the diagnostic families under that package boundary
before attributing them to individual runtime callers. Trace published
allocation and nullable owner witnesses, scoped acquisition and cleanup,
and initialized task fields through import, admission, consumption, and
lowering. Distinguish diagnostics on the producing body from downstream
requirements that depend on it. Validate capability and name changes
against the declarations and actual selected calls on the target.

A focused package-backed check reproduced all 40 unmet requirements,
the raw deallocation error, the name collision, and one postcondition
failure across 41 bodies. The complete product also checks declared
capabilities and reports the two remaining cascading postconditions.
Seven of those bodies reproduce the same ten diagnostics with either
source or packaged standard-library inputs.
The focused producer checks certify the allocation, event, deadline, and
execution-view postconditions at their source. Follow the same values into
the consumer before changing a producer or its callers.

The reached allocation clause has the same predicate and operands at
publication and admission. Joining the nullable alternatives replaces
its owning association with unknown, even where only the present row
has that association. The selected input retains the actual payload
owner. An allocation association restricts the use of independently
proved evidence. Keep a restriction from either predecessor, and keep
conflicting restrictions conservative after renaming. Admission compares
the restriction with the actual transferred owner identity. A copied
address or borrowed input cannot meet that restriction.

The deadline callback owner is present, but its portable last-element
expression still contains an unknown backing operand. Retain the identity
of the evaluated result when the expression cannot name an exact value.
Carry its physical capture and possible dependencies through the same
publication. Keep exact selected elements when the prefix supplies them.
The event owner's authored whole-result guarantee is established on
each normal row, then its operand shape changes at the result join. Bind
authored result clauses to the published value. Retain row-specific
address and witness support at the row that supplied it.

The link-slot proof initially establishes initialization for all three
admitted fields. The checked move reaches just the arena field in the
actual link slot. The physical invalidation lists the enclosing typed
pointee and discards the changed projection. Publish changes relative
to the typed pointee. Use those paths to invalidate and update the
corresponding current values. Update the nearest observable enclosing
path for an unresolved selector. A change to an enclosing path covers
its descendants. Keep unknown overlap at the whole pointee and combine
possible changes at row joins. The link-slot check must keep
the other two fields initialized and reject a second move of the same
field. The raw domain retirement and attachment result postcondition
still need their own reached transitions traced.

The focused comparison clears the deadline callbacks, event and arena
creation, task-slot field moves, and host storage preparation. All 22
remaining locations already appeared in the baseline. The remaining
declarations still require full product checking.

For the remaining scoped-view failures, trace the current acquisition
and its backing independently. In `publish_driver`, the loss is
reported after the host thread-local context operation. Its normal rows
distinguish the early return, protected static updates, and external
publication of the replacement address. Check whether the selected
row, the physical exposure, the current local marker and the
guarantee's lifetime support describe the same surviving value. A
possible external write or retirement must still invalidate its
dependent view. A stored raw address alone cannot expose its enclosing
inline owner.

`destroy_domain` checks and lowers by itself. Its caller has live duties
from service release and report draining and rejects the nested raw
retirement. The checked summary contains the consumption of the domain
and the reached retirements, with no ordered discharge attached to them.
Trace the exact retirement operand, the remaining duty's physical cell,
and the consumed cells at the retirement. A possible consumption, or
one that occurs after retirement, cannot discharge the duty.
The scheduler's own summary names both a consume and a retirement
through the same entry pointer. A raw intrinsic reads discharges from
its current memory. Re-export of a reached nested retirement currently
only forwards the attached list. Compare the producing prestate with
the nested retirement before changing that boundary. Deferred formation
knows the operands and possible effects, while actual discharge facts
belong to the reached execution.

The selected `StoragePlan` order determines each cleanup action.
`StorageFlowState` supplies its reached owner, receiver, and committed
effects and reports. Certification reads the checked publications by
operation and action. Both phases use the same continuation worklist.
Lowering consumes the checked order and emits runtime activity guards
and normal or failure transfers. Any change to the worklist must preserve
the failed action's performed prefix, its next slot and enclosing
continuation, pending-result abandonment, and convergence of repeated
array regions. See the [cleanup architecture](bra-694-architecture.md#cleanup-and-resumption).

The worklist currently joins registered failure and repetition states at
the same cursor. A resumed state runs the rest of the order after it is
taken from the queue. Determine how many joins come from that traversal,
normal lifecycle rows, and conditional cleanup selection before changing
the scheduler. Keep separately admitted outcomes when a later reader
needs their distinction. Check represented-child guarantees, exact
receiver writes, rejected conditional cleanup, replacement failures,
pending-result abandonment, and retained reports using the fixed case
inventory and full runtime archive.

The exact compiler's archive sample spent 28.8 of 43.2 sampled thread
seconds in execution joins. Replacements and equality rewriting used
14.6 s, including allocations of already retained aggregate and reference
observation nodes. During a bounded rewrite, read each retained child in
the existing order and keep the original canonical node when the result
has the same shape and values. Share that rule between structural walks
and equality rewriting. Reuse unchanged field lists and input nodes.
Constructors still normalize changed and noncanonical values. Preserve
the work limit, capture barriers, and the distinction between data
substitution and owner renaming. Use the existing bounded DAG,
nullable, witness, and frame controls with the fixed case inventory.
Compare joins and archive wall time before changing the cleanup scheduler.

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
