# BRA-619 completion acceptance

## Objective and baseline

Continue the authorized stack through BRA-619, including pushed checkpoints, independent PR review and fixes.
No merge or manual Done is authorized. No new language feature, syntax, Rust runtime frame, generalized
suspended synchronous body, authority relaxation, post-failure allocation or downstream validation pass.
Use only release xtask. Preserve the user's C formatting. Batch verification before mandatory formatting.
Read AGENTS.md, coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this record after compaction.

Worktree: C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch: feature/bra-619-scoped-cleanup-acceptance, based on d69592ca0, reviewed BRA-626 PR #613.
The branch HEAD carries the corrections below. Previous published checkpoint: 28f47c2e8, draft PR #614. BRA-561 PR #611, BRA-505 PR #612 and BRA-626 PR #613 are ready.
Preserve the older BRA-619 recovery branch at 6bbcb4a3. Do not replay it or restart completed reviews.

The tracked Linear rule plan, coverage inventory and sequence preserve the stack plan. The already-public proof
is pinned at https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
A new full-copy Linear proof document was auto-review rejected as external disclosure. No document was created.
The existing Linear sequence successfully links the public proof. Do not retry that rejected publication.

## Current implementation and evidence

28f47 fixes scoped error retention by sharing lifecycle_retention with finalizers. Immediate fallible exits whose
errors need quiescence publish exact body/with suspension in CleanupPlan. Async exits reuse awaited retention.
Abnormal scoped steps carry the exact exit identity. Keep the no-frame invariant assertion.

Uncommitted owning corrections:
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
entries 21, exits 17, errors 20, captures 20, finalizers 9. Do not weaken its outer assertions or swallow them in catches.

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

Continue at the LLVM value-representation boundary. Union construction, field projection, parameter passing and
returns currently materialize byte-buffer aggregates as SSA values. Derive one addressable representation that preserves
snapshots, aliases, consuming moves, block-parameter joins, imported and native ABI boundaries, and frame lifetimes.
Use actual MIR storage and bulk transfers instead of depending on SROA to repair aggregate copies. Add no language
feature, general framework, runtime frame or optimizer-disable workaround. Keep this bounded and justify production
code growth separately from tests. The scoped fixture and all of its acceptance remain required.

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

Production changes so far replace repeated context handling and preserve established retention. Tests and the fixture
account for most growth. No dependency ownership repair has been implemented yet. Do not enlarge architecture to fit an unproved diagnosis.
After all concrete gaps are fixed, audit every behavior category repo-wide, run the affected batch and native acceptance,
then rustfmt, mutating release style and Bray format. Commit/push, update PR description using required Change Summary
and Related Issues only, and publish evidence in an attributed comment. Complete a fresh independent PR review cycle,
push fixes, mark ready/In Review, and remove completed stack handoffs after their durable evidence is preserved.
Do not stop at this checkpoint or claim BRA-619 complete before ownership and native obligations are resolved.
