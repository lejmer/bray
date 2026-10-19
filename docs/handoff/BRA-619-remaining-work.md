# BRA-619 completion acceptance

## Objective and baseline

Continue the authorized stack through BRA-619, including pushed checkpoints, independent PR review and fixes.
No merge or manual Done is authorized. No new language feature, syntax, Rust runtime frame, generalized
suspended synchronous body, authority relaxation, post-failure allocation or downstream validation pass.
Use only release xtask. Preserve the user's C formatting. Batch verification before mandatory formatting.
Read AGENTS.md, coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this record after compaction.

Worktree: C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch: feature/bra-619-scoped-cleanup-acceptance, based on d69592ca0, reviewed BRA-626 PR #613.
The branch HEAD carries the corrections below. Published checkpoint: 0f90849c9, draft PR #614. BRA-561 PR #611, BRA-505 PR #612 and BRA-626 PR #613 are ready.
Preserve the older BRA-619 recovery branch at 6bbcb4a3. Do not replay it or restart completed reviews.

The tracked Linear rule plan, coverage inventory and sequence preserve the stack plan. The already-public proof
is pinned at https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
A new full-copy Linear proof document was auto-review rejected as external disclosure. No document was created.
The existing Linear sequence successfully links the public proof. Do not retry that rejected publication.

## Current implementation and evidence

28f47 fixes scoped error retention by sharing lifecycle_retention with finalizers. Immediate fallible exits whose
errors need quiescence publish exact body/with suspension in CleanupPlan. Async exits reuse awaited retention.
Abnormal scoped steps carry the exact exit identity. Keep the no-frame invariant assertion.

Owning corrections included in 0f90849c9:
- declared_type_lifecycle counts only actual Finalizer and Destructor as whole-value cleanup. Inactive scoped
  protocols can partially move fields. Active capability restrictions remain separate. Source/import tests pass.
- LastUse publication for repeated semantic occurrences now requires retirement to hold at every reachable CFG
  occurrence. This fixes premature boolean-parameter retirement on normal scoped completion. It reuses solved states.
- Suspension cancellation cleanup retains the same checked frame as its owning await/yield/event. Completed pending
  temporaries are included after materialization. A nested synchronous cleanup no longer erases enclosing retention.
- AbnormalCleanupStep owns one shared occurrence field for all operation variants. Saved pending values now receive
  their exact cleanup context, as do scoped exits, accesses and temporaries. Three repeated setup/restore paths are gone.
- Async scoped exits also publish lexical pending-result cleanup context. Their selected call already checks execution
  availability, so asynchronous checking avoids a duplicate context diagnostic.

Passing batch: 331 checker, 38 lowering crate tests, 316 source-query, 95 source-lowering, 22 exported lifecycle,
25 scoped, 1 awaited-owner retention and 51 realization tests. The scoped group covers all 16 combinations of
sync/async entry and exit, infallible/fallible entry, panic/yield body with aligned inactive-Future errors.
Native planning is checked in each row. The dedicated awaited-owner regression emits LLVM too.
All new inline source fixtures now follow repository layout conventions. The repeated batch also passes all 93 LLVM
backend tests and the new aligned-union move regression. Rustfmt, mutating release style and release Bray format pass.
Logs use target/bra-619-shared-broadcast-*, bra-619-report-move-* and bra-619-llvm-tests.log.
No independent review of PR #614 has started.

The native fixture prepares its entry/exit results in async caller code, then synchronous entry/exit only transfer
those values. It checks failed entry, normal/yield, pending transfers, shielding, precedence, inactive captures,
aligned errors, exactly-once ordering and denied allocation. Expected totals remain unverified until execution:
entries 23, exits 17, errors 20, captures 20, finalizers 9. Do not weaken its outer assertions or swallow them in catches.

## Remaining native performance failure

The fresh release full smoke passes async-values and ordinary-finalization in both profiles, then scoped-cleanup
exhausts memory in LLVM after reaching over 36 GB. Log: target/bra-619-final-native-smoke.log. The process exited
0xc0000409 with LLVM ERROR: out of memory before the attempted bounded stop. No task-owned native process remains
from that run. No scoped execution has passed. This is a blocking compiler cost defect, not an acceptance exclusion.

A bounded isolated release backend-ir build succeeds in 81.7 seconds, peaking around 5.1 GB and emitting 1,260.61 MiB
of LLVM text for 12.65 KiB source. Raw profile counts are 9,043 MIR blocks and 5,943 MIR operations across 28 bodies,
66 concrete codegen instances and 27 codegen units. One resume function is 248 MB, 952,360 lines, with about 93,000
blocks. Several other units are similarly excessive. Find the first expansion before changing optimization policy.
Do not split or weaken fixture coverage to hide the growth, disable safety checks or rely on LLVM to erase the cost.

Additional bounded cost findings and corrections:
- Cancellation broadcast cannot suspend. Structural expansion now leaves its existing shared synchronous helper in
  place instead of repeating that traversal in retained frame states. Native planning/emission regressions pass.
- LLVM frame contexts now have their already-established instance identity. Large layouts are declared once instead
  of printing a 69 KB anonymous type at every access. Together these reduce LLVM text only from 1,260.61 to 1,219.34 MiB.
  This is not a solution to the remaining native cost failure.
- An isolated opt trace proves the first scalar replacement pass grows the largest resume from 62,494 to 1,151,009
  instructions. Aligned nested unions use byte buffers of 4,096 and 8,192 bytes, which become scalar aggregate values.
  Native object generation still exceeds the enforced 8 GB bound with four workers. Do not repeat the broad smoke yet.
- Ordinary moved aggregates no longer have their dead bytes cleared by codegen. Lowering already publishes their
  initialization/ownership guards. The separate native PanicReport empty-slot representation remains explicit through
  moved_reports and clear_moved_reports, including self-transfer and call outcome routes. Its backend tests pass.
  The new unoptimized LLVM regression covers an aligned union transfer without a dead-payload zero store.
  This correction does not by itself resolve object generation cost. The later bounded run also reached 8 GB.

Retained artifacts: target/bra-619-native-ir-product and target/bra-619-shared-native-product.
The original trace profile is target/bra-619-native-ir-profile.json. Parse its single-line JSON with Python and print
selected fields, never Get-Content it. LLVM files are huge too. Raw pre-optimization units are target/bra-619-preopt-*.ll.
The focused LLVM pass trace is target/bra-619-pass-trace.stderr.log and .ll. All temporary production tracing has been
removed. All task-owned build, test, native and opt sessions have completed. No process needs resuming.

Current uncommitted replacement of that value boundary:
- Type mappings distinguish physical storage from the value carrier. Ordinary aggregates larger than two pointer
  words use private addressable snapshots. Native signatures/layouts and protected descriptor protocols stay fixed.
- Shared typed construction, projection, copy, memory transfer and ABI handling replace large byte-buffer SSA loads.
  Joins copy into parameter storage. Inputs aliasing another destination are snapshotted before parallel assignment.
  Private edge scratches never become published carriers. Frame constructors bulk-copy indirect captures.
- Constants, nullable values, arrays, strings and constant-pattern comparison use the same typed operations.
  Tagged pattern comparison reads only the common active payload. Native report empty-slot handling stays explicit.
- Backend and 142 product-codegen integration tests pass, including native mutation/branch/loop, generic, padded tuple,
  array, nullable and constant-product snapshots through C ABI, in source and imported caller configurations.
- Expanded direct lifecycle calls now contribute their selected execution requirements to the frame's owning lane
  analysis. Its existing per-call cache is shared with helper and iteration requirements. Generic/imported completed
  owners retain both their storage and Blocking requirement. The prior generic tests exposed this concrete omission.

The release scoped reproduction now emits LLVM and native objects in 35.883 seconds with four workers, sampled at
about 1.1 GB, within the same 8 GB bound. It emits 134.78 MiB total LLVM plus object artifacts. This resolves the
immediate OOM but is not proof that general code generation costs are optimal. No optimization pass was disabled.
Log/artifacts: target/bra-619-storage-native.stderr.log and target/bra-619-storage-native-product.

Fresh full smoke passes bootstrap, foreign entry, async-values and ordinary-finalization in both profiles. Scoped
cleanup development reaches execution, then traps 0xc000001d. Retained product:
target/native/bra-505/bray-runtime-artifact-smoke-zzFb53/scoped-cleanup-development/scoped-cleanup.exe with matching PDB.
Native stack and breakpoint tracing prove OutgoingActivation requests a descriptor whose shape has not been admitted.
The descriptor identity starts 9fd730b2cd209853, size 4096, alignment 2048. claim_value's find returns zero. Its caller is
frame.resume at source scoped-cleanup:211. Do not weaken the runtime invariant or add late allocation. Trace the exact
selected producer/admission mismatch in owning realization. Logs: bra-619-storage-native-smoke.log,
bra-619-storage-native-stack.log and bra-619-storage-native-claim.log. Bounded debug helpers own and terminate their
children. No debug process remains. An LLVM reproduction using package bray.runtime.execution.smoke is being emitted
under target/bra-619-smoke-identity-ir-product to match the smoke's semantic identities.

Remaining before checkpoint: finish that native gap, verify changed cost/semantics, complete maintainability audit,
rustfmt, release mutating style and Bray format. No fresh independent review has started. Ownership below is still open.

Persistent runtime package:
C:/Users/anton/source/repos/bray/target/bra-619-runtime/x86_64-pc-windows-msvc/bray-runtime.brayrt.
Standard library: target/runtime-bootstrap-standard-library/x86_64-pc-windows-msvc. Older target/release/lib runtime
metadata is stale for this catalog. Isolated native backend-ir arguments and logs are in the previous checkpoint.
The diagnostic debug brayc is stale and is not the current release compiler. Never use debug xtask.

Source destructor body and inherited tail are already split. Do not invent a new destructor ABI or suspended Rust
frame. All earlier no-frame failures were traced to omitted source cleanup context and are fixed by the changes above.

## Remaining ownership obligation

A concrete source check accepts a retained exit error whose destructor reads a borrow of a dying local resource.
Reproducer: target/bra-619-borrowed-exit-error.bray and .log. Failure holds value:&Value and destruct() reads it.
Resource.enter borrows self.value, exit returns Error(Failure { value=lease }), and caller returns inside with.
A bare unused borrow is insufficient evidence because its dependency can end at last use. Keep the destructor read.

Known omissions: storage-flow invocation_result(ScopedExit) returns None, selected_scoped_contracts has no exit result
access, with_scoped_references prepares only the input, result/scoped_values handles entry only, and guarantee flow
represents exit value as Unknown. Trace actual result ownership and disposal destinations through existing contracts,
run/catch reports and retirement. Finalizer error results may share this boundary. No ownership fix has been added.

Do not equate owned error payloads with independent payloads, ban borrowed types wholesale, impose static-only error
rules, add a parallel report walker, assume LLVM removes invalid paths, or publish tests as proof of this omission.
No wider result-slot/outcome metadata/frame design has been implemented or agreed. Inspect existing mechanisms before
proposing one. Fix owning analysis, reuse established results downstream, and remove superseded interpretations.

## Efficient continuation and completion

Shared CARGO_TARGET_DIR: C:/Users/anton/source/repos/bray/target.
CARGO_BUILD_BUILD_DIR: same /target/cargo. BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX: /target/toolchains/llvm/active.
Tool builds use default Rust1.99. Runtime native packaging uses process-local Rust1.98.1 and TMP/TEMP=target/native/bra-505.
Run native processes outside the sandbox. Never delete shared outputs. No competing Cargo builds. Reuse retained products
and the persistent runtime package for faithful selected runs. Use rg --files before unknown paths. PowerShell wildcard
paths passed to rg are invalid. Print command output directly and bound diagnostic lines by characters.

Production changes replace repeated context handling, aggregate SSA materialization and unadmitted scoped calls.
The new production code provides aggregate storage snapshots and scoped admission through existing mechanisms. No dependency ownership repair has been implemented yet. Do not enlarge architecture to fit an unproved diagnosis.
After all concrete gaps are fixed, audit every behavior category repo-wide, run the affected batch and native acceptance,
then rustfmt, mutating release style and Bray format. Commit/push, update PR description using required Change Summary
and Related Issues only, and publish evidence in an attributed comment. Complete a fresh independent PR review cycle,
push fixes, mark ready/In Review, and remove completed stack handoffs after their durable evidence is preserved.
Do not stop at this checkpoint or claim BRA-619 complete before ownership and native obligations are resolved.

## Scoped admission repair in progress

The source lowerer never admitted the selected exit before entering the scope. Native tracing proved the descriptor
is preserved exactly from the caller through activation/acquire/claim. The missing shape is the synchronous selected
exit's 4096-byte, 2048-aligned Result, not a 16-byte Future. The earlier manual hex reading was incorrect.

The current compiled repair reuses input-temporary cleanup for the admitted duty while entry is pending. It keeps
entry activity separate from admission. Before entry, cleanup only discharges admission. After successful entry and
before the body becomes active, that same temporary invokes the guarded selected exit. Once the checked body scope
is active, its existing plan owns exit. Failed fallible entry discharges without exit. Exit clears entry activity
before invoking user code, then discharges after result disposition. Panic/cancellation uses the same shared cleanup
machine and the surviving admission guard, preventing repeat invocation/discharge. Async entry uses existing factory
admission/discharge before awaiting. Both guard storages participate in frame retention. No late allocation, new
runtime frame, source feature or second cleanup traversal is introduced. The with lowerer retires its temporary
records after emitting all of the with's paths. Verify this batch and its failure paths before claiming it proven.

## Current acceptance checkpoint after admission

The batch passes 38 lowering tests, 25 scoped tests and all 142 product-codegen integration tests. The consolidated
aggregate module also passes all 93 LLVM tests after Rustfmt, mutating release style and Bray formatting. No C file
was changed. The native fixture now also checks panic and cancellation during async entry. Its expected final entry
count is 23. Other expected totals remain unchanged and native execution has not reached the final assertions.

The fresh release full smoke again passes bootstrap, foreign entry, async-values and ordinary-finalization in both
profiles. Scoped development progresses beyond the missing admission and then traps in OutgoingDischarge. Retained
product: target/native/bra-505/bray-runtime-artifact-smoke-nL6Inc/scoped-cleanup-development/scoped-cleanup.exe.
Its matching PDB and bounded debugger traces prove one BodyLocal destructor executes twice at the same self address.
One descriptor 5c062d3ee612de44 was admitted, then discharged twice. This is duplicate destruction, not just duplicate
budget accounting. First and second destructor caller RVAs are 0x2c53c5 and 0x2cde87. Discharge callers are 0x2c5a64
and 0x2ce506. The selected exit's 4096-byte descriptor 9fd730b2cd209853 is now admitted and discharged correctly before
these. Do not remove either native invariant or add a speculative guard. Find the duplicate owning cleanup transition.
Logs: bra-619-admission-native-smoke.log, bra-619-admission-native-stack.log, bra-619-admission-native-budget.log and
bra-619-admission-native-destruction.log. All native/debug processes have finished. No process needs resuming.

Current matching-package LLVM IR: target/bra-619-admission-ir-product. It emits in 31.927s with four workers, sampled
at about 0.82 GB, 132.74 MiB IR. Normal's resume is in scoped-cleanup.21.ll, symbol
bray_resume_7d8e0dfbf42f8b43f88412f359020b6b2f805b3e8583aa6a10527ebf326b4b20. BodyLocal's source destructor is
bray_instance_5d95cd72263918012b16200db8ff2d6eb060e2cd9ec79b8fa6ef53947da16c03. It has no receiver-record argument,
so do not assume its retained receiver tail discharged twice. Native breakpoint tracing proves the body itself runs
twice. Use raw MIR/storage/liveness and the actual source cleanup plan to distinguish duplicated plans from an
uncleared temporary or local guard. The code has not yet established which of those causes the duplicate.

The retained exit-error dependency omission is still unimplemented. No new result-channel or report-dependency
metadata design has been adopted. Existing Escape effects represent externally reachable raw addresses and are not
by themselves proof of finite incident ownership. Fix the owning semantic model after tracing actual producer,
retention and disposal paths. Do not substitute a blanket static-only rule or a downstream check.

This is an incomplete recovery checkpoint. PR #614 stays draft and BRA-619 stays In Progress. No independent review
has started. Continue to fix the duplicate cleanup, the dependency obligation, native acceptance and final review.
