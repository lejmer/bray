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

Current scalar-join-datum validation: compilation build 3m15s, checker 456 passed, lowering 38 passed. Full fixed
comparison has 690 passed, 66 failed, 99 ignored in 125s. No new failures against bounded-effect. Imported joined bounds,
raw-byte access and runtime text storage now pass. Ten focused source checks pass. The async fixture passes certificates and lowering for every function/lifecycle
body, plus its three invalid variants and loop controls. Source validity expectations remain fixed. The separate division-
guard and joined-index failures also occur in retained fc4b5160b baseline.

Boolean Begin, private Production and Completion evaluate the left once and right lazily. Actual cached inputs survive
until Production captures a scalar pending result. The join precedes one publication. No storage or dependency output
exists at Production. Fixed cache-lifetime, scalar-join, CFG and returned-predicate checks pass.

Ownership joins also preserve complete snapshot keys, keeping changed contents distinct from the original owner identity.
Const-call production preserves real owners independently of symbolic computation. Actual ConditionVariable loans use retained
logical storage without replacing unrepresented physical backing with Unknown. Their fixed regressions pass.

scalar-join-datum-native.log stops at standard-library planning with six scope-cleanup E7123. Remaining paths are bounded
channel creation and its two lambdas, endpoint send/receive and Windows environment_storage. No native composition or
runtime Source archive has executed.
Terminal-address and partial-stdlib SpinLock checks now pass.
Missing-slot-as-Absent experiments remain reverted because they lost independent cleanup authority. No diagnostic hooks
or reviewer are running.

Read-only commit_start trace: begin releases the pool guard before cell locking. The inspected path to publication uses
spin/atomic operations, total accessors and abort-only invariant checks. No recoverable failure is demonstrated there.
Source certification and native rollback/publication checks remain required before treating the protocol as verified.

Numeric ranges read attached data. Actual fill_spare_bytes passes, with changed-live-source negatives. Opaque machine
snapshots retain checked type bounds without source replay. Returned-reference relationships consume complete stored bounds.
Actual store_message passes, with shared/opaque/mixed/stale/raw/further-owner negatives. Effect export carries flat physical
regions and remaining origin uncertainty together. Both collection destructors and bounded summaries pass. Terminal checks
are unchanged. No library contracts or reconstructed state were added.

ScalarValue represents a produced opaque non-owning datum, separate from missing meaning and ownership identity. The
actual read_native_u64 trace previously admitted Scalar(Joined(...), Unknown), losing guards before call admission.
Scalar joins now produce usable data with the requested epoch. The red-before scalar law checks stable repeated joins,
predecessor order, same storage context, distinct epochs, no ownership and missing-predecessor conservatism. Type lookup
uses the immutable identity, finite evaluation dependency is retained, and runtime-only data stays unportable. Actual
byte reads now pass. Next pin and trace cleanup lifecycle entry for bounded channel creation, then the remaining scopes.
No diagnostic hooks, task-owned processes or reviewer are running.

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
