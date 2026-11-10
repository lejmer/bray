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
Fixed comparison:753 passed /52 failed /99 ignored, including raw-memory16. Argument-mode repair removes the numeric
and copied-pointer failures with0 additions. Checker475 and bound-tree133 pass. Callable Source/import28, numeric
Source/import8 and trusted diagnostics9 pass. Captured values survive replacement without preserving the old storage loan
or owner. Earlier conformance7, messages90 and JSON48 pass. Diagnostic readiness passed at8b08d8aab.
Expanded predicate group:25 passed /2 failed /2 ignored. Both failures reproduce in retainedfc4 baseline.

Accepted native.log:90 diagnostics(E7123=44,E7135=26,E7119=20), unchanged runtime Source64a4e5490.
The scalar-read bounds repair clears domain.selected()'s postcondition and2 dependent totality failures, with0 additions.
Matching native library built. No native async composition has executed.
Use the canonical complete233-file Windows request. Partial production_host_request has129 files and cannot certify it.

Embedded source snippets share an80-character/first-line formatter. Whole conditions/messages and structured JSON remain
complete. Unsupported bodyless execution promises report MissingEvidence before export. Native C assertions remain supported.

## Current boundary and next step

Current batch: scalar-read bounds, ready to checkpoint. No style/format or agent active. No temporary trace hooks remain.
Union completeness comes from declared variants at formal parameter and symbolic result entry. Boolean closure retains
proven ordinary guards for witness refinement. Union(2/3 variants, distinct inputs) and no-authority-promotion controls pass.
The nullable-result Source/import test covers equivalent guards, opaque calls, fabrication and overwrite(28 cases).
scoped-evidence.bray passes construction, Result consumption, scoped entry, mutation and cleanup. Fabricated owners and
replaced leases reject E7123. Opaque-call control passes. These are Source checks, not async native execution.

E7135 preserves required/actual conditions through certificate source conversions using the shared condition formatter.
Both cause and condition render. Failure7, diagnostics9, checker475, messages91 and JSON48 pass.

Scalar-read bounds: preserve_integer_value checks computed_value when recognizing a projected scalar read.
The actual failed exit retained previous != 0 but no unsigned bounds because Scalar wrapped the Projection. All strong/weak,
signed and mutation Source/import cases pass. Invalid outcomes are unchanged. Their assertion requires E7135(false
postcondition) and forbids E7119. Fixed suite753/52/99 removes only that failure, with0 additions. Native90 removes3 with0
additions. Next: group remaining runtime failures using their actual conditions and continue the transition inventory.

Scalar copies retain data without retaining the old storage loan or owner. The data index comes only from live values.
Composite storage-flow core.rs calls transfer_operation directly at671/1007/1024, bypassing flow.operation.

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
