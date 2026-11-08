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

Current validation uses materialized-owner logs. Build passes in 2m15s. Checker has 449 passes, lowering 38.
The expanded trusted/adjacent selection has 236 passed, 36 failed, 97 ignored. Four old failure identities are removed,
with no new failures among frozen identities. Two extra static proof tests pass. Export/lowering has 386 passed,
54 failed, two ignored, removing three old failures without regressions. The real mutable-async composition, three
invalid variants, publication law and original await cleanup control pass.

The call-result producer now distinguishes a computed const value from an actual produced owner. Previously its
identity-free symbolic Call made marker borrowing substitute valid completion clauses into Trusted(Unknown), leaving
static initializers without carriers. static-completion.log locates that loss. The real process-context getters and
fixed initializer/fabrication, selected-instance and imported-static controls now pass. Pointer identity projections
and scalar computation are preserved. All diagnostic hooks are removed.

The prior loan-refresh repair reads retained logical storage through current_storage_value, rather than replacing
unrepresented physical backing with Unknown. Its actual ConditionVariable regression still passes. Explicit unknown
logical contents still propagate. No global Unknown or predicate rule changed.

Fresh materialized-owner-native.log stops at standard-library planning with 11 E7123, 10 E7119 and two E7127.
All three process-context failures are removed. No native composition has executed. Runtime Source compilation and
native fixtures remain unverified. The terminal-address and partial-stdlib SpinLock probes stop on buffer/allocation
diagnostics before testing their full intended cases. All 13 focused terminal_destructor module controls pass.
Semantic checkpoint: 771157820. New fixed checks cover short-circuit predicate positives/negative, all production
is_complete methods, and equal-const-result owner separation. boolean-publication-baseline.log has two failures and
the owner-separation pass. These checks explicitly request execution certificates. Lowering alone omits ordinary
certificate checking. boolean-publication.log traces the reduced fixture's root expression 12: each outcome publishes
Boolean, but return 13 receives Unknown. The same loss affects ChildProcess.is_complete. No diagnostic hooks remain.

Next repair must preserve actual Boolean production across the join. build_short_circuit delegates to build_condition,
which publishes true/false on separate paths. merge_values then turns differing expression-cache values into Unknown.
Binary expressions have no outer result-region completion. Pending result joins already have finite expression identity,
but preserving the output's relationship to evaluated operands also needs proof. Do not reconstruct missing operands
from syntax or introduce growing path formulas. Existing CFG phase expectations need deliberate review if staging changes.
No process or reviewer is running. Use known log filenames only.

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
