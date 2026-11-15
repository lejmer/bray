# BRA-567 current work

Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Continue through implementation, verification and review. Checkpoint pushes and bounded full matrices are authorized.
Further performance optimization belongs in a separate PR. Compiler-owned versions stay at 1 / 1.0.
After restart, read this record and the work-on-bray-issues contract gate before editing or validating.

## Current repair

Pushed checkpoint: 18f2be5fe. Retained reviewer: /root/pr_review. Published review 5909417945 is verified.
Findings 4577296573 (commit_start regression) and 4577296576 (fixture indentation) remain open until the repair is pushed.
Earlier review threads are resolved. Both findings have local fixes verified by the focused controls and comparison. Re-review follows the checkpoint.

Producer permission now owns both native retention and exact inactive-exclusive claims. Inactive.into_producer
consumes the copied ABI token's authority. Its raw read copies data only. Root acquisition clears its mutable
native input slot and does not preserve frame_input. Restoring address bits must not permit reacquisition.
PreparedStart owns the exact Producer whose requirements it inspected, then takes its nullable slot before begin.
Pending Request owns Producer, continuation, destination/disposition and typed callback before current().
Composition adds the actual parent Cursor and origin. The task relation grants no independent child authority.
Native tuples convert once at entry. No reassertion of producer permission after callbacks is allowed.

The remaining regression came from shared references stored under a mutable holder. A direct sibling assignment
and take_nullable both reproduced it. Storage flow resolved the CURRENT physical reference slot but skipped
preserve_shared_views because no prior descriptor frame existed. The marker was absent from stable_values,
so invalidate_referents revoked its predicate before framing. This was not a proof-search failure.

Repair: classify the missing slot frame through the existing effect classifier with Address and no shared-loan
exemption. Publish via preserve_reference_slot, then apply existing current-target/exposure checks to the referent.
Retain the direct-slot write guard. A logical descriptor frame can describe the old pointee while the local
reference variable is replaced. That frame alone cannot justify physical slot preservation.
No new representation or historical reconstruction. Scope includes ordinary writes, call completion and cleanup
through preserve_storage_values. Reconsider on any replacement, raw-alias, previous-epoch or unknown-effect acceptance.

The shared regression fixture covers unchanged access, sibling assignment, generic extraction, nested reference
replacement and mutable-reference-parameter replacement. Both negatives must fail at the final observation.
Existing physical-slot, exposure and ownership controls keep their identities and expectations.
Temporary audit_changed_producer_bodies is removed from source. The retained binary still contains it.
No checker tracing remains. Superseded scratch reproduction and three timeout-run artifacts were removed.

## Evidence and next boundary

Current local repair passes 559 checker tests and all five cases of the new fixture.
The frozen 1,715 plus two added identities produced 1,712 passes, two failures and three unfinished tests.
The two failures share begin_child's empty-producer postcondition. The three unfinished tests exceeded individual 180-second bounds, then completed on corrected source:
provider profiling 178 s, native-exit 153 s, nested-host in the 181 s grouped provider run. Those final runs used 360/300 s bounds. comparison-current.json/log retain exact identities and partial evidence.
Do not expand migration until unexplained regressions are repaired. validation-current.json retains the frozen identities.
The intermediate runtime check passed 64/65 changed callable bodies. Both commit_start failures are gone.
The repeated 65-body check after direct-slot guard restoration still passes 64/65. Nested callable bodies
are covered separately: the latest provider run passes nested-host but retains the two empty-slot failures.
provider-current.json/log/err and producer-bodies.json/log/err identify their exact source and binary state.
The source manifest is generated from the producer checkpoint's changed sources, not a permanent validation list.

start_frame still fails scoped cleanup's view_owned requirement after reserve_start. Its source contracts agree:
enter and reserve establish inactive_start, commit establishes prepared_start, and exit requires prepared_start.
Normal ensures do not cover reserve_start's panic edge. The failed actual names prepare_start(...).value.selected.acquisition.
Next trace returned shared-reference transport through prepare_start, Ok extraction and scoped enter, then its
physical slot and marker preservation on normal AND exceptional reserve_start edges. No source reassertion is justified.

Last full native gate, at 86ef6ef86: 12 primary E7123 plus 15 dependent E7135. All five E7124 failures were removed.
This count predates the producer and sibling-slot repairs. Native execution remains unverified.
Other unresolved groups from that gate: borrowed views, raw allocation evidence, observer preparation and neighbor links.
allocation-current.log and links-current.log retain those unresolved traces, identified by proof-current.json.

Earlier unchanged gates: symbols 221, interfaces 217, bound-tree 136, lowering 38, tooling 115, runtime interface 29,
runtime ABI 35, test protocol 23 and backend identity 1. Current compiler changes may require affected gates again.
Source/import matrices and negative failure sites are required. A passing provider subset does not certify all runtime bodies.

The nested begin_child helper used &mut Composition, hiding its caller's private storage behind an external
input. Ambient pool/header effects then lost the ordinary empty-slot postcondition. The transition must also retain
the real protocol guarantee that the parent remains selected.
begin_child now consumes complete Composition and returns it with the admitted address. Keep the exact same
admission, producer, parent, origin and pool requirements, take-once operation and all postconditions,
including the ordinary empty producer slot. The helper owns private storage, and returns the complete current
owner for connection. No reconstruction, new type, tag-preservation widening or weaker checks. Invocation
stores raw header/output pointers, not an address of Cursor. Parent selection passes, but the ordinary empty-slot
postcondition STILL FAILS after the owned transition. Do not treat this source change as a compiler repair.
Next capture actual effects and slot preservation at activation.begin, and reserve_start on the failure edge.
For local stored references, a no-loan reference path can still name a nested slot. Determine whether the
coarse access-key guard confuses a reached effect with slot replacement before changing it.

## Invariants that must survive further repairs

- Trace producers and consumers across ownership, storage flow, liveness, lowering and cleanup. Verify real async composition
  with invalid variants. Production precedes consumption, moves retire old authority, and loop analysis stabilizes.
- Current owned layouts travel with returned snapshots before retirement. Explicit empty layouts stay authoritative.
  Nullable tag preservation alone is not payload preservation. Borrowed access never establishes ownership.
- Guarded enclosing transfers must exist at the same output path in every possible alternative. Unknown guards and empty
  alternatives cannot authorize separation. Reuse lazy per-invocation intersections without mutable proof dependence.
- Retaining identity does not retain overwritten contents. Inline preservation and reached-storage preservation are distinct.
  Actual completion mappings must carry typed borrowed fields, including generic, deferred and scoped completion.
- Staged outputs read pending producers. Other values use committed storage epochs. No historical entry-content recovery.
  Generic raw-buffer transport follows actual stored payload dependencies, including remaining contents after Pop.
- Preserve private deferred frame admission (d302124), raw exposure limits, role-sensitive snapshots and immutable proof graphs.
  Compiler invariant defects panic where discovered. Source failures preserve precise structured diagnostic causes.

## Completion and operations

Finish source/interface/native gates, including root/main authority, cleanup before terminal publication, normal/panic/cancel,
rollback exactly once, services through static disposal, synchronous control, real async execution and archive/link proof that
Rust orchestration is removed. Use async-ownership-composition.bray and runtime-root-smoke.c. Then finish maintainability
and independent PR review. Only then mark the PR ready and BRA-567 In Review. Closing follows merge.

Use optimized tooling. Set BOTH CARGO_TARGET_DIR and CARGO_BUILD_BUILD_DIR to the shared main target/cargo.
Checker incremental is disabled. LLVM is target/toolchains/llvm/active (22.1.8).
Native Rust must use installed 1.98.1-x86_64-pc-windows-msvc. Default 1.99 emits LLVM 23 bitcode.
Native LLVM/archive/link/execution runs outside the sandbox. Keep processes bounded and report partial outcomes on timeout.
Do not clean the entire shared target. Replace stale task logs instead of accumulating histories.
Run style at coherent batch finalization and format Bray. Inline fixtures need manual Bray formatting too.

Occasionally run the fixed five and loop. timing-current.json identifies them and separates provider from caller time.
Last timing: four callers 141-174 ms, loop 162 ms. Last loop count: 1,485,955 comparisons / 38,484 nodes.
counts-current.json identifies that older measurement. The four-counter patch is ignored and not applied.
Current counters do not measure destruction. An earlier run released every condition node. No leak has been established.
