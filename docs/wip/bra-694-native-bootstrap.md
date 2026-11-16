# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: feature/bra-694-native-bootstrap-contracts, develop base5de46243b.
Pushed:508462d1e. Reviewer:/root/bra694_review.
Verified published WIP review through508462d1e:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5918880038. No findings.

## Required outcome

Check/lower actual Linux and Windows bootstrap products on x86_64 and AArch64. Preserve source/imported contracts,
exact negative sites, conditional masks and authoritative empty ownership. Reject fabrication, replacement, old epochs,
actual mutation and retirement. Internal defects panic. BRA-695 execution and BRA-696 shutdown remain separate.
Performance is separate except shared setup reuse. No historical reconstruction, freshness from owned_allocation,
layout/exactness from alias bounds, predicate reassertion or blanket loan exemption. Stop on unexplained regressions.
Commit/push coherent checkpoints. Style remains deferred until finalization.

## Fixed comparison and established rules

All1714 retained frozen identities pass on the current physical-value batch. Baseline1718 on1ceb765eb stays fixed.
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

The existing native gates check15 actual bodies per target with body-start/finish reporting. Source/imported locks,
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
The temporary trace is removed. Published review508 is complete. Never turn the address carrier into Context data.

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
Next: checkpoint/push and published incremental review, then correlate remaining dispatch/start invalidation with
the verified flat OwnedTarget staging gap. No style pass yet.
Final gates: actual four-target check/lower/event layout, affected owned/deferred/projection/cleanup suites and fixed
comparison after material changes, fixed-five/loop counts, maintainability/DRY audit, mutating style and Bray format,
fresh native compiler products, final PR description and published review. Only then PR ready and BRA-694 In Review.
