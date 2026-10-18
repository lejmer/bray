# BRA-505 ordinary finalization and retained incidents

## Resume after every context compaction

The user explicitly requires an actual reread of AGENTS.md, coding-conventions.md,
crates.md and this task record after every compaction or restart. A summary is not a substitute.

The user requested a pause after finishing the current readability/formatting correction
and publishing a recovery checkpoint so they can restart the desktop application. Pause
there until they resume. After resumption, continue BRA-505 through BRA-619 without stopping
to answer questions or at checkpoints. No merge is authorized. Commit/push checkpoints are
authorized. Delete temporary handoffs when their obligations are accepted. No handoff
survives the stack.

Runtime ownership, activation, waiting, retirement and lifecycle policy belong in Bray.
Do not introduce or extend a Rust runtime frame, waiter owner, second scheduler or hidden
blocking root. Existing Rust runtime ownership is a migration target. No new language
feature, syntax, compatibility layer, ABI bump, post-failure allocation or downstream
reproof of established compiler results. Preserve the user's C Allman formatting.

Correctness is necessary but insufficient. Assess compiler time, generated execution,
binary size and whole-system maintenance independently. Do not rely on LLVM to remove
avoidable work. A helper extraction or file move does not remove competing mechanisms.
Work top down: establish the whole producer/consumer contract, implement the connected
path, then batch acceptance and fix defect classes. Do not organize work around local
proof/test/format checkpoints. Name what each change replaces and remove that machinery.

## Workspace and baseline

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch: feature/bra-505-admitted-ordinary-finalization. Draft PR #612.
Base: feature/bra-561-compiler-described-activation, reviewed PR #611 at 7a254388.
BRA-619 recovery remains at 6bbcb4a3 on feature/bra-619-complete-selected-scope-native-cleanup.
Preserve branches and the user's main checkout. Do not replay older stack work.

Previous pushed checkpoint: 9e4bfd5d9. This recovery checkpoint preserves the larger
incomplete batch and must not be described as accepted or finished. Independent review
5560583230 preceded active human architectural review. Do not launch another agent review
unless asked. PR #612 remains draft and BRA-505 remains in progress.

## One connected implementation contract

1. Checking selects lifecycle actions and whether their resolution can suspend, including
   child work in a finalizer's error. Source, temporary, replacement and imported consumers
   use that result. Quiescence abandons graceful finalization while retaining payload owners.
2. Lowering expands selected structural actions into the owning continuation. One existing
   cleanup collection owns the incidents. Arrays use a counter. Pending values and projections
   live in enclosing storage. Concrete specialization precedes expansion and layout. Do not
   rebuild a unit per operation or rerun authority, liveness or selection downstream.
3. The existing Bray activation driver composes asynchronous children into the same run.
   Ordinary composition consumes a child. Quiescence detaches a terminal owner into its
   original slot without entering its inactive body or destroying its captured owners.
4. Attachment transfers admitted error backing after child work is resolved. Later synchronous
   abandoned destruction consumes payload, captures, backing and provider dependencies once.
   Panic/cancellation preserve the collection and its execution/attachment requirements.

Replace the generated Immediate override, synchronous structural-helper assumption and
consuming interpretation of retained quiescence. Reuse selected expansion, the shared await
emitter, existing activation owner and report chain. No reservation per organizational helper.
Recursive/erased/raw-buffer family migration is BRA-506. Existing call boundaries prevent
recursive unrolling. Their presence does not prove asynchronous family acceptance. BRA-560
owns started tasks and BRA-562 waiter migration. Do not silently use consuming TaskObservation
or blocking resolve_task as an admitted non-consuming wait. General generators remain BRA-484.

## Whole-path boundary assessment

The current batch is not a complete implementation of the contract above. Do not proceed by
adding an abandoned-helper variant alone. That would leave source destructor bodies owning
potentially suspending receiver-tail cleanup and would merely move the same problem.

The working design hypothesis is to keep the synchronous destructor body shared and let the
enclosing selected cleanup own its remaining represented-part continuation. Preserve the
existing order: whole destructor body, then remaining initialized parts. Do not move child
finalizers before the whole destructor or destroy captured owners during quiescence.

Implementation mapping to finish before changing this boundary:

- `lowering/initialization.rs` already allocates the root and represented-part initialization
  guards and updates them on committed moves/stores. Reuse that exact partition and update
  logic. Caller-owned state must survive normal and abnormal body returns. Do not infer final
  initialization from a normal-return postcondition or add ownership history.
- `lowering/lowerer.rs` introduces the hidden abandonment argument, and
  `lowering/cleanup/lifecycle.rs` branches on it for implicit receiver cleanup. Replace that
  ownership boundary, not ordinary local cleanup or explicit receiver operations. The body
  still resolves its locals and leaves no escaping receiver-derived borrow.
- `synthetic/lifecycle/representation.rs` already expands selected represented operations.
  Reuse it for the remaining receiver tail under the published guards. Keep the same cleanup
  collection on body failure and continue remaining independent parts. The caller supplies
  ordinary versus already-abandoned disposition before realizing synchronous retained cleanup.
- `asynchronous/parts.rs` currently treats a source destructor as owning its represented tail.
  Checking must attribute the tail's suspension and execution requirements to the complete
  lifecycle operation while keeping the body contract synchronous. Source/imported consumers
  must share that distinction. No downstream rechecking of authority or liveness.
- Imported executable metadata, specialization, callable layout and lifecycle demand must carry
  the same receiver-state contract. Reuse one shared body. No copy of a destructor body per
  cleanup site, no new activation per destructor and no separate reservation per helper.

Execution context is a separate boundary with the same ownership discipline:

- `activation/execution/invoke.bray` asks for readiness before invoking a provider callback.
  Preserve that admission point and the exact live producer identity.
- `native/activation/execution.rs` currently obtains all readiness from a task registration.
  A synchronous action must instead use the actual enclosing task lane or native entry root.
  `native/activation/callback.rs` may install a resumable task continuation only for resumable
  operations. A synchronous callback must not publish a continuation or acquire composition
  authority, including during nested entry.
- `entry/execution.bray` owns native root/foreign dispatch, attachment acquisition and the
  context bridge. Any root capability state belongs to this existing Bray boundary, with
  exact nesting and restoration through callback, report and attachment retirement. A root
  kind supplies its defined capabilities. A foreign entry supplies none by default. Do not
  infer capabilities from being on an OS thread, or use a callback that always reports ready.
- Retained report backing must keep the actual provider and attachment dependencies alive.
  Trace the current allocation/transfer/release path before claiming that comments or a live
  byte buffer prove this. Returned reports and static cleanup are consumers, not exceptions.

Cost constraints for this hypothesis: reuse existing initialization guards instead of adding
another state history, one shared machine body per destructor specialization, tail continuation
storage only while cleanup is pending, no post-admission allocation, no Rust frame or scheduler
owner, no runtime descriptor-graph reconstruction. Compare these costs with the current hidden
argument, duplicate branches and local guards. Any schema or helper that cannot replace existing
machinery is a reason to reconsider rather than an automatic addition.

This is a working architecture assessment, not evidence that these boundaries are implemented
or proved. Finish mapping guard transport, abnormal returns, imported layout and root retirement
before adding code for the changed boundary. Then implement the connected path and batch tests.

### Guard-only transport is insufficient

A bounded semantic counterexample rules out publishing initialization bits as the complete
receiver-tail contract. A body can leave a child initialized and pending, or successfully
finalize that same initialized child. Both histories have the same initialization bit. Ordinary
cleanup must finalize only the first. Abandoned cleanup destroys both without finalizing.
`cleanup/control.rs` consumes `finalizer_is_complete(plan.exit(), access)` separately from the
initialization guard, so the production implementation already exhibits this distinction.

Therefore do not implement the guard-only metadata proposal above. A caller-owned tail must
preserve the selected checked continuation, including completion choices, guards, failure
outcomes and source order. Reinterpreting only its final storage state would lose established
analysis and require the downstream reproof that the user forbids.

The next architectural question is how to retain that existing checked MIR tail while sharing
the synchronous body. Map the actual normal/abnormal tail entries and their live inputs before
choosing an internal representation. Prefer splitting/reusing existing MIR and its ID mappings
and continuation expansion over serializing another semantic cleanup language. Do not duplicate
the entire destructor body at every caller. No source/body/tail ABI change has been implemented.

## Current batch and evidence

The batch now carries BoundCallResult through generated finalizer invocation, expands
structural cleanup at explicit existing Goto continuations, preserves existing operation IDs
until one final compaction, and materializes boundary SSA inputs into storage. It shares
MirFrameRetention and the existing cleanup collection. Generated blocks inherit the enclosing
phase. The checker includes suspending finalizer completions, not just async callable bodies.

Inactive captures have described quiescence/destruction entries derived from checked entry
cleanup plans. MirFrameComposition distinguishes Completion, Abandon and Quiesce. The existing
Bray header stores the disposition, preserves retention through panic/cancellation, and detaches
terminal retained children without releasing captures or outgoing records. The resident registry
has a Retained state which permits inspection but rejects reactivation. Later destruction uses
activation.inactive and existing callback containment, admission and report ownership.

Unused ExecuteCleanupBroadcast/ExecuteLifecycleResolution MIR operations were replaced by
BroadcastInactiveFrame/DestroyInactiveFrame, whose operands identify the actual owner. Their
walkers, mappings, imports, inspection and LLVM consumers were changed together. Source selection
now has LifecycleAction::Future. Quiescence reuses cleanup continuation construction. NativeRun's
existing callback scope masks composition authority during synchronous nested actions and still
rejects recursive resumable entry. No new Rust frame was added.

Synchronous action readiness reuses an existing task lane without publishing another run
continuation. Outside a task it grants no capabilities, permits movable actions with no lane
requirements, and checks exact attachment identity for origin-affine actions. Synchronous native
callbacks can execute without manufacturing a task context. Root-specific capabilities and
retained provider/attachment lifetime still require the bounded boundary audit below.

The existing ordinary-finalization native fixture now includes an inactive Future in its aligned
error. Its captured destructor checks that error destruction has begun, and the body must never
run. The future is prepared before allocation denial. This extension has NOT run yet.

Current verification, 2026-10-18:
- 331 checker, 44 MIR, 38 lowering and 191 package-interface unit tests pass.
  Logs: target/bra-505-result-unit-tests.log and target/bra-505-checker-joins.log.
- All 87 source-lowering tests pass. The array-guard regression now checks actual suspend/resume
  states instead of treating inactive capture entry states as body resumptions.
  Log: target/bra-505-result-lowering-tests.log.
- All 111 trusted-contract source tests pass, including owned tuple results through if, block,
  loop and match, mutation/reconstruction rejection, and rejection of mixed certified pairs.
  Log: target/bra-505-result-joins.log.
- Actual production report/activation declaration audit passes. Negative production tests for
  exact token/outcome pairing and independent output destinations passed previously.
  Log: target/bra-505-native-contract-audit.log.
- Rust runtime tests compile with --no-default-features. Three test consumers now pass the
  explicit composition enum instead of bool. This is compile evidence, not runtime execution.
- Fresh xtask builds. Native smoke session 92081 finished with failure during standard-library
  emission, E7123 in channel Sender.send. Log: target/bra-505-result-native-smoke.log.
  A source reproduction showed pending-value extraction changing an independent channel snapshot.
  Result publication had skipped its planned ownership transfer, leaving apparent cleanup effects.
  Restoring those transfers makes production_channel_declarations_check_their_own_bodies pass.
  Log: target/bra-505-channel-proof.log. Temporary checker tracing has been removed.
- The source regression/lowering batch completed successfully with 111 trusted-contract and
  87 source-lowering tests. Native acceptance must rerun with the corrected, rebuilt tool binary.
- The current batch remains unaccepted. The full retained-incident maintainability audit and native
  acceptance remain outstanding. The checkpoint's formatting results are recorded below.

The callback result failure was an owning control-flow defect. Yielded branch values became
Unknown before publication and lost common witness facts. Result regions now have explicit
begin/completion operations. Begin retires an earlier evaluation, completion publishes staged
results without invoking calls again, and the existing component join also joins yielded owners.
Borrowed referents retain their original identities. This adds checker operations only, no runtime
operations, language vocabulary or authority. There is one bounded recursive component join.

Result publication now commits the existing result transfer plans. It excludes iteration cursor
and element storage in both liveness and initialization, using StorageIdentity.is_iteration_storage.
A checker regression distinguishes result birth from iterator protocol birth. Direct initializer
destinations remain included. A temporary stricter value-only classification was wrong because
results can be initialized directly into local bindings. It was replaced, not retained as another
path. No call-effects pass or iterator invocation is replayed.

All 111 authority source tests pass after the ownership correction, including the small generic
regression using an owned nullable input in the core-only fixture. This is not native acceptance
or the destructor-body/tail repair.

The human readability correction replaces nested yield-if expressions with flat conditional
chains, combined pattern conditions or guarded match arms, and replaces return-if expressions
with direct branch returns. The formatter's shared punctuation rule now retains a space between
control-flow/effect keywords and parenthesized operands. It preserves ordinary call syntax.
Formatter regressions cover yield/break tuples and the related keyword operands, including
parse preservation and idempotence. Maintained source is reformatted with that rule.

Checkpoint verification after the human correction, 2026-10-18:
- All 58 formatter tests pass, including 42 grammar/layout tests. Log:
  target/bra-505-keyword-format-tests.log. The 331 checker tests also pass after the
  source-refinement helper extraction. Log: target/bra-505-checkpoint-format-tests.log.
- All 111 trusted-contract source tests pass. Actual production report/activation and channel
  declaration checks pass after the rewritten conditionals. Logs:
  target/bra-505-result-joins.log, target/bra-505-native-contract-audit.log and
  target/bra-505-channel-proof.log.
- The rewritten selected-exit destructor and storage boundary declarations pass their direct
  production tests. Logs: target/bra-505-readable-exit-proof.log and
  target/bra-505-readable-storage-proof.log.
- cargo xtask style, cargo xtask format, cargo fmt --all and git diff --check pass.
  Structural style still reports oversized-module warnings. These are not a completed
  maintainability audit of the retained-cleanup implementation.
- A repository search finds no remaining return-if, yield-if or targeted missing keyword spaces
  in maintained runtime, standard-library and xtask Bray sources. Deliberately unformatted
  formatter test input remains unformatted so the tests exercise the correction.
- Relative to 9e4bfd5d9, production files have net +2404 lines, including inline unit tests.
  Dedicated test/fixture files have net +284 lines. These counts cover the entire recovery batch,
  not just the readability correction, and are not acceptance of its remaining complexity.
- No task-owned process remains. Build artifacts remain in the existing shared Cargo target at
  C:/Users/anton/source/repos/bray/target. No shared artifacts or branches were removed.

Pause after publishing this checkpoint. Do not mark PR #612 ready or BRA-505 complete.
On resumption, rerun native acceptance with the rebuilt tool and finish the connected
body/tail and retained-dependency work below. Do not repeat the passing unchanged source
suites merely because the desktop application restarted.

## Remaining work, considered as one path

- Settle known-abandoned destruction before helper realization. The general helper takes a runtime
  abandonment flag and emits ordinary child cleanup too. That can demand async work in a synchronous
  helper even when an incident consumer knows abandonment. Do not rely on LLVM elimination. Carry
  the established choice into lowering and keep one selected-action implementation. The exact
  representation has NOT yet been changed. Consider its helper identity, call ABI and source
  destructor receiver cleanup together, not only the incident adapter.
- Verify capture destruction has the retention/entry contract needed by its actual children, with
  no early capture destruction during quiescence. Preserve whole-owner-before-part destruction.
- Complete synchronous action execution outside task callbacks under the retained report domain.
  Keep exact thread/lane, provider and attachment requirements. No new Rust owner or permissive ready
  callback is authorized. Static cleanup and returned reports are actual consumers.
- Verify native non-source descriptors such as TaskObservation handle quiescence and retained
  destruction exits exactly once. Existing task migration boundaries remain explicit.
- Check registry transitions, all native return paths, report/unused record ownership and anti-forgery
  against actual caller declarations. Native fixture modes must cover retained composition and disposal.
- Consolidate append_report_records with equivalent report-chain assembly before finishing the audit.
- Batch source/import/native acceptance: normal/panic/cancel, arrays/temporary/partial owners, retained
  child work, no inactive body execution, exact capture destruction, aligned errors, denied allocation,
  no new Rust frame or hidden blocking, and bounded compile/runtime/code-size costs.
- Finish AGENTS audit, style and format after tests pass. Push fixes, keep review/Linear/PR accurate,
  and continue BRA-619. Keep scoped_selected_calls_emit_native_artifacts required across both issues.

## Efficient execution

Shared CARGO_TARGET_DIR: C:/Users/anton/source/repos/bray/target.
CARGO_BUILD_BUILD_DIR: target/cargo. LLVM prefixes: target/toolchains/llvm/active.
Worktree writes/Git and native workflows require escalation. Do not clean shared artifacts.
The verified remote is git@github.com:lejmer/bray.git, PUBLIC lejmer/bray, authenticated lejmer.

Build Rust compiler tooling with 1.99. For native provider work, run the fresh xtask executable
with process-local RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc and TMP/TEMP=target/native/bra-505.
Do not use cargo check -p bray-runtime with bundled-bootstrap under Rust 1.99 again. Use
--no-default-features for Rust-only runtime checking. No global toolchain changes or artifact deletion.

Use bounded reads and real paths from rg --files. PowerShell does not expand rg path wildcards.
Do not dump complete Linear issues or large logs. Reuse compiled test binaries and fresh xtask
where possible. No parallel Cargo builds competing for the shared target. Task-owned processes
must be accounted for before starting more work.

For formatting/style with the existing debug compiler cache, use process-local
CARGO_ALIAS_XTASK='run --package xtask --'. This runs the same repository xtask commands without
rebuilding the optimized compiler through the default release alias. Do not change checked-in
Cargo configuration merely to select this local build profile.

## Native inactive-entry proof boundary

The real LLVM caller is `translation/unit/effect/task.rs::cleanup_inactive_frame`.
It materializes a separate initialized token temporary, passes the fixed callback/readiness
symbols, and supplies the independent checked-call outcome context. Broadcast borrows the
selected owner. Retained destruction consumes it through the checked MIR move. The registry
checks epoch, descriptor identity and inactive/retained phase before a header is selected.
No callback executes during cursor acquisition. Zero lookup aborts before dereference. The
local cursor issuer receives only the validated address while that native acquisition remains
exclusive. Ordinary source can construct the same fields but cannot establish the entry
predicate. Acquisition must require the exact token/outcome pair, with negative production-
declaration cases for absent authority and either differently paired slot.

Keep the exclusive-owner requirement on `retained_address`. Derive that owner only from the
complete native entry contract when reading its exact input. Do not derive it from lease or
cursor fields. Retaining invocation now has one shared implementation, reused by run advance
and inactive cleanup, and the latter supplies literal valid operation/argument pairs.

Automatic review rejected the broad root rewrite, a report serialization rewrite and a
proposal to weaken the new lookup helper's exclusive-owner precondition. None was applied.
Use narrow changes under the existing stronger contracts. No report representation or ABI
change is needed merely because the first failed invocation caused downstream witness errors.

The production declaration audit and all source lifecycle/lowering tests now pass as recorded
above. The persistent failed native build remains at target/native/bra-505/runtime, but current
native acceptance uses the full smoke command and retained failure directory. Do not claim
native retained-incident acceptance before that command and required fixtures pass.
