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

Checkpoint is branch HEAD and remains WIP. Fixed918:789 pass/30 fail/99 ignored, checker511/bound136.
Frozen907 retains original SHA and778/30/99 outcomes with16 explained repairs. Eleven added controls pass.
Owned248/deferred148 each source/import surface and witness matrices retain outcomes. Symbols221/interface216,
messages92, adjacent cleanup57/4, predicates26/2/2 and focused18 passed before the current batch.
Real async source/callable/lifecycle MIR and3 invalid variants pass. Native composition has not run.

Accepted native/source baseline25 has8 E7123/17 E7135,9 primary/16 cascades. Exact identities are in verification.json.
Descriptor selection now survives copy, substitution, import, joins and reached writes without granting ownership or
letting data equality rename identity. Source negatives and portable/property controls pass. This repairs panic338.
Whole/projected invalidation frames complete owners before selecting children. Reference descriptors survive,
reached contents expire, overwritten children lose authority. This repairs attachment exit464 and withdrawal58.
Correct system host/callback inputs reproduce the native identities in a direct source check. Both use ABI2 std.

Fixed918 and all matrices retain their outcomes after loop-continuation, partial-initialization and repeated-tag
properties. Adjacent checks above repeat with the same results. Latest runtime source23 has6 E7123/17 E7135.
It matches the accepted native25 inventory except repaired entry_scope422 and root_scope268, with no new identities.
The earlier completion77/observer333 failures and optional-operation cleanup regressions are repaired.
Native emission/execution has not repeated. Remaining E7123 groups: driving197, start234/prepared cleanup,
observer/polling105, and withdrawal185/199. E7135 reports include dependent cascades, not independent bug counts.

## Current boundary

Open contract: complete owned backing paths must survive checked borrow formation, private input admission,
view production, portable publication and caller effects. Dynamic selection does not grant exact address identity,
freshness, initialization or access authority. Unknown and independent alternatives remain unknown.

Actual path: dispatch_ready loses view_owned at wait_execution. Retire reaches retain_task's task pointer,
from retain_locked -> slot_at. slot_at selects Slab.slots through Entries.slabs, nullable payload and binary search.
Its declared inner buffer borrow reaches Entry.slabs.Nullable.OwnedTarget.Element.slots. The first producer gap
is the missing pointer record for that complete nested descriptor. The current declaration repair supplies it
without replacing actual or unknown pointer/descriptor records. Admission now carries known dynamic references
without wrapping them. Portable AnyElement preserves the full backing suffix as InputRegions. Closed checked
regions stage without independent uncertainty; extra owned indirection remains unknown. All three real source
cases (direct, nullable binary search, forwarded call), bound136, symbols221 and interface216 pass. Fixed918
retains every outcome and matrix case. Checker511, including corrected alias properties, also passes.
Native has not rerun.

The focused probe locates the next loss: private input admission wraps the known dynamically selected owner in
Returned, so initialized_slice_mut cannot find the recorded inner backing. Preserve evaluated known references
through admission without promoting a dynamic selector to an exact storage address. Do not resolve an arbitrary
Returned pointer to one of its region bounds. Existing opaque/bounded-reference negatives protect this distinction.

Publication also truncates at a dynamic Element, dropping slots.OwnedTarget. That outer allocation is not a
physical bound for the inner allocation. Portable summaries must retain the complete owned path with an unspecified
element selector. Such a selector is a conservative region, never exact forwarding. Carry it through substitution,
interface encoding/import and caller region staging. InputRegions currently adds independent uncertainty even for
closed checked bounds. Preserve closed bounds there, while Independent still stages the existing opaque bound.
The semantic-reference consumer must continue treating InputRegions as incomplete authority.

Source verification is blocked by a query-worker invariant panic: unit1592784, cleanup E58/access11/part0
destructor has no published mutation. The owned source process was stopped after the panic. No complete diagnostic
inventory was produced and the accepted source23 inventory remains the baseline. Trace the shared dispose guard
and selected cleanup publication/consumer states. Never add an absent-mutation fallback or weaken the panic.
The root is leave_domain in host/product/thread_cleanup.bray. E58 returns current after select_domain_exit_locked
and transition(key,6). Publisher proves PendingCleanup.execution absent at every E58 visit, skips destructor part0
and publishes statics part1. Consumer initially agrees, then loses that guard and selects part0. Trace the first
loss of S12.execution/F2992 through actual invalidations. Local bounded hooks are excluded from checkpoints.
Stop migration expansion until this publication contract and the runtime path are closed.

Design hypothesis: reuse descriptor declarations, References and checked region summaries. Remove the dynamic
prefix truncation and unnecessary private-input wrapping. One portable region-selector variant is justified only
because local selector identities cannot cross callable boundaries. No parallel ledger, view-type reconstruction,
consumer-specific owner guessing or narrowed opaque retirement. Reconsider if a suffix cannot survive an owning
boundary conservatively or a known alias is discarded. Temporary probe hooks must be removed before checkpoint.

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
