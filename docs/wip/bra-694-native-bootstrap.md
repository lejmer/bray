# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: feature/bra-694-native-bootstrap-contracts, develop base 5de46243b.
Pushed aa09a61c3. Retained reviewer /root/bra694_review.
Published review through that commit has no findings:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5921300355.

## Required outcome

Check/lower actual Linux and Windows bootstrap products on x86_64 and AArch64. Preserve source/import contracts,
negative failure locations, conditional masks and authoritative empty ownership. Reject fabrication, replacement,
old epochs, actual mutation and retirement. Compiler invariant violations panic. BRA-695 execution and BRA-696
shutdown remain separate. No historical reconstruction, freshness from owned_allocation, layout/exactness from alias
bounds, predicate reassertion or blanket loan exemptions. Stop on unexplained regressions. Commit/push checkpoints.
Style remains deferred until finalization. Native subprocess workflows run outside the sandbox.

## Established repair contracts

Logical-only transitions retain required physical descriptors. Finite assignments capture RHS before weak commit,
never initialize every candidate or fabricate exact identity. Overlapping observation suffixes require agreement.
Ambient confinement excludes only original mutable entry borrows at layer1, empty reference path and ordinary inline
projections. Other origins stay conservative. Direct opaque Ambient/Escape stays unbounded. Current storage subtree
walks share storage_subtree.

Context.pending uses existing SpinLock<Observation?>, initialized once by both constructors. Typed context,
view_context and owned_context publish lock validity. Snapshots end their guards before scheduling/dispatch.
take_ready copies and clears under one guard. Current owns a counted view, not exclusive mutation. Preserve
borrow_execution's exposed-address publication independently of its temporary ExecutionOwner loan. Retirement
inspects through owned_context, ends inspection, then moves/destroys whole Context after zero-view/phase/task checks.
No manual field teardown or new validity axioms.

Exact typed pointee snapshots live in existing CurrentValues. Only successful Borrow<T> results seed actual current
referents, keyed by physical target, checked T and producer occurrence. Raw pointers, alias bounds, previous/inexact
targets and entry fallbacks cannot seed. Exact lookup only, no parent reconstruction. Repeated views cannot revive
Absent storage. Read/VolatileRead capture before consumption and publish via existing normal-result preservation.
Writes update exact destinations. Storage flow alone computes survival masks using cached Footprint shapes.
Active loan/shared-view decisions publish logical and physical masks together, only for one actual Source.targets
pointee, never allocation-wide Source.regions. Unknown mutations stay conservative. Non-call/non-assignment moves
use actual Move plans and handle retirements. Pointee snapshots join normally but never enumerate as live owners.
Producer reuse retires roots and dependent conditions. Runtime-only roots cannot export through portable contracts.
Opaque field-reread origins, declaration-only static keys and unrepresentable UninitPayload logical paths remain
excluded. Equivalent consumers share current_target_value. One current-store regression covers observation versus
ownership, consumption and producer reuse. The batch adds about350 production lines, below its500-line limit.

Arena source composition acquires the actual Domain.arena borrow before pending mutation. task.retain_in_arena
replaces execution.retain_task and shares existing HostLease/arena_anchor/retain_locked handling with native retention.
Dispatch, worker failure and observer creation use that typed arena view. Existing status4, ownership and ABI remain.
No additional guard/acquisition or predicate reassertion. This source batch reduces runtime code.

## Current unresolved transitions

Only dispatch_ready and start_frame fail the current Windows18-body native gate. retire_driver now checks/lowers.
Do not confuse failed actual conditions with larger required clauses, or expected post-Attempt consumption with failed
admission. No compiler semantic patch since55763bb73.

Dispatch loses view_owned temporarily at dispatch_task and permanently at event.wait_execution, before recovery.
The original truncated listing showed Ambient/Escape/typed Replace through a returned task pointer, but full tracing
also found unbounded Retire effects. The arena source
repair preserves its original Entry pointer through dispatch_task. Follow current bounds beyond task retention.

Verified but unpatched transport gap: stage_reference InputRegions and stage_reference_value append projections
including OwnedTarget rather than following stored backing. Existing StorageFlowInput.stored_locations resolves
OwnedTarget through current/staged buffer descriptors. Trace Arena.entries/UninitPayload/slabs/OwnedTarget/Slab.slots/
OwnedTarget before changing transport. Inexact regions cannot recover exact identity or layout.

Start's actual failed commit_start condition is selected.status==0. Scoped exit fails view_owned(selected.acquisition).
prepare_start22 and ScopedEnter101 both return selected/reference with exact preserves_value relations. The new trace
shows selected still equals the original Entry Borrow through before_forward, after_forward and after_refresh at both
calls. Later captured commit arguments instead observe a fresh prepare_result.Ok.selected. Find the first intervening
current-value loss. The first loss is register_task54. A full marker trace exposes an UnboundedMutation Retire with
no access type and ambient=false there, beyond the truncated first12 effects. This drops stored selected despite its
correct exact Entry referent. Trace the original unbounded retirement producer before any policy change. Never repair call publication or relation generation based on the disproven earlier guesses.

## Implicit reference-read contract

The full trace finds the shared first cause: RawBuffer cleanup in host.product.progress and leave_domain resolves
Receiver.pointer through a valid prepared path but its current physical reference is Unknown after callbacks/swaps.
Other failures are nested buffers under OwnedTarget/AnyElement in complete_attachment_exit/acquire. Resolution widens
to nonambient Unbounded Retire, then provider summaries propagate that effect into register_task and wait_execution.
The earlier first12-effect listing concealed retirement. Do not infer its absence from that truncated listing.

Explicit call inputs already observe actual stored references with observe_stored_reference. Implicit buffer/lifecycle
cleanup must use that same current-read policy before resolving indirect effects. Read only prepared indirect effect
subjects from current live cleanup receivers. Parent pointer reads precede nested owned paths. Preserve authoritative
empty references. Unknown/inexact locations stay conservative. Establish no trusted facts, source equality, allocation
freshness, ownership or old contents. This is an actual implicit descriptor read, not entry reconstruction.

Prepare the cleanup receiver after existing pending-result publication, capture the skipped branch, observe required
current descriptor reads only on the selected cleanup branch, resolve current targets, then apply retirement once.
One helper serves buffer and selected lifecycle consumers through the existing observer. Ordinary calls remain the
established policy. Existing native18 catches the failure, with frozen raw/source/import66 and1714 controls guarding
excessive retention. No new bespoke fixture initially. Target under80 production lines, no new types or mutation
policy. Reconsider if bulk paths need invented identity/layout or unavailable storage is revived.

Current repair: checker564 pass0.53s. All1714 frozen identities pass, with1712 grouped212.0s and the same final2
isolated70.60s. All65 frozen raw/source/import controls pass, plus the additional nested-union returned-owner control
passes0.18s. Native18 gate44.16s still only dispatch/start. Failed actual conditions now name original selected.status
and selected.acquisition rather than prepare_result.Ok.selected. One correspondence loss is repaired, but composition
remains unverified. Independent review found and confirmed repair of a conditional-path issue: descriptor reads now
follow the skipped-branch snapshot. No further actionable findings. Unknown intermediate prefixes and inexact bulk
paths remain conservative and open. All11 diagnostic diffs and their superseded native-current log pair are removed.
No style pass yet. Publish the reviewed checkpoint, then continue the existing OwnedTarget transport contract.

## Fixed evidence and remaining gates

Frozen baseline1718 identities on1ceb765eb stays unchanged. Previous compiler baseline55763bb73 also passed all1714 retained identities:
1712 grouped213.34s, final2 isolated together70.13s. The same2 hit90s bounds when concurrent, not new failures.
Four removed provider identities remain open replacements by actual product gates, never passes:
fixtures::tests::production_fixture_phase_timings,
host_authority::production_host_nested_bodies_and_contracts_are_checked,
report_protocol::production_report_consumers_check_their_callback_protocol,
standard_storage::production_storage_boundaries_prove_their_declared_memory_effects.

On55763bb73: checker564, bound-tree136, raw/source/import controls66 pass. Fixed-five420/409/405/417/430ms.
Counts not remeasured. On aa09a61c3: controls66 pass11.16s, native18 gate44.12s with only dispatch/start failures.
New helper, native retention callback, prepare_start, worker failure and retirement check/lower.
The current comparison includes the aa09 source batch and this cleanup repair. Earlier catalog62, artifact reuse5 and boundary/cache13 remain valid.

Keep one ignored output per purpose in target/audit. comparison-baseline is frozen. comparison-current1712 and
slow-current final2 share557 code. comparison-pending.json retains scheduling identities. timing-current records the
five557 timings. proof-counter.patch and counts-current remain for final counts. Remove superseded traces, not
baseline evidence. Reuse shared Cargo/LLVM storage, never clean shared caches.

Final gates: actual four-target check/lower and event layout, affected owned/deferred/projection/cleanup suites,
fixed comparison after semantic changes, fixed-five/loop counts, maintainability/DRY audit, mutating cargo xtask style
and cargo xtask format, fresh native compiler products, final PR description and published independent review.
Only then mark PR ready and BRA-694 In Review. BRA-695 is the next separately scoped issue.
