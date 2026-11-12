# BRA-567 active audit

## Resume gate

Read this record and the active worktree's [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction or restart. Resume at the unresolved contract below, not the last command. Do not patch the next
diagnostic until the affected mechanism's producers, consumers and preservation limits are traced. A regression
reopens that contract and equivalent paths. Tests verify the reasoning, they do not replace it.

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Branch feature/bra-567-async-root-orchestration in target/bra-567-audit. Leave original develop untouched.
Checkpoint pushes authorized. After semantic completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.
Questions and corrections steer this work. Before any final response check the remaining gates and continue any
authorized work. A checkpoint, resolved subproblem or workflow question is not a stopping point.

Inventory actual CFG transitions and their ownership, storage, liveness and lowering consumers.
Prove native async-ownership-composition.bray and invalid variants. Check production before consumption,
authority retirement, identity versus contents and loop convergence. Preserve fixed identities/outcomes.
Unexplained regressions block expansion. Name traced paths, evidence and exclusions.

Keep one current-value tree, alias model, witness ledger and composite flow. Carry producer-owned identities.
Reconstruction is a last resort. Specific structured diagnostics. Internal defects panic at their owning boundary.
Batch independent checks. Proper Bray formatting. Delete consumed probes. Keep records short and current.
No style/format until semantic completion. Then simplify production code, prune redundant tests, audit AGENTS/DRY,
run mutating cargo xtask style and cargo xtask format, and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown returns
unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust. Verify worker/deadline
omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Fixed evidence

Pushed ed6548628. Current verified batch has consuming-copyable receiver moves and nullable argument inference.
Fixed920:791 pass/30 fail/99 ignored. Original907 identities/SHA remain frozen with16 explained repairs and13 added
controls. Owned248/deferred148 on source/import and witness matrices unchanged. Checker511/bound136 pass.
Symbols221/interface216/messages92 and proof9 remain valid. Cleanup57/4 and predicates26/2/2 retain outcomes.
Async composition source/MIR and3 invalid variants pass. Native composition remains unverified.

Runtime source23 =6 E7123 +17 E7135. Verified exact logical inventory after3 start.bray span relocations caused by
consuming start_frame. No repairs or regressions in this batch. Native last25 remains unverified for current changes.
Seven local failures, sixteen dependent reports:
- driving197 view_owned: raw retirement may alias borrowed Current marker.
- start230 producer(self) and224 cleanup service_owned after reserve_start.
- prepared_start175 registration success postcondition, not a dependency cascade.
- polling105 transfer_entry result layout.
- withdrawal185/199 nullable neighbor LinkAccess.
Do not mistake dependent reports or added passing controls for convergence. Fixed30 remains unchanged.

## Current contract and next action

Consuming receiver producer now plans Move even for @copy. Direct/staged calls share the producer. Lowering consumes
checked operands and owned-input classification already includes consuming receivers. Six source cases protect move,
ordinary-copy rejection, stale-source rejection, nullable conversion/inference and exact nested generic matching.
Inactive is copyable lease data. root_frame reads the complete initialized lease while its native caller retains the
frame/provider. start_frame consumes that value and lends self to PreparedStart until rollback/publication. Native ABI
unchanged. Source23 and fixed920 are verified. Native provenance/composition remains an open gate.

Before another production patch, classify actual fixed30 failures by source contract and trace shared mechanisms.
Invalid acceptance includes reference-slot replacement, address bounds and undeclared authority. Valid rejection includes
mutable owner updates, unions, nullable successes, raw transfers and imported cleanup. Summary-count expectations need
contract inspection, not automatic rebaselining. Current read-only hypothesis: shared-view preservation can confuse a
replaced reference descriptor with its old referent. Inspect Footprint preparation, active/shared preservation, resolved
physical effects and condition invalidation together. No fix decided. Pair invalid replace(&mut borrowed) with retained
old borrow, disjoint descriptor writes, nested references and raw aliases. Also trace reserve_start registration assignment
through demand_lane and success return. Reconsider the hypothesis if these use different physical/value boundaries.

AddressTarget finite raw-load paths and full retain_task InputRegions are verified. take_ready's possible-alias write
retires Domain.arena.pointer before call staging. Its location survives but no old regions record does. Current marker
then fails raw Retire with unknown + Entry(owner.pointer) bounds. Never narrow by final Slot view or preserve overwritten
values. Private descriptor/borrowed descriptor/pointee-authority controls pass. No source improvement resulted.
Proof graph omissions without recorded causes panic. Legitimate failures preserve typed conditions and leaf locations.

Checked complete-buffer borrows retain missing backing without replacing actual/unknown records. AnyElement
preserves owned suffixes as InputRegions without exact authority. Extra owned indirection remains unknown.
Every execution obligation replays checked physical effects. Observation confinement only classifies dependencies.

Closed contracts protected by accepted checks:
- Extracted nullable owners carry recorded extracted_values through sync/async admission, body effects, joins and
  failure retirement. Vacated slots lose moved lineage. Disjoint owned Values and tags survive parent writes.
- Deferred joins retire stale selectors against actual predecessor assumptions/current storage, for known and opaque
  alternatives. CapturedInput freezes copied facts. Valid-loop normal_completion prevents vacuous acceptance.
- Current-value preservation requires live complete contents, not identity prefixes or synthetic owner snapshots.
  Partial initialization retains initialized fields without certifying an entire owner.
- Classified tags use the existing discriminant selector across payload framing. Replacement tags remain unproven.
- Captured-input diagnostics resolve explicit source arguments. Implicit arguments retain numbered typed captures.

Remaining primary groups: driving197, start234/prepared cleanup, observer/polling105, withdrawal185/199.
E7135 includes dependent cascades. Retire alone proves no defect. Never narrow retirement by the final Slot view.

## Execution and remaining gates

One CPU-heavy job at a time. Cargo jobs24, tooling profile, same combined target set. No Rust edits during Rust builds.
Native tools run escalated outside sandbox. Shared target/cargo build dirs, no clean. Selected lowering takes30s.
Use ABI2 bundle target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Use system link inputs bray_runtime_host and bray_runtime_callback for the matching direct source check.
Remove consumed probes immediately after extracting evidence.

Remaining gates: all native errors, native async success/panic/cancel/startup rollback/blocked shutdown,
result/static disposal, foreign restoration, architecture/test pruning, final style/format, independent review,
PR and Linear review handoff. Never claim complete composition before these gates.

