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
Fixed comparison:749 passed /54 failed /99 ignored, including raw-memory16. No added failures. The existing
raw_slot_moves_preserve_only_live_disjoint_initialization now passes without expectation changes. Checker470 and
bound-tree133 pass. Source/import16 range cases pass, including exact failed predicates and genuine initialization errors.
Earlier failure-diagnostics6, conformance7, messages90 and JSON48 pass. Diagnostic readiness passed at8b08d8aab.
Expanded predicate group:25 passed /2 failed /2 ignored. Both failures reproduce in retainedfc4 baseline.

Accepted native.log uses compiler sources69a6e7dba and unchanged runtime Source64a4e5490:128 diagnostics
(E7123=64,E7135=41,E7119=23), exactly the same identities asb25a6c43b. Pointer-range normalization previously removed
result318 E7087/E7123. Matching native library built. No native async composition
has executed. Source/MIR success remains narrower evidence. Host/attachment/cleanup192 still has a generic local cause.
Use the canonical complete233-file Windows request. Partial production_host_request has129 files and cannot certify it.

Embedded source snippets share an80-character/first-line formatter. Whole conditions/messages and structured JSON remain
complete. Unsupported bodyless execution promises report MissingEvidence before export. Native C assertions remain supported.

## Current batch

MemoryCallEvidence replaces the conflated Option<Result<bool>>. Whole-contract failure, independently proved initialization
and empty-copy evidence remain distinct. Initialization is rechecked only after another requirement fails. The Source/import
negative checks name initialized_range_as or non_overlapping. Only actual missing initialization reports E7087.

A fully authorized source double move was accepted byb25a6c43b. The retirement property and source regression were red
before repair. Actual traces showed claims named copied pointer bits while retirement indexed their scalar storage context.
Shared pointer_identity now serves proof, disjoint-range preservation and retirement. Descriptor-slot borrows remain distinct.
Stable address bits cannot preserve reached initialization claims. Double moves reject, reinitialization succeeds, spatial
permissions survive, and empty-copy0/1 physical storage decisions pass. No temporary trace hooks remain.

The fresh CLI reports only non-overlap E7123 for the initialized-copy negative, and E7087 plus initialized_as E7123 for the
fully authorized double move. Both scratch sources/logs are removed. Checkpoint69a6e7dba is pushed.

Callable-address repair:CallableAddressValid shares the pointer policy. Clauses normalize at actual publication after
substitution, including call/cleanup completion, entry/static guarantees and intrinsic range evidence. Scalar slot writes
retire storage borrows, while copies retain data facts. Shared is_scalar_value includes address bits for borrowing, call
results and overwrite handling. Storage observations still distinguish Address from ordinary Value.
Checker470, Source/import20 callable controls, range16 and raw-memory16 pass. Copied values survive replacement, while
copied/replaced slot authority rejects with slot_ready. The fixed comparison retains exactly54 failures. Fresh CLI value,
borrowed-field and returned-pointer controls all pass. Probe and trace files are removed. Matching native verification is next.
No style/format has run. No agent is active.

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
