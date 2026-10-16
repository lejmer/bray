# BRA-561 activation implementation

## Current checkpoint (2026-10-17)

BRA-561 remains in progress on PR #611. Continue BRA-505, then BRA-619 after this
PR's implementation, audit and independent review. No issue is complete merely
because native fixture callbacks pass. The actual generated constructors and
imported native constructors must satisfy the same admission and retirement
contract. No new syntax, compatibility path, ABI version or cleanup scheduler.

The 54 production authority tests pass, including rejected forged, reconstructed
and differently paired owners. The 176 runtime tests pass. Runtime dispatch
invariants now fail outside the catch boundary for producer panics. The native
artifact fixtures pass against the previous archives, including six deep-chain
cases with a constant 3072-byte callback stack span and no allocations after
admission. These results do not cover the subsequent constructor fixes below.

Tracing actual source execution found two gaps:

- Generated constructors claimed admitted storage but left their outgoing report
  chain empty. They now claim the already-admitted primary record and storage
  together through `OutgoingActivation`. Publication retains both with the owner.
  Activated retirement transfers unused records out before releasing the header, and the
  native run drops the returned chain. Panic transfers a record into its owned
  report instead. Frame runtime demands now name that same operation.
- Native imports retained the producer constructor symbol but derived companion
  data symbols from the consumer's imported MIR key. Backing and factory symbols
  now derive from the mapped constructor identity in both producer and consumer.

All 14 constructor tests and 10 native-import tests pass, including the new
imported async companion regression. Full artifact verification rebuilt the compiler, standard library and runtime
from this batch. Its native tests pass. Permanent source coverage now executes the existing async value/borrow/recursive fixture and imported task,
panic and cancellation fixture in development and release configurations.
Logs: `target/bra-561-frame-hosts.log`, `target/bra-561-frame-imports.log`,
`target/bra-561-constructor-compiler.log`, `target/bra-561-constructor-artifact.log`.
The four source executions pass through both the CLI and the permanent artifact
smoke runner. The test emitter now requests the existing linker-generated debug
companion for development executables, following `BuildConfiguration` policy.
The temporary retained-artifact test entry has been removed. All 15 native ABI
checks pass across supported target layouts. The full artifact command initially
stopped at that test-emitter setup defect, so its remaining source checks were
resumed against its retained fresh artifacts and passed in 17.04s.

Additional logs: `target/bra-561-generated-acceptance.log`,
`target/bra-561-native-abi.log`, `target/bra-561-retained-source-smoke.log`.
Artifacts: `target/bra-620-authority/bray-runtime-artifact-smoke-V9TVHc/`.
Implementation checkpoint `6d6c2050` is pushed to PR #611. Rust formatting,
`cargo xtask style`, `cargo xtask format` and whitespace checks all pass after the
final module-placement audit. Inactive-frame construction now sits with the
existing task/observation constructors, reducing the general effect dispatcher.
All 15 native ABI tests pass after that move. Logs are
`target/bra-561-final-layout-audit.log`, `target/bra-561-checkpoint-style.log` and
`target/bra-561-checkpoint-format.log`. Fresh independent PR review remains.

### Maintainability and acceptance audit

The current batch removes the Rust frame owner, root adapter and dynamic frame
transfer description. The Rust run retains scheduling integration and an exclusive
dispatch claim, while Bray owns parent/child traversal and retirement. Generated
completion uses one admitted parent destination. No second graph, cleanup engine,
ABI version, source syntax, or Rust unsafe operation is introduced. Existing native
result-copy operations now avoid clearing/copying an already retained payload.

The audit follows these behavior categories through their producers and consumers:

| Behavior | Owning implementation and searched equivalents | Evidence |
| --- | --- | --- |
| Inactive construction and activation | LLVM frame/storage, activation provider/publication/begin and native start/compose. Search `OutgoingActivation`, `CleanupStorageClaim`, `NativeInactiveFrame`. | Constructor tests, failed-admission retry, actual generated source and production authority tests. |
| Iterative dispatch, result movement and retirement | Bray activation/run and execution, native run/state/observation. Search `retained_payload`, `completed_offset`, `CommitAwaitedCompletion`, `FRAME_COMPLETION_MOVE_SYMBOL`. | 176 runtime tests, six allocation-denied deep chains, actual source cancellation/panic/task results and exact native movement counts. |
| Concrete/imported metadata and ABI | Existing codegen symbols, native layouts, runtime role catalog, artifact declaration/link-map checks. Search `FrameFactory`, `CleanupStorage`, `MoveBeforeStart`, `NativeProtectedFrame`, `root_frame_adapter`. | 14 constructor, 10 native-import and 15 target ABI tests. Removed names have no remaining implementation consumers. |
| Anonymous callable identity and returned borrows | Bound call resolution feeds the existing result-dependency component inference, guarantees, storage flow and lowering. Search `anonymous_body`, `anonymous_result_dependencies`, `ResultDependencyOwner`, `parameter_root`. | Existing dependency/publication/returned-value suites and production contract tests. Shared parameter-root conversion replaces the separate publication interpretation. Keys share immutable identity data. |
| Cleanup execution requirements | Existing concrete lifecycle closure publishes requirements through helper mappings to observation metadata. Search `execution_requirements`, `lane_bits`, `frame_operation_dependencies`. | Native observation lane/retirement coverage and generated source task fixture. No downstream effect reanalysis. |
| MIR storage and persisted operations | Existing remap, walker, validator, encoder/decoder and inspection handlers consume the same completion storage identity. Search `ComposeAwaitedFrame`, `CommitAwaitedCompletion`. | MIR, lowering and interface tests plus development/imported-release execution. Format remains version 1. |
| Source formatting and artifact execution | Existing formatter list rules and native-product emitter. Search `TrailingComma`, `TypeExpression`, `requires_linked_debug_companion`, `emit_executable_with_configuration`. | 40 formatter tests and source execution smoke in both configurations. BuildConfiguration remains the debug-companion policy owner. |
| Report and failure ownership | Existing outgoing records, report transfer and producer-unwind boundary. Search `reserve_from`, `transfer_records`, `catch_unwind`, `assert_native_abort`. | Production report contracts, native panic/unwind fixture, callback invariant abort and runtime report tests. |

Public ABI changes are propagated through descriptors, catalogs, adapters, codegen
and native fixtures. New runtime test support is test-only. The run submodules are
organized by transition responsibility. Numeric ABI offsets/types use existing
layout and conversion helpers. User C formatting is preserved. Rust structural
checks report existing size warnings, which are not suppressed. No new recoverable
internal failure category, compatibility alias or fallback was added.

Relative to checkpoint `68bf049e`, the batch currently changes 144 files, with
approximately +1,886 production lines, +1,546 test/fixture lines and +722 tracked
handoff lines. These are physical net line counts with inline test modules counted
separately. The production growth implements the Bray driver and its admitted
native integration while deleting the old Rust frame owner. It is not a claim
that rearrangement or formatting reduces semantic complexity.

The enabled selected-scope native regression still reaches
`MissingRole(CleanupIncidentTransfer)`. Its production binding requires retained
error quiescence in BRA-505, then BRA-619 integration. It remains outstanding in
this authorized stack. The earlier full compilation run was not wholly green:
its stale ABI expectations and production authority findings have passing focused
replacements, while that selected-scope failure remains assigned to these next
steps. No test is disabled and no immediate-disposal binding substitutes for it.

### Cost evidence

On x86_64 Windows, the async value fixture builds in 0.934s development / 1.147s
release and produces executables of 2,338,816 / 1,495,552 bytes. The imported task
fixture builds in 1.938s / 2.683s and produces 2,566,144 / 1,663,488 bytes. These
are single warm runs including emission/linking, not comparative parity claims.

Retained optimized IR in `target/bra-620-authority/bra-561-accepted-ir/` contains
six concrete async definitions across two units. Their admitted frame sizes are
792, 792, 800, 1672, 1680 and 4368 bytes, all aligned to 8. Each specialization has
one 48-byte storage descriptor, 144-byte activation descriptor and 16-byte factory
on this target. The two erased implementations retain distinct concrete factories.
The recursive body has one descriptor independent of runtime depth. Simple-frame
storage remains a downstream BRA-540/BRA-542 cost concern, not a Rust/C++ parity
claim. No descriptor vector or per-await Box/Arc is constructed by the driver.
The 256-transition scheduling budget follows the composed execution design and
returns only at callback boundaries, without adding source cancellation points.

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


## Inactive failure resolution checkpoint

Generated failure resolution now invokes the existing cancellation entry. Calling
ordinary resume here could execute the source body of an inactive frame instead
of resolving its initialized captures. The selected entry returns cleanup progress
unchanged, including suspension. Both parent and child emitted callbacks are
covered, including LLVM's inlining of the cancellation flag store.

All ten host-emission tests pass. Rustfmt, mutating `cargo xtask style`, and
`cargo xtask format` pass. The audit re-read AGENTS.md and searched lifecycle
resolution, cancellation selection and generated callback signatures. Reuse the
existing checked cancellation path rather than adding another cleanup emitter.
This checkpoint is the compiler fix only. Logs are `bra-561-cancellation-hosts.log`,
`bra-561-cancellation-style.log`, and `bra-561-cancellation-format.log` under the
shared target's `bra-620-authority` directory.

## Current admission and dispatch work

The uncommitted callback boundary has passed its production body checks and
exact-pointer authority tests. It returns explicit progress and remaining admitted
records, restores the resident header before calling native code, then reloads it
to retain callback changes. Raw header take/restore helpers stay local to that
trusted implementation. A discarded borrowed-dispatcher prototype is removed.

The uncommitted native activation entry does not yet pass its body checks. Pool
mutation invalidates its raw token-slot witnesses. Do not publish it as a working
boundary or exclude it from the production audit. Its native smoke checks have
not run because the contract check failed first.

A test-only immutable admission value passes against the actual production pool
and `begin` declarations. The issued record survives pool mutation. Arbitrary,
identically reconstructed and changed-origin records fail the required witness.
This is compiler evidence, not a proof of the complete native protocol. The native
caller must retain an exclusive token slot, disjoint from mutated pool metadata
and frame-header storage, and its descriptor provider throughout the call. The
proposed issuer remains local to the entry function and never returns that record
to a caller. No production authority module has been added.

Automatic approval review rejected that production change twice, including after
the test-only evidence, and explicitly requires user approval of the exact design.
An approval question is pending. Do not retry or bypass that rejection without new
authority. Test evidence is `bra-561-admission-proof.log`. Continue unaffected work
while waiting. The current raw entry implementation remains incomplete in the
working tree. The old Rust graph, two-word Future ABI and completion backing still
execute, and BRA-561 remains incomplete.

## Native callback lock checkpoint

Task admission now returns the validated initial execution state alongside its
frame. Run construction reuses it instead of calling the producer twice, with the
second call previously occurring under the task-table lock. Terminal removal and
failed startup release frame, result and report owners after unlocking that table.
No additional retained field, allocation or wrapper type is introduced.

All 71 native runtime tests pass, including exact initial-query count, an unlocked
producer callback and an unlocked report-release provider. Rustfmt, mutating
`cargo xtask style` and `cargo xtask format` pass. Logs are
`bra-561-retirement-native.log`, `bra-561-retirement-rustfmt.log`,
`bra-561-retirement-style.log` and `bra-561-retirement-format.log` in the shared
target's `bra-620-authority` directory.

The audit re-read AGENTS.md and checked all four changed Rust files. Searches
covered frame construction, state-query consumers and task removal across native
execution, runtime shutdown and the scheduler. Runtime shutdown already moves its
table out before disposal. Observation retains a separate owner while replacing
its table entry. Scheduler registration retains a weak scheduler wake and does not
own a native provider callback. The old NativeRun remains scheduled for removal
as part of the activation replacement, rather than being split now for size alone.

The six-file unfinished admission/dispatch batch is preserved in Git stash
`25a3d9dd02f308d64dd3b97b0802b25da90f71b1`, including its two new Bray modules.
It was temporarily removed to verify this independent batch against the passing
bootstrap. Restore it before resuming admission work. Its previously recorded
contract failures and pending approval remain unresolved. Neither this checkpoint
nor the stash completes BRA-561.

## Restored work and remaining driver transitions

After checkpoint `4855365c`, the six-file admission/dispatch batch was restored
from stash `25a3d9dd02f308d64dd3b97b0802b25da90f71b1`. The stash is retained as a
recovery copy. Its native entry is still unverified and the exact admission design
still awaits the approval requested above. Do not infer completion from either
its presence in the working tree or the independently passing runtime tests.

The next driver batch must switch these existing transitions together:

| Existing operation | Replacement obligation |
| --- | --- |
| NativeRun compose | Validate complete inactive token and combined lane/attachment requirements before consumption. Attach within the run's admitted header chain. |
| NativeRun step and suspend_activation | Invoke outside locks and without a live mutable header borrow. Commit progress, then enter the child or return the run's actual wait. |
| NativeRun execution_at | Query the immutable producer state outside locks, combine retained parent requirements, and reuse that result downstream. |
| begin_pending_cleanup and finish_activation | Keep the parent's pending outcome while draining an attached child. Never overwrite it with the cleanup continuation. |
| broadcast_pending_frames | Traverse initialized obligations with one admitted iterative queue. Do not resume, resolve or destroy while broadcasting. |
| cleanup_activation and release_chain | Resume the same cleanup driver after suspension. Preserve all reports before retiring each owner. |
| resolve_completion | Move once into the admitted caller destination before detaching and releasing the child. Retain only storage with a remaining proven user. |

This mapping also exposes state queries still invoked under NativeRun.current in
`suspend_activation` and uncached `execution_at`. The initial-state checkpoint
fixes task admission only. Eliminate these remaining callback-under-lock paths
when replacing NativeRun. Do not claim that all callback lock obligations are
already satisfied or add another permanent descriptor owner to repair this soon
to be removed graph. The generated immutable descriptor and the admitted header
are the intended owners.

Existing callback operations 0 through 3 return explicit progress while retaining
the frame. Operations 4 and 5 move results and destroy storage. They need distinct
ownership commits and must not be routed through the retaining dispatch helper.
The current native callback bridge contains producer unwind and preserves any
report published before it. The new driver must carry its remaining admitted
records back into the same header after every callback, including reentrant child
attachment. None of this mapping is new completion evidence.

## Native admission proof and alternative (2026-10-16)

The user requested proof against actual callers and every return path before
continuing. Repo-wide searches find no production caller of ActivationEntry yet.
Its only current native caller is `check_activation_publication` in
`xtask/fixtures/runtime-bootstrap-smoke.c`. Generated constructors call
ActivationPublication, which is a different operation. Production NativeRun still
consumes the old two-word NativeInactiveFrame through `into_protected` before
checking all local/composed lane requirements. That path cannot establish the new
failed-activation-retains-owner contract and must be replaced, not cited as proof.

The immutable address-record proposal is withdrawn. Its anti-forgery test did
not prove the native lifetime promises, and its preservation mechanism is no
longer needed. The alternative reads the input slot once before taking the pool
lock. `begin` checks producer and exact entry authority at its own boundary,
reads the immutable descriptor, then locks and validates the resident lease.
It never reads or writes the input slot after mutation. Native callers consume
their own token only after a nonzero result. No token address is reconstructed,
no token-slot access witness is reissued, and no new authority record is added.
The old `resident` borrowed-header export is folded into `begin`, which returns
only the activated address and keeps the checked pool borrow local.

Current caller proof:

| Obligation | Actual implementation evidence |
| --- | --- |
| Token lifetime and exclusivity | The fixture passes a local InactiveActivation by address to a synchronous call. No callback or thread receives that slot. ActivationEntry performs its sole read before locking and never writes it. |
| Disjointness | Token and descriptor are caller stack objects. Pool metadata and frame bytes occupy separately allocated live RawBuffer storage. Native entry never publishes the token-slot address into that pool. |
| Provider lifetime | The fixture's const descriptor and its code remain live until activation, dispatch and storage release finish. Generated publication uses a concrete module constant, but production activation integration still needs its own retained-provider proof. |
| Origin and lane | activate_fixture requires the current live nonzero attachment identity and admits only its fixture's unrestricted state. This proves the fixture case, not general production lane composition. Exact-origin and exact-slot source requirements remain on the public internal entry declaration. |
| Resident identity | claim_address checks layout, address arithmetic, domain, shape, slot, epoch, backing address and registered descriptor identity before Inactive becomes Dispatched. Numeric token fields alone cannot authorize memory access. |
| Rejection ownership | Every ordinary zero result precedes the Dispatched transition. The native caller compares the token bytes before and after entry and leaves them unchanged on rejection. |
| Successful consumption | After the pool transition, begin updates the checked header without calling producer code. The caller clears its own token only for a nonzero result. A copied token cannot dispatch the same resident slot again. |
| Allocation and reentry | Entry only locks, reads established metadata and changes resident/header state. It allocates no storage and invokes no producer callback while holding the pool lock. Native descriptor state is checked before entry. |

Return-path inventory: the fixture rejects empty tokens, zero/wrong origin and
unsupported state requirements before entry. Native entry forwards begin's result.
Layout, arithmetic, resident identity and state rejection return zero without
consumption. Success has no further fallible operation before returning its
address. An impossible published-header phase/link state aborts as an internal
invariant failure, rather than returning a recoverable rejection after consumption.
This is not a rollback claim for compiler/runtime bugs or process termination.

Native cases cover bad epoch/address/slot, descriptor mismatch, overlapping layout,
wrong origin, successful transfer, repeated transfer and duplicate tokens. The
fixture denies allocations through these transitions and retains its descriptor
through dispatch and release. Production-body and native acceptance results are
pending. Full production token, lane and provider integration remains BRA-561 work.


The alternative passes all four exact-authority/publication/dispatch tests and the
complete production storage/activation body audit. Header initialization remains
under the pool lock in a checked local operation rooted in the ReportPool borrow.
This avoids copying the complete header and does not recreate a guard witness.
The discarded experiment with raw header read/write after unlocking is removed.
The proposed token-address record and its test-only fixture are also removed.
The user's conditional authorization and this replacement supersede the earlier
pending approval request for that withdrawn design. The actual production callers still require integration.


## Admission and callback proof checkpoint

The native smoke suite passes with the replacement admission design. It also
passes Rustfmt, mutating `cargo xtask style`, and `cargo xtask format`. The full
bootstrap compiles against the unchanged ABI 1.0. Dispatch accepts a typed raw
callback address with the existing exact callable-address requirement. Its source
test rejects absent proof and proof for a different address. No mapper fallback,
ABI version, language feature or compiler authority relaxation was introduced.

The actual fixture initially lacked a retained thread attachment. Admission
correctly rejected its zero origin. The caller now acquires attachment before
storage admission and allocation denial. Every exit after acquisition releases
any remaining storage while the local descriptor is alive, then leaves the
attachment. Allocation denial is reset before this common cleanup. Rejected
attachment has no acquired resource to release. The final return-path cases also
cover nonzero foreign origin and unsupported state requirements.

This checkpoint proves the current native fixture boundary, not the unconnected
production entry paths. The next batch must replace their consuming two-word
transfer with the admitted inactive token and enforce the same rejection contract
before switching the run driver. The withdrawn immutable-address-record proposal
and its historical approval request are no longer active work.


The production callback bridge also passes the native fixture. The bootstrap-only
variant retains its existing link-map exclusion of Rust runtime archives. A second
variant links the existing native archives and passes the actual
`bray_runtime_activation_callback` address through Bray dispatch. Both use the
same provider-lifetime, reentry and allocation-denial cases. Resume, cancellation,
broadcast and suspended resolution reach the selected descriptor callbacks.
Invalid retaining operation leaves the destination untouched. The complete smoke
suite, Rustfmt, mutating style and Bray format pass. Logs use the
`bra-561-production-callback-` prefix in the shared target's `bra-620-authority`.

The admission checkpoint `b2ebfcd1` adds 317 production lines net and 296 test lines
net, plus 83 handoff lines. Production growth supplies checked native admission
and retaining callback dispatch. It removes the exposed resident-header helper
and the rejected address-record experiment. There is still no second permanent
activation owner. The old Rust graph remains an explicit unfinished replacement.

## Consuming retirement and fixture style checkpoint

Dispatch now distinguishes retaining operations from consuming destruction before
callback entry. The retaining branch reloads the resident header after reentry.
Destruction requires committed, detached ownership with no pending child or
incidents, transfers unused report records to the caller, and never accesses the
header after the callback. A stack copy keeps a coallocated immutable descriptor
alive for the callback's entire borrow. Callback code and output slots remain
caller-retained. ABI 1.0 remains the only supported ABI.

The native fixture executes both the C provider and actual Rust callback bridge,
with both external and frame-resident descriptors. Destruction poisons and releases
the frame. Reclaiming that exact backing verifies that dispatch wrote no header
bytes after release. The C callback also reads its descriptor after destruction.
Allocation denial remains active throughout these transitions. The source contract
regression accepts repeated retaining dispatch and rejects dispatch after consuming
retirement. All six activation contract tests, the production body audit, complete
native smoke suite, Rustfmt, mutating style and Bray formatting pass. Logs are
`bra-561-consuming-retirement-*` and `bra-561-retirement-body-audit.log` in the
shared target's `bra-620-authority` directory.

The guarded postcondition is on the named invoke function. A local lambda instead
uses an unconditional retaining contract selected before entry. Full semantic
checking currently rejects guarded execution clauses on local lambdas even though
the language reference permits them. This existing guarantee-coverage gap belongs
to open [BRA-482](https://linear.app/bray-lang/issue/BRA-482), not closed BRA-500.
This implementation does not depend on the unsupported form.

At the user's request, audit all maintained C/C++ fixtures. Preserve their bootstrap
fixture edits and expand remaining compressed declarations and control blocks.
Apply the established Allman braces, type-attached pointers and semantic paragraph
spacing to the other inconsistent fixtures. No C/C++ tokens change in this style
pass. All five changed C fixtures and all 20 C++ performance workload variants pass
native syntax checks. The C++ checks use the repository's Windows target flag
`-D_AMD64_`. The first ad hoc invocation omitted it and failed in Windows SDK
headers. No source workaround was added. Bray formatting passes afterward.

This remains a checkpoint. Production token consumers, result movement and the
iterative Bray run driver still need integration and replacement of the Rust owner.

## Result movement before task retirement

Task observation no longer retires its observed task after writing the caller's
completion destination. That retirement can fail, so doing both inside the move
callback incorrectly converted an already initialized result into transfer failure.
The existing result slot now distinguishes pending, ready and consumed states.
The existing retirement flag tracks task retirement independently. Cleanup skips
an already consumed result while still retiring the task exactly once through its
existing incident-preserving path. No second scheduler, allocation or authority
mechanism is added.

The native regression denies further outgoing admission, moves the result normally,
then observes the expected retirement failure separately. Repeated cleanup neither
revisits the transferred value nor produces another retirement incident. Existing
cancellation, written-report-then-unwind, secondary-report and panic-result ownership
regressions remain green. All 71 native runtime tests pass, followed by Rustfmt,
mutating style and Bray formatting. The first build rebuilt the bundled bootstrap.
Logs use `bra-561-result-retirement-` in the shared target's `bra-620-authority`.

Audit the owning move callback, its NativeFrame caller, generated completion-move
code, native fixture providers and task-slot destruction. Generated movement is a
load, store and return. This repair removes the native provider's fallible operation
after that commit. The shared byte-transfer function and cleanup report aggregation
remain their existing owners. This checkpoint adds 9 production lines and 6 test
lines net. Full activation/run replacement and surviving dependency acceptance
remain outstanding.

## Remaining inline fixture style

The fixture audit also covers C/C++ source embedded in Rust tests. Expand the two
native callback/cleanup caller templates and the Linux C++ exception probe into
multiline source with the same brace and paragraph style. Preserve all native
tokens and Rust format placeholders. Both native caller tests pass. The C++ probe
passes syntax checking on Windows, while its Linux-only linking test remains
platform-restricted. No production behavior changes in this fixture batch.

## Exact callback invocation and destination authority

The low-level callback previously required memory-valid arguments without proving
that descriptor, context and admitted records belonged to the same invocation.
The native execute entry also reissued destination-write facts after callback
entry using only native-entry authority. That did not express the caller's duty
to retain output slots independently of a frame that destruction may release.

Require one exact callback invocation over descriptor, context, operation, argument,
progress slot and records slot. The only production issuer is a nonescaping local
lambda in the owned dispatcher. Its two calls use fields read from that exact
header, records removed from that header, and distinct initialized local slots.
The retaining branch uses the original descriptor. Consuming retirement uses the
previously established stack copy. Returned evidence binds both report progress
and unused records to their exact output slots. A callback consumes its invocation
authority. No field construction or generic memory-validity claim supplies it.

Require exact retained native destinations at execute entry. Its local dispatch
operation carries that promise from its precondition to the post-call write facts.
Remove the former native-entry-only post-call issuer. The actual C callers own
separate lexical output objects across the entire call, including poison/release
of the frame and coallocated descriptor. Invalid operation or resolution argument
returns before callback entry and leaves both output objects unchanged. Retaining
callbacks reload the header after reentry. Destruction never accesses it afterward.
The Rust callback documentation now states matching context, initialized empty
progress, exclusive admitted records and descriptor/code lifetime explicitly.

Eight activation authority tests and the complete production body audit pass.
Negative cases substitute every callback component, replay a consumed invocation,
and substitute the frame or either native output slot. Bare writable-memory facts
do not satisfy the retained-destination contract. An initial body-audit failure
identified a local name collision, which was corrected without changing contracts.
The complete native smoke suite passes with both callback bridges and descriptor
placements, allocation denial and the invalid-resolution-argument case. Rustfmt,
mutating style, Bray format and diff whitespace checks pass. Logs use
`bra-561-callback-authority-` in the shared target's `bra-620-authority` directory.

These requirements replace incomplete boundary contracts. They add no runtime
record, language syntax, compatibility path or validation pass. This batch adds
91 production lines net and 214 test lines net. The current native caller inventory
is still the fixture. Production entry, direct-await, root, task and static-finalizer
integration remains unfinished and must establish the same contracts when switched.

The final repository-wide native-source inventory also found the small cancellation
caller embedded in the same runtime test module. Expand it with identical C tokens
and format placeholders. Its exact native regression passes, followed by Rustfmt,
mutating style, Bray formatting and the whitespace check. No behavior changes or
new test infrastructure are introduced. The native-source search also covers the
standalone fixtures, linker probe and target-probe generator.

## Inactive requirement inspection and Win64 ABI

Inspection uses the same registered lease, descriptor, layout and epoch check as
activation without changing the resident phase. Its exact-owner contract retains
both the inactive frame and the independent output slot across the metadata
callback. The pool lock and header borrow end before that callback. Producer
identity alone does not grant exclusive ownership. Invalid ordinals, unsupported
lane bits, invalid affinity and stale or mismatched leases leave the output and
inactive owner unchanged.

The production body audit and two negative authority regressions pass. The native
fixture covers external and coallocated descriptors, repeated rejected admission,
provider reentry, unchanged token bytes, untouched invalid-query destinations and
allocation denial. Both the C bridge and production native bridge pass the full
runtime artifact smoke suite.

The invalid-lane case exposed a shared Win64 foreign ABI defect. Small aggregates
were marked direct without integer-register coercion, so the second u32 field of
a C result was lost. The owning ABI classifier now uses existing coercion mappings
for 1, 2, 4 and 8 byte aggregates and the required 16 byte indirect-argument
temporary alignment. Aggregate recognition and Win64 selection have one shared
implementation. No runtime compensation or ABI version change is introduced.
The contract is documented by Microsoft's x64 calling convention:
https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention

Five ABI classification tests pass. A native C/Bray/function-pointer round trip
checks distinct values in both fields across arguments and results. Rustfmt,
mutating style, Bray formatting and the full native smoke suite pass. Logs use
`bra-561-inspection-` and `bra-561-abi-classification` in the shared target's
`bra-620-authority` directory. The retained failing smoke artifact demonstrated the
pre-fix second-field loss. It is diagnostic evidence, not a substitute for the
passing complete rebuilt smoke suite.

The maintainability audit covers every changed file and searches shared ABI
classification, aggregate recognition, lease inspection, producer reads and exact
retained-slot contracts outside the edited functions. Existing storage lookup and
coercion machinery are reused. The new inspection contract is required by the
pending production activation callers. Those callers and the iterative Bray run
driver remain unfinished. Completion consumption, cancellation before consumption
and frame/provider retirement must be switched together with the Rust owner.

## Await outcome ownership before cancellation

An await now commits the child outcome before observing a pending cancellation
request. Child panic and cancellation retain their existing outcome path. A
successful owned result becomes an ordinary initialized input temporary during
the cancellation check, so that exit disposes it through existing cleanup. The
checker publishes its cleanup shape and lowering consumes that established result.
Selected asynchronous exits retain their existing cleanup shield.

All 76 lowering tests and ten native code-generation host tests pass. The new
regression checks outcome ordering and follows the cancellation CFG to cleanup of
the exact transferred result. Rustfmt, mutating style, Bray formatting and the diff
whitespace check pass. Logs use `bra-561-await-` in the shared target's
`bra-620-authority` directory. The audit searches temporary materialization, cleanup
shape publication and cancellation observation across checking and lowering. It
extracts the existing cleanup-phase selection instead of adding a second policy.
Native acceptance of the production driver remains outstanding.

The integration audit also identifies a concrete remaining gap: async cleanup
analysis gives Future both cleanup phases, while product lifecycle selection marks
its compiler-known representation as requiring no cleanup. Inactive captured-owner
retirement must use the shared activation driver during the production switch.

## Active execution metadata and draft PR

Draft PR: https://github.com/lejmer/bray/pull/611, based on `develop`. Completion
review remains pending production driver integration.

The active metadata query reads one requested compiler state and combines it with
retained parent requirements. Origin-thread constraints preserve any main-thread
lane requirement, and conflicting origin identities reject the query. Native
outputs remain untouched on invalid ordinals, provider metadata or incompatible
origins. No allocation or pool lock spans the provider callback. The exact active
owner and independently retained output remain required even when ordinary memory
validity is already available.

Reuse the existing owned-header read between dispatch and metadata inspection.
Move split modules under thin roots, and share state validation between inactive
and active queries. The Rust run's old execution merge remains pending replacement
with the activation owner, rather than becoming a second permanent policy.

The ABI layout tests, exact-owner/output negative regression, full production
storage/activation body audit and complete native smoke pass. Native cases cover
both callback bridges, external/coallocated descriptions, allocation denial,
provider reentry, both main/origin combinations and untouched rejected outputs.
The first body audit found a local-name collision. The native product check then
found an unused capability declaration. Both were corrected at their declarations.
Rustfmt, mutating style, Bray formatting and diff whitespace checks pass. Logs use
`bra-561-active-` in the shared target's `bra-620-authority` directory.

Next connect the checked execution metadata to each native callback invocation,
then switch the production token consumers and iterative run ownership together.
Native admission, inactive Future retirement, result consumption and suspendable
cleanup remain outstanding parts of BRA-561. This checkpoint does not complete it.


## Continuation admission integration in progress

Composition carries the parent's continuation ordinal from lowering through MIR,
LLVM, imported executable templates and the native ABI. Assign the ordinal after
cleanup expansion, which may create its own suspension states. It must be the
same ordinal subsequently published by the suspension terminator.

The Bray composition boundary queries that continuation before activating the
child. Its native admission contract binds the exact parent, inactive slot,
continuation and origin. Metadata callbacks preserve these independently retained
owners and their attachment. Successful resident admission connects the returned
child header. Token bytes remain unchanged, and the caller consumes its inactive
owner only on success. No runtime allocation is introduced by this transition.

The active snapshot now carries its exact header address and requested state to
the callback. The generic native callback still only contains producer unwind.
An unused scoped-dispatch implementation was removed. The future production run
bridge must install the snapshot's execution context after lane admission.

For the current native caller, retain its single immutable descriptor in the
existing shared terminal record, so its continuation remains queryable while a
source callback temporarily holds the Rust frame wrapper. This relocates the
existing description without another allocation or descriptor copy. NativeFrame,
NativeActivation and their backing ownership still require replacement.

The focused authority audit previously omitted nested callable units. Extend its
existing traversal to include nested lambdas. The real native product exposed
missing access evidence in the composition connector. Issue that evidence at
each access to the two distinct admitted headers, using the local connector only.
No exported helper grants ownership to an arbitrary reconstructed header.

The expanded production-body audit, five exact-pairing cases, async lowering,
all 76 lowering tests and 190 package-interface tests pass. ABI layout tests and
all 173 runtime tests pass. Rustfmt, mutating style and Bray formatting pass.
Native smoke first stopped on a C fixture pointer conversion, which was corrected
with the explicit cast used by neighboring fixtures. The full native artifact smoke rerun passes, including both callback bridges, allocation denial and recursive entry. The checkpoint audit covers continuation serialization, immutable metadata ownership, exact callback pairing and fixture formatting. Logs use `bra-561-composition-` in the shared
target's `bra-620-authority` directory.

The remaining driver switch must address these obligations together:

- Replace the two-word inactive adapter with the admitted lease and immutable
  description. Switch roots, starts, direct awaits, static finalizers and native
  observation factories, then remove MoveBeforeStart and the Rust activation graph.
- Keep the pending parent outcome while its child drains. Broadcast before
  resolving, preserve cleanup shields across suspension and dispatch callbacks
  outside locks. Query metadata for the actual requested continuation.
- Distinguish completed but unconsumed results from transferred results. A failed
  parent cannot silently destroy backing containing an unconsumed owned result.
  Completion movers must leave their input intact on failure before commit.
- Retain frame backing through the generated completion load and any remaining
  dependent users. A copied payload address is not a storage lifetime proof.
- Wire inactive Future capture retirement through the shared driver. Its current
  product lifecycle selection still incorrectly treats Future as trivial.
- Native providers reserve unwind-report backing at owner formation. Source
  factories and their callbacks must not acquire it during direct composition.

These are BRA-561 acceptance obligations, not new issues or completed evidence.
BRA-505 and BRA-619 remain next in the authorized sequence after this PR's
implementation, verification and independent review.


## Current checkpoint: completed results and pending cleanup

Continue BRA-561, then BRA-505, then BRA-619. Checkpoints do not end the task.
PR #611 remains a draft. Independent completion review has not begun.

The completion mover now requires the exact frame and disjoint admitted destination.
Failure leaves the source result owned and retryable. Success commits transfer once.
The native fixture exercises failure followed by successful retry with allocation
forbidden, through both callback bridges. The authority regression rejects missing,
wrong-frame and wrong-destination witnesses against production declarations.

Completed source results with lifecycle obligations have an explicit MIR cleanup
entry. Its storage aliases the existing result field rather than adding payload
bytes. It reuses the existing abnormal cleanup machine, with broadcast separate
from resolution. Successful movement marks the source control state consumed.
Resolution skips already consumed source state. The executable template preserves
this state and storage at format version 1. ABI versions remain unchanged.

The checked cleanup description records demanded result-cleanup execution
requirements. A blocking destructor does not impose its lane on the producer body.
Source predicates and imported phase requirements both contribute. Only async
bodies demand this additional analysis. The same terminal-state calculation serves
resume, failure resolution and movement.

Bray pending-outcome helpers preserve the original source outcome while lifecycle
resolution suspends. They append admitted cleanup reports through the existing
report-record implementation. These helpers still need the production driver.

Completed checkpoint evidence: 44 IR, 39 lowering-library and 190 interface
unit tests pass. All 78 compilation-lowering tests, completed-result LLVM emission,
the complete production storage/activation body audit and exact destination tests
pass. Rustfmt uses installed stable because toolchain 1.98.1 lacks that component.
Mutating style and Bray formatting pass. Logs are in the shared target directory
`target/bra-620-authority/`, prefixed `bra-561-result-`. The final audit removed
repeated terminal-state calculations and unnecessary synchronous result analysis.
Its focused checks, mutating style, Bray formatting and full native artifact smoke pass, including allocation denial and completion-move failure followed by retry through both callback bridges.

Concrete realization now derives completed-result cleanup requirements from the
existing generated lifecycle dependencies and checked invocation signatures. It
replaces the partial source-only result analysis. Tagged cleanup continuations
retain those requirements through imported templates without applying them to the
producer body. The temporary MIR content-identity cache uses allocation identity,
so two specialized descriptors cannot reuse a digest through a template-local ID.

Direct and nested generic results, all three imported async frames, completed-result
LLVM emission, 44 IR, 39 lowering-library and 190 interface tests pass. All 78
compilation-lowering tests and the concrete partition identity regression pass.
Rustfmt and mutating style pass. Native artifact smoke passes; its log is
`target/bra-620-authority/bra-561-concrete-native.log`. Other current logs use the
`bra-561-concrete-` and `bra-561-generic-` prefixes in that directory.

The previous result-retention checkpoint `c5ffa8d4` received an independent review
with no findings: https://github.com/lejmer/bray/pull/611#pullrequestreview-5548809104.
That was an intermediate review, not acceptance of the unfinished issue.

Still verify native execution of completed-result abandonment. During the driver
switch, account for cancellation-entry execution requirements separately from
normal source execution: an unconsumed generic capture may require a different
cleanup lane even when the ordinary body transfers it without destruction.

Then finish the production driver switch listed above: exact per-callback execution
context, one iterative Bray parent/child chain, destination-based completion,
inactive Future capture disposal and all native consumers. Remove the superseded
Rust graph and allocations as consumers switch. No completion claim or PR readiness
is justified until these obligations and the independent review are complete.

## Dispatch admission checkpoint

The concrete completed-result checkpoint `9f95e358` received an independent
review with no findings:
https://github.com/lejmer/bray/pull/611#pullrequestreview-5549142871.
This remains intermediate evidence, not issue completion.

Every callback dispatch now requires the caller's exact admission callback as well
as its invocation callback. Query the immutable current execution snapshot once,
then ask the native caller whether the actually selected lane satisfies it. A
non-success status precedes every header, report, result and record mutation,
including consuming retirement. The native bridge preserves the specific status.
`PENDING` means reschedule on a compatible lane, not failed owner admission.

The shared native-state conversion replaces its previous copy in NativeFrame.
It uses static lane slices and empty dependency/storage views without allocation.
The native scheduler's existing registration and selected-lane comparison own the
admission decision. No copied descriptor, header field or callback address grants
activation ownership. An unconditional admission callback was rejected and was
not applied.

The authority matrix includes both exact callable addresses, wrong or absent
addresses, retaining callbacks, admitted retirement and deferred retirement.
All ten activation authority tests, the production-body audit, 173 runtime tests
and 20 ABI tests pass. The native-root regression exercises successful cooperative
admission and blocking-lane deferral through the actual task registration.
The C fixture checks all six deferred operations, untouched result destinations,
header and output slots, one metadata query and no invocation or allocation.
Composition also covers a parent whose lifecycle cleanup has resumed.
Rustfmt, mutating structural style, Bray formatting and the full native artifact check pass. The first native check exposed an unnecessary readiness ABI role. It was removed: production callers pass the native callback directly. Current logs use
`target/bra-620-authority/bra-561-readiness-`.

Continue with the production driver and consumer replacement above. The owning
Bray chain must retain source progress while child cleanup suspends, release no
backing before result movement, and preserve exact callback context through
reentry. Operation-specific requirements remain an acceptance obligation:
completion movement must not acquire its result destructor's lane, and inactive
capture cleanup must use the checked cancellation path. Do not solve this with
blanket unions of initialized types or a second cleanup scheduler.

Audit: the changed behavior categories are dispatch admission, callback-address contracts, status-based ownership retention, checked native-state projection, and cleanup-continuation composition. Repository searches found the existing lane comparison and state conversion, which are reused. All raw dispatch call sites supply admission, and no version or compatibility path was added. This checkpoint adds about 120 net production lines and 130 net test lines. The remaining driver replaces the Rust graph and will consume the new admission callback.


## Active operation metadata batch

The standing objective remains BRA-561, then BRA-505, then BRA-619, including
reviewed PRs and pushed fixes. Do not stop at a checkpoint. HEAD 7b461268 has a
verified clean independent review, GitHub review 5549809993 on PR #611.

The current uncommitted batch replaces completed-result-only requirement
propagation with concrete callback-entry requirements. It reuses checked MIR
cancellation branches, stops at suspension, and closes the existing lifecycle
helper graph. Resume, cancellation and broadcast entries share the runtime
model's eight immutable lane sets. Metadata queries take the exact operation
and argument. Native failure resolution selects cancellation, while ordinary
source resolution, result movement and empty backing release execute no source
cleanup. A suspended cancellation continuation selects its cancellation entry
from the existing frame cancellation byte. No ABI or artifact version changes.

The first batch passed generic captured/result destructor, unentered body lane,
production body/contract, ten activation authority and imported generic tests.
The native bootstrap build then exposed unqualified Header/Progress names in
the new advance helper. Those names are now qualified. The later operation
argument and shared-set changes require the running final validation batch,
style, native smoke and independent review before publication. Logs use
bra-561-operation- in the shared target's bra-620-authority directory.

The production driver switch remains outstanding. In particular, retirement
must admit before unlinking a child, preserve the chain on deferral, and query
only once. The admitted inactive token and native consumers still need their
switch, and the replaced Rust activation graph must be removed. Do not treat
this metadata checkpoint as BRA-561 completion.

The focused activation-body audit is
production_storage_boundaries_prove_their_declared_memory_effects. The broader
host-nested test covers selected host entry sources, not every activation body.
The exact activation audit exposed unproved contracts in the unused advance
prototype, so that uncommitted prototype was removed. Integrate progress commit
with the production driver's exact operation branches instead of retaining an
unused wrapper that obscures which authority each operation needs.

Retirement design to carry into the driver: the native dispatch claim excludes
same-run dispatch/reentry while a producer callback runs. Therefore admit and
consume a committed child while its parent link remains intact, then clear the
parent link and advance the root cursor using retained parent identity, without
reading the destroyed child. Deferral changes no link. This needs an explicit
whole-chain owner contract and native enforcement of the same-run exclusion,
including composition during destruction. The current exact-header owned
predicate alone must not grant authority over arbitrary parent/root addresses.
Do not apply this relaxation to generic destruction before those caller and
return-path obligations are implemented and tested. It avoids a second readiness
query and a second retired graph, but is not yet implementation evidence.


## Operation metadata verification and driver continuation

The operation metadata batch passes 173 runtime, 20 runtime ABI, 44 MIR,
190 package-interface and five runtime-model tests. The exact production storage
contract audit passes. The LLVM native aggregate ABI regression passes after
correcting a test-only ambiguous conversion. The full native runtime-artifact
smoke test passes, including the operation/argument boundary checks. Logs are
`bra-561-operation-runtime-tests.log`, `-storage-tests.log`, `-abi-test.log` and
`-native-smoke.log` in the shared target's `bra-620-authority` directory.

The native invocation bridge now installs the admitted snapshot's continuation
under the existing task-context guard. It retains the actually selected lane
without repeating admission. Both focused callback tests pass, including nested entry and unwind restoration
with further admission denied. The imported generic requirement regression also
passes. Mutating style, Rust formatting and Bray formatting pass after the tests.
The native smoke result above precedes the scoped-context change, whose direct
production callback regression supplies the additional evidence.

The driver switch should give composition the parent's typed completion storage
before suspending. The child moves its result there, retires on an admitted lane,
and only then lets the parent resume. This avoids requiring a synchronous parent
callback to suspend midway through consuming a child whose destruction needs a
different lane. Lowering should use existing MIR temporary storage, retaining it
in the already-admitted parent frame. The parent takes the transferred outcome
before its existing cancellation checkpoint. No separate retired-child list or
new source language feature is needed. Preserve this as the implementation
hypothesis, and verify the abnormal and suspended-cleanup paths before claiming
it is implemented.

The source Future can retain its existing two-pointer protected representation,
now pointing to owned context and immutable descriptor. At native transfer the
compiler reads the live header's lease into the existing full inactive token.
The native token must retain its epoch. Do not replace it with an unchecked
address that could activate a reused slot. Replace the old adapter callback and
its generated descriptor reconstruction across roots, starts, observations and
inactive disposal in the same switch.


Audit for this checkpoint: re-read AGENTS.md and the conventions. Trace the changed
behavior through concrete helper closure, callback-entry CFG edges, MIR metadata,
serialization, LLVM callbacks, native provider signatures and Bray query callers.
Search for the replaced completion-only analysis and old query signatures. The
shared static lane sets and existing graph traversal replace local allocation and
repeated closure work. Native context installation reuses the existing restoration
guard and trusts admission downstream. The production driver remains unfinished.


## Uncommitted driver replacement, 2026-10-16

Baseline remains pushed 68bf049e on PR #611. This replacement batch is unfinished
and has not passed the production audit. Earlier checkpoint results do not validate
it. Run format and style after tests pass, as requested.

The source Future retains two pointers. Native transfer carries the full lease and
immutable descriptor. MIR reserves the parent result slot before suspension. Bray
contains the iterative active-chain transitions. Rust NativeRun now holds one run
claim, callback scope and execution snapshot instead of an owning activation graph.
Task registration precedes activation. Composition checks combined capabilities
before consuming its inactive child. Observation abandonment uses join suspension.
Root retirement retains its registered task until release.

Removed the generated MoveBeforeStart adapter, its operation metadata and redundant
helper projection. Inactive moves transfer the existing representation. Known and
erased awaits use the same completion operation. The old NativeProtectedFrame and
native/frame.rs remain only for test migration and must still be removed.

Current evidence:
- Runtime and adapter production compilation passed during the batch. LLVM field
  index errors were corrected afterward. The whole batch needs another Rust check.
- The production authority audit now includes provider, completion, all run modules,
  and contract-clause binding diagnostics. It currently reports ten failures.
- Provider initialization passes after moving outgoing records by value. No new
  unconditional write-authority grant was added.
- Passing fresh address results directly removes every witness-copy error. Bray
  has no consume expression for call arguments. An attempted use was removed.
- Parenthesizing the operation-authority disjunction preserves its existing meaning
  and avoids a recovered declaration that lost following contract conditions.
- The actual Cursor declarations reject fabricated fields, exact reconstruction,
  and pairing a member with another root. A selected cursor is accepted.
- CatastrophicAbort has no returning storage effect. This follows the specification
  and analysis/execution.rs's explicit Divergence edge without cleanup. Its pure
  and total properties remain unchanged. The source regression accepts a returning
  branch and rejects an unknown foreign call without preservation.
- Advance now returns scheduling scalars. Its old successful return contained an
  empty report, and its admission-failure return also contained an empty report.
  This removes the spurious escaping local-cursor dependency. Actual reports still
  transfer into the header before a successful return.
- Explicit destination branch assignment retains a symbolic value at the join and
  passes the completion-destination obligation. The equivalent if-expression lost
  that value in pending_results merging. Do not infer a new language requirement.

Logs are under the shared target/bra-620-authority directory. Current production
results are in bra-561-driver-proof.log. The minimal conditional-preservation probe
uses bra-561-conditional-proof.log. The temporary BRAY_TRACE_AUTHORITY trace has been removed from the checker.

Automatic approval review rejected two proposed local preservation contracts in
run/advance.bray. They are NOT applied. The user has been asked whether to apply
and test them. Each requires selected cursor authority and the existing report
callback proof. One retains both while reading header bytes. The other retains
selection while transferring report records. Inspection shows read invokes no
provider callback, report retention does not retire activation backing, and native
dispatch holds DispatchClaim throughout. Native lifetime, allocation and reentry
checks are still required. Do not bypass the pending decision or claim these proofs
are already validated. Continue independent work while waiting.

Remaining acceptance-critical work:
- Resolve the ten production authority failures in advance, abandonment, completion,
  retirement and the root traversal path. Preserve exact identities and every
  surviving ancestor. Never grant authority from copied fields or phase flags.
- Finish execution snapshots without duplicate metadata work and release roots on
  their required lane and affinity. Do not fabricate lane authority.
- Implement inactive Future cleanup through the same iterative driver, including
  nested disposal and start/root/static-finalizer admission failures. Failed
  activation retains ownership, but its callers still need disposal paths.
- Supply checked requirements for native observation value cleanup and resolve
  retirement deferral before committed result movement can become unreportable.
- Migrate C/Rust fixtures for full tokens, parent result slots, Execution inspection,
  composition admission and incident outputs. Remove the obsolete native adapter
  and test-only frame after retaining its coverage.
- Verify cancellation broadcast, suspended cleanup, result dependencies, report
  retirement, allocation denial, callback reentry and native stack/allocation cost.
- Finish the mechanical audit, tests, style, formatting, commit, push and PR review.

The active sequence remains BRA-561, BRA-505, BRA-619. The pending approval applies
only to the two proposed preservation wrappers, not the remaining independent work.

The minimal conditional preservation regression independently reproduces a merge
bug. Its explicit-preservation case fails, even though both branch paths establish
the predicate. The required observation names Borrowed(Constructed(Input(owner),
OwnedTarget = Unknown)), while the retained predicate names Borrowed(Input(owner)).
The existing merge substitutes borrowed observations through identity-only mappings
and loses the merged component snapshot. A proposed bounded correction adds the
complete borrowed-observation mapping only when old and merged snapshots have the
same known, nonliteral owner identity. It leaves identity mappings, ambiguous-alias
handling and branch-fact intersection unchanged. Automatic approval review rejected
both the initial and this narrower proposal, and requires explicit approval of the
proof rule. The user has been asked. No merge implementation change is applied.
Run conditional_calls_rejoin_preserved_borrowed_pointer_records together with
borrowed_invariants_survive_loops_only_when_every_back_edge_preserves_them and
equal_scalar_contents_do_not_identify_borrowed_predicate_subjects after approval.


### ABI verification while authority approvals are pending

The production runtime/adapter and LLVM check now passes:
`cargo check -p bray-runtime-adapter -p bray-codegen-llvm --no-default-features`.
The existing Backing dead-code warning remains in the incomplete migration.

Runtime ABI tests passed, 20 tests. Updated LLVM tests no longer expect the old
two-word inactive token or two-argument state query. Native ABI tests passed,
15 tests, covering the 48-byte token and 40-byte lease offset on every supported
target, plus byval attributes at both SysV declarations and calls. Removed the
obsolete LLVM imports reported by that run. No ABI version was changed.

The C bootstrap fixture now uses Execution inspection output, the composition
admission callback, and the separate incident output on every dispatch path.
Its existing lifetime, failure and deferral checks remain. Clang syntax checking
with incompatible-pointer-types promoted to an error passed. This is not a
native execution result. The runtime-smoke.rs provider and runtime unit-test
providers still require migration from NativeProtectedFrame and MoveBeforeStart.

Logs in target/bra-620-authority:
- bra-561-abi-tests.log
- bra-561-native-abi-tests.log
- bra-561-c-fixture-syntax.log
- bra-561-production-check.log

The authority merge fix and the two local preservation contracts remain unapplied
pending the already-requested explicit approvals. These checks do not validate
those changes or remove the ten production authority failures. The batch remains
uncommitted and unreviewed. No formatting/style command was run, following the
user's request to wait until tests pass. No build or review process remains active.


### Preservation proof correction, 2026-10-16

This section supersedes the earlier claim that report retention only updates
records and the proposed negative-list test for a known merge identity. Neither
rejected implementation has been applied.

The prior approval requests were premature. Inspection plus a positive failing
regression established a problem, not the soundness of either repair.

For the read wrapper, `run.read` reaches `execution.read`, a local authorization
projection and `std.memory.read<Header>`. It calls no provider. It writes neither
the cursor nor the incoming progress. The report is moved as a whole to the return
tuple. Its owner and callback provider remain retained. This proves preservation
on normal return under the existing selected-cursor and owned-report premises.

For acceptance, the return-path inventory in `execution.accept_progress` is:
- Invalid progress/operation and invalid phase abort, with no normal return.
- Broadcast updates the local header and may retain a report.
- Suspension updates the local continuation and possibly the resolving phase.
- Running completion moves progress into the local header and starts cleanup.
- Cleanup completion may retain a report and marks the local header terminal.

Only report retention crosses a provider call. `retain_report` exchanges the
reserved record, then calls `release_primary` on the previous primary. The old
primary need not be empty: Rust admission installs unused panic backing there.
Therefore a proof based on absence of callbacks is false. The selected-frame
argument must instead retain the native run claim across that callback.

`NativeRun.drive` keeps DispatchClaim through the entire Bray step. The native
source callback's CallbackScope ends inside activation/callback.rs before the
Bray call returns and accepts progress. Consequently reentrant composition has
no active callback scope. `NativeRuntime.destroy_task` accepts only a Terminal
slot, while this run remains Started through task.resume. Recursive dispatch
cannot acquire DispatchClaim. Report disposal owns its separate reservation and
message provider, not the frame lease already owned by the run. Direct native
release of that lease would violate the existing ownership contract, not be an
operation authorized by report disposal. Other-run activity cannot consume this
run's lease. No preservation is promised after an abort or abnormal return.
Native lifetime/reentry acceptance remains required to validate the implementation
against this argument, including the provider-call path, not just empty records.

For merging, let P be any predicate, b a reachable predecessor, V_b the current
borrowed observation of one place, and J its joined observation. The permissible
renaming is P(V_b) to P(J) within that predecessor only when V_b and J refer to the
same established owner. Keep P(J) only in the intersection of predecessor facts.
Every incoming concrete state then satisfies P of the actual selected owner.
This is a join operation, not a rule preserving P across mutation. Invalidation
must already remove stale observations before the join. Existing flow/invalidation
and state.invalidate_contents/invalidate_referents own that obligation.

`owner_identity()` can return Constructed(Unknown, fields), so excluding Literal,
Boolean and Unknown does not establish the identity premise. Withdraw that exact
proposal. A narrower candidate uses the existing observation_path representation
to require a known identity path, alongside exact owner equality. It changes only
whole borrowed-observation substitutions, leaves captured entry guards untouched,
uses existing ambiguous-substitution rejection, and retains branch intersection.
Allocation owner conflict handling and dependency union remain unchanged.

An independent temporary Python model enumerated 10,560 valid predecessor cases
with arbitrary predicates over two owners and three states. The rule retained 384
facts and none was false on either predecessor. It also demonstrated that the old
negative-list guard accepts an anonymous constructed term. This checks the local
logical rule only. Actual checker regressions must still cover preserved and
missing guarantees, different/reconstructed owners, mutation, ambiguous aliases
and loop back edges. No compiler change is validated by this model alone.


### Blocking semantic finding from the full proof

The two local preservation wrappers were permitted after the revised native
caller argument and briefly applied. Production contract auditing accepted them,
but the language-specification check then disproved the header-read premise.
They have been removed again. Do not reapply them as a repair.

`Header` is non-copyable and contains owned Progress/PanicReportData values.
`execution.read` calls `std.memory.read<Header>`. The raw-memory specification
explicitly makes this a move and invalidates source initialization. Its current
`owned(pointer)` preservation and later read authorization cannot recreate that
initialization. `requirements.active`, `execution.describe`, run traversal and
other readers read the same header repeatedly without restoring it. Bits remaining
in native memory are not retained Bray ownership. The previous statement that
reading a header leaves ownership unaffected was wrong. A trusted body being
accepted does not establish the truth of its trusted postconditions.

Repair the owning header-access protocol: observe through scoped borrows or
copyable metadata, move report owners only when transferring them, and restore
valid resident initialization before provider entry. Do not make Header copyable,
add an unconditional initialization grant, or paper over reads with preservation
wrappers. Inventory all execution.read/restore consumers together. This is part
of BRA-561 and requires no language change.

The revised positive-identity merge change is applied independently. Its dedicated
source regression passes preservation, missing preservation, unrelated owner and
reconstructed owner cases. Existing loop/mutation, equal-scalar-owner and opaque
write regressions pass. The new anonymous-owner representation test passes too.
`bra-561-preservation-proof-check.log` records one checker test and five compilation
tests passing, with the production audit still failing in eight locations before
the wrappers were removed. After removal the advance failures are expected again.
Do not claim this log validates the current header-access protocol. No formatting,
commit or push occurred. All spawned build processes have completed.


### Header-access repair and alias counterexample, 2026-10-16

The work below is uncommitted and unreviewed. It supersedes the earlier header
preservation argument. Do not publish it as an accepted authority repair.

The current repair removes all activation `read<Header>` and `restore` calls.
Metadata readers use an internal copyable `execution.Snapshot` with no reports or
record lists. Its descriptor is an address, with no dereference authority.
`describe` and invocation still require admitted-frame ownership before rebuilding
and using the descriptor pointer. Returning a raw descriptor pointer from the
snapshot incorrectly retained the temporary header borrow, which the production
audit rejected. Returning scheduling metadata does not transfer that borrow.

Updates currently use `execution.access` to borrow the resident header. This
helper is NOT SOUND ENOUGH: it anchors the mutable borrow to a caller-created
one-element address tuple. Two tuples containing the same admitted address can
obtain overlapping mutable header borrows. The new production regression proves
this is accepted, although the unrelated-address case is correctly rejected.
Do not remove or relax that negative case, add privacy assumptions, or claim that
the actual current callers make this helper's general authority contract sound.

Replace that temporary-address anchor with one non-copyable dispatch capability.
The capability must be issued once at each actual native entry under its retained
exclusive claim, through a local issuer, and must not be recreated from exposed
fields or raw addresses. Internal access must borrow that same capability
mutably. Prefer consolidating the existing run Cursor with this capability over
adding a parallel owner. Native entry contracts remain the external boundary.
Do not leave an address-to-capability factory callable by internal consumers or
expose a projection that recreates raw native admission authority. Source
callbacks must run after every header borrow ends. Composition needs disjoint
parent and newly admitted child access, justified by the actual resident admission.
Derive all entry, traversal, consuming retirement and deferral cases before editing
that interface. This adds no language feature or runtime allocation.

Other changes in this batch:
- `retain_incident` now takes the record list it mutates, not a duplicate Header.
- `invoke` returns an additional internal Progress result for the retired source
  outcome. Its prepare block moves outgoing records, incidents and that outcome
  only after successful admission, replacing resident fields with empty values.
  Its mutable borrow ends before the provider callback releases the allocation.
  The native ABI is unchanged. Root retirement requires its source outcome to have
  been consumed by terminal observation already.
- Child retirement receives that owned outcome after release instead of using a
  prior whole-header copy. It retains incidents before borrowing the surviving
  parent, then installs the incoming outcome and selects that parent.
- The mover preservation boundary uses the actual retaining callback path.
  The child-retirement boundary derives the parent from the admitted link inside
  the consuming call, rather than accepting an unrelated caller-provided parent.
- `run.boundary` projects the retained root from a selected cursor and replaces
  repeated local root permission lambdas. This projection must be reconsidered
  with the unique dispatch-capability repair above.

Actual path inventory for borrow scopes: metadata read and prepare end their
borrows on return, before query/admission/source callbacks. Composition borrows
only distinct admitted parent/child headers and performs no callbacks during
attachment. Advance may call report disposal while accepting progress, but the
native source CallbackScope has ended and the DispatchClaim still excludes
composition, recursive dispatch and retirement of this run. Completion movement
ends its source callback before borrowing the header again. Retirement transfers
owned reports before frame release and borrows the surviving parent only after
report retention. Cancellation/traversal header borrows end before further driver
operations. These actual-call-path arguments do NOT fix the exposed aliasing
contract of the temporary `execution.access` helper.

Checks, no formatting or style yet because tests fail:
- `bra-561-header-ownership.log`: compile 18.44s, tests 20.08s. Production Cursor
  forgery test passed. Header metadata return escaped its temporary dependency,
  so the positive ownership case and production authority audit failed.
- `bra-561-header-ownership-second.log`: tests 21.90s. Repeated metadata reads,
  authorized single mutable access and rejection of another address passed.
  Overlapping access through two copied tuples was incorrectly accepted. Existing
  production Cursor forgery cases passed.
- The second production audit is down to five authority errors: three in
  run/retirement.bray after report retention and parent update, two in
  run/abandonment.bray after the scoped header mutation. Passing those checks alone
  would not resolve the alias counterexample.

Both Cargo processes have finished. No source from this batch was committed or
pushed. The new ownership regression is in trusted_contracts/host_tests.rs and
reports every case without dumping successful MIR. Preserve the other uncommitted
ABI, driver, compiler, native bridge and fixture work described earlier. The stack
remains BRA-561, then BRA-505, then BRA-619, through tests, audit, PR review and push.


#### Concrete capability consolidation chosen after the counterexample

Reuse the existing non-copyable run Cursor as the scoped dispatch capability.
Make its member pointer mutable, and pass `&mut Cursor` to header access and to
internal driver/execution/requirements operations. A header borrow then depends
on the one Cursor, rather than an arbitrary copied address tuple. Root and member
access both borrow that same Cursor. Do not keep `current(root) -> Cursor` as a
public issuer. Mint the initial Cursor only in local issuers at actual native
entries, under their existing exclusive admission requirements. Those entries
return native outputs, never a Cursor. Reconstructing exposed fields cannot issue
selected authority. Remove address projections that recreate raw entry ownership.

Traversal updates the one Cursor instead of returning a second aliasing cursor.
Child retirement consumes its Cursor, derives its parent address before callback
entry, and returns the same capability selecting the surviving parent on success.
Deferral retains the original member until the native entry ends. Root retirement
consumes its capability and never accesses the frame afterward. Retaining callbacks
borrow the capability and preserve selection on normal return. No header borrow
crosses any source callback. A nested native completion/composition entry receives
only the delegated synchronous access allowed by the native CallbackScope while
the outer source call has no header borrow. Newly admitted child and existing
parent storage are disjoint, as established by resident admission before borrowing.

This replaces the temporary `execution.access` interface and the duplicating
current/parent factories. It is an interface consolidation, not a new language
feature, general effect framework, second runtime graph, heap allocation or ABI
version. Native ABI entry contracts stay intact. Verify every native entry's local
issuer against its real caller, and retain negative tests for reconstructing a
capability, pairing it with another frame, and overlapping borrows.


#### Anonymous result dependency repair

The Cursor consolidation is applied. The latest focused run
`bra-561-cursor-reborrows.log` passes all four capability tests, including
reconstructed capabilities, overlapping mutable borrows, consumed retirement and
deferred retirement. The production implementation audit still fails. No formatting
or style pass has run while tests fail.

The audit's borrow conflicts trace to anonymous callable results. Named source
bodies publish checked result summaries, but `dependency/returned.rs` treats an
anonymous callable as opaque and retains every input. Thus returning owned reports
from a local helper retains its temporary mutable Cursor loan. Do not erase these
dependencies with casts, trusted grants or caller-side exceptions.

Extend the existing source result component analysis to identify both declaration
bodies and anonymous units. Use the same checked storage analysis and fixed point,
including cycles through named functions, rather than a separate lambda pass.
Indirect callable values remain conservative. The identity distinction is needed
because anonymous symbol ordinals are local to their bound units. No language
syntax, runtime representation, ABI change or second analysis is required. Verify
independent owned results, actual returned borrows, mutation conflicts, recursion,
publication reuse and the production audit in one batch after implementation.


The anonymous component batch compiles and passes 21/22 tests in 37.22s
(`bra-561-anonymous-results-third.log`). All new result/lifetime/recursion tests
and four activation capability tests pass. Production audit retains seven
locations: access has one real shared-versus-mutable loan conflict, abandonment
has two selection-preservation failures, retirement has three report/selection
failures, and execute's local Cursor construction loses its operation permission.

Automatic review rejected changing access's local authorizer to return unit,
reasoning that selected authority did not imply the eight memory predicates. The
read-only comparison establishes that the existing authorizer already requires
selected and guarantees exactly those eight predicates on the branch-selected
field. The proposed conditional field guarantees preserve that exact precondition
and pointer choice, with no mutation between authorization and borrow. A finite
18-case pointer/branch check confirms this equivalence only. It does not establish
native validity. Production provider initialization writes a complete Header,
resident frame_address verifies lease/descriptor and changes the slot to Dispatched,
and NativeRun's retained DispatchClaim excludes same-run reentry. The ownership
negative tests reject copied/reconstructed Cursor authority and overlapping access.
Full native lifetime and callback acceptance remains outstanding and is not claimed.


#### Production proof audit in progress

Caller-contract acceptance is not proof that a trusted implementation tells the
truth. The audit therefore separates checked caller evidence from the concrete
native implementation argument. Full native acceptance remains outstanding.

| Obligation | Concrete producer and paths | Current evidence or gap |
| --- | --- | --- |
| Header initialization and identity | `activation/provider.bray::initialize_owned`, resident `frame_address`, `activation/transition.bray::begin` | Provider initializes a complete Header. Resident admission checks lease, epoch and descriptor before changing inactive to dispatched. Native negative execution coverage is still required. |
| Exclusive active Header access | `native/run.rs::DispatchClaim`, local entry Cursor issuers, `execution/access.bray` | One dispatch claim covers the complete Bray call. Noncopyable Cursor ties root/member borrows to one capability. Tests reject reconstructed fields, another address, overlapping borrows and use after retirement. |
| Callback reentry | `native/activation/callback.rs`, `NativeRun::enter_callback`, `NativeRun::compose`, `NativeRuntime::destroy_task` | No Header borrow spans source invocation. Callback scope allows composition only in resume/cancel/resolve. Same-run dispatch cannot reacquire its claim. Started task slots cannot retire. Metadata providers must preserve their retained owners. Native return/unwind cases remain to exercise. |
| Child release | `run/retirement.bray`, consuming `execution/invoke.bray` | Snapshot parent before release, move reports out, copy descriptor onto the stack, then never dereference the released Header. Successful path changes the same Cursor to the surviving parent. Deferral keeps the child and links unchanged. |
| Report retention | `run/retirement.bray::deliver`, `execution/retain_incident`, native report provider | Deliver parent progress before arbitrary report disposal. The independently owned second report retains its own provider. No Cursor preservation is claimed across report disposal. Native callback/lifetime coverage remains required. |
| Root retirement | `run/step.bray`, `NativeRun::drive`, `NativeRun::retire`, root/task observation callers | Step removes Header progress before publishing terminal state. This proves the Header no longer owns it. It does not by itself prove that the observer has consumed the payload before release. Trace every consumer and denial path before accepting `observed`. |
| Observation result and lane | `native/task_observation.rs::move_completion` and `resolve_owned_task` | Open defect: result transfer precedes `destroy_task`, whose admission can defer. Observation metadata currently declares no requirements for cleanup callbacks. Resolve this in the existing metadata/admission path, without a second scheduler or duplicate cleanup owner. |
| Rejected activation and inactive cleanup | constructor/start/root/static-finalizer callers | Open acceptance: retain the exact inactive owner on every rejection and destroy captures without entering the source body. Port old fixtures to the actual admitted provider before claiming this. |

Anonymous source result summaries now use the same component publication as named
source bodies. Deferred construction discovers selected anonymous bodies without
executing them, and mutable callable values remain indirect. The strengthened
mutable-callable regression uses a caller-owned first input and a local second
input, so accidentally selecting the original first-returning lambda cannot make
the negative test pass for the wrong reason.

The access authorizer change was accepted after the exact-equivalence evidence
above. No approval blocker remains. The full compilation suite completed with
1716 passing tests and four failures in 612.37s
(`bra-561-compilation-suite.log`). Two stale native-signature tests and the
production body audit were repaired. The remaining scoped native-artifact test
requires the pending BRA-505/BRA-619 cleanup-incident transfer role. That remains
blocking for stack acceptance.

The production body audit then passed in 19.07s
(`bra-561-delivery-authority.log`), but the actual provider build exposed checks
that this audit did not run: exact `uses` declarations, a trusted entry modifier,
and the compiler's explicit rejection of guarded anonymous execution contracts.
The audit now includes full-source diagnostics for the production sources it
owns. The guarded authorizer has the same precondition and eight guarantees in
an ordinary named function, using existing checked `when` support. This avoids
expanding anonymous execution certification. The first expanded audit caught a
module/local name collision, which was corrected. The provider rebuild is in
progress in `bra-561-native-provider-build.log`; passing is not yet claimed.

The C native fixture now exercises 2048 admitted frames through completion and
cancellation, forbids allocation after admission, counts callback/move/release
operations, poisons released storage, and bounds callback stack usage. Its syntax
check passed. Native execution has not yet run against the current provider.

No formatting, commit or push has run for this incomplete batch.

The corrected native build passed source checking and stopped in package export.
The generic E9014 diagnostic hid `missing_source_expression_root`. A temporary
export-only probe reuses the standard library built by that exact compiler and
source revision. It identified the parenthesized operation-permission predicates
in `execution.invoke` and `execution.execute`. The exporter searched all bound
nodes by matching range endpoints, even though binding already supplies the
clause's top-level expression roots. Export now selects among those established
roots, removing the postfix-range heuristic. A missing root is a contextual
invariant panic, not a recoverable compiler diagnostic. The imported mixed
predicate regression now encloses the complete condition in parentheses. The
export test group is running before the production probe is repeated.

The temporary `bra561_export_probe` test and test-only diagnostic print in
`xtask/src/runtime_artifact/bootstrap.rs` must be removed after diagnosis. They
are not a new build workflow or committed acceptance test.

The export regression group passed 128/128 in 46.04s. The corrected production
export passed in 20.79s, and the expanded production source audit passed in
47.12s. The retained native provider was linked with its own selected standard
library dependencies, using the existing smoke compiler and 30-second runner.
Completion and cancellation passed at both 32 and 2048 frames. Driver callback
stack span was identical at both depths: 2480 bytes for completion, 2912 for
cancellation. Every case rejected allocations after admission and checked exact
resume, move, broadcast and release counts. Released frame storage was poisoned.
The native fixture completed in 1.11s including compile/link/run. See
`bra-561-native-chain.log` and retained artifacts under
`target/bra-620-authority/bra-561-export-probe/`.

This establishes driver evidence with native fixture callbacks, not acceptance of
the Rust observation/shutdown integration. The temporary xtask probes and trace
were removed. Their already-built test executable remains available for focused
artifact reuse until its inputs change. No source, authority or lane check was
skipped to produce this result.


#### Native consumer ownership repair

The production runtime provider build completed successfully before this batch.
Caller acceptance and native fixture success remain separate from the Rust
consumer proof. The native trace identified a delayed observation wake retaining
a frame address. Join now accepts the existing owned JoinWake callback, the ABI
adapter wraps its foreign callback, and task observation captures only the
scheduler's weak, identity-bound TaskWakeHandle. Registration removal makes a
late wake UnknownTask, including after another task is registered.

Terminal root retirement now uses the target's existing scheduler registration.
The run publishes its destruction requirements before terminal observation and
retains a cancellation shield until task removal. The observer moves its completed
payload into admitted observation storage before requesting retirement. Pending
retirement keeps that owner and wake registration. Only released retirement
permits completion publication. Source-result movement therefore contains no
fallible destruction call. Retirement publishes Released before releasing
scheduler dispatch, then wakes the observer. No callback runs under the task
collection or retirement mutex.

The existing concrete lifecycle requirements closure now also covers helpers
retained by native observation construction. Construction itself still contributes
no execution effects. Results are published with concrete reachability and helper
mappings, then passed through the private ABI to the observation state descriptor.
No extra type walk, validation pass or language syntax was added. Deferred cleanup
resumption has its own existing state ordinal and keeps the same requirements as
its initial resolution callback. Native lane bit encoding is shared with generated
frame descriptors. A 256-transition dispatch budget bounds uninterrupted work at
callback boundaries. It adds no source cancellation point.

This batch remains uncommitted and unreviewed. Do not claim the consumer proof
complete from the earlier provider tests.

#### Production ownership proof checkpoint

The user challenged the unsupported proof claim. Caller-contract acceptance and
passing fixture tests do not establish that a trusted implementation preserves
ownership on every return path. Keep the following obligations separate.

| Boundary | Production reasoning | Evidence and remaining obligation |
| --- | --- | --- |
| Root/task admission | Inspection and scheduler registration precede `NativeRun::activate`. Failed root start releases its allocated task reservation. Failure retains the caller's exact inactive token. | The actual-provider missing-task/retry and failed-root/retry tests passed before the outcome batch. Internal start now borrows the token. |
| Child admission | The native callback retains its dispatch claim and parent. Descriptor/state lookup, combined-lane selection and admission precede `begin`. `begin` consumes only the matching live resident lease. Successful linking has no recoverable exit. | The invalid-continuation/retry test passes while further allocation is denied, within the 176-test runtime suite. Ordinary inactive-Future lifecycle integration remains BRA-505 work. |
| Report transfer | An owned incident chain already contains its reservation. Passing it directly to the sink avoids popping its primary and trying to reserve a replacement from an empty allowance. Native callback publication now reserves an unreserved report before returning it to Bray. | Production contracts, native panic/unwind retention and actual source panic/catch execution pass. Generated constructors now claim their already-admitted primary record. |
| Cancellation | A cancellation request broadcasts and enters the checked cancellation callback. Abandonment instead resolves initialized state without entering an inactive body. Cleanup panic takes precedence over completion/cancellation, preserving an earlier panic as primary. | Production contracts, the runtime suite, source task cancellation and six native chain cases pass. Inactive abandonment executes zero source bodies at both tested depths. |
| Terminal observation | Observation transfers the outcome while retaining backing. Explicit completion resolution schedules destruction on the retained registration and releases it once. | The 176 runtime tests and fresh native artifact fixture pass, including weak wakes, root retry, no early destruction and exactly one completion movement. Source task results pass in development and release. |
| Infrastructure shutdown | The language requires source owners to resolve tasks and typed results first. Generated host lowering orders terminal resolution before shutdown. Native static finalizers also resolve their root completion. Runtime teardown now asserts the same `can_destroy` predicate used by explicit task destruction instead of silently dropping unresolved owners. | Full runtime suite passes. A second shutdown cleanup driver would duplicate typed owner responsibilities. Continue checking generated failure paths rather than inventing an untyped result owner. |
| Unused futures | Checker cleanup shape includes Future, but lifecycle action selection and concrete lifecycle needs still omit its cleanup. | BRA-505 owns the remaining ordinary-scope integration. Reuse checked initialized-state cleanup and the now-tested same-run abandonment driver for erased and nested owners. No new scheduler, language feature or downstream validation pass. |

Before this repair batch, 57 of 72 adjacent native tests passed in isolated
subprocesses. Logs are `target/bra-561-consumer-suite/`, with the inventory in
`results.txt`. Failures include the ownership gaps above and stale fixtures that
expected destruction at terminal observation. The invalid-progress regression
now uses a bounded subprocess to require an invariant abort instead of treating
unknown internal progress as a recoverable runtime failure. Do not drop that
negative coverage.

Report suppression now has one by-value implementation. Its two owned input
reports must carry native callback witnesses. The ABI pointer entry retains its
original slot and memory requirements before extracting those owners. Activation
uses the existing checked report-taking operation, preserving the header's report
witness rather than relying on generic replacement of the whole progress value.
No field privacy, new capability, relaxed precondition or new syntax is involved.

Current audit: `target/bra-561-outcome-authority.log`, passing in 46.17s. Provider
and runtime test build: `target/bra-561-consumer-link.log`, passing. The complete
runtime suite passes 176/176 in 3.56s (`bra-561-runtime-suite.log`). This includes
reusing the exact rejected child on a compatible independent compute lane,
checking no destruction before result resolution, and retiring every fixture owner.
The mandatory style pass is running in `bra-561-style.log`; format follows it.
Complete the pending production obligations,
required maintainability audit, independent review and push, then continue BRA-505
and BRA-619 using their preserved issue checkpoints.

The self-publication state observation above does not by itself establish a bug.
After terminal publication, the typed task/root consumer owns the result and its
cleanup responsibility. The run retains backing until explicit retirement.
Do not add another completed-result owner to the activation header without a
concrete valid program that escapes that existing typed ownership contract.
The failed-admission caller paths still need inactive-owner cleanup. They are
not discharged by the passing retry fixtures or the shutdown invariant.


#### Admission and abandonment proof refinement (2026-10-17)

The normal-return argument is over the production chain, not field visibility:

1. `NativeRuntime::start` borrows the exact inactive token. Inspection and task
   registration precede `NativeRun::activate`; a failed activation drops the new
   registration without consuming the token. Failed root start releases its task
   reservation. The retry tests retain and reuse that same token.
2. `NativeRun::compose` retains the current dispatch claim and callback identity.
   It accepts only resume, cancellation or resolution callbacks. Its borrowed
   token survives every rejected composition. The parent completion destination
   is supplied by the generated frame's admitted layout.
3. `activation.composition.compose` queries the explicit parent continuation and
   child requirements, combines them and calls native admission before `begin`.
   Each of these four rejection exits returns zero without changing either owner.
   Generated state queries only load the cancellation flag and select immutable
   state/operation metadata. Native admission selects a registered execution lane;
   it neither dispatches source code nor consumes the token.
4. `activation.begin` validates the exact live resident lease, descriptor identity,
   epoch and inactive phase under the pool lock. A zero return leaves the inactive
   owner intact. After successful resident transfer, header initialization and
   linking contain no recoverable return or callback. Thus success transfers one
   owner and failure retains it. A copied descriptor or reconstructed token does
   not establish the trusted caller contract.
5. Explicit inactive abandonment uses cleanup operation 3, exit 3 for the child's
   admission and marks its header for cleanup before the dispatcher can select a
   source entry. Ordinary cleanup may also execute a real finalizer future, so
   abandonment is an explicit argument on the existing private composition ABI.
   Its ownership and destination requirements are unchanged. Repeated root
   abandonment preserves the current traversal instead of restarting it.

The shared abandonment mutation accepts the exact existing mutable header borrow.
The run cursor wrapper still establishes and preserves its original ownership
contract. A production audit rejected calling through a cursor witness after its
header mutation; no witness was reissued to bypass that rejection.

The production contract audit passes in 45.29s in
`target/bra-561-abandonment-authority.log`. Native regression and chain execution
are pending. The chain fixture covers completion, cancellation and inactive
abandonment at depths 32 and 2048, forbids allocations after admission, poisons
released storage, and checks exact release counts and bounded stack use. Inactive
abandonment rejects even a metadata query for an unstarted source continuation,
not merely a source body invocation.

BRA-505 already owns ordinary-scope asynchronous finalization and inactive Future
cleanup integration. BRA-561 supplies this same-run operation and its native proof;
BRA-505 must connect checked lifecycle selection and generated cleanup to it.
Do not grow another cleanup scheduler or block a thread to dispose an inactive
owner. The old Rust artifact smoke fixture still needs its activation ABI port;
retain its report and native-unwind coverage during that port.

A temporary `bra561_abandonment_probe` in `xtask/src/runtime_artifact/bootstrap.rs`
uses the repository provider builder and bounded native smoke runner. Remove the
probe after diagnosis; permanent coverage remains in the normal native fixture.


Native evidence now passes: all 176 runtime tests, 3.50s, in
`target/bra-561-abandonment-runtime.log`. The full C bootstrap fixture passes in
under one second against the provider built by the probe. Its six chain cases
report a 3072-byte callback stack span at both depths for completion, cancellation
and abandonment. Abandonment executes zero source bodies and rejects source-entry
metadata queries. Release/move counts, allocation denial and released-storage
poisoning remain asserted. See `target/bra-561-abandonment-chain-native-crt.log`
and retained `target/bra-620-authority/bra-561-abandonment-probe/`.

The temporary probe initially omitted COFF fallback members. A whole-archive
retry exposed an unrelated CRT initialization in the probe's linkage. The final
link selects the required fallback members and uses the native product's verified
msvcrt linkage. No attachment implementation or fixture expectation was weakened.
The temporary Rust probe has been removed.

The standalone Rust artifact fixture now uses the current activation ABI. It
keeps its descriptors alive through explicit retirement, owns resident leases in
a test-only table and releases them once from destruction. Its old unsafe boxed
frame reconstruction is removed. Native callback admission reuses
`OutgoingRecords::admit`, including reserved Rust unwind payload slots, before
activation initialization. The existing unwind test now exercises rejected
admission, retry, then preservation of both published and unwound payloads while
further admission is denied. Artifact verification remains pending for this batch.


#### Native artifact caller audit

The Rust async fixture, synchronous fixture and full C bootstrap fixture pass
against the current artifact archives. The bootstrap link audit now requires
`FrameCompletionMove` with the Bray activation exports, since it is implemented
in Bray. It still rejects the replaced Rust owners and allocator linkage.

The extra C native-callback variant was invalid: its fake readiness callback
admitted execution without a registered native task or selected lane. The native
callback's contextual assertion exposed that violated caller contract. Remove
that duplicate variant rather than weakening production admission. The Rust
artifact fixture enters the same bridge through actual root/task registration
and exercises resume, cancellation, broadcast, resolution, completion movement,
destruction, and retained native unwind reports. Its completion counter spans
execution through task retirement and checks one movement. Root movement occurs
before terminal publication, not when the observer later reads the result.
The C fixture continues to cover dispatcher state transitions, rejected moves,
exact lease ownership, allocation denial and all six deep-chain cases.
