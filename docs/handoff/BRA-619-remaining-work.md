# BRA-619 completion acceptance

## Resume contract

Continue the authorized stack through BRA-619, verification, maintainability audit, independent PR review and pushed fixes.
Do not stop at status answers, restarts, checkpoints or PR boundaries. No merge or manual Done is authorized. Delete
this temporary record when its obligations are accepted. No handoff survives the stack. After compaction actually reread
AGENTS.md, coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this file.

No language feature, syntax, weaker authority, Rust runtime frame, TLS report owner, deferred destruction queue,
generalized suspended synchronous body, post-failure allocation or downstream semantic validation pass. Runtime ownership
stays in Bray. Work top down, reuse established owning results and remove superseded mechanisms. Correctness, compile
time, generated execution speed, binary size and maintainability are independent acceptance criteria. Batch tests, then
format. Use only release xtask. Preserve the user's C formatting. BRA-619 has changed no C files.

## Workspace and publication

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch feature/bra-619-scoped-cleanup-acceptance, based on reviewed BRA-626 d69592ca0. Published checkpoint c329313fd.
Draft PR #614: https://github.com/lejmer/bray/pull/614. BRA-619 remains In Progress. No independent review has started.
BRA-561 PR #611, BRA-505 PR #612 and BRA-626 PR #613 are ready. Preserve their branches and older BRA-619 recovery
6bbcb4a3. Do not replay them or restart completed reviews. Destination verified as git@github.com:lejmer/bray.git,
PUBLIC lejmer/bray, authenticated lejmer. Reuse this evidence unless destination or authentication changes.

The tracked Linear plan and coverage inventory retain the sequence. Semantic proof:
https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
Auto-review rejected copying the full proof into a new Linear document. No document was created. Do not retry.

## Established fixes and evidence

- Scoped cleanup preserves selected invocation, pending transfer, execution requirements, admission and frame retention.
  Exit admission precedes entry, with separate admitted and entered guards. Failed entry discharges only admission.
- Repeated semantic occurrences retire a dependency only when every reachable occurrence retires it. Actual destructor
  and finalizer hooks alone require whole-value cleanup. Inactive scoped protocols can partially move represented fields.
- Source structural cleanup now uses Lowerer::push_operation before shared outcome.check. The deleted forwarding resolve
  bypassed initialization retirement and caused duplicate BodyLocal destruction. Synthetic cleanup uses its existing sink.
- Storage analysis publishes actual Result(Completion) joins. The removed Bound(ScopedUse) interpretation marked a
  reachable continuation Unreachable and caused normal execution to cancel.
- Native large aggregates use private addressable snapshots while preserving native ABI and physical layout. Typed
  transfers cover construction, mutation, projection, joins, patterns, arrays, nullable values, imports and ABI calls.
  Ordinary moved bytes are not zeroed. Native PanicReport empty-slot ownership remains explicit.
- Fixtures enable allocation denial inside the entered body after local construction. Admission may allocate before
  establishing cleanup. All six pending transfers retain their no-allocation checks. propagate_error returns its with
  result because try targets that nearest compatible result boundary. No assertion was weakened.

Published c329 passes 38 lowering, 27 scoped, 316 source-query and 171 product-codegen tests, plus earlier unchanged
93 LLVM tests. Rustfmt, mutating release style and Bray format passed. Logs use bra-619-retirement-* and completion-*.

## Current connected report repair

The uncommitted batch extends the existing owned outcome, rather than adding a second owner or runtime protocol:

- MIR Return carries an optional owned normal report. Protected frames carry it in typed Completed terminal state.
  CheckCallOutcome has an optional completed-report destination. Walkers, remapping, validation, demand, inspection,
  executable serialization and LLVM consume all operands. Scalar inlining excludes report returns. Receiver-tail append
  forwards normal reports through its existing MirEdge. Shared report joins preserve panic over cancellation over incidents.
- The existing native RunOutcome and FrameProgress already carry reports. Every synchronous Bray invocation already has
  an incoming outcome context. No ABI field or version change is needed. Native direct-await delivery already retains
  normal progress reports. Source lowering still emits report None, so the native loss is not fixed yet.
- Callable dependency summaries distinguish no report from an independent report, separately for normal and abnormal
  outcomes. Substitution, contextual Self, witness resolution, default-input expansion, fixed-point merge, portable
  encoding/decoding, selective record inventory/remapping, validation and specialization hashing include those fields.
- Checked call contracts have immediate and deferred report outputs identified by CallReportOutput. The oversized
  dependency_table module is split into call contracts and the table, with a thin root. Scoped exit result contracts
  now retain their dependencies even without a source expression for the result. The old input mapping incorrectly
  treated the exit parameter as the original initializer and now uses the capability's actual access.
- Existing CFG blocks record their lexical catch collection. Catch entry has a separate block. Edge flow forwards
  collections across normal and nonlocal exits, or transfers caught reports into the ordinary catch result. Scoped call
  completion records its exact StorageExitPoint. The normal-with-result classification is shared in bray-bound-tree.
- Pending report dependencies use the existing storage fixed point, borrow retirement, escape diagnostics and suspension
  retention. Report values snapshot portable dependencies at call entry and commit on the actual outcome. Static value
  retention seeds ordinary with errors and caught reports through the existing ownership closure. StorageFlow publishes
  which source/catch collections can retain reports, for lowering to consume directly.
- CFG call phases also delimit compiler-owned evaluation and with results. Report lookup classifies actual selected
  source invocations and asserts their checked contracts, rather than requiring a callable contract for every phase.

Current batch compilation succeeds. Batched tests pass: 317 source-query, 331 checker, 45 MIR, 38 lowering,
192 package-interface and 205 symbols. The new retained_scoped_exit_errors_keep_borrowed_origins_alive regression rejects
an error whose destructor reads a dying owned resource and accepts the same error borrowing caller-owned storage.
Logs: bra-619-report-owner-check.log, report-owner-test-build.log, report-owner-query.log and per-test-binary logs.
The previous scoped batch passes all 27 tests. No formatting pass has run on this uncommitted batch yet.

## Work still required

1. Finish the same ownership boundary for default-provider evaluation reports, opaque/trait/generic abstraction,
   recursive summaries, direct await and finalizer report/error capture. Do not infer absence from total, pure, ownership
   or an unresolved contract. Preserve entry snapshots and do not query only on failure edges with lost typed errors.
   The current symbolic report-instantiation fallback handles top-level terms only and needs recursive handling.
2. Complete targeted source/imported acceptance for ordinary with errors, retained incidents, nested catches, repeated
   scopes, moves, mutation, cancellation and destruction. Validate report-presence inference as well as lifetimes.
3. Implement the source accumulator using the published StorageFlow collection demand and existing CleanupOutcome.
   IMPORTANT: CleanupOutcome::new/begin ENTERS A CLEANUP SHIELD. A pending source collection must use allocate plus
   initialization of Normal and EmptyPanicReport without entering a shield. Keep ordinary source cancellation intact.
   Catch success forwards pending incidents, caught panic owns them in its Error value, nonlocal exits forward the
   collections they leave, and normal Return/Completed publishes the report. Checked calls and direct await capture
   completed reports before another native call resets the context. Local cleanup dispatch forwards Incidents instead
   of dropping them. Reuse PanicReportSuppression, with no allocation and no immediate incident disposal.
4. Preserve pending collection state/report storage through frames and cancellation cleanup. Allocate before relevant
   frame states. Include these physical initialized slots in existing MirFrameRetention. Inactive frame cancellation
   must not read source-body storage whose initialization never ran. Normal source pending state is not an active shield.
5. Finish native acceptance, cost checks, mechanical audit and formatting. Checkpoint/push, independently review PR #614,
   resolve findings, push fixes and mark ready/In Review only when complete. Remove stale handoffs before final completion.

## Native observation and cost

Fresh c329 product vL9bks reaches final assertions. Entries 23 and exits 17 pass. Errors and captures are 12 instead of20.
The eight missing errors match six return/break/continue cases, nullable propagation and result propagation.
CleanupOutcome::dispatch lets Incidents continue normally without transferring its report owner. The source accumulator
above must fix this ownership loss and its lifetimes together. No new native build has tested the current report batch.
All previous bootstrap, foreign-entry, async-values and ordinary-finalization smokes pass both profiles.
Required scoped totals: entries23, exits17, errors20, captures20, finalizers9, correct exit-before-local ordering,
no inactive body entry, no allocation after admission and exactly two final incident stderr lines. Do not weaken checks.

Retained product target/native/bra-505/bray-runtime-artifact-smoke-vL9bks/scoped-cleanup-development/scoped-cleanup.exe.
Logs: bra-619-pending-native-smoke.log, pending-native-panic.log, pending-native-counts.log. Counter trace labels used old
PDB offsets. Current .data RVA0x5e2000, entries+2712, exits+2720, errors+2704, captures+2672, finalizers+2688.
The earlier nL6Inc duplicate destruction, L5EJSv unreachable join, EcWNsS pre-admission denial and Xh3zAJ discarded try
result have established causes and fixes above. Do not re-debug them.

Old native object generation exceeded36GB. LLVM SROA expanded one resume from62,494 to1,151,009 instructions.
The aggregate storage repair emits LLVM plus objects in35.883s with four workers, sampled near1.1GB under8GB,
134.78MiB combined artifacts. No optimizer pass was disabled. Latest retained matching-package IR is
bra-619-retirement-ir-product,132.88MiB,9,943MIR blocks,7,099operations before the completion publication repair.
Reuse this cost evidence unless changed inputs justify a new measurement.

## Execution

Shared CARGO_TARGET_DIR=C:/Users/anton/source/repos/bray/target and CARGO_BUILD_BUILD_DIR=the same /target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use /target/toolchains/llvm/active. Compiler tools use default Rust1.99.
Native packaging uses process-local Rust1.98.1-x86_64-pc-windows-msvc, TMP/TEMP=/target/native/bra-505.
Native LLVM, archiver, linker and executable workflows run outside the sandbox. Worktree writes and Git need escalation.
Never clean shared storage. Use rg --files before unknown paths. Never wildcard rg paths in PowerShell. Python always
uses explicit UTF-8 read_text/write_text. Read short relevant slices. Parse selected facts from large single-line profiles
and LLVM files. Task-owned debug helpers terminate their children and close handles. No debuggee remains.
