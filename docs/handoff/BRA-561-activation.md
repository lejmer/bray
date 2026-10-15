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

## Remaining implementation

Replace NativeFrame/NativeRun ownership, including Box/Arc activation links,
activation-time completion allocation and six-record admission during composition.
The source-frame allocation currently reserves a completion tail which the Rust
consumer does not yet use. One admitted owner must retain that backing through
result dependencies. Do not introduce another graph or a replacement allocation
inside a transition.

Validate backing epoch, concrete shape, provider and inherited execution authority
before committing activation. Rejection retains the inactive owner. Keep trusted
native ownership contracts explicit. Numeric fields or pointer equality alone
cannot establish that contract, and Bray fields have no privacy guarantee.

Complete indirect factories, inactive Future lifecycle, initialized-state broadcast
and iterative resolution. Setting a cancellation byte alone does not visit captured
task obligations. Calling resume once and ignoring suspension does not resolve
cleanup. Migrate task-observation and static-finalizer consumers through the same
driver, retaining only bounded scheduler/task bridges for their separate issues.

Finish native allocation, deep-await, alignment, affinity, retained-result,
imported/erased and failure acceptance. Record artifact and cost evidence, then
push a draft PR and conduct its independent review before BRA-505 and BRA-619.
This checkpoint does not complete BRA-561 and has not had a completion review.
