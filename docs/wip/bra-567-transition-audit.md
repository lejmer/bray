# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft
[PR #710](https://github.com/lejmer/bray/pull/710), branch feature/bra-567-async-root-orchestration.
Worktree target/bra-567-audit. Original develop remains untouched. User authorizes checkpoint commits and pushes.

1. Derive transitions from executable CFG producers and ownership, storage-flow, liveness and lowering consumers.
2. Prove async-ownership-composition.bray through native execution, with invalid variants.
3. Verify production before consumption, retirement of moved authority, identity independent of overwritten contents,
   and stable loop joins at their owning boundaries.
4. Freeze comparison identities and validity expectations. Explain and repair regressions before expansion.
5. Name traced paths, evidence and exclusions. Source/MIR passes do not establish native completion.

Retain producer-owned semantic state. Reconstruction is a last resort. Distinguish scalar data, loan descriptors,
complete owners, physical targets and lifetime roots. Keep one current-value tree, alias model, witness ledger and
composite storage flow. Remove superseded mechanisms. No weaker authority or missing-as-absent fallback.
Fix vague diagnostics encountered, retaining specific causes, actual arguments, declarations and known mutation sites.
Keep verified design improvements even when they do not solve the immediate failure. Keep records concise and current.

Root cleanup precedes terminal publication. Genuine Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission retains cleanup access. Blocked
shutdown returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify optional worker/deadline omission, native provenance and comparative costs.
PreparedStart retains admission/registration through publication or rollback. EntryRestoration retains displaced context
through abnormal exits. RetiredCell disposes control, pin and arena before vacancy. WaitResources retains stable leased
storage through membership changes. Verify paths, not struct names.

## Current evidence

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca. Latest pushed checkpoint is the copied-address boundary repair.
Full transition inventory and native async composition remain unverified. Do not stop at a local fix or checkpoint.

Accepted semantic repairs preserve entry observations, stored-reference referents, component joins and formed loans.
Moves consume registered produced operands without retiring projected receivers. Pattern subjects survive their condition
scope. Pattern views no longer initialize containing roots: value_transfer_destinations exposes only owned destinations,
removing three lossy conversions in liveness and cleanup preparation. Source/import paired regressions preserve invalid
escape and overwrite rejection. Scalar copies, RawBuffer bounds and aggregate admission retain their declared guarantees.

Proof APIs return required/actual conditions. Diagnostics distinguish call, cleanup, postcondition and certificate causes,
retain Source/import callable and argument identities, and show known current-value mutations. Borrow types carry kind
and target through checker, renderer and JSON. English stays in bray-messages. E7119 is no longer used for postconditions.
Fifteen expected diagnostic codes changed to E7135 without changing validity expectations or frozen test identities.

Current comparison716 passed /57 failed /99 ignored in96s, zero changed failing identities against accepted57.
Checker465 passed. Diagnostics44/messages85/JSON46 passed after the diagnostic batch. Focused Source/import laws pass.
The real CLI initialized-extraction and independent-result probe now passes using the freshly built 3bd2aaf72 driver.
Its permanent boundary test owned_match_results_release_the_subject_loan uses clear() in the core-only fixture harness.
The actual std.memory.move_initialized + replace_value case is verified by the real CLI/native audit, not that unit test.

Accepted native.log is 3bd2aaf72: standard-library planning/emission pass, bootstrap186 errors
(E7123=95, E7135=58, E7119=28, E7036=2, E7113=2, E7087=1), zero E7035.
The E7135 in task/retirement267 is an explained unmasking after a local E7123 repair, not a new invalid body.
The two retirement E7036 diagnostics disappear, with no added diagnostic identities. The initialized-extraction CLI probe and stale logs are removed.
No native async composition has executed. The source fixture and three invalid controls remain in the fixed comparison.

## Remaining work

Finish native bootstrap proof failures by owning transition classes, then native composition and invalid controls.
Trace selected/default/operator inputs, call attempt/completion, construction/replacement, projections/patterns, branches/
loops, await/yield/scoped suspension, scope exit/cancellation, outcomes/recovery and cleanup across all consumers.
Verify Source/import certificates and actual native success, panic/cancellation, startup rollback, blocked shutdown,
result/static disposal and nested foreign restoration. commit_start inspection found no recoverable failure between
activation.begin and publication. Runtime proof remains required. Audit from_core admission/refcount failure ownership.
Two old negatives need genuine forwarding/fabrication and mutation controls:
trusted_implementation_does_not_export_undeclared_authority and
shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. Do not weaken production to satisfy them.

The copied-address boundary repair distinguishes a loan used to read an address from loans carried by a borrowed component. Source/import regressions pass direct composition, a second exported call boundary, genuine borrowed-field escape rejection and non-authorizing raw addresses. The three frozen borrow-carrier regressions are repaired. Checker465 and comparison715/57/99 pass. The real retry-loop CLI passes at8f68e51fa. The new native run has182 errors and removes all E7036/E7113, but remains unaccepted pending the changed publish_wait failure. Remaining native errors include deadline/worker execution transfers and result copying. Retirement whole-slot conflicts are verified repaired. Trace actual proof data before
changing runtime contracts, initialized-memory policy or authority. Do not adopt unverified hypotheses from old notes.

Active diagnostic batch replaces the callable placeholder with parameter names/types/authority, result, ABI, trust, constness and async mode. It reuses existing typed enums, moves shared semantic converters to bray-symbols, removes duplicate ABI converters and shares machine keys on diagnostic enums. Source, template, rendering and JSON consumers are changed. Compiler conflict, renderer and JSON tests are added. Compiler/foreign42 passed/1 ignored, checker465, diagnostics44, messages86 and JSON47 pass. Comparison716/57/99 has unchanged57 failing identities. No driver rebuild is needed for this diagnostic-only batch yet.

Native-current.log at8f68e51fa has182 errors: E7123=96/E7135=57/E7119=28/E7087=1. One E7135 at publish_wait130 becomes E7123 at207 in the same already-invalid callable. Keep both current/baseline logs until explained. The terminating-branch positive control already passes. Its frozen failure is the capture-free opaque negative, so it does not establish a normal/abnormal effect leak. Do not make the speculative normal_effects change. Trace publish_wait with its actual owner and mutation boundaries. The opaque negative needs an explicit mutable-authority control before any expected-outcome correction.

At semantic completion, reread AGENTS and audit every changed file/behavior for DRY, APIs, contracts, errors and structural
cost. Only then run mandatory mutating cargo xtask style and cargo xtask format, inspect and check affected behavior.
No style/format pass has run. No goal, agent or reviewer is running. Dispatch the authorized independent reviewer on
committed draft PR710 only after semantic completion. Reviewer publishes its own GitHub review artifacts. Resolve and
rereview with the same agent. Then mark PR ready and Linear In Review. Never merge or mark Done.

## Execution and retained artifacts

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Shared original target, build directory target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX point to target/toolchains/llvm/active. Tooling builds use default stable.
Native LLVM/linker/executables run outside sandbox with Rust1.98.1 and LLVM bin on PATH. TMP/TEMP use worktree target/audit.
Native: retained target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Driver rebuild completed for 3bd2aaf72. Test binaries use all-five-package tooling features, compilation a64b618a hash.

Retain frozen baseline-failures.txt (67 IDs), native-baseline-identities.txt, accepted comparison.log/native.log, reusable
native intermediates and deferred-report baseline executable for the two controls. Keep only active build/probe/native logs.
Delete stale task-owned logs/probes/traces immediately after preserving necessary evidence. No debug hooks remain.
Git trust verified: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. Reuse until destination/auth changes.





