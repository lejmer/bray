# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch feature/bra-694-native-bootstrap-contracts, develop base 5de46243b. Use Git HEAD for checkpoint identity.
Retained reviewer /root/bra694_review. Published review through4f767fbb2 has no findings:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5922465574.

## Required outcome

Check/lower actual Linux/Windows bootstrap products on x86_64/AArch64, with source/import contracts and negative
failure locations. Preserve conditional masks and authoritative empty ownership. Reject fabrication, replacement,
old epochs, actual mutation and retirement. Internal invariant defects panic. BRA-695 execution and BRA-696 shutdown
remain separate. No historical reconstruction, freshness from owned_allocation, layout/exactness from alias bounds,
predicate reassertion or blanket loan exemptions. Commit/push checkpoints. Style stays deferred until finalization.
Native subprocess workflows run outside the sandbox. Shared Cargo/LLVM storage must never be cleaned.

Joint gate: actual dispatch_ready -> dispatch_task/event.wait_execution must check AND lower, and all1714 fixed
comparison identities must pass on the SAME candidate. Focused controls and checker units cannot substitute.
Do not expand migration while either gate is red. The repair unit is the connected owner composition: allocation,
insertion, replay, owner moves, publication/import, escape and cleanup, including normal/panic/cancellation paths.
The first loss locates the defect, not the repair scope. No speculative preparation or local patch checklist.

## Established contracts

Logical-only transitions retain physical descriptors. Assignments capture RHS before weak commit, never initialize
all candidates or fabricate exact identity. Observation suffixes require agreement. Ambient confinement excludes only
original mutable entry borrows at layer1 with empty reference path and inline projections. Other origins stay
conservative. Direct opaque Ambient/Escape stays unbounded. Physical subtree walks share storage_subtree.

Exact typed pointee snapshots use CurrentValues, seeded only by successful actual Borrow<T> outputs. Raw, inexact,
previous and entry-fallback targets cannot seed. Exact lookup only, no parent reconstruction or revival of Absent.
Read captures before consumption, Write updates exact current destinations. Storage flow owns physical masks.
Loan/shared-view decisions publish logical and physical preservation together for Source.targets, never Source.regions.
Moves use actual Move plans. Pointees join normally but never enumerate as live owners. Producer reuse retires facts.
Runtime-only roots cannot portable-export. Opaque rereads/static declaration-only/Uninit logical paths stay conservative.

Context.pending uses existing SpinLock<Observation?> initialized by both constructors. Typed context views publish
validity. Guards end before scheduling. take_ready copies/clears under one guard. Current owns a counted view.
Preserve borrow_execution exposed-address publication independently of its temporary ExecutionOwner loan. Retirement
ends inspection before moving/destroying whole Context after zero-view and phase/task checks. task.retain_in_arena
shares HostLease/arena_anchor/retain_locked with native retention. Dispatch, worker failure and observer creation
acquire the actual Domain.arena view before pending mutation. Preserve ABI/status. No validity reassertion.

## Current candidate and evidence

Last green comparison code4f767fbb2 passes1714, checker564 and raw/returned66. Native18 still fails only dispatch_ready
and start_frame. Fixed-five414/414/408/419/434ms, cursor loop231ms. Counts not remeasured. Bound-tree136 passed.
The unproven typed-route experiment aaafc22d5 is reverted as a unit, including its later uncommitted observers.
Its pushed source remains in Git history. No parallel implementation or migration remains in the worktree.

One reviewed simplification remains: remove preserve_shared_views' duplicated access-map guard. An access entry can
represent a pointee mutation while the actual descriptor is separately disjoint. Existing physical descriptor and
referent decisions remain authoritative. Descriptor replacement, raw overwrite, retirement and source reassignment
must retain their negative outcomes. Raw/returned66 passed after this removal on the discarded typed candidate.
Current reverted candidate is not yet validated. Rebuild once, restore frozen comparison, run actual native18 and
completion_observes_the_evaluated_borrow_after_its_source_slot_changes on this same candidate, then checkpoint/push.

Useful retained deductions, not completed repairs: native dispatch_task raw Retire(DataPointer<u8>) operates through
a task.Reference Slot view whose bounds are wider than the actual freed RawAllocation. Byte/Slot type exemptions are
unsound. The discarded candidate also lost reference-bearing buffer transport despite successful generic push,
initializer source export and caller endpoint resolution/capture/commit. All120 inspected allocation anchors were
eligible singletons, so the anchor-filter hypothesis was false and removed. Do not repeat it or widen confinement.
A new connected semantic repair must explain preservation and invalidation through all downstream consumers first.

## Fixed evidence and remaining gates

Frozen comparison1718 identities on1ceb765eb is unchanged. Four provider identities remain OPEN product replacements,
never passes: production_fixture_phase_timings, production_host_nested_bodies_and_contracts_are_checked,
production_report_consumers_check_their_callback_protocol, production_storage_boundaries_prove_their_declared_memory_effects.
The same attachment/admission two identities need isolated bounded runs after grouped comparison. Keep baseline,
comparison scheduling identities and one current output per purpose in target/audit. Remove stale probe outputs.
Earlier catalog62, artifact reuse5 and boundary/cache13 remain valid. No temporary probes remain in source.

Final gates: actual four-target products/event layout, affected owned/deferred/projection/cleanup suites, frozen
comparison, five/loop counts, maintainability/DRY and AGENTS audit, mutating cargo xtask style and cargo xtask format,
fresh native compiler products, final PR description and published independent review. Only then PR ready and BRA-694
In Review. No completion claim. BRA-695 remains next separately scoped issue.
