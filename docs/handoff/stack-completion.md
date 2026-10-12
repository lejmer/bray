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
retains historical findings. The implementation order farther below is historical, not permission to resume.
The user accepted the groundwork and requested actionable issues. Start from BRA-620 using the
[Linear execution guide](runtime-stack-linear-execution.md), which records the recoverable checkpoint and current order.
Issue preparation preserves inherited work and publishes the plan. It does not implement the replacements or merge the integration.
## Current baseline

The branch chain is BRA-609 / #601, BRA-619 / #602, BRA-620 / #604, BRA-621 / #605, then BRA-553 / #606.
The implementation and review checkpoint for BRA-553 is `8c789dfa3febea1ec03c120016d44bf6c4b7a98a`.
Revalidate live branch heads before implementation. Earlier handoff commit IDs remain evidence for their recorded results.

BRA-621 and BRA-553 are In Review in Linear. PR #605 is still draft, while #606 is ready. BRA-609, BRA-619 and BRA-620
remain In Progress with draft PRs. Readiness of an individual issue does not establish completion of the combined stack.

## Active BRA-620 repair

The active implementation branch is `feature/bra-620-repair-synchronization-authority`, based on BRA-553 commit
`cca4bd4429d0648cae0493db72045b7d4a7a3e73`. Live remote ancestry was verified on 2026-10-11: original BRA-620 head
`22473ff6384db081ad3e131e0726e16e7ff9c682` is fully contained in this base, through BRA-621 head
`fb69346e31b225c39105d095e9f900c9cebad97c`. No original BRA-620 commits were missing.

This branch owns the remaining BRA-620 synchronization authority repair and associated compiler findings. First unify
cleanup state transitions and remove redundant analysis, then reassess the existing `Uninit` witness flow. Proposed
storage-identity and entry-state language additions are on hold. Do not implement these findings independently on the
original BRA-620 branch.

## Integration and publication

The user selected one master feature branch and one final PR into `develop` on 2026-10-11. Finish BRA-620 against the
combined source first. Then create `feature/trusted-storage-integration` from the repaired tip and reconcile every
existing stack PR against it. Verify each live PR head is included and review its outstanding requirements and findings.
Merge additional branch commits only when they are absent. Do not copy already inherited fixes or merge individual stack
PRs directly into `develop`.

Validate and review the integrated result, then open the integration PR into `develop`. Any deliberately deferred work
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
| Sibling-field witness invalidation, conditional-destructor proof limitation, and returned-owner proof after mutation through a borrow | [BRA-553 findings](https://github.com/lejmer/bray/blob/8c789dfa3febea1ec03c120016d44bf6c4b7a98a/docs/handoff/BRA-553-other-work.md). BRA-620 owns these findings. Focused sibling-field and scoped-borrow regressions now pass in the repair worktree. The remaining cleanup/owner-identity proof is still open. Preserve the language authority contract. |
| Contract-pattern export and loss of the specific export diagnostic | The same BRA-553 findings document. BRA-620 owns both findings. The repair worktree normalizes nullable absence for export and preserves precise publication diagnostics. Focused tests pass; integration review is pending. |
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
