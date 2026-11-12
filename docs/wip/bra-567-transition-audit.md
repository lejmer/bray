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

Remaining groups: prepared cleanup/start234, driving197, root_scope optional-operation cleanup, observer/polling105,
withdrawal185/199 and reserve_start175. Follow reserve_start into start_frame. Unbounded Retire alone
proves no defect. The simple nullable control passes, while an opaque operation invalidates an Access predicate
containing a nested borrow. Do not assume every lost whole predicate is unsound.

### Open contract: mutable nullable-owner extraction and epoch preservation

Checked whole extraction transfers the current value's promised evidence and presence guard, without inventing
evidence or transferring a different owner's proof. Producer completion publishes loan and contained-value carriers.
Admission now records extracted_values separately from physical by-value inputs. Mapping, joins/intersection,
normal/failure retirement and deferred proof traversal carry those snapshots. Await admits the actual future before
body effects. Failure retirement precedes deferred metadata removal. Explicit extraction uses the existing bounded
layout traversal, never seeding ownership through a bare borrow or crossing opaque PostState/reference/scalar data.
Disjoint owned tags use footprint.owned_observations and classified stable_values, never the mixed borrowed index.
No new ledger or reconstructed tags. Fixed14 source outcomes include sibling/parent positives and mutation negatives.

These changes repair entry_scope422. The latest pre-batch runtime source gate remains28, adding primary failures
activation/run/completion77 and task/observer/lifecycle333 plus2 dependent E7135. This is not accepted.
The permanent regressions check lowered caller diagnostics because production std source has unrelated failures.
Native composition remains the full-program gate.

Runtime trace confirms P4212 report evidence exists after collection's first extraction U2897244/E8. Its carrier is
E2.collection.report. Admission of the second extraction E14 suspends that first carrier; completion drops it and
retains only incoming.report. In transfer U226341, take_report E93 produces E58.progress.report, then assignment E98
drops that carrier despite the old report remaining in local7. These are actual transitions, not rendered-call guesses.
No carrier lifetime dependencies are recorded. The vacated slot retains the moved value's owner identity because
ValuePreservation keeps every owned context even for checked extraction. A later admission therefore selects the
old result's witness through the slot's new contents. Separately, owned inline Value observations are omitted from
the disjoint-preservation producer, like the earlier missing owned tags. That permits a former-parent write to revoke
an independently held inline report. The actual source control reproduces known/unknown second selection and parent
overwrite. Its replacement producer must be pure, as empty_panic_report is. An opaque producer erases lineage before
the move and makes the control pass for the wrong reason. Require a nonempty published transfer and a caller output
contract in the permanent oracle, so the tested composition actually runs.

Rule: checked extraction ends the old value identity in the vacated slot while preserving the returned snapshot.
Read the recorded extracted_values at mutation, carry the same sources through owned deferred frames, and renew
only matching owned contexts when physical framing changes them. Preserve reference descriptors and unrelated
owners. Ordinary in-place mutation keeps its established identity. Joins map/intersect the recorded sources;
unstarted cancellation performs no body mutation; await/start consume the captured sources at actual body effects.
Failure uses the same framed current values before cleanup. Repeated epochs must stabilize and never restore the
old source lineage. No new ledger or reconstruction from a final view.

Also publish disjoint owned Value observations in the existing typed stable_values map. No preservation for borrowed
referents or overlapping/unknown mutations. Positive sync/async known/unknown selection, parent overwrite and loop
composition are oracles. Mutating the extracted result remains negative. Fixed917 remains unchanged as comparison.
Growth limit: reuse extracted_values and bounded framing, no parallel transport representation. Reconsider any
change that reanchors the returned value or confuses a physical reference slot with its current owner.
Await body effects now classify as contents mutation. Yield hooks, scoped and iteration calls already have call-table
entries. Separation checking is unchanged. All24 extraction outcomes passed before the final selector repair.

Disjoint Value publication exposed a latent selection defect. Await E36 had a real E19 producer, but its selection
retained index<2 from formation against a reused Joined counter. Loop exit proved index>=2, so normal_completion
wrongly removed the continuation and skipped observe. DeferredValue.join_values filtered only newly added context.
It now filters existing selection, entry facts and copy-source correlations against the actual predecessor, for both
known and opaque alternatives. Copied facts freeze through existing CapturedInput identities before old context ends.
State passes unrenamed assumptions and current storage together. No new epoch or guard ledger.
The fixed frame-move property no longer uses a masking invalidation. The real same-site negative reaches observe and
rejects with E7123. Full918 restores789/30/99, including every148 deferred case in source/import, checker511/bound136.
A valid-loop normal-completion assertion prevents vacuous acceptance. Partial initialization retains its live field
without certifying the whole owner. Both pass in the final comparison. No trace hooks or consumed traces remain.

Value preservation also requires live complete storage, actual contents rather than owner prefixes, and no synthetic
bare-identity snapshots. Terminal data remains readable for postconditions. Admitted by-value entries retain their
explicit private snapshots. Owner containment remains separate for ownership and overlap. Existing current-slot and
changed-parent properties protect that distinction. Runtime source25 reveals the optional operation-owner cleanup boundary above. Reopen that contract before another
semantic patch. Trace entry_scope completion through root_scope status/driver mutations, old-operation replacement,
return and EntryRestoration exit. Do not assume the three new diagnostics are three independent bugs.
Closed tag boundary: publish and query classified tags through the existing discriminant selector. Complete
snapshot equality lost the optional-owner clause after an earlier cleanup reframed the payload under the same tag.
The selector retains no payload authority or old tag after replacement. No new representation or reconstruction.
The transferred-tag property covers framed replay and changed-tag rejection. Historical immutable facts may remain,
but cannot prove a replacement's current tag. The conditional-cleanup source test now pairs pure/atomic effects,
sibling owners, optional replacement and invalid authority. Fixed918, checker511/bound136 and adjacent checks pass.
Runtime source23 has no new identities versus accepted native25. Traces and scratch controls are consumed.

Rendering defect found: CapturedInput displays capture(owner, ordinal). Resolve its recorded invocation/input on
the failure path to display the actual source argument, with a clear description for implicit captured inputs.
This is diagnostic lookup, not evidence reconstruction. Next semantic work resumes the owned-backing contract above.

## Execution and remaining gates

One CPU-heavy job at a time. Cargo jobs24, tooling profile, same combined target set. No Rust edits during Rust builds.
Native tools run escalated outside sandbox. Shared target/cargo build dirs, no clean. Selected lowering takes30s.
Use ABI2 bundle target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc.
Use system link inputs bray_runtime_host and bray_runtime_callback for the matching direct source check.
Remove consumed probes immediately after extracting evidence.

Remaining gates: all native errors, native async success/panic/cancel/startup rollback/blocked shutdown,
result/static disposal, foreign restoration, architecture/test pruning, final style/format, independent review,
PR and Linear review handoff. Never claim complete composition before these gates.
