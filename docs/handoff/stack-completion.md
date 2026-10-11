# Current PR stack completion plan

Updated on 2026-10-11 after BRA-553 completed implementation and review in [PR #606](https://github.com/lejmer/bray/pull/606).

This is the shared completion plan for the current stack. The issue-specific handoffs retain investigation evidence,
reproductions and acceptance details. Their dated branch heads and earlier task boundaries are historical snapshots.
The former instruction to stop before BRA-553 is retired. This documentation update does not start the remaining
implementation work, change issue contracts, or authorize merging the stack.

## Current baseline

The branch chain is BRA-609 / #601, BRA-619 / #602, BRA-620 / #604, BRA-621 / #605, then BRA-553 / #606.
The implementation and review checkpoint for BRA-553 is `8c789dfa3febea1ec03c120016d44bf6c4b7a98a`.
Revalidate live branch heads before implementation. Earlier handoff commit IDs remain evidence for their recorded results.

BRA-621 and BRA-553 are In Review in Linear. PR #605 is still draft, while #606 is ready. BRA-609, BRA-619 and BRA-620
remain In Progress with draft PRs. Readiness of an individual issue does not establish completion of the combined stack.

## Remaining order

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
6. Complete combined validation and review, reconcile PR and Linear status, and prepare the dependency-ordered merge.

Git ancestry differs from this remaining implementation order. Preserve the combined work when preparing issue-specific
repairs and prerequisite branches. Do not merge a descendant back into an ancestor merely to put a repair under an older
issue number. Reconcile final PR boundaries before merging. BRA-619 remains incomplete until its native acceptance passes.

## Evidence and outstanding findings

| Work | Evidence and ownership |
| --- | --- |
| Synchronization acquisition authority and later refactor review | [BRA-620 handoff](BRA-620-remaining-work.md). BRA-620 owns the repair and sibling audit. |
| Native scoped-exit transfer | [BRA-619 acceptance](BRA-619-remaining-work.md) and [dependency investigation](BRA-619-native-transfer-dependency.md). BRA-502 supplies backing, BRA-558 supplies consumption, and BRA-619 completes the scoped consumer. |
| `CleanupIncidentTransfer` failure found during BRA-553 | The same BRA-619 transfer gap above. Do not create a second repair for the same failure. |
| Sibling-field witness invalidation, conditional-destructor proof limitation, and returned-owner proof after mutation through a borrow | [BRA-553 findings](https://github.com/lejmer/bray/blob/8c789dfa3febea1ec03c120016d44bf6c4b7a98a/docs/handoff/BRA-553-other-work.md). Reproduce on the combined tip and establish active ownership before implementation. Resolve conservative limitations against the language contract without weakening authority. |
| Contract-pattern export and loss of the specific export diagnostic | The same BRA-553 findings document. Active ownership still needs to be established. Both findings remain outstanding. |
| Completed host implementation and cost evidence | [BRA-553 verification](https://github.com/lejmer/bray/blob/8c789dfa3febea1ec03c120016d44bf6c4b7a98a/docs/handoff/BRA-553-verification.md). Preserve this verified behavior through later repairs. |

## Completion criteria

- Every stack issue and required prerequisite satisfies its acceptance criteria.
- Every recorded handoff finding has a fix or a conclusive, evidence-backed resolution.
- Combined compiler and native API/outcome validation has no accepted outstanding failure.
- Material changes after earlier reviews have completed review, and review findings are resolved.
- PR readiness and Linear status agree. Issues become Done through the merge workflow, not this plan.

Update this file when order or completion status changes. Keep detailed evidence in the linked handoffs instead of
maintaining separate execution plans in each one. A new dependency requires concrete evidence and an explicit owner.
