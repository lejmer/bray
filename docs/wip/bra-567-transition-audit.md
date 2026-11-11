# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. After completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively with invalid variants.
3. Check production before consumption, authority retirement, identity versus contents and loop convergence.
4. Keep comparison identities/expectations fixed. Unexplained regressions block expansion.
5. Name paths, evidence and exclusions. Source/MIR success does not certify native composition.

Keep producer-owned facts, one current-value tree, alias model, witness ledger and composite storage flow.
Reconstruction is a last resort. Reads must not replace whole-owner contents snapshots. Preserve specific diagnostic
causes. Internal compiler defects panic where detected. User-source and external-input failures use structured diagnostics.
Batch independent fixes/checks. Use proper Bray formatting. Keep records short and delete consumed task-owned probes/logs.
No style/format until semantic completion. Then simplify production code/prune tests, perform AGENTS/DRY audit,
run mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains
admission/registration through publication/rollback. EntryRestoration retains displaced context through abnormal exits.
RetiredCell disposes control, pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence

Explicit Present snapshots survive substitution, identity/reference remapping, portable decoding, evaluation expiry
and returned-content filtering. Predicate arguments retain canonical nullable value identity. Join proposals reuse
witness refinement with each predecessor's evidence. Two boundary checks cover these contracts and authority retirement.
Per-call scalar seeding and numeric-equality aliases are excluded because they break existing value contracts and NaN semantics.

Frozen907 identities:772 passed,36 failed,99 ignored. Nine production repairs and one scalar-copy test correction
explain all10 differences. comparison.txt stays unchanged. An additional allocation import case passes.
Checker498, bound135, symbols221, interfaces216 pass. Cleanup57/4 and predicates26/2/2 retain their known failures.
Focused18 pass, including Source/MIR composition and3 invalid variants. The operation-cleanup fixture publishes valid MIR.
Both witness matrices pass all positive/negative cases. Deferred source/import matrices have1 positive failure
and no accepted invalid cases. All loop controls pass. Native composition has not run.
Both nested-future positives pass in source and imported form.

Accepted native:35 diagnostics,21 E7123 and14 E7135, complete233-file request. No new identities.
Intrinsic construction fallibility removes retirement205 and dependent267. Native logs/verification.json are current.
The current checkpoint freezes copied-input facts at frame moves, expires mutable source links for pending and active
captures, filters old loop incarnations, and distinguishes known-empty producer sets from opaque frames.
Impossible completions panic. Intrinsic packing creates no source-panic edge. MIR still preserves operands across
outgoing admission and asserts validity before publishing a lowered unit. Scalar data survives storage movement.
All temporary tracing hooks are removed. Read-materialization experiments are removed because they changed whole-owner snapshots.

## Remaining

Deferred positive: nullable_completed_permission in source/import. Task permission now passes.
Proof-only ValueAlias preserves copied immutable data through joins without assuming numeric equality or carrying
ownership. Both predecessor snapshots must prove every alias. Guarded aliases use the existing ordinary solver.
All498 checker tests, the fixed comparison and adjacent suites pass. Native retains the same35 diagnostic identities.
The permanent fresh-owner negative rejects presence as evidence of transferred authority. Temporary tracing is removed.

Verified remaining cause: opaque cleanup of an unreturned generic input replaces the returned nullable discriminant.
The none exit becomes a scalar observation with unknown contents, so ownership publication cannot recognize its absence.
Carry the existing classified tag preservation through storage effects and the current-value tree. Preserve unknown tags
without retaining overwritten payloads. Do not infer absence from empty ownership or add nullable consumer exceptions.
permission-api.bray and permission-boundaries.bray are the current probe. Direct await and Task return pass. Nullable
return fails. Fabrication, fresh-owner and retirement controls reject. Permanent case identities/expectations stay fixed.
Native families include returned-pointer retirement at driving197, address expiry through mutation/input postconditions,
PreparedStart registration, heap allocation extents, waiting-link authority and initialized-range permissions.
Do not weaken initialized-memory policy or add preservation exceptions. tuple-context-projection.bray remains an unresolved control.
Trace remaining operation/input/result, scoped, iteration, await/yield, cancellation, recovery and cleanup paths through
all consumers. Verify native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal and foreign restoration.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its opaque negative has no owner input. Add a mutable-authority control before changing expectations.

## Execution

One CPU-heavy operation at a time. No Rust edits during Rust builds. Default24 Cargo processors/16 tooling codegen units.
No clean. Share original target and target/cargo build dir. LLVM prefixes use target/toolchains/llvm/active.
Compiler/linker/produced-executable workflows run outside sandbox. Native uses Rust1.98.1, LLVM bin on PATH and worktree target/audit as TMP/TEMP.
Build compiler, driver and tests together. Capture executable paths/hashes from Cargo JSON in verification.json.
Native command: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Compare native diagnostics only after the explicit completion marker. comparison-candidate.log holds current fixed-suite results.
Keep only original comparison.txt, current native.log, verification.json and unresolved probes. Consume other logs promptly.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
