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
Reconstruction is a last resort only when no robust producer-owned representation can carry the needed state.
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

Current captured-numeric validation: compilation build 2m21s, checker 454 passed, lowering 38 passed. Trusted/adjacent has
241 passed, 34 failed, 97 ignored. Combined export/lowering/condition selection has 444 passed, 36 failed, two ignored.
No new failure identities against captured-boolean logs. Twenty-two combined failures and two trusted failures are removed.
Focused numeric/async selection has 31 passed, two failed, two ignored. The division-guard and joined-index failures also
occur in retained fc4b5160b baseline. The actual async fixture passes certificates and lowering for every function/lifecycle
body, plus its three invalid variants and loop controls. Source validity expectations remain fixed.

Boolean Begin, private Production and Completion evaluate the left once and right lazily. Actual cached inputs survive
until Production captures a scalar pending result. The join precedes one publication. No storage or dependency output
exists at Production. Fixed cache-lifetime, scalar-join, CFG and returned-predicate checks pass.

Ownership joins also preserve complete snapshot keys, keeping changed contents distinct from the original owner identity.
Const-call production preserves real owners independently of symbolic computation. Actual ConditionVariable loans use retained
logical storage without replacing unrepresented physical backing with Unknown. Their fixed regressions pass.

captured-numeric-native.log stops at standard-library planning with nine E7123, one E7119 and two E7127. Four postcondition/
totality and two Windows call failures are removed. Remaining paths are channel cleanup/store_message, byte-buffer loops,
Windows scope cleanup and moved collection fields. No native composition or runtime Source archive has executed.
Terminal-address and partial-stdlib SpinLock checks now pass.
Missing-slot-as-Absent experiments remain reverted because they lost independent cleanup authority. No diagnostic hooks
or reviewer are running.

Read-only commit_start trace: begin releases the pool guard before cell locking. The inspected path to publication uses
spin/atomic operations, total accessors and abort-only invariant checks. No recoverable failure is demonstrated there.
Source certification and native rollback/publication checks remain required before treating the protocol as verified.

Numeric range consumers now read attached computed data consistently. Bounded-sum and captured-range laws failed before
repair, including changed-live-source negatives. Opaque machine snapshots retain checked type bounds without revisiting
old operands. Actual fill_spare_bytes certificate and lowering now pass. No library contract or reconstruction was added.
Next trace the first remaining native defect before changing implementation.

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
