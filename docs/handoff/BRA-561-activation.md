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
strings. Frame execution supplies the requested immutable state; tasks and the
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
scheduler lane selection. Compiler and test descriptors remain immutable; runtime
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
executable template format is version 3.

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
for a backslash; that expectation is corrected. The complete `cargo xtask readiness native-execution` run passes in 376.2 seconds,
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


The existing `MirFrameInitializer::TaskObservation` already carries the selected
result representation and both cleanup helpers. Reuse that initializer rather
than adding another compiler-provided callable kind. Before the Bray consumer can
invoke those helpers, replace its current `helper_address` native-entry callbacks
with explicit caller-owned outcomes, as static cleanup already does through
`invoke_static_boundary`. The current native adapter can still contain fixture
unwinds while preserving a report written before unwinding. This change should
remove the source cleanup callback's dependence on foreign unwinding, not add a
second report protocol.
