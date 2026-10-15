# BRA-620 Step 4: dependency retirement and cleanup composition

## Contract and baseline

Authorized as a bounded insertion after Step 3 and before backing/native stack completion. Baseline is the reviewed
Step 3 commit `a3de108339f01a2f6487a721a6d85e1d938105bf` on `feature/trusted-storage-integration`. Step 3 acceptance at `a3de1083` is published in PR #607,
with a clean independent review. This record is the implementation map, findings and completion evidence.

A last ordinary use does not discharge pending cleanup or another live owner's retained dependencies. Retirement
must account for those obligations on the relevant control-flow path. Liveness, storage and authority must agree on
selected operations, guarded presence and normal versus abnormal completion. Missing internal handling fails fast
with useful context. Legitimate absence, opaque external effects and diagnosed source recovery remain distinct.

No new language feature, syntax, general framework or downstream validation pass. Reuse established results.
Change only concrete gaps exposed by this map. Batch edits before builds. No broad test reruns without changed
behavior or unresolved evidence. Keep production growth small, remove duplicated transfer rules and avoid new
per-operation allocation or a second cleanup/lifetime solve.

## Obligation map before changes

| Obligation | Existing owner and evidence | Required closure |
| --- | --- | --- |
| Retirement after pending cleanup | `storage_flow/model/input.rs::prepare_cleanup_retention` indexes checked owner retention. `state.rs::retains_borrow_for_cleanup` checks live, not fully moved owners. Operation, edge and scope retirement use it. Cleanup-free storage retires only after ordinary last use and no active borrow. | Clarify `LastUse` contract. Exercise task consumption, surviving nested/moved owners, branches, replacement and cancellation. Check represented partial moves conservatively retain all still-required loans. |
| Consistent cleanup | `guarantee/cleanup/transition.rs::dispose` selects the prepared guarded lifecycle operations. Storage flow supplies initialization/move/absence and publishes observations. Execution checking consumes those published mutations. `body_semantics.rs` reruns liveness only when cleanup feasibility removes edges. | Trace normal/abnormal cleanup, nullable/union guards, replacement, scope exit and suspension. Backward liveness must not invent a competing cleanup interpretation. |
| Explicit missing semantics | `liveness/effects/core.rs` has sparse occurrence effects, with legitimate empty entries. Graph and access-plan lookups are internal identities. Suspension has Await, Yield and ScopedCall, but only Await currently selects Attempt semantics. `liveness/solve.rs` repeats the same backward transfer equation. | Make suspension handling exhaustive, preserve explicit scoped call attempt/completion, remove duplicate backward transfer, replace impossible missing identity fallbacks with contextual assertions. Audit equivalent operation/cleanup matches. |
| Composition | `trusted_contracts/sync.rs` covers task cleanup, guarded cleanup, nested owners, mutation, loops, replacement, opaque callbacks and production guards. `unit/storage.rs` covers last use, entry owners, scalar temporaries, branches and escaping dependencies. `export/build/tests/returned_dependencies.rs` covers generic/imported dependencies and reference replacement. Lowering lifecycle tests cover failed construction, replacement and retained exit values. | Add only missing distinct compositions, including imported cleanup-bearing owners. Run the affected suites as one batch, then format and audit. Record exact evidence, not just Step 3 production success. |

## Initial findings and distinctions

- `LastUse` currently documents unconditional lifetime termination, although storage flow intentionally adds cleanup
  and surviving-owner conditions. This is a contract documentation gap.
- Suspension retention combines ordinary liveness and invocation dependencies, but omits owners retained solely for
  pending cleanup. The same omission loses their retained loans and thread/product roots across suspension. Publish
  this obligation from storage flow using the existing cleanup-retention index.
- Liveness treats Yield suspension as an ordinary completed operation. The graph now publishes completion only on
  resume, as for Await. ScopedCall already has a separate attempt operation, so its suspension must not repeat it.
- Liveness duplicates scope-exit retention, recovery and use/definition transfer between block preparation and
  per-operation collection. One implementation can serve both without another representation.
- Missing graph operation/edge and planned storage access records currently become no effect or source recovery.
  Those records have established internal identities and must fail fast if absent.
- Sparse effect entries, empty edge-retirement lists and unknown opaque external effects are intentional. A wildcard
  is not itself a defect. Each no-effect category needs an explicit reason, and newly added operation kinds must force
  an owning decision.
- Static owner-retention summaries conservatively include possible assignments. Storage flow uses current owner
  liveness and move state. Do not replace lifetime anchors with physical referents or infer authority from aliasing.

## Acceptance status

Step 4 is complete at `246c4db37107e5e6fc57d636b8b2536168f89fa2`. Mapping, implementation, compiler/native checks,
formatting and the [independent published review](https://github.com/lejmer/bray/pull/607#pullrequestreview-5526458884) are complete. The verified review reports no actionable
findings and no inline comments. Each obligation has supporting evidence below. All identified concrete gaps are
repaired in their owning analysis and pushed to PR #607.

BRA-619 is an explicit later acceptance step in this PR, after its backing/entry/incident prerequisites. Its pending
outcome and retained typed-incident contracts constrain this step. Step 4 does not establish physical incident
transport or final native acceptance, and BRA-619 must re-run affected compositions when that transport is complete.

## Cross-step contract: BRA-619

The missing native `CleanupIncidentTransfer` role is not an authority-analysis failure. Lowering already emits the
selected exit and preserves the pending transfer while moving an exit error into an incident. The native provider
cannot yet own arbitrary typed payload backing under denied allocation. That missing owner affects physical retention,
provider lifetime and final native acceptance, so it is explicitly Step 6 after Step 5 supplies the owner contracts.

For this step, check that ordinary last use cannot retire the pending transfer's dependencies or those of any
surviving owner. `effects/core.rs::retain_owner_control_transfer_dependencies` and the shared backward transfer retain
exit-value dependencies on preserving scope-exit paths. `cleanup/transition.rs::dispose` derives completion from
normal/abnormal outcomes and guarded initialized parts. Published cleanup effects are consumed downstream. Existing
MIR tests for propagation, failed entry and preserved pending outcomes remain part of the batch. No Step 4 change
assumes a fixed panic record, immediate incident destruction, or allocation after failure. These are mandatory design
inputs for Steps 5 and 6, not deferred architectural choices.

## First implementation batch

The cleanup-retention index now records checked dependency subjects and their possible owners, using the already
prepared cleanup requirements. Storage flow uses the same live/not-fully-moved owner predicate for ordinary retirement
and suspension publication. A suspension snapshot carries the unresolved cleanup dependencies. Async retention consumes
that result alongside ordinary liveness and invocation requirements. This includes thread/static dependencies, rather
than retaining only local borrow IDs. No additional solve or downstream cleanup traversal was added.

Backward block preparation now calls the same operation transfer used for last-use collection. Missing internal graph
and storage-plan records fail fast in both dependency and evaluated-value liveness. The liveness operation match handles every current operation kind and distinguishes
Await/Yield attempts, normal completion and scoped suspension between separate invocation phases. Cleanup transition
matches explicitly name operations with no disposal work. Diagnosed source cleanup-order recovery is named Recovery,
remaining distinct from opaque native effects and missing implementation.

The first composition batch includes source/imported task ownership, moved and discarded owners, branch-specific join
and cancellation, nested construction, replacement, and suspension with an unused cleanup owner. The imported fixture
initially started a task in a synchronous factory, which Bray forbids. It now returns an inactive Future and starts it
inside the async caller, preserving the intended imported dependency test. The corrected fixture and the consumed-owner suspension case both pass. See the acceptance evidence below.

## Composition evidence map

| Distinct transition or boundary | Acceptance evidence |
| --- | --- |
| Constructed, nested, moved and discarded cleanup owners | `task_cleanup_retains_loans_until_the_task_is_consumed`, `nested_owner_cleanup_preserves_its_required_authority`, `cleanup_materializes_every_verified_lifecycle_storage`. |
| Branch-specific consumption and cancellation | Task composition cases consume on one branch versus all branches. `conditional_cleanup_requires_authority_for_the_selected_payload` and `conditional_cleanup_cannot_publish_unconditional_guarantees` cover guarded cleanup authority. |
| Mutation and replacement | `memory_evidence_uses_the_same_cleanup_state_as_authority`, `cleanup_guarantees_reach_following_blocks_and_replacement`, `replacement_guarantees_follow_the_remaining_partition`, `unit_valued_replacement_retains_old_owner_cleanup_exits`. |
| Suspension and temporary owners | `suspended_results_are_defined_only_on_normal_completion`, `suspension_retains_unused_cleanup_owners_and_their_dependencies`, scalar temporary and failed-call input retirement tests in `unit/storage.rs`. |
| Source/imported publication | `imported_task_owners_retain_dependencies_through_cleanup_and_consumption`, imported generic owner forwarding and reference replacement tests in `export/build/tests/returned_dependencies.rs`. |
| Construction failure and pending transfers | `default_failure_keeps_initialized_inputs_in_cleanup`, `generic_union_returns_preserve_the_callable_result_through_cleanup`, `propagated_nested_scope_exit_publishes_its_cleanup_plan`, scoped lifecycle tests. Native typed incident transport is the explicit Step 6 acceptance gate. |
| Scope and destruction outcomes | `cleanup_reference_outputs_feed_the_next_lifecycle`, `cleanup_keeps_completion_guarantees_for_represented_children`, `task_cleanup_revokes_preceding_guarantees`, production loop/guard destruction tests. |

These tests exercise distinct operations and their compositions. Source and imported calls share checked summaries,
so no new interface format or second imported cleanup algorithm is required. Partial owners conservatively retain
loans while represented cleanup remains. No test treats a passing production suite as proof of this entire map.

## Acceptance evidence and maintainability audit

The first composition batch ran 327 tests covering the mapped trusted contracts, async queries, storage retirement,
imported dependencies and lowering. It passed 326 cases. The sole failure was the invalid synchronous test factory
noted above. After correcting it, all three selected task/import/suspension tests pass, including the consumed-owner
case. The final bound-tree and checker run passes all 123 and 321 tests respectively. Logs are
`step4-composition-1.log`, `step4-focused-2.log` and `step4-analysis-2.log` in the shared target's
`bra-620-authority` directory. The final sibling-path audit also replaces silent graph/expression lookup skips in evaluated-value liveness and
explicitly lists refinements that do not read cached values. The same audit makes a missing selected dependency contract, bound subtree node or bound Await node fail fast instead
of silently losing dependencies or becoming source recovery. With the final changes, all 123 bound-tree tests, 321
checker tests and 73 selected composition/lowering tests pass (`step4-analysis-4.log`, `step4-focused-4.log`).

Native acceptance passes all 160 API cases, seven API plans and 25 outcome cases (`step4-native.log`). This covers the
behavior changes. The later evaluated-value edits only replace impossible missing-record fallbacks with assertions
and spell out existing no-read refinement cases. All later invariant-only changes pass focused compiler checks without repeating unchanged native
acceptance. Rustfmt, mutating `cargo xtask style` and final maintained Bray formatting pass. The separate `cargo xtask format` command also passes.

Re-read AGENTS.md and reviewed every changed file against crate ownership, API contracts, visibility, module/function
size, error handling, numerical policy, comments, ownership, unsafe code and formatting rules. No new crate, type,
unsafe operation, numeric conversion, runtime allocation or runtime authority check is introduced. Existing large
cohesive modules receive small changes to their existing operations, with no new size exemption.

Repo-wide behavior searches covered last-use transfer and `SetTransfer`, scope-exit retention, cleanup requirements,
live/fully-moved owner checks, operation/access identity lookups, every `CleanupMutation` arm, every suspension-state
constructor and every retained-suspension consumer. Ordinary retirement and suspension publication now use one existing
retention index and one live-owner predicate. Shared backward transfer replaces the duplicated equation. The new
snapshot field publishes an established result through the existing immutable storage-flow boundary. It does not add
a solver, interface format, downstream proof or cleanup traversal.

Production Rust changes before any review fixes are +243/-136 physical lines, a net +107. Test changes are +240/-4.
The production growth buys explicit suspension-retained cleanup dependencies, exhaustive operation handling and
contextual invariant failures. The existing index is widened instead of adding a parallel representation. Extra
snapshot work is proportional to indexed cleanup dependencies at suspension points, with no new ordinary-operation
allocation. Compilation and generated-code performance claims still require the planned cost acceptance.
