# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. Independent review, PR ready and Linear In Review follow completion.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively, with invalid variants.
3. Check production before consumption, moved authority retirement, identity versus contents, and loop convergence.
4. Preserve comparison identities/expectations. Explain and repair regressions before expanding migration.
5. Name traced paths, evidence and exclusions. Source/MIR success does not certify native composition.

Retain producer-owned facts. Reconstruction is a last resort. Keep one current-value tree, alias model, witness ledger
and composite storage flow. Distinguish owners, loans, addresses, pointees and contents. Preserve specific diagnostic
causes. Remove duplicate mechanisms rather than adding parallel mappings or new dependency kinds without evidence.
Keep records short/current and delete stale task-owned logs/probes by exact path. Proper Bray formatting applies to fixtures.
No style/format until semantic completion. Then perform the architectural/test-pruning pass requested by the user,
mechanical AGENTS/DRY audit, mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance and costs. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Accepted evidence

Audit baseline:b90076c1f21f3853ebfd90954b3babe26f122eca.
Fixed comparison:756 passed /52 failed /99 ignored, including raw-memory16. Argument-mode repair removes the numeric
and copied-pointer failures with0 additions. Checker475 and bound-tree133 pass. Callable Source/import28, numeric
Source/import8 and trusted diagnostics9 pass. Captured values survive replacement without preserving the old storage loan
or owner. Failure diagnostics7, messages91 and JSON48 pass. Diagnostic readiness passed at8b08d8aab.
Expanded predicate group:26 passed /2 failed /2 ignored. Both failures reproduce in retainedfc4 baseline.

Accepted native.log:85 diagnostics(E7123=39,E7135=26,E7119=20), unchanged runtime Source64a4e5490.
The produced-tag repair clears formation291 and its dependent worker/entry postcondition, with0 additions.
Matching native library built. No native async composition has executed.
Use the canonical complete233-file Windows request. Partial production_host_request has129 files and cannot certify it.

Embedded source snippets share an80-character/first-line formatter. Whole conditions/messages and structured JSON remain
complete. Unsupported bodyless execution promises report MissingEvidence before export. Native C assertions remain supported.

## Current boundary and next step

Yield consumed its source while its actual block/loop result remained only in pending_results until after scope cleanup.
An unrelated opaque destructor therefore removed the owner's guarantees. branch-composition.bray isolates this loss.
It does not yet reproduce the remaining formation291 failure. The fixed Source/import regression passes for block results and rejects fabricated owners.
The first comparison preserves all52 failure identities and checker475 passes. That run adds1 valid regression.

Current repair reserves region storage before planning nested paths and initializes its actual destination at Yield/Break.
Completion reads current post-cleanup contents. Liveness defines direct initialized storage, never a reached borrow.
Callable Return had the same owner gap. Removed ExecutionState.result and its special merge/retirement handling. Returns
and callable propagation now use the planned result slot in the authoritative current tree. The24-case Source/import
matrix covers block Yield, loop Break and ordinary Return with no/empty/opaque cleanup plus fabricated-owner rejection.
All valid cases lower to MIR. Comparison754/52/99 preserves every prior failure identity. Checker475 passes.

Lowering already initializes private cleanup-transfer storage before cleanup and forwards/disposes it on outcomes.
The native run at b6d16d501 preserves all90 diagnostics, including formation291. Its exact failure state has the live
Existing owner and an available unconditional ledger clause with no dependencies. The established truth lacks that
clause. A4-variant nested-choice reduction rejects both valid owner arms even without cleanup. The permanent matrix
was extended and confirmed red at complex=true/no guard before the current semantic edit. Its earlier24 cases still pass.

Constructors now establish active/inactive tag facts against their actual produced identity. Pairwise joins preserve
those ordinary exclusions when several alternatives merge. One prepared union-type table supplies formal entry
invariants and production facts. This introduces no new authority kind or value tree. Known-tag extraction also replaces
the separate return-outcome traversal. The40-case two/four-alternative Source/import matrix passes, with MIR for valid
cases and fabrication rejected. Comparison754/52/99 retains every failure identity. Checker475 passes.

Native verification at ff997a8cb removes formation291 and its dependent worker/entry postcondition, with0 additions.
The real4-way source reduction also passes the rebuilt driver. Expanded predicate26/2/2 retains the2 pre-existing failures.
No hooks or stale branch/trace probes remain. No style/format or agent is active.

Catch forms its private Ok/Error wrappers before their join. complete_caught_result now records production tag facts
there, preserving nullable-container evidence through consumed Ok payloads and presence tests. The12-case matrix covers
direct/caught/fabricated values, sync and async/await, Source/import, and MIR for valid cases. All pass. Comparison755/52/99
preserves every prior failure identity. Checker475 passes. Native7ad701e8c removes4 call failures, including both targets.
It exposes1 dependent E7135 at execution/ownership47, whose cause is the existing task/native140 arena_anchor requirement.
execution/task_entry20 now names the same existing dependency. Retained diagnostic headers are otherwise unchanged.
The exact catch source reduction passes the rebuilt driver. Consumed probes/logs were removed.

Root expiry sent dependent reference views through pointee retirement. The arena proof was lost at request-block cleanup
and reached the loop header absent. The independent input witness had no lease dependency, while a separate Borrowed(input)
witness depended on the lease. Root evidence expiry now removes scoped reference witnesses without retiring referent data.
Owned lifetime dependents still retire. Original changed-snapshot handling remains intact. Independent owner evidence
survives, an expired sole view loses evidence, and actual owner retirement/marker replacement revoke it.

The12-case Source/import/MIR matrix covers a single block and repeated loop, sync and actual async/await, plus genuine
marker-replacement negatives. All475 checker tests pass. Comparison756/52/99 preserves every prior failure identity.
No hooks or consumed trace/red logs remain. borrowed-wrapper-composition.bray remains the active real-source control.
Next: uninstrumented native run, targeting task/native140 and its exposed dependent postconditions. No style/format or agent.
Keep propagation-to-yield under audit. Do not retain prospective call results as live owners.

## Remaining

Repair native proof/execution failures by transition class, then execute the complete async fixture. Trace Bound and
PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction inputs, attempt/completion,
iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup through all consumers.
Verify Source/import certificates and native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount ownership. Do not weaken initialized-memory policy to
hide task/result318 copy<Uninit<u8>> failures.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its negative opaque call has no owner input. Add a mutable-authority control before changing expectations. Do not
implement the disproven normal_effects hypothesis. Keep a fresh review agent for the committed PR only at semantic completion.

## Execution

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Share original target and target/cargo build dir.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Tooling uses default stable. Compiler/linker and
produced-executable workflows run outside sandbox. A10s rustc sample averaged2.39cores with all24cores available.
Native uses Rust1.98.1, LLVM bin on PATH, TMP/TEMP worktree target/audit.
Native: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Always parse build.log for fresh executable identities. Cargo feature selection changes test hashes.

Retain baseline-failures.txt67, native-baseline-identities.txt, accepted comparison.log/native.log, native intermediates
and deferred-report-fc4b5160b-baseline.exe for the old controls. Keep only active build/probe logs. No temporary hooks remain.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
