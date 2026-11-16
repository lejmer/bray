# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch feature/bra-694-native-bootstrap-contracts, develop base 5de46243b. Use git HEAD for the current checkpoint.
Retained reviewer /root/bra694_review. Published review through4f767fbb2 has no findings:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5922465574.

## Required outcome

Check/lower actual Linux/Windows bootstrap products on x86_64/AArch64, with source/import contracts and negative
failure locations. Preserve conditional masks and authoritative empty ownership. Reject fabrication, replacement,
old epochs, actual mutation and retirement. Internal invariant defects panic. BRA-695 execution and BRA-696 shutdown
remain separate. No entry reconstruction, freshness from owned_allocation, layout/exactness from alias bounds,
predicate reassertion or blanket loan exemptions. Stop on unexplained regressions. Commit/push checkpoints.
Style remains deferred until finalization. Native subprocess workflows run outside the sandbox.

Joint gate: the actual dispatch_ready -> dispatch_task/event.wait_execution composition must check AND lower, and all
1714 fixed comparison identities must pass on the SAME candidate. Neither focused controls nor checker units substitute
for either gate. Do not expand the repair while either is red. More preparation mechanisms, accumulating exceptions or
new semantic assumptions require reconsidering the original hypothesis, not extending its checklist.

## Established contracts

Logical-only transitions retain physical descriptors. Assignments capture RHS before weak commit, never initialize
all candidates or fabricate exact identity. Observation suffixes require agreement. Ambient confinement excludes only
original mutable entry borrows at layer1 with empty reference path and inline projections. Other origins stay
conservative. Direct opaque Ambient/Escape stays unbounded. Physical subtree walks share storage_subtree.

Exact typed pointee snapshots use CurrentValues, seeded only by successful actual Borrow<T> outputs. Raw, inexact,
previous and entry-fallback targets cannot seed. Exact lookup only, no parent reconstruction or revival of Absent.
Read captures before consumption, Write updates exact current destinations. Storage flow owns physical masks.
Loan/shared-view decisions publish logical and physical preservation together for actual Source.targets, never
Source.regions. Non-call moves use actual Move plans. Pointees join normally but never enumerate as live owners.
Producer reuse retires dependent facts. Runtime-only roots cannot portable-export. Opaque field-reread origins,
declaration-only static keys and unrepresentable UninitPayload paths remain conservative.

Context.pending uses the existing SpinLock<Observation?>, initialized by both constructors. Typed context views
publish lock validity. Guards end before scheduling. take_ready copies/clears under one guard. Current owns a counted
view, not exclusive mutation. Preserve borrow_execution's exposed-address publication independently of its temporary
ExecutionOwner loan. Retirement inspects, ends the view, then moves/destroys the whole Context after zero-view and
phase/task checks. task.retain_in_arena shares HostLease/arena_anchor/retain_locked with native retention. Dispatch,
worker failure and observer creation acquire the actual Domain.arena view before pending mutation. ABI/status remain.

Implicit cleanup observes actual stored descriptor reads through observe_stored_reference, like explicit calls.
Parent pointer reads precede nested owned paths. Snapshot the skipped conditional branch before these reads. Never
revive unknown/inexact locations or establish trusted facts, initialization, ownership or historical contents.

## Active typed-path contract

The real retained task path crosses Arena.entries/UninitPayload/slabs/NullableValue/OwnedTarget/AnyElement/Slab.slots/
OwnedTarget. Result staging and indirect effect resolution previously flat-appended projections, losing actual
allocation edges. StorageReferencePath now retains typed borrowed/address loads and declared owned-load policies.
State owns one current/staged/captured traversal. Each nonterminal load uses actual, current, nondisplaced and exact
reference records with matching payload types. Dynamic selectors stay dynamic. Closed inexact bounds are allowed
only at a terminal region endpoint. Traversal creates no ownership, freshness, initialization or trusted authority.
Staged lookup uses matching pending payload types and current fallback only outside the staged result root.

BoundCallReference retains complete source-relative target routes. Binding uses entry source and exit pointee types,
including explicit dereferences. Indirect effect suffixes are prepared immutably once for calls, deferred execution,
capture/result cleanup, lifecycle and buffer cleanup. All consumers use the same typed traversal. Logical proof still
withholds exact-origin authority from region relations. Runtime buffer-type search and path segmentation are removed.

First-batch comparison exposed six regressions. Concrete corrections now written: an actually empty route must be
recognized BEFORE expanding Returned external bounds. A nonempty referent with empty external bounds still exists.
Allocation-result publication types only the actual projected target using checked addressed_type. It grants no
initialization or invented root layout. Existing null/allocation tests cover both distinctions.

Preparation must inventory every pointer prefix in typed target routes, not just source endpoints. Buffer policy
retains its declared owner type. One finite helper adds full source routes and each owned Buffer pointer prefix,
then populates the existing buffer inventory before entry classification. Normal/deferred call routes, result/static
routes, lifecycle source/target routes, and buffer cleanup source/target routes all enter that helper. Result root
completion type is retained in the existing call subject table, including nonreference aggregates. Consumers require
actual Borrow/reference kinds before publishing authority. Declaration metadata alone creates no entry identity.
Inline demand closure deliberately stops at ownership/address boundaries and cannot substitute for these prefixes.

Keep all existing fixed outcomes. Oracles include nested_buffer_views_preserve_checked_storage_effects, reference-
bearing RawBuffer source/import escape rejection, production channel bodies/fabrication/endpoint loops/async cleanup,
and raw/returned controls. Existing nested-backing test covers current/staged overwrite/retirement, unknown/inexact/
displaced middle edges, terminal bounds and emptiness. No bespoke provider or expectation changes. Target under300
net production lines and one policy enum, no second graph. Reconsider above400 lines, invented layout/identity,
widened authority, repeated type queries in the fixed point or parallel traversal policies.

## Current evidence and next action

On4f767fbb2 all1714 retained frozen identities pass, checker564, raw/returned66 pass. Native18 still only dispatch_ready
and start_frame failures, involving original selected.acquisition/status after register_task/wait_execution effects.
Fixed-five414/414/408/419/434ms and named cursor loop231ms. Counts not remeasured. Bound-tree136 passed previously.

Current canonical source-mapper batch compiles, checker564 passes0.23s. The unchanged bound-tree136 passed earlier.
The four-case comparison11.35s now passes nested_buffer_views including its replacement negative. Remaining THREE
regressions are channel declarations/fabrication and endpoint loops. Raw-buffer source/import reachability and async
channel composition passed on the preceding publication/preparation batch and need reconfirmation with the full suite.
Native18 isolated86.26s on that preceding batch still had only dispatch/start failures. It is not new-candidate evidence.
Fixed-five926/1103/821/807/844ms and cursor loop437ms, counts not remeasured, machine contention not isolated.

Prep repair inventories both staged inputs and all original source places through existing
ValueInputs::projected_input_value, including conversion adaptation and carried components. Runtime invocation_argument
deliberately hides staged operands and was the wrong accessor here. Source and staged projection slices stay distinct.
This supplies DirectoryLink.directory plus nested owner prefixes before entry seeding. rewrite_link still retires
initial records with no late reconstruction. Independent pre-build review found no concrete issue in this mapper.

Temporary probes confirmed missing nested buffer pointer records, not terminal-bound rejection, in channel and native
effects. First native example is pop_record_value, followed by host.product paths. Both broad probe runs hit bounds
and do NOT supply final gate evidence. Probes are removed. Compact examples remain in typed-path-current.json and
oversized stderr streams are removed. Channel failures first arise at duplicate and receive_waiters.pop_head.
No read/authority policy has changed. No active build/test processes. Current typed-traversal hypothesis remains
unverified and is reopened: missing current descriptor records behind checked SpinLock views still prevent finite
effect resolution. Trace the selected actual bootstrap chain's first loss, including reference publication and
framing/escape consumers. Do not add another preparation extension or late reconstruction on speculation. Restore
the three channel identities and the selected real chain together before broader migration or new failure families.

Frozen baseline1718 on1ceb765eb remains unchanged. Four removed provider identities remain OPEN product-gate
replacements, never passes: production_fixture_phase_timings, production_host_nested_bodies_and_contracts_are_checked,
production_report_consumers_check_their_callback_protocol, production_storage_boundaries_prove_their_declared_memory_effects.
The same attachment/admission two tests require isolated bounded runs after grouped comparison. Preserve baseline,
current comparison and scheduling identities. One ignored output per purpose in target/audit. Remove superseded traces.
Shared Cargo/LLVM storage is reused and must never be cleaned. Earlier catalog62, artifact reuse5 and boundary/cache13
remain valid.

Final gates: actual four-target products/event layout, affected owned/deferred/projection/cleanup suites, fixed
comparison, five/loop counts, full maintainability/DRY and AGENTS audit, mutating cargo xtask style and cargo xtask format,
fresh native compiler products, final PR description and published independent review. Only then PR ready and BRA-694
In Review. No completion claim. BRA-695 remains the next separately scoped issue.
