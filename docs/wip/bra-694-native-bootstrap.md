# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: feature/bra-694-native-bootstrap-contracts, develop base5de46243b.
Pushed:55763bb73. Reviewer:/root/bra694_review.
Verified published WIP review through55763bb73:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5920332136. No findings.

## Required outcome

Check/lower actual Linux and Windows bootstrap products on x86_64 and AArch64. Preserve source/imported contracts,
exact negative sites, conditional masks and authoritative empty ownership. Reject fabrication, replacement, old epochs,
actual mutation and retirement. Internal defects panic. BRA-695 execution and BRA-696 shutdown remain separate.
Performance is separate except shared setup reuse. No historical reconstruction, freshness from owned_allocation,
layout/exactness from alias bounds, predicate reassertion or blanket loan exemption. Stop on unexplained regressions.
Commit/push coherent checkpoints. Style remains deferred until finalization.

## Fixed comparison and established rules

All1714 retained frozen identities pass on55763bb73. Baseline1718 on1ceb765eb stays fixed.
Four c606 provider-test removals remain open replacements with actual native product gates, never passes.

Logical-only transitions retain prepared descriptors whenever physical tracking is needed. Finite assignments capture
RHS before weak commit, never initialize every candidate or fabricate exact identity. Observation suffixes require
agreement across overlapping targets. Alias bounds confer no layout. Ambient confinement excludes only original
mutable entry borrows at layer1, empty reference path and ordinary inline projections. Other origins stay conservative.
Direct opaque [Ambient,Escape] stays unbounded. Current exact storage-subtree walks share one helper.

## Pending-state requirements

- Current owns a counted view, not exclusive mutation. Context.pending uses existing SpinLock<Observation?>, created
  once by each constructor. No raw pending slot helpers remain.
- context/view_context/owned_context publish actual lock validity through their typed Context borrow. No new validity
  lambda or protocol axiom. owned_context is shared by borrow_execution and retirement.
- Snapshots copy Observation/none after the short guard ends. Acquisition writers assign under short guards.
  take_ready copies and clears under one guard. Never hold guards across scheduling, dispatch or callbacks.
- Preserve borrow_execution's exposed-address pointer publication. Its counted Current is independent of the temporary
  ExecutionOwner loan. Binding, closing, provider, publication and worker handoffs must preserve that distinction.
- Retirement requires zero views. Inspect through the owner-backed typed helper, end inspection, then move and destroy
  the whole Context. Never replace whole destruction with a manual field teardown list. Type-derived layout only.

The existing native gates check18 actual bodies per target with body-start/finish reporting. Source/imported locks,
loop cleanup, escaped-guard and raw invalidation controls remain fixed. The pending source change reduces code.

## Remaining boundaries and evidence

Native dispatch_ready loses view_owned at its later recovery/context call after demand_lane. start_frame loses
producer_owned at commit_start and view_owned at scoped exit. Current whole-Context retirement checks/lowers on Windows.
Actual four-target products and event layout remain open. Catalog62, artifact reuse5 and boundary/cache13 passed
before this batch. Final required gates remain listed below.

Verified transport gap: stage_reference InputRegions and stage_reference_value append target_projections, including
OwnedTarget, instead of following current backing through stored_locations. BoundCallReference has relation/type but
no prepared target path. Trace retain_task staging through Arena.entries, UninitPayload, slabs, OwnedTarget, Slab.slots
and the second OwnedTarget. Correlate its lost bounds with the two remaining native invalidations before editing.
No transport semantic patch started. Inexact regions must never recover exact layout or pointer identity.

Ignored evidence keeps one current output per purpose. comparison-current records1712 passes, slow-current the final2
on identical code. comparison-baseline stays frozen. comparison-pending.json retains shard selection identities.
Stale context traces and the old pending log were removed. No diagnostic-only compiler branches remain. Resolve test
executables from Cargo output. Native subprocess workflows run outside the sandbox. Never clean shared Cargo storage.

## Current physical-value repair contract

Trace confirmed retire_driver read77 reaches current Returned root20. The valid fact and available witness survive,
but observe_memory_read creates no pending result because value_observation_path correctly excludes this origin.
The raw-read trace is removed. Never turn the address carrier into Context data.

Retain exact typed pointee snapshots in existing CurrentValues, keyed by physical target, checked pointee type and
producer occurrence. Normal completion of an actual Borrow<T> result supplies its current referent once. Raw address
values, alias bounds, inexact/previous targets and entry Input fallbacks cannot seed it. Repeated views never reset
Absent storage. Exact lookup only, with no parent reconstruction or reinterpretation. Equivalent consumers are moving
Read/VolatileRead, borrowed_value, observe_references, both trusted reference refreshes and complete_memory_write.
Opaque field-reread Returned origins are excluded because their existing producer retirement is independent of the
declaration occurrence. Admit only evaluated/invocation/scoped result origins until that handoff is represented.
Declaration-only static keys are excluded because they lack selected instance identity. Unrepresentable logical
UninitPayload paths remain outside this initial resolver. Neither exclusion affects the confirmed Context target.

Transition inventory:
- Normal typed view publication registers actual result contents, then aliases read the same current snapshot.
- Read attempt captures before consumption. The existing valid normal completion publishes the result and its
  ordinary/init snapshot preservation. The source becomes absent, and a second read remains rejected there.
- Storage flow alone resolves mutation reachability and classifies surviving observations from cached Footprint
  shapes and declared memory types. Extend resolved All/Observed frames with exact typed physical masks, using the
  same survival decision and ValuePreservation framing. Missing masks preserve nothing. Paths is preparation only
  and is resolved to All/Observed before this boundary. No shared-loan exemption after its carrier ends.
  Active loans and retained shared views publish their established preservation to both logical and physical masks
  at the original decision. Do not reconstruct that decision from logical aliases or erase their surviving pointees.
  Physical publication requires one actual current target from Source.targets. Source.regions can bound displaced or
  inexact pointers and must never lend their allocation-wide bounds to a different exact pointee snapshot.
  Non-call/non-assignment moves use the actual phase's Move targets and direct value-retirement handles as finite
  Consume effects. An absent call effect remains unknown. Moving a pointer handle never consumes its referent.
- Writes replace only checked exact destinations. Unknown/overlapping writes reframe contents, preserving only
  established descriptor/tag/value classes. Masks establish neither initialization nor availability.
- Pointee snapshots participate in existing data joins but are excluded from live owner enumeration everywhere.
  Scope exit cannot turn them into independent witness carriers. Producer re-evaluation retires their roots and
  dependent Input/Joined/InitializedSnapshot conditions. Portable export continues to reject runtime-only roots.

Fixed oracles before expansion: actual whole-Context retirement, existing raw extraction/second-read/fabrication,
raw writes and retirement, source/imported returned dependencies, conditional projection and guard cleanup controls.
Add one small typed returned-view/read composition if those fixtures lack this origin. Preserve exact negative sites.
All1714 retained comparison identities remain fixed. Reconsider on unexplained regressions, new ownership from a
snapshot, type recovery from alias bounds, revival after consumption or duplicate mutation policies. Replace the
logical-only consumer gates with one current-target resolver, with no second value graph. Target under500 net
production lines, reuse existing fixtures and no general projection/type-query machinery in the fixed point.

Current batch adds exact typed snapshots and physical preservation to the existing
value store and mutation frame, with no second graph. Pre-build read-only review covered phase-specific moves,
pointer-field epochs, selected statics, clause preparation and ownership exclusion. One current-store regression
checks observation versus ownership, consumption and producer reuse. Current code: bound-tree136 and checker564 pass,
raw/source/import controls66 pass10.90s, fixed comparison1712 pass213.34s plus final2 isolated pass70.13s. All1714 retained
identities pass. Fixed-five420/409/405/417/430ms, counts not remeasured. Windows native15-body gate43.93s on this
final code state has no retirement error, leaving the established dispatch/start failures.
Net Rust growth396 lines by file placement, including the new current-store test. No second value graph.
Current trace: dispatch unit749970 loses view_owned at call35 then restores it, and permanently at call53
(event.wait_execution), before recovery. Effects are Ambient/Escape/typed Replace on returned task pointer
tuple10.0?.pointer5633, no Retire. First compare demanded Domain.arena.pointer before take_ready3 and retain_task9.
Only if retained should staging be followed through both OwnedTarget boundaries.
Start still proves both fact names before commit_start98. Its Attempt correctly consumes the token,
so post-attempt losses cannot explain failed admission. Compare the actual failed observations with captured
entry.proof_facts at98 and scoped exit101. No compiler semantic patch is authorized by these traces.
Temporary BRAY_AUDIT_NATIVE diagnostics in flow/operation.rs and storage collector core/transfers print selected caller
facts/effects, failed entry observations and marker rejection bounds. Their build is active. The bounded trace reader
deduplicates only native/phase lines into native-current. Remove diagnostics before the next semantic checkpoint.
Second trace confirms Domain.arena.pointer is changed from original Entry to newly observed Returned between3 and9.
The owning boundary is memory.invalidate_target(Clear), which retires an overlapping descriptor lacking an actual
typed shared view. The first repair is source composition, not a broad deep-frame exemption.

## Arena-view source contract

Acquire the actual Domain.arena typed borrow before pending mutation and use that same view for task retention.
Replace execution.retain_task's implicit Domain recovery with task.retain_in_arena(&ArenaReference, handle), requiring
the existing arena_retained condition. Share its existing HostLease/arena_anchor/retain_locked sequence with the native
retention callback. Dispatch and worker-failure callers retain the arena view through snapshot/clear and lookup.
Observer creation obtains the same view from its bound execution. All four paths keep existing status4 behavior,
task-reference ownership and arena/host lifetimes. End views after actual lookup, with no extra acquisition or guard.
Never reassert predicates, reconstruct entry fields, protect all deep address roots or infer freshness from retention.
Delete the execution wrapper and duplicated callback setup. Target no production growth. Verify current pointer before
lookup, dispatch native gate, direct helper/callback bodies, worker/observer controls and fixed source/import negatives.
This source batch is implemented. Current66 controls pass11.16s. Windows18-body gate44.12s checks/lowers the new
helper, native retention callback, prepare_start and worker-failure body, with only dispatch_ready/start_frame failing.
The original arena pointer remains Entry through dispatch_task43. Source ownership/ABI/status behavior is unchanged.
Full comparison on this source batch remains pending. Keep this design improvement even if later bounds still fail.

Start correction: actual failed commit_start clause is selected.status==0, not producer_owned. Required observations
remain prepare_start(...).Ok.value.selected.status/acquisition while captured facts identify original selected.
Both prepare_start22 Result.Ok.selected/reference and exact return-self enter101 selected/reference relations are
Exact with preserves_value=true. Completion22 publishes selected -> original Entry Borrow root1. construct(false)
retains siblings. Normal calls publish at CallCompletion, not ResultCompletion. Next compare actual forwarded argument,
result selected after complete_returned_references, after refresh_returned_references, and after scoped entry.
No compiler semantic patch until that correspondence boundary is identified. No style pass yet.
Final gates: actual four-target check/lower/event layout, affected owned/deferred/projection/cleanup suites and fixed
comparison after material changes, fixed-five/loop counts, maintainability/DRY audit, mutating style and Bray format,
fresh native compiler products, final PR description and published review. Only then PR ready and BRA-694 In Review.
