# BRA-619 completion acceptance

## Resume contract

Continue the authorized stack through BRA-619, including verification, maintainability audit, independent review,
pushed fixes and In Review publication. Do not stop at a question, checkpoint, restart or PR boundary. No merge or
manual Done is authorized. Delete temporary handoffs when their obligations are accepted. None survive the stack.
After compaction, actually reread AGENTS.md, coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this file.

No new language feature, syntax, authority relaxation, Rust runtime frame, generalized suspended synchronous body,
post-failure allocation or downstream semantic validation pass. Runtime ownership remains in Bray. Trust established
phase facts downstream. Work top down and remove redundant mechanisms. Correctness, compiler time, execution speed,
binary size and maintainability are independent acceptance criteria. Batch tests, then format. Use only release xtask.
Preserve the user's C formatting. This issue has not changed C files.

## Workspace and publication

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch feature/bra-619-scoped-cleanup-acceptance, based on reviewed BRA-626 d69592ca0. Published checkpoint 83cb2881c.
Draft PR #614: https://github.com/lejmer/bray/pull/614. BRA-619 remains In Progress. No independent review has started.
BRA-561 PR #611, BRA-505 PR #612 and BRA-626 PR #613 are ready. Preserve their branches and older BRA-619 recovery
6bbcb4a3. Do not replay them or restart completed reviews. Destination already verified as git@github.com:lejmer/bray.git,
PUBLIC lejmer/bray, authenticated lejmer. Reuse this evidence unless authentication or destination changes.

The Linear plan and coverage inventory preserve the sequence. Public semantic proof:
https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
Auto-review rejected copying the full proof into a new Linear document. No document was created. Do not retry.

## Connected behavior and review matrix

- Scoped cleanup keeps its exact selected invocation, lexical pending transfer, execution requirements and frame
  retention through normal completion, return, yield, break, continue, propagation, panic, cancellation and failed entry.
  The 16-way sync/async and fallible entry/exit matrix requires native planning. Actual destructor/finalizer hooks alone
  require whole-value cleanup. Inactive scoped protocols can partially move represented fields.
- Repeated semantic occurrences retire a dependency only when every reachable occurrence retires it. Scope cleanup,
  temporaries, discarded owners, finalizer errors and suspension cleanup use the established owning facts.
- Large ordinary LLVM aggregates use private addressable snapshots. Storage layout and native ABI remain fixed.
  Construction, projection, constants, arrays, nullable values, patterns, copies, raw/protected memory, branch joins,
  loop swaps, imported callers and ABI transfers use shared typed operations. Joins snapshot overlapping sources.
  Protected String, PanicReport and flexible representations keep their established value protocols. Ordinary moved
  bytes are not zeroed. Native PanicReport empty-slot ownership stays explicit.
- Expanded lifecycle calls contribute selected execution requirements through the existing realization cache.
  Cancellation broadcast retains its shared synchronous helper. Named frame context types avoid repeated huge layouts.
- Selected exit admission precedes entry. Admitted and entered guards are distinct. Before entry success, temporary
  cleanup only discharges the reservation. Successful entry publishes activity before pattern binding. Checked body
  cleanup takes over once its scope is active. Async entry uses existing factory admission. Exit clears activity before
  user code and discharges after its result disposition. Failure paths discharge exactly once without late allocation.
- Source structural cleanup must use Lowerer::push_operation, which owns initialization and move transitions.
  CleanupOutcome::resolve bypassed it and left BodyLocal initialized, causing duplicate native destruction. Delete that
  forwarding path. Source lifecycle/capture cleanup uses its operation sink followed by shared outcome.check. Synthetic
  cleanup uses its existing operation sink and the same outcome handling. No parallel ownership implementation.
- Result-region completion is published by storage analysis for AnalysisOperationKind::Result(Completion). The obsolete
  Bound(ScopedUse) special case omitted real with completion. Lowering then marked a reachable join Unreachable.
  Publish the existing graph's completion, remove the obsolete interpretation and trust the table downstream.

## Current verification

Current retirement/completion batch passes 38 lowering, 27 scoped, 316 source-query and 171 product-codegen tests.
The new scoped regressions check guard retirement before cleanup and source continuation after sync/async with joins,
including fallible entry. Earlier unchanged LLVM coverage passes all 93 tests, including aligned union bulk transfers,
padded tuple storage and native source/imported snapshot mutation, branch, loop, generic, array, nullable and pattern cases.
Rustfmt, mutating release style and release Bray format pass. Release tooling rebuild completed in 1m57s.
Logs use target/bra-619-retirement-* and bra-619-completion-*.

The previous fresh full smoke passes bootstrap, foreign entry, async-values and ordinary-finalization in both profiles.
Retained scoped product L5EJSv gets beyond duplicate destruction but returns cancellation during the first normal case.
Bounded native tracing proves prepared_entry completes and normal then cancels, without source panic construction.
Current source MIR proves its with result join bb577 was marked Unreachable. The owning completion repair above has
not yet been verified natively. Retained evidence: bra-619-retirement-normal.mir and native-progress/root/counters logs.
The old duplicate trap is nL6Inc, logs bra-619-admission-native-*. Do not re-debug that solved cause.

Fixture acceptance still requires the exact final totals: entries 23, exits 17, errors 20, captures 20, finalizers 9,
no inactive body entry, correct exit-before-local ordering, no allocation after admission and exactly two stderr lines
for the final cancelled boundary's incident. Do not weaken assertions or hide failure inside a catch.

## Native cost evidence

The old native scoped build exhausted over 36 GB. Optimized LLVM was 1,260.61 MiB for 12.65 KiB source. Its first scalar
replacement pass expanded the largest resume from 62,494 to 1,151,009 instructions by scalarizing aligned union buffers.
The storage/value repair emits LLVM and objects in 35.883s with four workers, sampled near 1.1 GB under the same 8 GB
limit, 134.78 MiB combined artifacts. No optimizer pass was disabled. This resolves the immediate OOM, not every general
codegen cost question. Latest matching-package IR is target/bra-619-retirement-ir-product, 132.88 MiB for 13.50 KiB source,
9,943 MIR blocks and 7,099 operations, before the completion publication repair. Do not repeat unchanged broad builds.

## Remaining dependency ownership obligation

A concrete source check accepts a retained exit error whose destructor reads a borrow of a dying local resource.
Reproducer: target/bra-619-borrowed-exit-error.bray and .log. Failure holds value:&Value and destruct() reads it.
Resource.enter borrows self.value, exit returns Error(Failure { value=lease }), and caller returns inside with.
An unused borrow is insufficient evidence because dependency can end at last use. Preserve the destructor read.

Known omissions: storage-flow invocation_result(ScopedExit) returns None, selected_scoped_contracts has no exit result
access, with_scoped_references prepares only the input, result/scoped_values handles entry only, and guarantee flow
represents exit value as Unknown. ScopedCapability storage holds only the successful capability, not the full fallible
entry result. Correct result/payload identity and disposal destination must agree across normal values and incidents.

Trace producer, retention, catch/run ownership, imported summaries and retirement before implementing the connected
repair. DependencyStorageEffect::Escape represents external raw-address reachability. It does not establish finite
incident ownership. No new result-channel, report-dependency metadata or frame design has been adopted. Fix the owning
analysis, reuse established results downstream and remove superseded interpretations. Do not impose static-only errors,
ban borrowed errors wholesale, equate owned with independent, add a parallel report walker or downstream validator.
Source destructor body and inherited tail already exist. Do not invent another destructor ABI or runtime frame.

## Execution and immediate next action

Shared CARGO_TARGET_DIR=C:/Users/anton/source/repos/bray/target, CARGO_BUILD_BUILD_DIR=the same /target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use /target/toolchains/llvm/active. Tool builds use default Rust 1.99.
Native runtime packaging uses process-local Rust 1.98.1-x86_64-pc-windows-msvc and TMP/TEMP=/target/native/bra-505.
Native LLVM/linker/executable workflows run outside the sandbox. Worktree edits and Git writes also need escalation.
Never clean shared storage. Use rg --files before unknown paths. PowerShell does not expand wildcard rg paths.
Read bounded logs. Large LLVM files and profile JSON can have huge single lines. Parse and print selected facts.
Task-owned debug helpers close handles and terminate their child on bounded exit. No debuggee remains.

Completion native smoke EcWNsS passes the initial normal scenarios, then encounters denied allocation on the second
transfer's scope admission. The fixture incorrectly enabled denial before with admission. Admission may allocate before
establishing its obligation. Both transfer fixtures now enable denial inside the entered body after constructing locals.
Their zero-allocation assertion and cleanup coverage stay unchanged. Rebuild release xtask to embed the corrected fixture,
then repeat native acceptance. Then checkpoint/push the small retirement/completion repair, resolve the
retained dependency obligation, finish acceptance and audit, push, independently review PR #614 and push review fixes.

Native Xh3zAJ passes all six denied-allocation return/break/continue transfers and nullable propagation. The next assertion
expected propagate_error() to return Error, but its try targets the nearest compatible with result, which the fixture
discarded before returning Ok. The specification explicitly selects the nearest compatible single-yield boundary.
The fixture now returns the with result and keeps the payload assertion. Rebuild release xtask before native acceptance.

Code tracing also confirms CleanupOutcome::dispatch sends Incidents to normal continuation without transferring its
owned report. TransferCleanupIncident stores the owned chain only in that collection's report place. No implicit runtime
handoff is visible in that path. Verify the complete native counts after correcting the fixture before selecting the
connected outcome/dependency repair. Reuse existing native report and frame-progress ownership, not a new Rust owner.
