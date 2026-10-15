# Current PR stack completion plan

Updated on 2026-10-12 with the specification-derived rule coverage and replacement plan. BRA-553 previously completed implementation and review in [PR #606](https://github.com/lejmer/bray/pull/606).

This is the shared completion plan for the current stack. The issue-specific handoffs retain investigation evidence,
reproductions and acceptance details. Their dated branch heads and earlier task boundaries are historical snapshots.
The former instruction to stop before BRA-553 is retired. The active implementation and publication decisions below supersede earlier task boundaries. They do not authorize
merging the final integration PR.

## Current plan and Linear execution

On 2026-10-12 the user required complete rule coverage before implementation, rejecting a plan that would continue
finding missing semantic implementations through successive consumers. The [rule coverage and replacement plan](runtime-stack-rule-coverage.md)
and its [closed operation inventory](runtime-stack-coverage-inventory.md) now supply the agreed rule ownership,
transitions, source coverage, removals, protocol design, costs and active issue sequence.

The [earlier proposal](runtime-stack-implementation-plan.md) is superseded. The [assessment](stack-architecture-assessment.md)
retains historical findings. The user authorized the agreed specification checkpoint and complete shared authority
analysis in BRA-620. Production migration and the later stack remain separate steps.
The user accepted the groundwork and requested actionable issues. Start from BRA-620 using the
[Linear execution guide](runtime-stack-linear-execution.md), which records the recoverable checkpoint and current order.
Issue preparation preserved inherited work and published the plan. The implementation checkpoint below records subsequent work.

## Published integration checkpoint, 2026-10-13

[Draft PR #607](https://github.com/lejmer/bray/pull/607) targets `develop` from `feature/trusted-storage-integration`.
It contains all five original PR heads. Steps 1 and 2 are complete at `5144e732`, with
[no remaining actionable Step 2 review findings](https://github.com/lejmer/bray/pull/607#pullrequestreview-5509446980).
Continue on that integration branch, using the current matrix and evidence in
[BRA-620 remaining work](BRA-620-remaining-work.md). The old repair branch retains `ffbff1f5`. Step 3 is complete at `a3de1083` and bounded Step 4 at `246c4db3`, both with clean published review. Backing/entry/incident prerequisites and
the explicit BRA-619 native acceptance step remain in the [current plan](runtime-stack-rule-coverage.md#implementation-order), so the integration remains draft and BRA-620 remains In Progress.

## Earlier baseline, retained as evidence

The branch chain is BRA-609 / #601, BRA-619 / #602, BRA-620 / #604, BRA-621 / #605, then BRA-553 / #606.
The implementation and review checkpoint for BRA-553 is `8c789dfa3febea1ec03c120016d44bf6c4b7a98a`.
Revalidate live branch heads before implementation. Earlier handoff commit IDs remain evidence for their recorded results.

BRA-621 and BRA-553 are In Review in Linear. PR #605 is still draft, while #606 is ready. BRA-609, BRA-619 and BRA-620
remain In Progress with draft PRs. Readiness of an individual issue does not establish completion of the combined stack.

## Active BRA-620 repair

The original repair branch was `feature/bra-620-repair-synchronization-authority`, based on BRA-553 commit
`cca4bd4429d0648cae0493db72045b7d4a7a3e73`. Live remote ancestry was verified on 2026-10-11: original BRA-620 head
`22473ff6384db081ad3e131e0726e16e7ff9c682` is fully contained in this base, through BRA-621 head
`fb69346e31b225c39105d095e9f900c9cebad97c`. No original BRA-620 commits were missing.

The integration branch owns the remaining BRA-620 production authority repair. Shared cleanup transitions,
exact-reference summaries and observation dependencies are complete. Next apply the agreed whole-carrier and
immutable-marker representation to production APIs. No new storage-identity or entry-state language feature is needed
by the agreed plan. Do not implement these findings independently on the original BRA-620 branch.

## Integration and publication

The user selected one master feature branch and one final PR into `develop` on 2026-10-11. Finish BRA-620 against the
combined source. Continue on the now-published `feature/trusted-storage-integration` branch and reconcile every
existing stack PR against it. Verify each live PR head is included and review its outstanding requirements and findings.
Merge additional branch commits only when they are absent. Do not copy already inherited fixes or merge individual stack
PRs directly into `develop`.

Validate and review the integrated result before making PR #607 ready. Any deliberately deferred work
must have an explicit issue and updated downstream dependencies. Incomplete issues stay open when their old PRs are
superseded. Closing a superseded PR does not establish that all of its issue requirements are complete.
Keep the existing PRs as review history until the
integration PR lands, then reconcile their superseded status. Creating the integration PR does not authorize merging it.
This replaces the earlier plan to relocate the repair onto PR #604 and restack descendants.

## Earlier remaining order, retained for context

1. Finish BRA-620's synchronization acquisition authority and sibling API audit. Resolve the associated compiler
   witness/proof findings against the combined source, preserving both valid authority and rejection of fabricated
   authority. Reconcile changes after BRA-609's and BRA-620's last clean reviews.
2. Implement [BRA-502](https://linear.app/bray-lang/issue/BRA-502): concrete physical cleanup backing before ownership
   becomes live. BRA-553 supplies its resident host prerequisite.
3. Implement [BRA-554](https://linear.app/bray-lang/issue/BRA-554): synchronous and foreign entry authority over that
   host and admitted backing.
4. Implement [BRA-558](https://linear.app/bray-lang/issue/BRA-558): typed incident consumption, value destruction and
   separate backing release.
5. Complete BRA-619's native scoped-exit incident transfer using those shared contracts, including exactly-once
   cleanup and preservation of pending outcomes under denied allocation.
6. Reconcile completed and explicitly deferred requirements, complete combined validation and review, and prepare
   the single integration PR into `develop`.

Git ancestry differs from this remaining implementation order. Preserve the combined work on the integration branch.
Do not merge a descendant back into an ancestor merely to put a repair under an older issue number. Reconcile remaining
issues and dependencies before the integration PR. BRA-619 remains incomplete until its native acceptance passes.

## Evidence and outstanding findings

| Work | Evidence and ownership |
| --- | --- |
| Synchronization acquisition authority and later refactor review | [BRA-620 handoff](BRA-620-remaining-work.md). BRA-620 owns the repair and sibling audit. |
| Native scoped-exit transfer | [BRA-619 acceptance](BRA-619-remaining-work.md) and [dependency investigation](BRA-619-native-transfer-dependency.md). BRA-502 supplies backing, BRA-558 supplies consumption, and BRA-619 completes the scoped consumer. |
| `CleanupIncidentTransfer` failure found during BRA-553 | The same BRA-619 transfer gap above. Do not create a second repair for the same failure. |
| Sibling-field witness invalidation, conditional-destructor proof limitation, and returned-owner proof after mutation through a borrow | [BRA-553 findings](https://github.com/lejmer/bray/blob/8c789dfa3febea1ec03c120016d44bf6c4b7a98a/docs/handoff/BRA-553-other-work.md). BRA-620 owns these findings. Focused sibling-field and scoped-borrow regressions now pass in the repair worktree. Shared cleanup and exact-owner analysis passed Step 2 review. Production guard representation remains Step 3. Preserve the language authority contract. |
| Contract-pattern export and loss of the specific export diagnostic | The same BRA-553 findings document. BRA-620 owns both findings. The repair worktree normalizes nullable absence for export and preserves precise publication diagnostics. Source/import checks pass and Step 2 review is complete. Final production integration acceptance remains outstanding. |
| Completed host implementation and cost evidence | [BRA-553 verification](https://github.com/lejmer/bray/blob/8c789dfa3febea1ec03c120016d44bf6c4b7a98a/docs/handoff/BRA-553-verification.md). Preserve this verified behavior through later repairs. |

## Completion criteria

- Every requirement included in the integration PR satisfies its acceptance criteria. Deferred requirements have
  open issues and explicit dependencies. No incomplete issue is presented as completed.
- Every recorded handoff finding has a fix or a conclusive, evidence-backed resolution.
- Combined compiler and native API/outcome validation has no accepted outstanding failure.
- Material changes after earlier reviews have completed review, and review findings are resolved.
- PR readiness and Linear status agree. Issues become Done through the merge workflow, not this plan.

Update this file when order or completion status changes. Keep detailed evidence in the linked handoffs instead of
maintaining separate execution plans in each one. A new dependency requires concrete evidence and an explicit owner.
