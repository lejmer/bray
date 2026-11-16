# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: `feature/bra-694-native-bootstrap-contracts`, based on develop 5de46243b.
Pushed: 3b77524da. Reviewer: `/root/bra694_review`.
Verified published review through that commit:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5916535230. No findings.

## Required outcome

Check and lower actual Linux/Windows bootstrap products for x86_64 and AArch64. Preserve source/imported
contracts, exact negative sites, conditional masks and authoritative empty ownership records. Reject fabrication,
replacement, old epochs, actual mutation and retirement. Compiler defects panic. BRA-695 execution and BRA-696
shutdown remain separate. Performance work is separate except shared setup reuse already authorized.

Trace equivalent producers and all consumers before semantic edits. Never recover historical contents, derive
freshness from owned_allocation, recover exact cells/layout from alias bounds, reassert predicates, or waive effects
for an entire loan category. Commit/push coherent checkpoints. Keep style until finalization.

## Current repair contract

Frozen comparison on 3b found two regressions. Restore physical logical-only consume/write guarding without
filtering finite alternatives into false singleton writes. This fixes scoped_borrow_postconditions_survive_capability_retirement
case4 in both sync and async. Existing finite-choice fixture now checks sibling preservation before real overwrite.
Observation targets are deduplicated and use a field suffix only when every overlapping candidate agrees.

The remaining async composition failure is at await direct(), bytes1434..1456, before visits+=1. The child has no
receiver/arguments, captures no owner, and publishes only Ambient after deferred adaptation. Actual loss:
ambient_regions -> reachable_regions treats the unescaped Entry Borrow receiver as an external root.

Rule: the original exclusive entry borrow does not publish its inline referent to ambient work. Shared,
nested and deeper-layer borrow roots stay conservative because the root metadata does not establish their own
exclusive authority. Exclusion requires mutable entry_borrows, layer1 and an empty reference path. Entry Address origins,
explicit escapes, external storage and current stored reference edges remain authoritative. Raw aliases can still
invalidate a borrowed owner. Unknown and whole invocation Escape remain conservative. No survival shortcut. Do not broaden this rule without separate authority evidence.

Producer/consumer inventory:
- begin_entry_borrow seeds the entry referent plus the owned parameter reference slot. Entry reference preparation
  separately seeds ordinary raw fields with Entry Address origins. RawBuffer pointers instead retain the borrowed
  owner plus OwnedTarget, so preserve non-inline entry origins and their current stored edges.
- reachable_regions currently adds all Entry contents and externally stored reference slots. Restrict automatic
  roots to actual external origins. Only inline original exclusive Entry Borrow referents are excluded. OwnedTarget and other
  non-inline routes stay conservative. Returned closed bounds must use the same classification, so a forwarded
  borrow cannot republish the private inline root. Bounds are already expanded by stage_reference_regions.
  Explicit roots/escapes still traverse current stored reference edges.
- ambient_regions is shared by preservation and physical apply_effects. Terminal ownership and unexposed_storage
  also use reachable_regions, so their explicit input roots and raw aliases must retain reachability.
- Deferred capture maps unknown input effects to captured writes/escapes plus Ambient. Empty captures must stay
  empty. Direct opaque calls retain [Ambient,Escape], so they remain unbounded, including no-argument opaque().
- Await/task replay retains real body/cleanup effects. Scheduling may change external state. Do not remove Ambient
  or erase captured writes merely to make yield pass. Cancellation owns its separate cleanup edge.

Oracles: real async ownership composition must pass. Existing fabricated/moved/borrow-conflict variants still fail.
Frozen completion_after_opaque_execution_requires_a_terminal_state and exclusive-entry tags remain negative.
Existing memory reachability test gains unescaped borrow, explicit escape and raw alias cases. Retain nested/raw,
finite-choice, scoped-borrow and source/imported comparisons. Reconsider if nested pointer edges disappear,
unknown reachability narrows, direct opaque becomes accepted, or any frozen result regresses.

Design: repair external graph roots once, with existing Origin/References/escape machinery. Remove automatic
publication of language borrow roots, not checks. No new effect variant or representation. Expected production
change under30 lines, reuse existing tests. Compiler-owned hook contracts are also imprecise, but suspension still
needs external-state invalidation, so hook exemptions alone do not close this contract.

## Native blocker after frozen regressions

Actual Domain.arena.pointer is correctly seeded before take_ready and absent before retain_task. Raw Context.pending
write in take_ready can alias the independent raw Domain pointer. Source retain_task/retain_locked summaries preserve
the full Arena/Entries/Slab/Slot path. Observing the already-cleared pointer intentionally produces a container-bounded
opaque Returned reference. Do not restore entry contents or add redundant demands.

Pending source repair, not started: use existing SpinLock<Observation?> for Context.pending, copy observations under
short guards, migrate all readers/writers and constructors, remove pending_storage raw escape. Never hold guards
across callbacks, dispatch, scheduler acquisition or failure handling. Consider retaining the actual Domain/Arena view
through pending processing. A separate unresolved InputRegions staging path flat-projects OwnedTarget rather than
following current stored backing. Trace both without recovering layout from region bounds.

## Valid evidence and exclusions

- 3b: all563 checker tests240ms, returned-dependency57 tests2.55s, platform source/import390ms,
  protected nested cleanup4.90s. All27 production probes17.29s, first cold dependency10.445s.
- Current guard/common-mask/external-root batch: checker563 pass220ms. Both frozen regressions and async invalid
  variants pass8.27s together. Exact opaque-call control passes40ms. Returned-dependency57 pass3.50s, platform2
  pass440ms, protected nested cleanup passes7.26s. Fixed test outcomes and rejection locations are unchanged.
  No compiler-owned hook exemption or loan survival shortcut was added.
- Frozen1718 baseline1ceb765eb retained unchanged. Latest comparison on3b:1714 retained identities,
  1675pass,2fail,37unreported after four90s group bounds, total181.35s. Four c606 provider-test removals are explicit
  open replacements with actual native product gates, never passes. Previous three failures passed current run.
- Both normal native controls still fail39.79s at dispatch_ready/view_owned and start_frame/scoped exit.
  Source-summary diagnostic probes return early and are never native gate passes.
- Earlier valid checks: catalog62, bound-tree136, artifact reuse5 and boundary/cache13 controls pass.

Ignored evidence uses one current output per purpose under target/audit. comparison-baseline.json is the frozen
inventory, comparison-current.json retains exact current results and timeout identities. Seven slow proof cases still
need bounded investigation. Do not rerun unchanged broad suites.

All temporary checker and bootstrap diagnostic changes have been removed. Resolve test executables from Cargo output, never retained filenames.

Next: checkpoint the verified batch, then corrected comparison and bounded slow-case investigation. Remaining: native source ownership
repair, actual four-target check/lower/event layout, owned/deferred/projection/cleanup gates, fixed-five/loop counts,
maintainability/DRY audit, mandatory mutating style and Bray format, published incremental review, final PR description.
Only then mark PR ready and BRA-694 In Review. No completion claim yet.
