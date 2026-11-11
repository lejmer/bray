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

Frozen907 identities:776 passed,32 failed,99 ignored, with14 explained repairs.
The original comparison.txt is unchanged. Three added controls pass, giving fixed910 totals779/32/99.
Checker503/bound136 pass. Cleanup57/4, predicates26/2/2 retain known failures. Focused18 pass, including
Source/MIR async composition and3 invalid variants. Native composition has not run.
All244 owned-transfer and148 deferred cases pass each in source/import. Per-case identities and outcomes are fixed.
An aggregate FAILED does not prove its later cases executed.
Accepted native:30 diagnostic identities,17 E7123 and13 E7135, complete233-file request.

Current source-validated batch preserves input loan evidence and independently retained owner evidence in the same ledger.
Borrowed input storage cannot count as owned authority. Selecting only one carrier either breaks owner transfer or loses
the loan across a loop. The source/import regression covers borrowed/owned inputs and absent renewal guarantees.
Physical storage-flow tracing confirms that a pointee write is disjoint from its stored loan descriptor. The producer
now publishes that descriptor identity, which reaches guarantee analysis unchanged. Actual contents remain invalidated.
Candidate native29 has no new diagnostic locations and clears scheduler_reference(owner) after the resume_binding backedge.
All8 source/import loop cases now pass. Framing consumes the existing ownership layout with the value, handling
normalized and explicit owned observations identically while replacing overwritten contents. Both forms, prior-snapshot
independence and replay are covered. Fixed910 is779/32/99 with no unexplained changes. Checker503/bound136 and focused18 pass.
All244 owned-transfer and148 deferred cases pass each in source/import. Adjacent known failures are unchanged.
No temporary tracing remains. Latest native verification is next.

Audit the full downstream path before another implementation batch. Diagnostics validate the audit, not select its next boundary.
The29 native sites group into7 scoped/optional cleanup requirements,4 marker requirements across calls/mutation/loops,
5 relational requirements involving multiple values, and13 postcondition failures. At least8 postcondition failures directly
name already-failing required callables. These are sites and dependency cascades, not29 identified compiler bugs.
Trace families by shared producers before deciding whether a source contract or compiler transport is wrong.

Current framing path audit: physical slot disjointness -> published StorageInvalidation -> atomic value/layout framing
-> current tree and aliases -> input loan/actual-owner publication -> join value/identity remapping and predecessor proof
-> captured call/Future entry -> strict-role transfer/retirement -> cleanup. Constructor normalization, both invalidation
consumers, common_components, owned_result, collect_reference_data, join_values, rename_witnesses, retire_evaluation and
retire_trusted_values were inspected. Structural unwraps do not themselves grant ownership. Repeating-call admission with
a matching renewal requirement passes the real borrowed source probe. No speculative expiry or join changes were made.
Native full composition and remaining families still require evidence. Strengthen the permanent loop fixture with that
renewal requirement when batching the next necessary test build.

## Remaining

Keep the carrier/descriptor repair fixed while validating native family outcomes. Then trace scoped cleanup, PreparedStart registration,
returned-pointer retirement, attachment restoration, initialized ranges and wait-link authority. Keep the current tree,
physical storage model and witness ledger authoritative. Do not introduce preservation exceptions or recover data from traces.
The unresolved tuple-context-projection.bray probe does not establish that copying its owner is valid language behavior.

Trace operation/input/result, scoped, iteration, await/yield, cancellation, recovery and cleanup through all consumers.
Verify native async success/panic/cancel/startup rollback/blocked shutdown, result/static disposal and foreign restoration.
Then simplify production mechanisms, prune redundant tests, complete the AGENTS/DRY audit, run mutating style/format,
rerun affected checks and obtain independent draft PR review before Ready/In Review. No final verification claim yet.

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
