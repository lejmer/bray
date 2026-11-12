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

Latest checkpoint is branch HEAD. Frozen907:778 pass/30 fail/99 ignored,16 explained repairs, original SHA unchanged.
Eight added controls give915:786/30/99. These outcomes, checker510/bound136 and all matrices pass.
Owned248/deferred148 each source/import surface and witness matrices pass. Adjacent cleanup57/4, predicates26/2/2,
focused18 retain outcomes at checkpoint. Real async source/callable/lifecycle MIR and3 invalid variants pass.
Native composition has not run. No style pass yet.

Complete native/source gate25:8 E7123 and17 E7135 comprise9 primary failures and16 explicit cascades.
Attachment exit464 and withdrawal58 are repaired. Withdrawal26 now cites existing unlink_registration failures.
verification.json contains exact accepted identities.
Reports are not independent bug counts.

## Current boundary

Both whole-value and projected invalidation frame from the complete owner before selecting the affected component.
Producer-owned reference descriptors survive, reached contents expire and overwritten owned children lose authority.
Regression covers whole owners, direct slots, projected updates, replay and a real conditional compound-lease contract.
The native attachment exit464 and withdrawal58 obligations are repaired. Withdrawal26 cites existing unlink errors.
No trace hooks remain. Correct system host/callback inputs make direct source checking reproduce all native report
identities exactly. Use this semantic gate while errors remain, then rerun native emission/execution. Both use ABI2
standard-library root target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.

### Verified contract: slice descriptor observations

Rule: descriptor observations survive reached-content writes and real descriptor copies. They grant no ownership,
do not follow unrelated descriptors and do not turn unknown reference identity into known identity.
Hypothesis: carry one explicit Descriptor selector through substitution and portable terms. Replace eager owner
selection for SequenceLength/IsEmpty. Growth limit: one selector and portable unary operator, no hook reconstruction
or parallel descriptor framework. Reconsider if ordinary equality can rename physical identities.

| Transition | Required behavior and current evidence |
| --- | --- |
| SliceIndex production | Produce evaluated projection identity. Enclosing borrow supplies lifetime/access. Source controls pass. |
| Copy and borrow | Unwrap actual copied reference, retain original descriptor and no extra authority. Property passes. |
| Substitution and import | Select descriptor after argument substitution, preserve selector in portable terms. Roundtrip property passes. |
| Mutation and expiry | Descriptor skips reached contents, unknown owner stays unknown. Property passes. |
| Bounds and postconditions | Compare descriptor observations consistently, normalize ordinary data equality. Full source positives and unrelated-slice negative pass. |
| Equality and joins | Descriptor uses only the identity map, shared with observation owners and captured owners. Data equality/replacement cannot rename it. Actual transfers can. Positive and negative properties pass. |
| Other consumers | Traversal/mapping retain the selector, snapshot grants no authority, expiry removes expired defining expressions, constants/lowering pass through the qualifier and diagnostics show the source. |

Oracles: descriptor_selection_survives_substitution_without_observing_reached_contents,
portable_descriptor_calls_keep_their_observation_after_substitution and
projected_slices_keep_their_descriptor_in_downstream_contracts. The source test uses ordinary callers and full
check_diagnostics. Trusted definitions and dependent execution_properties alone cannot discharge its negative oracle.
Equality/join preservation and rejection assertions precede the repair. result_replacements and
reference_data_replacements establish the separate identity/data maps. Formal substitution and actual copy snapshots
remain value substitutions. The traced transitions and fixed checks agree. This verifies descriptor composition,
not the remaining runtime ownership transitions or native async execution.

Current descriptor batch: fixed915 gives786/30/99 with all original identities preserved. Checker510/bound136,
symbols221/interface216/messages92, matrices and adjacent groups pass. Refreshed native gate gives25 reports,
9 primary/16 cascades. Only removed identity is panic338, no new identities. The final identity-rule repair gives
the same25 exact source identities as that native run. Temporary slice probes are deleted, no trace hooks remain.

### Open contract: owned backing retirement

dispatch_ready source63/unit746973 loses view_owned at wait_execution call53, before join76. Active loan and marker
classification are present. Retire reaches retain_task's result.0 nullable task Reference.pointer, produced at E9/S10.
Its origins include execution owner S2.F2021 and an independent alternative. Reference.pointer points to Slot,
not a complete allocation extent. Never use its view type to narrow retirement. retain_task calls retain_locked,
whose issue lambda takes address_of<Slot>(slot) after slot_at. The failing effect is not the buffer cleanup in release_arena.
Trace that slot's backing through reference staging, owner transfers, effect publication, alias resolution and marker
preservation. Unresolved: whether an owning boundary is lost or no disjointness is established.
Retain real aliases and independent alternatives. Do not reconstruct ownership from a final pointer view.
Existing owned-region transport and independent-alias tests are negative oracles. Establish a real source control at
the identified producer before any semantic patch. Temporary trace hooks and consumed output are removed.

Remaining groups: prepared cleanup/start234, driving197, entry_scope422, root_scope268, observer/polling105,
withdrawal185/199, reserve_start175 and panic338. Follow reserve_start into start_frame. Unbounded Retire alone
proves no defect. The simple nullable control passes, while an opaque operation invalidates an Access predicate
containing a nested borrow. Do not assume every lost whole predicate is unsound.

## Execution and remaining gates

One CPU-heavy job at a time. Cargo jobs24, tooling profile, same combined target set. No Rust edits during Rust builds.
Native tools run escalated outside sandbox. Shared target/cargo build dirs, no clean. Selected lowering takes30s.
Use ABI2 bundle target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Use system link inputs bray_runtime_host and bray_runtime_callback for the matching direct source check.
Remove consumed probes immediately after extracting evidence.

Remaining gates: all native errors, native async success/panic/cancel/startup rollback/blocked shutdown,
result/static disposal, foreign restoration, architecture/test pruning, final style/format, independent review,
PR and Linear review handoff. Never claim complete composition before these gates.
