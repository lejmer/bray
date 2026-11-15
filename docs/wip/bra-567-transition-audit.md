# Async integration checkpoint

PR: https://github.com/lejmer/bray/pull/710.
BRA-567 now owns the integration baseline, not the remaining native closure. Freeze implementation in this PR.
The incomplete checkpoint is intentional for the unreleased project. Do not resume an unbounded repair loop here.

## Verified baseline

Checkpoint bc737b885 passes all 1,718 fixed comparison identities, with no failures or omissions, all 561 checker
tests and 24 focused controls. The controls include actual observer poll/destroy on Linux and Windows, source/imported
projection and witness transport, nullable extraction, raw retirement and allocation disposal. Negative expectations
and rejection locations remain fixed. Format and mutating style pass. No temporary tracing remains.

Fixed five case bodies were 307-344 ms and loop was 345 ms. Current node counts were not instrumented.
Native product compilation, execution, archive/link closure and final simplification are not established by this baseline.

Incremental review of bc737b885 is published and verified:
https://github.com/lejmer/bray/pull/710#pullrequestreview-5911302526. No new actionable findings.
Retained reviewer: /root/pr_review. Documentation-only scope updates do not reopen semantic review.

## Remaining work and dependencies

- BRA-694: native bootstrap owner and registration contract checking.
- BRA-695: root/main-lane execution, failure, cancellation and startup rollback.
- BRA-696: outcome disposal, product teardown, retained services and native implementation selection.
- BRA-697: bounded compiler/test architecture and maintainability audit.

Order: BRA-567 -> BRA-694 -> BRA-695 -> BRA-696 -> BRA-697.
All four new issues directly block BRA-507. Its other prerequisites are unchanged. Their Linear descriptions contain
standalone scope and acceptance criteria with tracked repository references. BRA-507 retains suspending host/static
fallback. BRA-559 retains child test-host policy. Performance optimization keeps its existing owners.

PR #710 still targets feature/bra-566-demanded-worker-lifetime. Parent PR #709 is open and targets
fix/bra-669-deferred-completion. Preserve this stack. No parent merge or retargeting is implied by the checkpoint.

## Contracts to preserve

Follow the active worktree's work-on-bray-issues contract gate before future cross-cutting repairs. Derive transitions
from executable code, preserve complete compositions and fixed positive/negative comparisons, and keep claims narrow.

Yielded values stay in the authoritative pending transfer until ordinary result completion. Failure cleanup consumes
that exact produced result once during receiver preparation, before conditional snapshots. Match its canonical result
slot to exclude scoped carriers and old replacements. Cancellation uses the same admission. Do not move ordinary
publication or reconstruct an overlay to make cleanup work.

Allocation backing follows RawAllocation.pointer. Versioned inline fields use explicit projections over their epoch root.
Stored aggregate roots are replacement markers, with uncertainty only on actual reference slots and incomplete lineage.
Retained borrowed contents do not confer ownership or preserve overwritten data. Preserve deferred admission d302124.

Keep main/root authority genuine, cleanup before terminal publication, rollback exactly once, and services alive through
required result/static disposal. Do not reassert predicates after callbacks or grant a shared-loan exemption from raw effects.
Internal invariant defects panic. User diagnostics retain specific structured causes.

Keep records concise and replace stale evidence. Preserve user edits, readable inline Bray fixtures and multiline
control-flow bodies. Apply style only at coherent batch finalization. Never publish references to untracked local artifacts.
