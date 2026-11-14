# BRA-567 current work

Read this record and the work-on-bray-issues contract gate after compaction.
Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Commits/pushes and full bounded owned/deferred matrices are authorized. Continue through final review.
Further performance optimization belongs in a separate PR. Keep compiler-owned versions at 1 / 1.0.

## Current code and verification

Pushed checkpoint 8992071f1 preserves ownership guards, tagged payload snapshots, returned nullable
shape, compiler-only conversion values and definite absence during scope cancellation. It also shares
the owned-storage preservation mapper. Source discovery uses all selected production roots.

Checkpoint evidence in target/audit:
- transition-validation.json: checker559, frozen30, sync and native-thread ownership pass.
- Owned matrix492/496, deferred292/296. Remaining patterns in both source/import modes:
  conditional_optional_join, select_nullable_inputs_join_both, task_frame_borrow_ends,
  task_returns_observation_future.
- fixed-move.json: fixed916 has897 pass,11 fail,8 unverified. Eight earlier failures are repaired,
  three previously incomplete cases pass, and no prior passing identity regressed. The first wrapper
  lost four timeout outputs through a str/bytes error. Recovery ran only unsaved identities and wrote
  directly to files. Keep doing that. The eight unverified cases still require bounded individual runs.
  Keep exact identities and negative failure locations. A passing subset is not final verification.
- The full-provider report fixture exceeded240s. body-trace.log identifies serial production-body
  checking through entry/bridge::install, without identifying a particular hot transitive query.
- Previous stored-value checkpoint:221 symbol and217 interface checks passed. Version alignment:
  tooling115, runtime interface29, runtime ABI35, test protocol23 and backend identity1 passed.

Checkpoint0b7f698d2, pushed:
- Physical in-place framing should retain each visited component's existing owned role, including
  implicit input/projection representations. Standalone mask application keeps its narrower contract.
  Tag-only preservation cannot retain ownership. Extracted owners and unvisited children stay retired.
  The mode distinction now passes the fixed retained_discriminants test and all559 checker tests. Extend the existing mutation test for nested physical framing.
- Result observation typing uses PostconditionResult storage_type, including checked root accesses.
  This is the produced value's type, not the Boolean clause type. Explicitly enabled inactive cleanup,
  attachment protocol and shutdown owner checks now pass. Frozen30 and checker559 also pass.
  Numeric domain normalization and intrinsic totality already exist. Do not duplicate those policies.

All temporary hooks are removed. move-validation.json records checker559, frozen30 and all five
composition checks passing. Owned501/505 and deferred294/298 include their helper tests. Their four
failing cases each are unchanged. Bound-tree136 and lowering38 pass. production-total-checkpoint.log records
the three --include-ignored production passes. The staged scalar Result had no observed range before
the storage_type fix. The discarded witness probe observed replay rather than the producer and did
not locate witness removal. Scope cleanup previously diverged because replay left moved sources live.

## Unresolved causes and fixed constraints

Nullable traces in nullable-trace.log locate loss during cleanup framing before portable publication.
Optional join reduces whole preservation to payload-only. Nested join changes an owned payload into
scalar/unknown and eventually publishes only empty alternatives. Determine physical overlap before
preserving more data. Existing explicit-exit controls pass. Empty ownership remains authoritative.

witness-trace.log shows held witnesses present during inner future capture but missing before the
RunResult join in the two deferred failures. Capture logging describes future construction, not body
completion. Locate the actual removal operation. Do not remove the admission availability filter:
carrying dormant evidence must not grant authority before the owning completion.

Move replay now uses the checked operation's exact plan, status and access target at the same CFG phase.
Producer apply_valid_operation and replay both call observe_move. Previously replay retired evaluated
slots only, and staged reads of locals could have no evaluated slot. This repairs all three scope controls.
Preserve
entry snapshots before vacating the source, and complete the destination afterward. Copy, rejected
moves and failure-before-admission must not retire source storage. Scoped, projected and staged moves
share this rule. Keep composition, fabrication, invalid-copy, loop and source/import controls fixed.
Publish the exact plan instead of reconstructing operation identity from node/expression/access.
Share its lookup with lowering, removing lowering's separate lookup machinery. No new authority or
cleanup registry. Reconsider if any move is applied at a different producer phase or without a valid status.
The existing lookup test now distinguishes node and call-input accesses with the same expression and
storage access. Lowering's separate lookup/index was removed. Missing selected cleanup mutations still panic.

Several production fixtures share one total diagnostic at SourceId370, bytes29439..29515. Rust path
component ordering maps this to std/os/generated/x86_64_unknown_linux_gnu.bray, the write postcondition
using truncate_to<usize>(result). The source-discovery change exposed this obligation. Negative source
arithmetic must still reject unbounded intermediate values after any result-type repair.

Other fixed failures include dormant/retired authority invalid acceptances, extracted-owned lineage,
loop cleanup, a diagnostic name invariant and outdated fixture API/type usage. Group by actual cause
before editing. Preserve fabrication, overwrite, raw/atomic mutation, imported contracts and loop controls.

## Stored-value and transition contracts

RawBuffer Push/Pop transport payload dependencies through the existing State.values/Transfer/
ReferenceExit graph. Whole-source lineage is possible data, never borrow/equality/witness authority.
Complete-empty differs from missing/unknown. Exact overwrite replaces contents, conditional and weak
stores retain possible survivors, Pop preserves remaining elements. Publish only normal completion.
Resolve output allocation anchors before payload replay. Scalar buffers stay independent. No historical
entry resurrection. Sources/destinations use actual Receiver/Parameter/Result/static storage paths.
Result is forbidden as an entry source. Allocation payload is owner.OwnedTarget.AnyElement.

Trace executable operations and consumers, prove complete async composition with invalid variants,
check phase properties, keep the fixed comparison, and make claims name their paths and exclusions.
A regression blocks expansion. Compiler defects panic at the violated invariant. Preserve specific
structured diagnostic causes. Keep inline Bray fixtures readable. Style runs at batch finalization.

## Remaining gates and performance scope

Final gates: corrected fixed916 and owned/deferred matrices, source/interface checks, full native
runtime-artifact smoke-test outside the sandbox, maintainability audit, and independent draft-PR review.
Verify root/main authority, cleanup before terminal publication, normal/panic/cancellation, rollback once,
services through static disposal, synchronous control and archive/link removal of Rust orchestration.
Use async-ownership-composition.bray and runtime-root-smoke.c. Preserve deferred-admission d302124.
Mark PR ready and Linear In Review only after all gates. Close the issue only after merge.

Use optimized tooling, shared target/cargo and target/toolchains/llvm/active. Checker incremental is off.
Delete stale task traces when replaced. Never clean the entire shared target. Current production and
regression evidence is separated from probe builds by hashes and patches. Keep records concise.

Performance optimization is paused, regression monitoring is not. At coherent correctness checkpoints,
run the fixed five from proof-indexed-current.json with bounded execution. Its fifth case,
same_site_independent_second, is the loop. Compare condition comparisons and constructed nodes with
proof-count-current.json, and caller phase timing with proof-indexed-current.json. Keep provider time
separate. Shared-machine timing is informational. Investigate large unexplained count regressions.
Baseline: fixed five140-154ms, loop154ms,1,471,543 comparisons and zero interner lookups.
Current performance-current.json: all five pass at168-199ms, loop199ms. Loop comparisons1,481,729
(+0.69%), constructed nodes38,060 versus38,058. Counter patch retains only the four measured hooks.
Timing and counts use separate runs. No large regression. Production count hooks are removed.
The remaining graph cost crosses joins, owner/capture substitution, proof scopes and publication replay.
Retained models: immutable structurally hashed conditions, scope-keyed ProofMemo, ordered BooleanGraph,
role-sensitive RetainedSnapshots and immutable prepared comparisons. Preserve source precedence,
branch budgets, actual predecessor evidence, absence/unknown and owner/contents/capture distinctions.
No new caches or evidence representations without removing superseded machinery.

Nullable preservation rule: a retained discriminant plus the complete NullableValue payload retains
the whole value. A tag alone or only part of the payload cannot. Use that rule in both actual mask
application and portable mask publication, retaining the existing source rather than reconstructing it.
Existing tag-only and partial-payload controls remain fixed.
