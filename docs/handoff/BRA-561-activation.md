# BRA-561 activation implementation

## Agreement and baseline

The user approved BRA-561, then BRA-505, then BRA-619 on 2026-10-15. Use one
reviewed PR per issue. No new language features or syntax. Work from consolidated
develop `055f1d4f` and sequencing document `5f1141e4` on
`feature/bra-561-compiler-described-activation`.

Preserve the published BRA-505 checkpoint `b3022d87` and BRA-619 checkpoint
`6bbcb4a3`. Transplant only each issue's changes after its prerequisites finish.
Their earlier pending-scope notes are superseded by this authorization.

## Design and removals

The compiler owns concrete immutable layout, state and operation descriptions.
An inactive owner admits frame, completion and cleanup backing before ownership
commits. Activation consumes that backing without allocation. Failure retains the
inactive owner. Bray owns the iterative activation chain inside the current run.
The existing Rust scheduler may bridge into it until its separate migration, but
must not retain another activation graph or reconstruct compiler descriptors.

Remove dynamic descriptor/state-vector reconstruction, generated symbol strings,
per-await Box/Arc allocations, separate completion allocation at activation and
native recursion proportional to async depth. Retain one result-movement state,
physical backing until its last user retires, checked lane/attachment authority,
and separate cancellation broadcast and resolution. Source callbacks execute
outside runtime collection locks.

No second cleanup scheduler, runtime registry or downstream semantic validation
pass. Reuse compiler frame plans, existing storage leases, outgoing admissions,
report ownership and runtime role bindings. Track production and test growth
separately. Reconsider representations if a change requires duplicate ownership
or a consumer workaround instead of replacing the owning mechanism.

## Coverage and implementation map

| Obligation | Current owner | Replacement and acceptance |
| --- | --- | --- |
| Concrete metadata | LLVM frame/core, MIR frame descriptor | Immutable generated descriptor and state lookup, shared for source/imported generic frames. Remove Rust reconstruction. |
| Inactive admission | LLVM frame constructor, cleanup storage descriptors | Frame, activation and completion use admitted aligned backing. Denial preserves captures. |
| Activation authority | storage leases, run dispatch context | Check lease epoch and lane/attachment before transfer. Rejection retains owner. |
| Same-run composition | Rust native/run | Bray iterative parent/child transitions, no child task or transition allocation/enqueue. |
| Completion | frame callback and native terminal state | Move once, retain backing for surviving dependencies, release once. |
| Cancellation | generated broadcast/resolution callbacks | Broadcast all initialized obligations first. Resolve inactive, recursive and erased frames without source execution or native-depth recursion. |
| Suspension and wakes | native scheduler bridge | Preserve parent state and run wake handshake. No callback under collection lock. |
| Disposal | native/run retired chain and frame Drop | One Bray owner and iterative release, preserve callback failure reports. |
| Cost and artifact ownership | native tests, artifact inspection | Measure frame/descriptor bytes, transition allocations, deep stack and specialization. Link map excludes migrated Rust owners. |

Known-producer and erased/recursive paths must share the same ownership rules.
Exercise ready and suspending children, cancellation and panic, allocation denial,
over-alignment, wrong lane, stale epoch, imported generics and retained results.
Existing tests are evidence candidates, not current acceptance. Review only after
production consumers switch and replaced Rust ownership is removed.

## Metadata checkpoint (2026-10-15)

Remove native reconstruction of full frame descriptors and generated symbol
strings. Frame execution supplies the requested immutable state. Tasks and the
scheduler retain that state rather than copying an independent descriptor graph.
Lane combinations use shared static slices, and empty native storage/dependency
metadata has no allocation. Root dispatch consumes the scheduler's selected lane.

Validation: 165 runtime tests and five runtime-model tests pass, including deep
direct awaits, lane changes, suspended cleanup and over-aligned completions. The
additional native query regression passes: construction reads only state zero,
requested state 127 is read once, and invalid state 128 makes no callback.
`cargo xtask style`, `cargo xtask format`, and `git diff --check` pass.

Audit: re-read AGENTS.md and inspect changed APIs and their callers. Search the
runtime/model for descriptor copies, state lookup, lane-set normalization and
scheduler lane selection. Compiler and test descriptors remain immutable. Runtime
reconstruction and the duplicate scheduler lookup are removed. Snapshot retains
root identity and current execution metadata. This checkpoint does not complete
BRA-561: frame admission, capture rollback, Bray activation ownership, cancellation
and artifact/cost evidence remain outstanding. No completion review is claimed.

## Admission and cancellation checkpoint (2026-10-16)

Frame construction uses outgoing admission to reserve concrete aligned backing
before capture transfer. MIR names either a value owner or a selected invocation,
shared with cleanup admission. Constructors claim the reserved slot, retain its
lease and release through the same storage provider. Rejected release traps
because compiler-created ownership must establish it. Root admission failure
retains the ordinary root outcome, skips construction and observation, and still
runs host teardown.

Cancellation selection is explicit in MIR. Each resumed state checks delivered
cancellation before its continuation. State zero initializes guards, then can
clean captured parameters without entering source statements. The owning CFG
models that exit. LLVM no longer searches suspension terminators for cleanup
edges. Initial metadata contains initialized input storage identities.

Validation passes 321 checker, 1,683 compilation, 44 MIR, 39 lowering and 190
package-interface tests, plus doctests. The sole failure is
`scoped_selected_calls_emit_native_artifacts`, with
`MissingRole(CleanupIncidentTransfer)` owned by the pending BRA-505/BRA-619 work.
All nine focused regressions pass again after the final audit refactors. Rustfmt,
`cargo xtask style`, `cargo xtask format` and `git diff --check` pass. They
cover frame admission, inactive capture cancellation, imported generics, started
tasks, root host construction, callback outcome consumption and scalar facts.

The audit shares runtime-status traps, void intrinsic emission and executable-root
lookup. It constructs state zero once and splits the host module into a thin root
with root-execution and host-lifecycle implementations. Obsolete recoverable
cleanup-target errors are removed. Native callback regression assertions now
check outcomes already consumed before boundary release, with distinct failure
codes for report, destruction, shield and cancellation counts.

## Erased factory implementation

An asynchronous callable value points to one immutable constructor/backing pair.
It stays one pointer wide. Direct and indirect construction use ordinary call
translation. MIR admission retains the evaluated factory before arguments can
change its source storage. Dynamic backing operands participate in demand,
validation, remapping, inspection and executable-template serialization. The
executable template format remains version 1, with its current representation.

Native code-address conversion and foreign callable validation share the ABI's
execution-mode rule. A native code pointer cannot manufacture an async factory.
Imported generic coverage keeps the erased invocation and both async lambdas in
one exported family. The existing native i32 fixture now executes two factories
with distinct results. Its native execution passes in the concrete-backing acceptance batch below.

Call translation distinguishes constructing an async cleanup frame from invoking
a synchronous cleanup body. The former claims its already-admitted frame backing
without activating a second cleanup record. Symbol realization is split into
mapping, native boundary and identity modules with a thin root. Identity export
reuses the existing source-backed-key predicate instead of repeating conversion.

Validation: the broad batch passed 2,345 tests and exposed the known selected-scope
missing role plus two static-finalizer construction regressions. Both construction
regressions are fixed. All 18 focused regressions now pass, including imported
families, static finalizers, callee retention and erased backend emission. Rustfmt,
`cargo xtask style`, `cargo xtask format` and `git diff --check` pass. Logs are
`target/bra-620-authority/bra-561-erased-final.log` and
`target/bra-620-authority/bra-561-erased-audit.log` in the shared repository target.

The audit searches factory selection, symbol declarations, ABI execution modes,
operand traversal/remapping and async cleanup activation across their producers
and consumers. It removes the separate source-frame helper invocation path and
repeated native ABI rules. No new language feature, runtime allocation or owning
runtime graph is introduced. The factory record is two pointer fields per concrete
callable, while each source callable value remains one pointer.

## Next activation replacement boundaries

Keep the next change coherent across the compiler description, Bray driver and
Rust consumers. The current graph remains the baseline until those consumers
switch. Do not add another owning graph beside it.

- Retain the complete lease epoch in the inactive owner. Validate its pool domain,
  slot, concrete shape and epoch before dereferencing reusable frame backing.
  Use the concrete instance frame identity, not the generic template identity.
- Coallocate activation state, source storage and completion. Store immutable
  callbacks and state metadata once per concrete description. Runtime mutation
  belongs to the admitted frame, not another Box or descriptor copy.
- Establish driver authority before consuming the inactive token. Rejection must
  leave the token and captures intact. Existing by-value composition and start
  boundaries need to change together with their compiler consumers.
- Preserve the current source callback outcome contract, cleanup incident order
  and exact-thread requirements. Reuse native scheduler/context bridges only for
  their existing responsibilities. Generated callbacks already report language
  panics explicitly. Resolve the existing native unwind fixtures without adding
  allocation to activation or weakening failure retention.
- Generated resume already publishes the result address in frame progress. Use
  that owned result slot and remove the extra completion copy/allocation. Retain
  the existing payload until its consumer's move commits exactly once.
- Resolve lexical cleanup before terminal publication, but retain frame and
  completion backing while the parent or task result still depends on them.
  Remove NativeTerminalState's separate completion allocation and owner.
- Broadcast initialized obligations separately from resolution. Source-frame
  broadcast currently only sets a byte, and resolution ignores suspension.
  Replace both through the owning MIR cleanup paths and one iterative driver.
  Inactive Future cleanup and task-observation frames must use that same driver.

## Remaining implementation

Replace NativeFrame/NativeRun ownership, including Box/Arc activation links,
activation-time completion allocation and six-record admission during composition.
The source-frame allocation contains its initialized result slot. One admitted
owner must retain that backing through result dependencies. Do not introduce another graph or a replacement allocation
inside a transition.

Validate backing epoch, concrete shape, provider and inherited execution authority
before committing activation. Rejection retains the inactive owner. Keep trusted
native ownership contracts explicit. Numeric fields or pointer equality alone
cannot establish that contract, and Bray fields have no privacy guarantee.

Complete native evidence for indirect factories, inactive Future lifecycle,
initialized-state broadcast and iterative resolution. Setting a cancellation byte alone does not visit captured
task obligations. Calling resume once and ignoring suspension does not resolve
cleanup. Migrate task-observation and static-finalizer consumers through the same
driver, retaining only bounded scheduler/task bridges for their separate issues.

Finish native allocation, deep-await, alignment, affinity, retained-result,
imported/erased and failure acceptance. Record artifact and cost evidence, then
push a draft PR and conduct its independent review before BRA-505 and BRA-619.
This checkpoint does not complete BRA-561 and has not had a completion review.


## Concrete backing identity correction

LLVM backing descriptors and activation adapters now use the already-established
concrete `CodegenInstance` frame identity. They previously emitted the generic MIR
template identity, which could conflate same-sized generic specializations. The
emitted backing layout is the source context itself, including its existing result
field. Remove the unused second result field rather than reserving it speculatively.

Nine host-emission tests pass, including a regression that creates same-sized
`identity<i32>` and `identity<u32>` frames and checks both emitted descriptor and
adapter identities against concrete specialization. The first regression run
failed only because its expected LLVM string spelling used a hexadecimal escape
for a backslash. That expectation is corrected. The complete `cargo xtask readiness native-execution` run passes in 376.2 seconds,
including erased factories, independent tasks, panic shutdown, static cleanup and
runtime artifact smoke tests. This is production behavior evidence, not completion
of the remaining activation-owner migration.

The audit consolidates four native identity-byte emitters into `native::identity_value`
and removes the static identity forwarding wrappers. After this refactor, all nine
host tests and both targeted static-finalizer/storage regressions pass. Rustfmt,
`cargo xtask style`, `cargo xtask format` and `git diff --check` pass. Keep their
small deterministic formatting fixes to avoid repeatedly undoing the formatter.
Production code decreases by 17 lines, excluding those unrelated formatting-only
changes. The new regression adds 95 test lines. Logs are `bra-561-concrete-hosts.log`,
`bra-561-native-acceptance.log` and `bra-561-identity-audit.log` under the shared
`target/bra-620-authority` directory.

The remaining driver change must resolve these boundaries together:

- Generated source callbacks already return explicit owned outcomes. Preserve
  report-before-unwind evidence for native fixtures without requiring an unwind
  to cross Bray frames. Any native containment bridge borrows preadmitted records
  and has no activation ownership, allocation or scheduling policy.
- Task observation currently owns a separate Rust context, result allocation and
  cancellation path. Its initialized result and retained task must follow the
  same result-retirement rule as ordinary frames. A successful payload move does
  not itself prove that releasing the task's backing is safe.
- Existing `ContinueCleanup` edges identify the transition from cancellation
  broadcast to lifecycle resolution. Preserve that boundary in owning lowering.
  Do not rediscover it by searching LLVM control flow or run source lifecycle
  resolution from a broadcast callback. Cleanup suspension must return to the
  same iterative driver with its pending outcome intact.


## Explicit observation cleanup outcomes

Task-observation cleanup now uses generated C callbacks with caller-owned
`NativeRunOutcome` destinations. It invokes the selected source helper directly,
using the same `invoke_with_outcome` emitter as static cleanup. Remove the former
static-only invocation helper and the duplicate native-adapter callback type.
`bray-runtime-abi` owns that callback declaration.

The native bridge preserves a report written before a fixture unwinds. Disposal
continues through lifecycle cleanup and task retirement after cancellation.
Admission reserves five outgoing records before construction: two callbacks can
each publish a report and then unwind, and task retirement can fail once. The
regressions deny further admission and check ordered, exactly-once report release.
This bridge remains temporary until the activation owner is replaced.

Keep the runtime ABI at 1.0 and executable template format at version 1. This
unreleased project has no earlier supported compatibility contract. Remove the
unnecessary ABI bump, its fixture changes and the earlier template-version bumps.
Regenerate disposable artifacts for the current representation. Follow the owning
[serialization rules](../contributing/coding-conventions.md#serialization-and-persisted-data).

After restoring the original versions, all 27 focused tests pass: seven runtime
observation tests, nine compiler host-emission tests and eleven executable-template
tests. The obsolete ABI-2 native acceptance run was stopped. Native acceptance for
the callback changes passes in 350.8 seconds. The log is
`target/bra-620-authority/bra-561-callback-native-v1.log`. This run predates the
checked broadcast implementation below. The selected-scope missing role remains owned
by the approved BRA-505/BRA-619 prerequisite sequence. Test logs are
`target/bra-620-authority/bra-561-version-correction-tests.log` in the shared target.
Rustfmt, `cargo xtask style`, `cargo xtask format` and `git diff --check` pass.

The behavior audit covers explicit callback outcomes, report-before-unwind
retention, shared signature-aware invocation and format versioning. Other native
frame callbacks still belong to the pending driver replacement. The new report
merge retains an Option-owned incident because this adapter unwinds its combined
failure only after task retirement. It reuses existing report reservations and
suppression without adding another report container.

## Driver replacement constraints

Do not implement broadcast by merely resuming the cancellation path up to its
first `ContinueCleanup` edge. A source frame may already be suspended in shielded
cleanup. Its ordinary cancellation edge is intentionally unreachable, but other
initialized task obligations can still need broadcast. The owning lowering must
supply broadcast operations for every suspended state, including cleanup states,
from its existing guarded cleanup plans. Preserve the current pending outcome
and exact lifecycle continuation. Do not rediscover plans or guards in LLVM.

Result transfer also needs an explicit commit boundary. The current native call
returns an address and LLVM loads afterward, so that call cannot release the
result backing before it returns. The replacement must move into a caller-supplied
admitted destination before retiring the child. Checked source returns cannot
export a borrow into dying local storage. External borrow and provider roots
still follow the published result contract, and native callbacks must meet that
same contract. Do not retain every scalar loop iteration in a retired-child list.
These requirements guide the replacement. They are not claims that the existing
native owner already implements them.


## Checked broadcast implementation in progress

Suspension analysis selects cancellation owners from its established storage-flow
snapshot, including scoped-exit suspension. Lowering uses the same initialization
and represented-part guards as ordinary cleanup, but supplies a distinct broadcast
entry for each frame state. State zero visits initialized captures without reading
body guards. Broadcast owns separate callback-local incident destinations, leaving
pending lifecycle outcomes and continuations intact. Its scratch storage occupies
no retained frame bytes.

MIR reconstruction, concrete optimization roots, executable-template encoding and
inspection retain the broadcast entries. LLVM emits only each callback's reachable
blocks. Native broadcast callbacks return explicit outcomes and preserve reports
written before unwind. Completed generated frames must publish a terminal state
whose broadcast entry is empty, because native terminalization calls broadcast
after source cleanup. This terminal-state correction is awaiting verification.

The first batch passes 66 native runtime tests, 15 lifecycle-lowering tests, nine
host-emission tests and eleven executable-template tests. This includes retained
Task captures during shielded scoped exit and broadcast report-before-unwind under
denied admission. Log: `target/bra-620-authority/bra-561-broadcast-tests.log`.
These results precede the terminal-state correction. Final formatting, broader
checks and the checkpoint push remain pending. No completion review is claimed.


## Broadcast audit and awaited outcome repair

The broad broadcast batch passed 2,540 tests and exposed two regressions plus
`scoped_selected_calls_emit_native_artifacts`, whose missing
`CleanupIncidentTransfer` role remains in the approved BRA-505/BRA-619 sequence.
The MIR reconstruction and imported-family regressions are repaired by returning
broadcast outcomes through existing propagation terminators. Broadcast never
publishes its suspended frame as terminal. An empty broadcast has no shield calls
or scratch storage.

The stronger shielded-exit regression exposed pre-existing inconsistent MIR:
selected async cleanup construction was rejected, lifecycle suspension resumed
into an ordinary block, and scoped calls checked for an outcome immediately after
infallible lazy construction. Preserve lifecycle blocks across their suspension
and check only actual execution. The old awaited-completion boundary also replaced
child panic/cancellation with a generic failure. It now supplies the owned child
outcome to the existing caller cleanup dispatcher. Ordinary await, scoped enter
and shielded exit share this boundary. The result address remains retained until
the caller loads it. Eliminating the retired-child list still belongs to the
activation-owner replacement.

Current verification is `bra-561-outcome-build.jsonl` and
`bra-561-outcome-tests.log` under the shared target's `bra-620-authority` directory.
All 92 focused tests pass: 66 native runtime tests, 16 lifecycle-lowering tests,
one imported-family regression and nine host-emission tests. Temporary validator
instrumentation has been removed. A native original-outcome transfer regression,
final style/format and native acceptance remain to run. No PR or completion review
has been created for this branch.

## Verified broadcast checkpoint

The original-outcome transfer regression passes under denied further admission.
`cargo xtask style`, `cargo xtask format` and `git diff --check` pass. Native
execution readiness passes in 442.0 seconds, including archive export checks,
source execution and runtime artifact smoke tests. Logs are
`bra-561-outcome-transfer.log`, `bra-561-outcome-style.log`,
`bra-561-outcome-format.log` and `bra-561-outcome-native.log` in the shared target's
`bra-620-authority` directory. The broad compilation run finishes with 1,687 passing tests and one known
selected-scope role failure in `bra-561-outcome-compilation.log`, in 1,122.66
seconds. That missing `CleanupIncidentTransfer` role remains in the approved
downstream work, so this checkpoint is not issue completion.

The audit re-read AGENTS.md and reviewed the changed production paths, ABI
consumers, template encoding/decoding, reconstruction, optimization roots and
inspection. Searches covered broadcast selection/guards, explicit callback
outcomes, report suppression, completion checks and frame pointer construction.
Reuse the existing cleanup dispatcher, report ownership and signature-aware call
emitter. The new broadcast scratch lifetime removes retained frame bytes and
empty broadcasts emit no cleanup work. Patch growth is 870 production lines added
and 279 removed, with 475 test lines added and 45 removed. Those counts exclude
this task record and separate inline test modules from production code.

## Next replacement boundaries

Replace the owning chain as one coherent change before issue completion. Keep
immutable concrete descriptors with source and completion offsets. Store the
activation links and pending progress in admitted frame backing. An external
inactive token must carry the complete lease, not only its reusable address.
Validate the lease and descriptor shape before dereferencing that backing, then
check execution authority before consuming the inactive token.

Commit a completed child into the caller's admitted destination before retiring
its backing. Account for an unconsumed completed value as an owned result requiring
its selected cleanup, not merely bytes to free. Preserve external/provider
requirements carried by the result. Remove the retired-child list rather than
retaining every scalar loop iteration until the enclosing frame finishes.

Use one iterative broadcast queue owned by Bray. A broadcast callback may enqueue
nested inactive futures, but must not recursively drive their callbacks. Native
bridges may carry the borrowed queue address through callback context, contain
native unwind and enter the selected lane. They must not own a second queue or
activation graph. Cleanup suspension resumes the same driver and retains the
parent's pending outcome. Never recreate the current one-shot resolution bug.

Native fixture callbacks need admitted Rust-panic payload backing before their
inactive owners become live. Keep that native callback requirement explicit.
Generated Bray callbacks already transfer admitted report ownership through their
outcome ABI. Do not introduce universal per-transition admission or reconstruct
source cleanup plans in the runtime. Existing task admission, scheduler queues and
root orchestration remain the bounded unmigrated callers described by BRA-561.

## Activation replacement in progress

The next uncommitted batch adds the immutable activation descriptor, a coallocated
Bray header and production authority checks. The compiler emits and initializes
the header after claiming the complete frame region. Its invalid state-description
branch now traps instead of returning an unrestricted default state.

The resident pool distinguishes initialized inactive storage from dispatched
storage. Its locked transition checks the complete domain, shape, slot, epoch and
address before consuming the inactive token. Duplicate tokens cannot borrow a
header concurrently with its dispatcher. Layout and address-overflow rejection
precede consumption. The descriptor comes from the exact producer token, with
provider retention and initialization required by an opaque trusted contract.
Entry authority is a separate requirement. Neither numeric fields nor field
visibility establish either contract.

This batch is not the driver replacement. The old Rust activation graph, adapter
and completion backing still execute. Remaining integration must establish entry
authority from the existing native lane admission before calling `begin`, replace
the two-word future with its complete inactive token, implement callback-boundary
iteration and result commitment, then remove the superseded Rust graph. The new
Bray attachment and detach transitions are not yet connected. Production contract
checks and ABI layout tests are being built in `bra-561-activation-build.*`.

The focused batch passes 99 checks: 19 ABI tests, 69 native runtime tests, the
production storage/activation contract audit, producer/entry authority rejection
and nine host-emission tests. The first contract run rejected a captured witness
in the descriptor-reader lambda. The corrected projection takes the exact owner
as its parameter and passes the production declaration checks. The callback bridge
preserves explicit suspension and a report published before native unwind without
new admission. Resume, broadcast and lifecycle callbacks share this containment
operation. The new scheduler adapter passes `cargo check --features scheduler`.

`cargo xtask style`, `cargo xtask format`, Rustfmt and `git diff --check` pass.
Logs are `bra-561-activation-tests.log`, `bra-561-activation-style.log`,
`bra-561-activation-format.log` and `bra-561-activation-adapter.log` in the shared
target's `bra-620-authority` directory. These checks do not establish the missing
native token-transfer and driver integration. Native execution readiness remains
the earlier checkpoint's evidence, not evidence for this batch.

The checkpoint audit re-read AGENTS.md and checked every changed source, thin
module roots, visibility, ABI signatures and callback outcomes. Repo-wide searches
covered lease/shape validation, state transitions, header offsets and written
report/unwind composition. Reuse the existing lease checks and `RuntimePanic::record`
without adding another report owner. Growth is 840 production lines added and 153
removed, plus 224 test lines, excluding this document. The new metadata, access
contracts and callback bridge are prerequisites for deleting the still-present
Rust activation graph and completion owner, not a second permanent implementation.


## Publication checkpoint

Generated constructors now publish their exact immutable activation descriptor
only after initializing the header and captures. The resident slot uses one
ordinary tagged state for its free-list link or published descriptor identity.
Dispatch requires that identity as well as the complete lease and storage shape.
This replaces the earlier interpretation that any active byte region was an
initialized inactive frame. It adds no registry or per-frame allocation.

The native publication export uses a raw token pointer with the existing checked
memory requirements and an exact producer-slot contract. Its internal operation
requires the producer witness for that token. Borrow parameters are not permitted
in the C ABI. A witness for another token does not authorize publication.

Eleven focused compilation checks pass, covering production storage and activation
bodies, exact-producer rejection, and all nine host-emission tests. Native runtime
artifact smoke tests pass, including reserved/repeated publication, stale leases
before and after slot reuse, and allocation denial after admission. The native
build also checks complete production declarations and rejected an unused outer
raw-memory capability, which was removed. Logs are
`bra-561-publication-build.*`, `bra-561-publication-style.log`,
`bra-561-publication-format.log`, and `bra-561-publication-native.log` in the shared
`target/bra-620-authority` directory. Rustfmt, mutating style, Bray formatting and
`git diff --check` pass.

The audit re-read AGENTS.md and reviewed all twelve changed source/test files.
Searches cover slot states, lease/shape checks, descriptor publication, token
witnesses, runtime-role consumers and frame field allocation. Existing shape and
lease checks remain shared. The old numeric states and separate free-link field
are removed. Growth is 209 production lines added and 33 removed, plus 139 test
lines. Publication establishes initialized ownership before the consumer starts.
The two-word inactive ABI and Rust owning graph still execute and remain to be
replaced before BRA-561 can be reviewed or completed.
