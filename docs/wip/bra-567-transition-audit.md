# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft
[PR #710](https://github.com/lejmer/bray/pull/710), branch feature/bra-567-async-root-orchestration.
Use target/bra-567-audit under the original repository. Original develop is untouched.
User authorizes checkpoint commits and pushes. Keep this single record concise and current.

1. Trace actual CFG producers and ownership, storage-flow, liveness and lowering consumers. Record production,
   consumption and failure survivors for every operation.
2. Prove async-ownership-composition.bray through native execution, plus its invalid variants.
3. Check production before consumption, retirement of moved authority, identity independent of overwritten data,
   and stable loop joins at their owning boundaries.
4. Preserve comparison identities, validity expectations and code versions. Explain and repair regressions before expansion.
5. Name evidence and exclusions. Source or MIR passes never establish native completion.

Use producer-owned values and current storage. Reconstruction is a last resort when robust retained state is impossible.
Keep scalar data, copied values, loan descriptors, complete owners, physical targets and lifetime roots distinct.
Use one current-value tree, alias model, witness ledger and composite storage flow. Preserve imports and remove superseded
mechanisms. No weaker authority, missing-value-as-absent fallback, compatibility shim or language change.

Root cleanup precedes terminal publication. Genuine Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission retains cleanup access. Blocked
shutdown returns unresolved ownership. Use shared teardown state. Sync products retain no async executor or migrated Rust.
Verify optional worker/deadline omission, native provenance and comparative costs.
PreparedStart owns admission/registration through publication or rollback. EntryRestoration owns displaced entry context
through abnormal exits. RetiredCell disposes control, pin and arena before vacancy. WaitResources retains stable storage
while lease-bounded membership changes. Verify actual paths, not struct names.

## Current checkpoint

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca. Current cleanup checkpoint is the PR branch HEAD.
Current cleanup work is WIP. Native execution and the complete transition inventory remain unverified.

Accepted earlier fixes cover Boolean production/publication, complete ownership joins, captured scalar ranges and joins,
actual borrowed descriptors, local publication, separate completed-await cleanup, report capture and bounded reference
effects. Async fixture functions/lifecycles and three invalid variants pass source certificates/lowering.

Cleanup uses Dispose(T) for an actual value layout and Cleanup(T) for conditional effects on a region, in the existing
direct/indirect/unbounded effect locations. Known typed layouts expand selected
lifecycle effects while composing the receiver scope. Region bounds carry no claim of T's layout. Type-only cleanup and
receiver-based escape guessing are removed. Selected-call preparation retains capture/result cleanup templates and their
reference/union paths. Physical effects are captured at future formation into existing Transfer metadata, separately for
capture cleanup and discarded-result cleanup. Staged result references do not publish result authority or fall back to
live output storage. task_cleanup_types is removed. Semantic cleanup presence and diagnostic types remain in their plan.

Current repair separates reached-data traversal from ownership matching. Data queries follow actual retained loan
contents. Ownership queries still follow only owned components. A finite visited-node walk replaces the shared scan
quota, with the existing depth bound applied to each candidate projection. No identity-based data reconstruction.

Regression law retires a temporary containing a loan. Unchanged marker facts survive, changed contents under the same
identity fail, and the loan cannot preserve ownership. Both cases also run with 256 unrelated loans. Both defects were
reproduced red before their fixes. Current checker458 passes. Current comparison: 694 passed / 65 failed / 99 ignored,
89s. Relative to frozen7b5dc5634, bounded creation and two cleanup paths are repaired: imported_cleanup_postconditions_follow_the_updated_borrowed_storage
and cleanup_reference_outputs_feed_the_next_lifecycle. The separately added endpoint positive remains red. No other new
failing identities. All diagnostics, Source controls and failed carrier/mask experiments are removed.

Endpoint still has four call/four cleanup failures. Prior trace located loss at loop-body scope expiration retiring wake
and a later LoopBack into the header. On the repaired version retirement no longer reports removal of an already-proved
witness clause. Trace the actual first loss anew through all solver operations/propagation/joins, not failure counts.
Do not repeat carrier-role, availability, foreign-wait or reference-mask experiments without new evidence.

Native remains unverified. Retained xtask6ab61fffe predates Dispose/Cleanup and data-query fixes and must rebuild before
native checking. native.log records that older nine-error stdlib planning stop, not current behavior. No native async
composition has executed. Async fixture Source/lowering and invalid controls passed before this checkpoint and are in
current comparison. Interface216 passed at abe445829 and is unaffected by the data-query-only repair.

Retain baseline-failures.txt, comparison.log, native.log and deferred-report-fc4b5160b-baseline.exe for necessary comparisons.
Reuse probe.log only for the current diagnostic. Delete passed build/check logs and superseded probes after recording.

## Remaining work

Trace selected inputs/defaults, admission/completion, construction/replacement, projections/patterns, branches/loops,
suspension, outcomes/reports and cleanup through native correspondence. Verify runtime Source bodies/certificates,
imports, native success/panic/cancellation, startup rollback, blocked shutdown, result/static disposal and nested foreign
restoration. commit_start inspection found no recoverable failure between activation.begin and publication. Source/native
evidence remains required. Verify from_core admission/refcount failure ownership before claiming that protocol complete.
Two old negatives need forwarding/fabrication and real-mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. Do not weaken production to satisfy them.

At semantic completion, reread AGENTS and audit every changed file/behavior for maintainability, DRY, APIs, contracts,
errors and structural cost. Run cargo xtask style and cargo xtask format only at the end, inspect edits and rerun affected
checks. Then dispatch the authorized independent reviewer on draft PR #710. Reviewer posts its own artifacts and IDs.
Resolve findings and reuse it for rereview. Only then make PR ready and move BRA-567 to In Review. Never mark Done.
No reviewer, subagent or goal is running. Do not stop at a commit or local repair.

## Execution

Reuse original repository target. CARGO_TARGET_DIR=original target, CARGO_BUILD_BUILD_DIR=target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX=target/toolchains/llvm/active. Tooling profile, default parallelism, one Cargo at
a time, no cleaning. Native LLVM/linker/executables run outside the sandbox with Rust 1.98.1 and LLVM bin on PATH.
Build xtask with default stable. Native command: retained target/tooling/xtask.exe runtime-artifact smoke-test
--output target/audit/native --profile release. TMP/TEMP point to worktree target/audit. Logs/artifacts remain there.
No style/format pass has run. Git destination verified: public lejmer/bray, remote git@github.com:lejmer/bray.git,
authenticated account lejmer. Reuse that trust evidence while destination/authentication are unchanged.
