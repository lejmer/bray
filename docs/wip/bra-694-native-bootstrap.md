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
inventory of 1,709 cases, 584 checker tests, and 139 bound-tree tests.
The package-backed check and lowering attempt covers 41 bodies and
leaves 12 distinct diagnostics from the 43 in the baseline. The four
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

Repeated short-circuit and result joins can qualify the same carried trusted
claim with different path guards. Candidate selection currently keeps only
those whole guarded clauses, so a later join cannot recover the common claim
even when every actual predecessor proves it for the same joined owner. Treat
nested trusted atoms as proof candidates, not assumptions. Publish a direct
claim only when each actual predecessor proves it from that carrier's witness
evidence and its own ordinary path facts. Preserve owning-witness support only
when both predecessors separately prove the claim from owning clauses on that
carrier. Address-only evidence cannot become ownership authority. A clause that is merely vacuous on a
path with no authority must remain unavailable, an unrelated carrier cannot
lend evidence, and an inactive nullable payload carries no authority. The
positive oracle joins opposite guards whose carrier clauses both imply the
same claim. The negative oracle has authority on only one predecessor and
must not manufacture the claim. Reconsider if the joined owner substitution
is wrong or if any negative oracle gains authority.

A copyable raw-address promise can name its address without depending on any
tracked storage owner. An empty owner set is no lifetime dependency. Preserve
that promise while the same address value remains observable across cleanup,
but do not convert it into ownership authority. Retire it when the address is
no longer carried or an actual address effect revokes the predicate.

A returned reference has a distinct identity, while its retained input bound
may still identify one exact actual operand. Physical retirement reads prior
discharges through that stored exact binding, and summary publication uses the
same origin. Multiple alternatives, region-only bounds, older incarnations,
and cycles remain footprints and cannot claim a discharge.

The attachment exchange postcondition reaches the terminal state with both
`execution != none` and `status != 0` as exact ordinary facts. Its required
condition is `status == 0 || execution != none`. Mixed proof scans the left
comparison's large derived relation first, exhausts its bounded work, and
never reads the decisive exact right alternative. Boolean proof must inspect
either direct decisive child before recursive implication work, independently
of source order. An unknown or non-decisive child remains insufficient, and
trusted evidence cannot establish an ordinary atom. The positive oracle puts
an expensive unresolved comparison before an exact true alternative. Negative
oracles keep the alternative false or unknown. Reconsider if the terminal
state loses the exact entry fact or if a non-explicit alternative starts
bypassing bounded proof.

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
creation, task-slot field moves, host storage preparation, execution
publication, the attachment result postcondition, the second lifetime body,
and the host execution-owner postcondition. Twelve distinct package-backed
diagnostics remain, all at locations already present in the baseline. They
group into carried trusted authority across result and cleanup joins,
allocation or address authority across owner transport, and ordered retirement
export. The remaining declarations still require full product checking.

The structured-escape correction is present, but the remaining scoped-view
failures occur later. Cleanup admits the reached receiver after sibling writes
have renamed it to a post-state or joined identity. The exact `view_owned`
clause remains attached to the producing identity. The same split appears in
nullable catch results and native-provider or observer owners: the selected
value and owning witness reach the call, but the carrier key and clause operand
do not always follow the value's publication rename. Allocation facts can reach
an admission with no allocation-owner restriction for the transferred owner. A
safe producer proves a declared clause from one live source witness and retains
that source support. A trusted producer does not receive a body proof obligation
because its declaration supplies the clause. Portable witness support still
requires checked evidence. The call binds declaration-only support against the
actual arguments and result. Keep all declaration guarantees and
subjects available for publication, while keeping safe-body proof obligations
separate. Bind both through the same entry-and-result instantiation. Do not
require the proof source itself to have already become a returned owned
observation, reconstruct an owner at admission, or make a historical allocation
fact unrestricted. A copied address or borrowed alias must still be unable to
spend an owning carrier.

`destroy_domain` checks and lowers by itself. Its caller reaches raw retirement
with three outstanding typed cells below the same execution owner, including
two direct service fields and one nested report field. The nested summary
contains their definite consumes and the later retirements, but the retirement
effect has no ordered discharge attached. The performed prefix at the reached
retirement must carry only definite earlier consumes of exact cells. A
conditional consume, a possible alias, a consume after retirement, and a cell
reinitialized after consumption cannot discharge the duty. The positive
oracle consumes all three exact typed cells before raw retirement. Preserve the
discharges on the actual reached retirement. A singleton returned region
cannot identify an exact stored binding. The portable effect set
describes possible changes. Its sort order cannot establish a discharge.
Deferred formation knows the operands and possible effects. Actual discharge
facts belong to the reached execution.

The three-body package probe at `cd6c582a5` checks and lowers
`create_execution` and `destroy_domain`. The allocation reaches
`initialize_domain` with its original owner, pointer, size, and alignment
after the arena call. The nullable allocation test with an opaque call fails.
`retire_pin` reproduces E7086 for three pending cells under the domain pointer.

The package-backed probe checked and lowered all 13 selected bootstrap
bodies with the routed-discharge candidate. It reported 16 diagnostics at 10
source sites. The next route batch preserves an inactive nullable payload
while its tag survives, and bounds an unresolved result pointer by the reached
input descriptor instead of its safe-borrowed owner. The same 13 bodies now
report five E7123 diagnostics at two sites: `execution/ownership.bray` and
`execution/worker_context.bray`. `root_scope`, `retire_pin`, and the other
previously failing sites check and lower. The remaining sites both transport
an owned nullable value from a trusted producer through a consumed `Result`
or catch payload, then lose the producer's predicate before the consumer.
The probe uses a standard-library bundle built with Rust 1.98.1 and LLVM
22.1.8. Rust 1.99.0 emits LLVM 23 bitcode that Bray's pinned LLVM 22 symbol
tool cannot inspect.

### Current reference-route contract

A result pointer copied from a reached input descriptor keeps that descriptor's
possible pointees. The input's safe borrow can keep its owner alive, but does
not make the owner a pointee. An unknown descriptor can refer to external or
previously exposed storage. It must not gain an exact address, initialization,
ownership, or a new route to private sibling storage. An actual raw pointer to
an inline owner still exposes its enclosing allocation.

`current()` publishes independent or empty pointer alternatives. `entry_scope`
transfers the selected `Current` and publishes an inactive `operation`.
`nested_context` admits `&scope.current` and returns a `Context` whose
reference relation reads `parent.pointer` and its typed fields. When the
caller cannot complete that deep read, output staging reads the reached pointer
descriptor's possible regions from the admitted capture. Its safe borrow
remains a lifetime support only. The following result-bounds pass preserves
the narrower symbolic pointer region. `commit_driver` then leaves the
disjoint inactive `operation` payload intact. The same output staging serves
ordinary calls and deferred frames. Intrinsic result bounds retain their
distinct inputs.

The positive oracle is the package-backed `root_scope` check at line 89 plus a
small source and imported-call control that returns a pointer through an
opaque descriptor while a disjoint nullable sibling remains empty. Negative
oracles return an actual pointer into the owner, expose a raw address, or
write the tag. Those paths must still invalidate the sibling, and a bounded
region must never become an exact cell. The fixed 13-body package probe fell
from 16 diagnostics at 10 sites to five at two sites after the route batch.

Use the admitted capture, prepared `StorageReferencePath`, and existing
`StorageAddressValue` and symbolic returned identity. The all-root fallback
remains only where no reached pointer descriptor gives a narrower region.
Do not retain a parallel origin map or reconstruct the caller state from the
portable summary. Keep code growth to the route computation and its direct
consumers. Reconsider if a known local pointer loses its local target, a raw
exposure stays protected, or an unresolved route cannot provide a sound
bounded alternative.

### Current owned-payload contract

The focused package probe now reports two E7123 diagnostics in
`admit_source_task`. It reported five across this body and
`create_worker_context` before the latest grouped repair. Both pass a selected owned payload to a trusted
consumer. A producing guarantee attached to the payload must move with the
same owner. The rejected variant must carry no authority, and the old storage
must lose authority after consumption. A later mutation, retirement, or
incompatible join must still revoke dependent claims.

The admission probe shows the correct owner and available witness at both
consumers, but the trusted fact is absent. `admit` republishes its declared
claim after its nullable result-row join. The caught `Result` keeps the
transferred payload witness, but the success and panic join cannot prove its
guarded claim against both predecessor snapshots. In `create_worker_context`,
provider selection retains the claim. Opaque `try_allocation` preserves the
provider local and its nullable payload physically. Preserving that live
nullable's inline tag keeps the already proved guarded claim through the
frame and clears its three downstream requirements. A scalar has no such tag.
The caught-result join still fails after projected snapshot substitution
follows the predecessor's owner identity. Trace its complete admitted proof
before changing the remaining join boundary. `StoragePlan` identifies
the source and target slots. `StorageFlowState` supplies the reached selected
owner and physical preservation. Proof must use those same identities. The
positive package oracle is these two bodies. The current comparison is two
E7123 diagnostics at one site. Add focused
source and import controls for the selected owner and rejected branch. Do not
reconstruct ownership from a condition string or add a second persistent
owner map. Reconsider if the producer never publishes its carrier or if a
guarded join has more than one possible owner.

The call that extracts the whole `Domain` publishes a definite typed
discharge through the admitted domain pointer. The reached scheduler
retirement sees that cell but uses a different address. Later cleanup effects
can also remove the consumption marker before the allocation record retires
its backing. Publish typed discharge routes alongside the retirement route.
Keep the actual transfer evidence across effects that cannot reinitialize
the cell. Validate reinitialization, possible aliases, guarded paths, and
package import with the same route binder.

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
