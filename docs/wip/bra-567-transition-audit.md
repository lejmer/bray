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
Keep verified design improvements even when they do not solve the active failure.
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

Audit baseline b90076c1f21f3853ebfd90954b3babe26f122eca. PR branch HEAD is the latest pushed checkpoint.
The complete transition inventory and native composition remain unverified. Do not stop at a local fix or checkpoint.

Accepted repairs retain producer-owned data through actual loan descriptors, component joins, borrowed receivers,
entry snapshots and nullable conversions. Moves retire authority. Nested entry observations preserve their original
contents. Repeated joins stabilize. Owning RawBuffer formation retains declared backing bounds without freshness or
initialization claims. Explicit aggregate formation excludes runtime admission failure. Source/import regressions include
local use and invalid escape/overwrite controls. The real channel endpoints and Windows environment bodies pass Source.
Cleanup retains actual Dispose(T)/Cleanup(T) effects, captured inputs and result cleanup separately until completion.
No type-only cleanup, source-root reconstruction, debug hooks or scratch probes remain.

Current diagnostic batch preserves required and actual conditions, call/cleanup/postcondition roles, declaration origin,
source/import names, actual field arguments and scalar snapshots. The Boolean proof API is completely replaced.
Current PostState values supply known mutation locations without an invalidation ledger. Tuple and array projections
retain distinct spellings. Certificate failures distinguish postconditions, cycles, missing evidence, dependency errors,
contract mismatch and depth limits. Unexportable conditions have a separate diagnostic. E7119 no longer invents an
execution property named "ensures". English remains in bray-messages. Fifteen exact diagnostic-code expectations changed
from execution-property failure to postcondition failure. Validity expectations and test identities remain fixed.

Current checks: fixed comparison709 passed /57 failed /99 ignored in90s, zero new failing identities against accepted57.
Nine focused diagnostic regressions pass. Checker464, diagnostics44 pass. Messages84 pass.
The expanded certificate suite is127/7/2. Its seven remaining proof/validity failures are not waived or claimed passing.
Source async fixture and three invalid controls remain in the comparison. Bound133/interface216 previously passed and
are unchanged by diagnostic work. No native composition has executed.

Shared native brayc/xtask and std bundle still represent846a110b9, prior to diagnostics. That native run passes standard-
library planning/emission, then bootstrap fails with212 diagnostics: E7119=92, E7123=106, E7035=7, E7036=4, E7113=2,
E7087=1. Rebuild before inspecting current diagnostics. Remaining moved-storage errors alias retired nullable pattern
payloads in execution/entry_scope and scheduler/lane. The two escape errors borrow execution before transferring it into
creation. Native191122aaf identity baseline remains frozen in native-baseline-identities.txt.

Retain frozen baseline-failures.txt and native-baseline-identities.txt, current compact comparison.log/native.log, native
artifacts and the deferred-report baseline executable for two pre-existing negative controls. Remove passed build/check
logs, superseded diagnostic outputs and temporary traces immediately after recording needed results. Keep only one active
probe.log. No style/format pass has run. No reviewer, subagent or goal is running.

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
