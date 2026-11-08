# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft
[PR #710](https://github.com/lejmer/bray/pull/710), branch feature/bra-567-async-root-orchestration.
Keep this single record concise. Remove superseded notes. Historical evidence is available in Git and ignored logs.

1. Derive transitions from CFG producers and ownership, storage-flow, liveness and lowering consumers. Record production,
   consumption and failure survivors for every operation.
2. Prove the real async-ownership-composition.bray program through native execution, plus its invalid variants.
3. Check production before consumption, retirement of moved authority, identity independent of overwritten data, and
   stable loop joins at their owning boundaries.
4. Preserve comparison identities and expectations. Stop expansion on unexplained regressions.
5. Name evidence and exclusions. Source or MIR passes never establish native completion.

Use actual produced values and current storage. Keep immutable captures, copied data, loan descriptors, complete owners,
physical targets and lifetime roots distinct. No reconstruction from syntax, marker authority or missing-value fallbacks.
Use one current-value tree, physical alias model, witness ledger and composite storage flow. Preserve inferred contents
summaries through imports. Replace superseded mechanisms completely. No language change or weaker authority.

Root cleanup precedes terminal publication. Genuine Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission retains cleanup access. Blocked
shutdown returns unresolved ownership. Use the shared teardown state. Sync products retain no async executor or migrated
Rust implementation. Verify optional worker/deadline omission, native provenance and comparative costs.

PreparedStart owns admission/registration until publication and rollback. EntryRestoration owns displaced entry context
through abnormal exits. RetiredCell owns control, pin and arena before vacancy and disposes them in that order.
WaitResources remains stable while lease-bounded membership changes. Audit actual acceptance paths, not struct names.

## Current state

Worktree: target/bra-567-audit under the original repository. Pushed checkpoints use the actual PR branch.
Audit baseline: b90076c1f21f3853ebfd90954b3babe26f122eca. User authorizes checkpoint commits and pushes.

Current snapshot-ledger validation: build 1m29s, checker 450 passed, lowering 38 passed. Trusted/adjacent has 237 passed,
37 failed, 97 ignored. Old failure identities are unchanged. The additional failure is the pinned production completion
predicate check. Export/lowering has 386 passed, 54 failed, two ignored with identical failure identities. Execution
condition controls have 26 passed and five failed. Logs use snapshot-ledger. No diagnostic hooks or processes remain.

The snapshot-key law failed before repair. ExecutionState::join_values now renames owned_results keys as complete values,
keeping changed contents snapshots distinct from the original owner identity. Collapsing them previously mixed preservation
promises and oscillated under precise conditional initialization. The fixed async positive and invalid variants, loop
cleanup and borrowed-invariant controls pass. The composition test still needs ordinary execution certificates for every
body as well as lowering. Next extend that check before further implementation changes.

The const-call producer preserves real owner identity independently of symbolic computation. Static initializer/fabrication,
selected-instance and imported-static controls pass. Two extra static proof tests passed at 771157820. Retained logical
storage also preserves actual ConditionVariable loans without replacing unrepresented backing with Unknown.

Native evidence remains materialized-owner-native.log at 771157820: standard-library planning stops with 11 E7123,
10 E7119 and two E7127. No native composition or runtime Source archive has executed. Terminal-address and partial-stdlib
SpinLock probes stop on buffer/allocation diagnostics. All 13 focused terminal_destructor controls pass.

Pinned Boolean failures: short_circuit_results_preserve_returned_predicate_meaning and
production_completion_predicates_match_returned_booleans. boolean-publication.log shows produced Boolean outcomes becoming
Unknown before return. build_condition publishes before the join and merge_values loses differing cached values. Binary
expressions have no outer result-region completion. Preserve actual operands and conditional meaning without reconstructing
missing values from syntax or growing path formulas. Treating missing internal slots as Absent exposes independent cleanup
authority loss, so that experiment is reverted. Changing only cache equality also failed. Keep these expectations fixed.

Read-only commit_start trace: begin releases the pool guard before cell locking. The inspected path to publication uses
spin/atomic operations, total accessors and abort-only invariant checks. No recoverable failure is demonstrated there.
Source certification and native rollback/publication checks remain required before treating the protocol as verified.

## Remaining verification

The executable transition ledger remains incomplete. Trace selected inputs/defaults, admission and completion,
construction, replacement, projections, pattern binding, branches/loops, suspension, outcomes/reports and cleanup.
Known boundary checks already cover await admission, separate completed cancellation, local publication, descriptor role,
report-channel capture, actual loan lifetime and bounded joins. Native correspondence is outstanding.

Resolve supported failures, then verify all runtime Source bodies/certificates, Source/imported contracts and native
success/panic/cancellation, startup rollback, blocked shutdown, result/static disposal and nested foreign restoration.
commit_start currently begins activation before locking/publishing task control. Prove abnormal ownership and lock order
before changing that protocol. Two unsupported old negatives need explicit forwarding/fabrication and real mutation
controls: trusted_implementation_does_not_export_undeclared_authority and
shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. Do not weaken production to satisfy them.

At semantic completion, reread AGENTS and audit every changed file, shared behavior, API, error path and structural cost.
Run mandatory cargo xtask style and cargo xtask format only at the end, inspect edits and run affected final checks.
Then dispatch the authorized independent reviewer on draft PR #710. Reviewer must post its own artifacts and return IDs.
Resolve findings and reuse it for rereview. Only then make the PR ready and move BRA-567 to In Review. Never mark Done.
No reviewer is running. Do not stop at a commit, test pass or local repair.

## Execution

Reuse original repository target storage. CARGO_TARGET_DIR=original target, CARGO_BUILD_BUILD_DIR=target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX=target/toolchains/llvm/active. Use tooling profile and default parallelism.
One Cargo operation at a time. Native LLVM/linker/executables run outside the sandbox with Rust 1.98.1 and LLVM bin
on PATH. Build xtask with default stable. Run retained xtask runtime-artifact smoke-test --output target/audit/native
--profile release. Logs and temporary files belong in this worktree's target/audit. No style/format pass has run.
