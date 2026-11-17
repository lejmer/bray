# Native bootstrap contracts

## Active refactor contract

The requested starting point is 05d2dc1b6 on feature/bra-694-native-bootstrap-contracts. Do not run baseline tests.
Replace independent interpretations with captured values, explicit storage transitions and guarded outcomes.
Keep the existing CFG, CurrentValues, storage plans, proof engine and demand-driven fixed-point computation.
Possible regions never grant identity, initialization, allocation lifetime or ownership. Preserve unknown and empty
alternatives distinctly. Normal publication follows private production and required cleanup.

The first connected replacement is outcome-specific effects. StorageFlowCollector::record_effect is the producer.
DependencyContractTemplateData, BoundCallDependencyContracts and InterfaceDependencyContract carry its normalized
transitions. Consumers include ordinary and hidden calls, input conversions, recursive summary joins, imported
contracts, deferred frames, ordered lifecycle cleanup, physical invalidation and guarantee invalidation.

Each possible transition retains its mutation kind and checked access type. Normal transitions overapproximate
changes along paths reaching normal completion. All transitions conservatively include effects before abnormal
completion. Admission captures effects against incoming memory. Normal execution applies the normal transitions.
Failure applies the captured possible transitions without producing a normal result. Deferred formation captures
inputs and leaves body transitions latent. Cleanup instantiates transitions against its current receiver.
Preservation must be derived from these transitions, never supplied as a separate boolean promise.
Failure capture preserves the incoming storage of an ordinary invocation using its resolved effects. Observing a
running task has no fresh body-entry memory snapshot at the observation boundary. Its partial execution must retain
only support justified by its possible effects. Neither boundary uses normal-result transport on failure.

Remove normal_storage_preserved, deferred_storage_preserved and has_normal_effects throughout publication,
specialization hashing, serialization, import and application. Replace them with actual normal transitions.
Do not add a parallel interpreter. Production growth must buy an explicit contract or delete an old interpretation.
Reconsider if a conversion loses outcome guards or if a consumer still reconstructs normal effects from a flag.

Regression oracles include normal mutation versus abnormal-only mutation, pre-failure effects, source/imported
equivalence, deferred execution and cleanup, and recursive stabilization. Existing negative owner, raw mutation,
retirement, overwritten descriptor and moved-source cases must remain rejected at their original boundaries.
The final joint gate remains actual native checking and lowering plus the frozen comparison on the same candidate.

Remaining migrations after this boundary include captured addresses separate from footprints, whole-value
dependency transport, connected guarded result bindings and one observation-survival classification. These require
tracing their owning producer and consumers before edits.

Validation on checkpoint d087dde8f: cargo xtask style and cargo xtask format pass. Symbols222, interface217,
checker564 and all1714 frozen comparison identities pass, with zero missing identities. The comparison takes272.38s.
The source/imported task-ownership, loop-cleanup and both production channel regressions pass together. The bounded
async-composition case passes in isolation and in the comparison. The saved-borrow source-slot overwrite case passes.
Actual Windows native18 still fails dispatch_ready and start_frame, with specific unproven contract diagnostics.
The full refactor and joint acceptance gate remain incomplete. No baseline tests have run.

This checkpoint removes the independent normal-storage flags throughout publication, hashing, import, call
instantiation and deferred transport. It binds each effect once and shares translated identities across outcomes.
The remaining work is the captured-value graph, actual addresses distinct from footprints, connected guarded result
bindings, whole-value dependency transport and unified observation survival. Continue with shared value bindings
and dependency transport. The native failures remain final acceptance checks and do not block the next slice.
Keep the PR in draft and BRA-694 In Progress.

## Binding checkpoint contract

Replace the memory References and StoredValue representations with AddressValue and ValueBinding. AddressValue
distinguishes evaluated addresses, conservative regions, displaced addresses and unknown addresses, and retains an
explicit empty alternative. ValueBinding carries that address knowledge together with whole-value dependencies and
typed completeness. A region cannot satisfy an actual-value query. Unknown is never empty. A nullable alternative
cannot establish one definite address even when its nonempty alternative has one target.

The authoritative producers are memory capture_value, address formation, checked reference output preparation and
intrinsic value transport. Capture copies the evaluated binding before source mutation or consumption. Definite
replacement publishes that captured binding. Weak writes retain feasible previous contents without granting definite
initialization. Joins union possible addresses and dependencies and intersect completeness. NoPayload predecessors
contribute no live value. Borrow observations, reference publication, effect resolution, escape traversal, deferred
capture and ordered cleanup consume the same binding. Source and imported output preparation instantiate the same
reference alternatives. Explicit empty output alternatives survive this boundary.

Stored-value summary relations remain possible dependency bounds, not whole-value equality. Store their address
bounds separately from the binding's evaluated address. Dependency staging never assigns or merges the evaluated
address. Conservative queries can use these bounds when the evaluated address is unavailable. Actual-value queries
cannot. This removes staged clone/merge/restore reconciliation and makes assembly independent of staging order.
Alternative-value joins still join actual evaluated addresses and carried bounds separately. Invalidation clears both,
and retirement and deferred relocation update both. Retain allocation recency and do not infer permissions from address
precision. Displacement and origin widening are distinct states because publication consumes both distinctions.

Conservative traversal visits carried bounds together with an uncertain evaluated address. A complete evaluated address
set supplies its own origins. Exact queries never consume the carried bounds. The audit found and corrected a fallback
lookup that would hide useful carried targets behind an unknown actual address. The staged-order regression must cover
both exact and unknown produced addresses, including their resulting reachability.
Typed pointer reads, reborrows, empty-value publication and unchanged-entry-field suppression require the evaluated
address field. A carried bound cannot supply the pointer value needed to follow a route, publish null or justify
suppressing a separately captured escape. Widening a known nonempty alternative does not erase an actual empty
alternative. Displacement from null has unknown reachability and cannot become an empty effect footprint.
Inactive nullable and union payloads carry a no-payload binding, independently of a live null pointer. Pending joins
ignore those inactive alternatives, and unknown mutation loses their absence rather than turning it into an empty value.
Captured-value assembly combines explicit reference slots with opaque lineage. CFG joins intersect coverage. Neither
operation saves and restores completeness after applying a different merge rule.

The binding batch passes564 checker,222 symbols,217 interface and all57 returned-dependency tests. The new
specialization identity control distinguishes independent, null and inactive slots. All1714 frozen comparison identities
pass in303.88s, with zero failures and zero missing identities. The final build source fingerprint is recorded with the
comparison and binary hashes. Mutating cargo xtask style and cargo xtask format pass on this source.

Portable reference bindings retain inactive-slot shape separately from null, including stored allocation payloads.
Source/import, identity remapping, specialization, SCC joins, lifecycle and deferred instantiation retain that shape.
A no-payload alternative contributes no live address and cannot erase a live alternative in either staging order.
Unknown remains unknown. The package interface format revision is2 because reference attributes gain a shape bit.
The temporary diagnostic trace is removed. The earlier nullable, propagation and escape regressions are corrected.

No native retry or repair is permitted before the full refactor is done. Continue the remaining slices after this
checkpoint. Neither native failure is a prerequisite for advancing the agreed migration.

Positive oracles cover saved pointers after source overwrite, ordinary value moves, generic buffer transport,
source/imported empty alternatives and deferred cleanup. Negative oracles cover nullable exact-location use,
InputRegions as exact locations, replacement and retirement, and generic dependency escape. Existing frozen1714
comparison identities remain fixed. Native failures remain acceptance checks, not this slice's repair target.

This is the internal binding boundary, not the complete portable captured-value graph. No new graph identities or
type-driven generic reconstruction are introduced. Replace boolean address states and tracked/complete combinations
with typed states. Keep growth below300 production lines unless required to remove a duplicated interpretation.
Reconsider if an address consumer needs to recover a removed boolean combination, if generic bounds gain identity,
or if staged binding assembly still requires saving and restoring an old address after an alternative join.
Current logs and the frozen comparison are under target/audit in the active worktree. Existing shared Cargo output
is reused and must never be cleaned. Before builds, target/cargo/tooling measured 232.83 GiB and the drive had 99 GiB free.

## Production checkpoint contract

A pending memory Transfer owns staged output bindings and captured entry information. The output fields are values,
payloads, deferred frames and returned-input bounds, with the weak-write policy. Entry information is inputs, possible
failure effects, latent deferred effects and captured/result cleanup. These have different lifetimes.

Private production drains only the output fields into current memory. Cleanup can replace or retire that produced
value, and repeated private production cannot replay its earlier snapshot. Captured entry references and abnormal
transitions remain available until an outcome consumes them. Normal completion produces any still-staged output, then
ends the capture. This replaces cleanup's use of complete_transition to expose a private receiver.

The immediate producer is prepare_cleanup_receiver for a result-region value. Normal consumers are ordinary and hidden
call completion, intrinsic completion, result-region completion and lifecycle completion. Deferred formation moves the
whole pending capture without executing it. Deferred completion installs its relocated pending output through the same
normal completion. Failure consumes captured possible effects without installing normal outputs. Do not clear the whole
pending transfer on task-body failure because the observer can still produce a normal RunResult.Failed alternative.
Retirement must keep updating retained entry descriptors and effects while private output is live.

The extended private_production_retains_captures_until_normal_completion test produces a private pointer, overwrites its input,
changes the private output during cleanup, and verifies capture retention and surviving normal publication. Repeated
production and normal completion must not restore the old pointer. The existing deferred-input move/retirement and
produced_owned_results_survive_enclosing_scope_cleanup compositions cover adjacent paths, including source/import and
lowering. Keep the frozen1714 comparison unchanged.

Use one output-draining implementation. Add no phase booleans, new identity graph or alternate commit engine. Keep
production growth below50 lines. Reconsider if captures need copying, if private production grants caller permissions,
or if the existing cleanup scheduler cannot retain its ordering. This boundary separates physical production from
capture completion. Full connected outcomes and their unified logical publication remain subsequent work.

The completed production boundary passes564 checker tests and362 deferred, returned-dependency and private-result
cleanup compositions in9.88s, including source/import and MIR lowering. Mutating cargo xtask style passes. The existing
format pass covers the maintained Bray sources, which did not change. Production grows by19 lines and the extended test
by32 lines. The full1714 comparison last passed on binding checkpoint551d12bb1 and has not been rerun after this boundary.
No native test or repair has been attempted. Continue the full refactor before either native-failure repair.

## Active whole-value tracing

The next boundary must transport a complete captured generic value without a caller-side reference-field inventory.
Entry whole-source producers are initialize_entry_references for demanded opaque paths and buffer AnyElement storage.
They seed possible generic lineage, not value equality. Intrinsic push/pop capture full internal bindings, but portable
DependencyStoredValue remains a possible-source bound. Do not promote its complete flag into actual value identity.

prepare_returned_values consumes actual DependencyOwnedTransfer identities, input paths and preservation masks. Its
stage_preserved_references currently enumerates BoundReturnedValue.observations, as does logical returned-value
reconstruction in guarantee/trusted/borrows through ValuePreservation::returned. Both must use the same captured value.
The observation list is prepared by dependency/call/references::returned_values using the substituted source type.
It cannot be removed from one consumer while leaving a second reconstruction in the other.

prepare_result_references currently creates symbolic reference outputs before prepare_stored_values adds dependency
bounds. Physical input captures live in memory Transfer.inputs while logical data, ownership, proof and duties live in
ExecutionCallEvidence.arguments and its other fields. Their admission, deferred execution and publication phases must
be traced before joining them. CurrentValues remains authoritative. Missing data must stay unknown, not empty.

The new whole-value contract is not closed yet. Trace actual argument capture, inline mutation of carried graphs,
source/import publication, deferred pointee reads versus by-value captures, and cleanup/abnormal consumption before
production edits. Reuse the existing generic buffer reachability composition and owner forwarding controls. Do not
return to dispatch_ready or start_frame while resolving this architecture.

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
predicate reassertion or blanket loan exemptions. Commit/push checkpoints. Run style at implementation-batch finalization.
Native subprocess workflows run outside the sandbox. Shared Cargo/LLVM storage must never be cleaned.

Joint gate: actual dispatch_ready -> dispatch_task/event.wait_execution must check AND lower, and all1714 fixed
comparison identities must pass on the SAME candidate. Focused controls and checker units cannot substitute.
The user's current direction is to continue the contract refactor while native failures remain. Do not focus the
next slice on repairing dispatch_ready or start_frame. The repair unit is the connected value composition: allocation,
insertion, replay, owner moves, publication/import, escape and cleanup, including normal/panic/cancellation paths.
The first loss locates the defect, not the repair scope. No speculative preparation or local patch checklist.
Complete every agreed refactor boundary before attempting to fix dispatch_ready or start_frame. Correct regressions
introduced by each slice, then continue to the next slice even while those two native failures remain. This ordering
is the user's explicit instruction. A green native gate is not a prerequisite for expanding the contract migration.

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

## Starting-point evidence

Last green comparison code4f767fbb2 passes1714, checker564 and raw/returned66. Native18 still fails only dispatch_ready
and start_frame. Fixed-five414/414/408/419/434ms, cursor loop231ms. Counts not remeasured. Bound-tree136 passed.
The unproven typed-route experiment aaafc22d5 is reverted as a unit, including its later uncommitted observers.
Its pushed source remains in Git history. No parallel implementation or migration remains in the worktree.

One reviewed simplification remains: remove preserve_shared_views' duplicated access-map guard. An access entry can
represent a pointee mutation while the actual descriptor is separately disjoint. Existing physical descriptor and
referent decisions remain authoritative. Descriptor replacement, raw overwrite, retirement and source reassignment
must retain their negative outcomes. Raw/returned66 passed after this removal on the discarded typed candidate.
The reverted starting point is not retested, following the current request. Validate the refactored candidate with
the frozen comparison, actual native18 and completion_observes_the_evaluated_borrow_after_its_source_slot_changes.

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
