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

Current data rules follow actual retained loan contents without promoting them to owners. Retirement checks unchanged
versus overwritten data and 256 unrelated loans. Framing updates reached data before retaining a loan descriptor.
Absent terminal snapshots remain readable but cannot rename live data. Joins pair actual component paths and shield
unchanged nested data against ancestor replacement. Laws cover parameter/expression origins, missing proof, no owner
promotion and repeated-join stabilization. Both actual channel endpoints pass certificates/lowering.

Produced owning buffers stage a declared backing-region bound in the existing pending transfer. It supplies no freshness
or initialization, survives transport, and does not narrow ordinary raw addresses. The actual Windows environment body
and imported environment cursor check pass. All diagnostic hooks and temporary Source controls are removed.

Current repairs pass bound133, checker463 and the fixed comparison:
703 passed / 58 failed / 99 ignored, 93s, zero new failing identities against the preceding checkpoint and frozen7b5dc5634.
Static CFG construction excludes runtime admission failure for explicit aggregate formation. Nested retained cleanup
passes while discarded temporary cleanup remains rejected. The actual imported-library prepared-domain initializer passes.
Implicit method inputs now retain an existing borrowed receiver's dependencies rather than its temporary descriptor slot.
Source/imported guard factories and local use pass, returning a guard over local storage remains rejected. The actual
imported std guard reproduction passes. Span lookup uses the existing node_span helper. Scratch probes and traces are removed.
Never weaken expectations. Fresh xtask191122aaf passes native standard-library planning/emission. Bootstrap diagnostics
fall350 to237: E7119=99, E7123=117, E7036=4, E7113=2, E7035=14, E7087=1. Constant formation is repaired and escaping
lifetimes fall100 to2. The two remaining escapes involve borrowing an owner before transferring it during creation.
Entry preparation shares StorageIdentity::is_initialized_at_entry. Entry origins retain DependencyReferenceKind through
completion, removing root-type reconstruction. Body readers preserve original entry snapshots after descriptor changes.
Clause inputs name their enclosing evaluation frame and compose nested borrowed paths. Structural paths retain their role.
Local bindings copy under the source layout before conversion. All nine nullable borrowed-owner variants pass, including
invalid ownership controls. Nullable borrowed witness guards also pass. Consuming cleanup/carriers and condition-variable
authority retain their previous results, including rejection after descriptor overwrite. Temporary traces/probes are removed.
Next rebuild brayc/xtask together and run actual native bootstrap checking. They remain191122aaf until that build completes.
No native composition has run. Source fixture/lifecycle and invalid controls remain in the fixed comparison. Interface216
passed before these checker-only repairs and is unchanged.

Retain only frozen baseline-failures.txt, current comparison.log, reusable native artifacts and the deferred-report
baseline executable needed for two pre-existing negative controls. Use probe.log for the active diagnostic, then delete
it once its result is recorded. Delete passed build/check logs and superseded native/probe logs immediately.

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
