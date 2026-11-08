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

Focused prepared-version checks: 24 passed, including all five regression identities, three recursive-summary
checks, terminal destructor controls, bounded channel creation and async fixture controls. Prepared checker: 457 passed.
Interface: 216 passed outside the sandbox. Deferred-move law retains both cleanup phases and staged output references.
comparison.log: 692 passed, 66 failed, 99 ignored. No new failing identities. Bounded-channel
failure removed. Runtime was 213s versus 94s, with concurrent driver compilation. Investigate isolated cost before handoff.
These source runs predate the last semantic-presence simplification and unary release/projection preparation corrections.

Latest rebuilt xtask includes those corrections. native.log stops in stdlib planning with nine
E7123: send/receive outer and generation-block cleanup, lock/get on both loop back-edges, and environment_storage cleanup.
Bounded-channel creation is repaired. The new loop call failures are regressions from the four-error committed baseline.
Do not expand or claim convergence until repaired. No runtime Source archive or native composition has executed.

production_channel_endpoints_preserve_loop_and_cleanup_authority pins both actual endpoint bodies and is red.
endpoint-trace.log shows await exporting an escape of the channel generation word address, exposing the core
allocation and invalidating sibling lock markers at the back-edge. Next trace that effect through the platform timed-wait
boundary and preserve its actual contract. Do not weaken alias rules or add unproved library guarantees. Trace hooks are
restored. The trace executable differs from this checkpoint only in diagnostic output.
Explicit disposal/region distinction is verified: checker 457, interface 216, fixed comparison 692 passed / 67 failed /
99 ignored in 99.4s without concurrent compilation. Relative to the 7b5dc5634 baseline, bounded-channel failure is removed
and the newly pinned endpoint positive is added. No other changed failing identities. No inference from access_type
remains. Endpoint source diagnostics still include four call and four cleanup failures. Native environment cleanup remains.

Retain only current evidence: baseline-failures.txt, comparison.log, endpoint-trace.log, native.log and the executable
baseline deferred-report-fc4b5160b-baseline.exe. The executable supports the specific pre-existing-failure comparison.
Native products remain reusable. Replace active logs in place and remove superseded artifacts immediately.

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
