# BRA-619 completion acceptance

## Resume contract

Continue the authorized stack through BRA-619, verification, maintainability audit, independent PR review and pushed fixes.
Do not stop at status answers, restarts, checkpoints or PR boundaries. No merge or manual Done is authorized. Delete this
temporary record when accepted. No handoff survives the stack. After compaction actually reread AGENTS.md,
coding-conventions.md, crates.md, BRA-505-retained-incidents.md and this file.

No language feature, syntax, weaker authority, Rust runtime frame, TLS report owner, deferred destruction queue,
generalized suspended synchronous body, post-failure allocation or downstream semantic validation pass. Runtime ownership
stays in Bray. Work top down, reuse owning results and remove superseded mechanisms. Correctness, compilation speed,
execution speed, binary size and maintainability are separate acceptance criteria. Batch tests, then format. Use only
release xtask. Preserve the user's C formatting. This batch has changed no C or Bray files.

## Workspace and publication

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch feature/bra-619-scoped-cleanup-acceptance, base reviewed BRA-626 d69592ca0.
Latest pushed checkpoint f5be6ce4def6fba88e3fc8ede34af1fb66c0b6e8, following f12e6e62b and c329313fd.
Draft PR https://github.com/lejmer/bray/pull/614. BRA-619 remains In Progress. No independent review has started.
BRA-561 PR611, BRA-505 PR612 and BRA-626 PR613 are ready. Preserve their branches and old BRA-619 recovery6bbcb4a3.
Do not replay completed work or restart old reviews. Destination verified git@github.com:lejmer/bray.git, PUBLIC
lejmer/bray, authenticated lejmer. Reuse this evidence unless destination or authentication changes.

The tracked Linear rule plan and coverage inventory retain the initiative order. Earlier semantic proof:
https://github.com/lejmer/bray/blob/d69592ca0/docs/handoff/runtime-stack-semantic-proof.md.
Auto-review rejected copying the full proof to a new Linear document. No document was created. Do not retry.

## Established implementation

- Selected enter/exit invocation, execution requirements, admission, frame retention and pending transfers are preserved.
  Exit admission precedes entry. Separate admitted and entered guards preserve failed-entry ownership.
- Repeated semantic occurrences retire dependencies only when every reachable occurrence retires them. Actual lifecycle
  hooks require whole-value cleanup. Inactive scoped protocols can partially move their represented fields.
- Structural source cleanup uses Lowerer::push_operation before the shared outcome check. The deleted resolve wrapper
  bypassed initialization retirement and caused duplicate destruction. Result(Completion), rather than Bound(ScopedUse),
  owns successful with completion. The old interpretation made valid tails unreachable.
- Large native aggregates use private addressable snapshots without changing ABI/layout or clearing ordinary moved bytes.
  This fixes SROA expansion. Native report empty-slot ownership remains explicit.
- Source and catch report collections use StorageFlow demand and existing CleanupOutcome. Allocate and initialize Normal
  plus EmptyPanicReport without entering a shield. Capture normal call/await reports before another call resets context.
  Local cleanup forwards Incidents. Catch success and nonlocal transfers forward leaving collections to their survivor.
  Panic/cancellation adopt the previous owner. Normal Return/Completed publishes it. Frame retention includes the physical
  slots, and inactive/stored-result callbacks cannot read body collections whose initialization never ran.
- Report merges thread ordinary SSA values through MirEdge arguments using forward_cleanup_edge. No runtime value temp.
  The cleanup outcome root is thin, with model/state/reports/calls/finalization submodules.
- MIR Return.report, CheckCallOutcome.completed_report, TakeCallReport and Completed.report carry owned reports. Every
  walker, codec, mapping and native consumer handles them. The existing checked-call inventory is reused.
- native_run_outcome_value takes separate optional integer result and report fields. All seven producers agree. No ABI
  or schema version bump. Version1 remains the only version.
- One output selector on DependencyRequirement::ResultCall distinguishes Value, CompletionReport and AbnormalReport.
  Existing substitution, interface metadata, witness fixed points, relevance, remapping and structural identity preserve
  it. Report requirements use the existing storage/liveness/retirement analyses, not a second solver or validation pass.
- Direct await snapshots the consumed Future's dependencies. Factory sites are not frame identities. Default-provider
  evaluation reports are separate from lazy body execution. Generated lifecycle helpers forward normal reports too.
- Suppression handles earlier headless incidents without losing a later primary panic.

## Current uncommitted repair and proof

Output-specific parameter relevance now follows the selected output through the existing graph and memoization keys.
Report inference uses the same deferred_result path as ordinary values during recursive body inference. The direct
report lookup had repeatedly expanded the preceding approximation. Both previously cancelled recursive tests now pass.
Predicate outputs use their established effect-free contract. Treating predicates as arbitrary report-producing callbacks
was invalid and caused the production lock and scoped authority failures.

An unconstrained callback can return normally while retaining a scoped exit Incident that borrows its argument, even
when Incident is absent from the callback's explicit Result type. The new concrete source regression proves the producer
has nonempty normal report dependencies, accepts caller-owned input and rejects a local borrow. It covers both an opaque
callback parameter and a known immutable function value. The former positive fixture calling an arbitrary callback with
&local was therefore not universally safe. It now tests caller-owned input. Do not restore a report-free default for
opaque callbacks or weaken their authority. The dependency spec now explains this existing ownership rule.

Known immutable callable values need their producer's exact contract. Named reference selection already publishes a
CallableReference with its exact instance. CheckedSemanticSelections now consumes that identity when constructing the
final table, and records it on an indirect call without changing runtime dispatch. Mutable aliases are not resolved to
an initial producer. Source-component discovery follows this established identity. The earlier candidate-time
GenericDeclaration/CallableSignature queries were redundant and have been removed. Binding subjects and immutable
initializers have one BoundTree implementation. The superseded checker pattern/source module is deleted.

A source/imported regression calls the same known mutable callback twice and requires no normal report collection.
The imported fixture now uses the existing dependency_contracts API. The current debug test build includes this fixture.

## Verification and active native finding

The current checkpoint passes 320 source-query, 143 code-generation, 332 checker and 38 lowering tests.
The connected full compiler run passed 1850 tests and exposed ten test assumptions. Focused reruns pass after correcting
cleanup destruction expectations, accepting the required report-suppression runtime call, and disambiguating a fixture's
retain function from the production runtime declaration. The outgoing-capacity expectations remain one and zero.
The existing 64/128/256-path inference scale test passes but takes 609.87s in debug. Obtain release cost evidence before
acceptance. Logs use target/bra-619-checkpoint-*.log, bra-619-connected-compilation-tests.log and bra-619-empty-scale.log.

Destructor body handoff now accepts an owned normal report. Return validation still checks its operand. Native fixed-result
callbacks consume their implicit reports through the existing report consumer. Bray calls publish them to the caller.
Native inferred contracts exclude those consumed implicit reports while preserving ordinary value dependencies and reports
from caller-evaluated defaults. Regression coverage checks both boundaries.

StorageFlow retains abnormal report lifetime dependencies without demanding a normal pending collection. Joins and catch
forwarding preserve both properties. Completed and abnormal candidate snapshots retire together after selection. Empty
caught values use the existing implicit-empty assignment representation. No-op StaticFinalize no longer allocates an empty
pending collection. Reporting synchronous finalizers initialize that collection before invocation.

The fresh standard-library build passes. Native smoke passes bootstrap, foreign entry, async-values and ordinary-finalization
in both profiles, then fails the scoped cleanup total: entries23, exits17, errors18, captures18, finalizers9. A bounded destructor trace shows two retained report payloads are destroyed only after the final assertion fails.
The explicit result error is destroyed during its catch cleanup. Investigate discarded catch report lifetimes, including
nullable and result propagation, before changing suspension or finalizer lowering.
Do not weaken the required totals of20. Retained product:
target/native/bra-505/bray-runtime-artifact-smoke-bj4T13/scoped-cleanup-development/scoped-cleanup.exe.
The release binary predates the latest candidate-state retirement and native Bray-ABI assertion. Rebuild before acceptance.

Actual production-source MIR and lifecycle expansion are retained in target/bra-619-{current,expanded}-*-compact.mir.
The local error is slot65, initialized with guard66. Its finalizer abandonment flag is selected after quiescence resumes,
so it does not cross the newly expanded suspension. No flag-spilling change is justified by this observation. Trace the
actual guarded owner and report chain. No temporary inspection test or native debuggee remains. No review has started.

## Remaining acceptance and architectural checks

1. Resolve the remaining result-propagation ownership failure and finish native acceptance. Required scoped totals are entries23, exits17, errors20,
   captures20 and finalizers9, exit-before-local ordering, no inactive body entry, no allocation after admission and exactly
   two final stderr incident lines. Never weaken the fixture. propagate_error returns its with result because try targets
   that nearest compatible result boundary.
2. Complete the operation coverage audit. Source calls, scoped calls, direct awaits, parameter defaults and lifecycle
   calls are connected. A concrete gap remains: dependency/check.rs skips SemanticSelection::Operation, and
   storage_flow/check/analysis/reports.rs says these operations invoke no source callable. That is false for selected
   operators, conversions, custom indexing and type-form construction. behavior::collect_operation_behavior already owns
   the invocation/default inventory. Use that existing owner and actual input mappings. Do not add a second call walker,
   treat missing handling as no effect, or hide missing semantics with a broad fallback. Include field defaults and
   temporary/discarded/replacement owners in the composition matrix.
3. Report presence is distinct from dependency emptiness. None versus Some([]) currently distinguishes absence from an
   independent report, but resolving a symbolic report call to an absent provider loses this distinction. A generic
   report-free specialization can acquire needless pending storage/joins and an inlining barrier. Close the owning
   summary/presence boundary before claiming cost acceptance. Do not treat [] as absence, drop independent reports,
   infer absence from total, or rely on LLVM to remove avoidable work. Use established results during specialization.
4. Verify imported reporting contracts, recursive/generic/opaque composition, returned callable/Future values and the
   total-destructor outgoing-capacity shortcut. Total alone excludes abnormal return, not normal incidents. Outgoing
   admission concerns callback error/panic payloads, while already-owned normal reports have their own admitted backing.
   Check that distinction against the actual producer before changing the approved zero-capacity rule.
5. Complete mechanical maintainability/DRY audit, production/test growth accounting and formatting. Release mutating
   style, Rustfmt and Bray format are mandatory after tests pass. Update plan evidence, checkpoint/push, then dispatch a
   fresh independent review on pushed PR614. The reviewer must post actual summary/inline artifacts. Resolve findings,
   push fixes and re-review until clean. Only then ready/In Review. No Done or merge. Remove stale handoffs before acceptance.

## Prior native evidence and costs

At c329 fresh product vL9bks passed entries23 and exits17 but errors/captures were12 instead of20. The eight missing
incidents matched six nonlocal transfers plus nullable/result propagation. Source pending report ownership repairs that
missing transfer. This older product does not validate the new source report implementation. All earlier bootstrap,
foreign entry, async-values and ordinary-finalization smokes passed in both profiles.
Retained failure: target/native/bra-505/bray-runtime-artifact-smoke-vL9bks/scoped-cleanup-development/scoped-cleanup.exe.
Logs bra-619-pending-native-{smoke,panic,counts}.log. Counter offsets .data RVA0x5e2000: entries+2712, exits+2720,
errors+2704, captures+2672, finalizers+2688. Older nL6Inc duplicate destruction, L5EJSv unreachable join, EcWNsS pre-admission
denial and Xh3zAJ discarded try result have established causes/fixes. Do not re-debug them.

Old object generation exceeded36GB after SROA expanded a resume from62,494 to1,151,009 instructions. The aggregate repair
emits LLVM plus objects in35.883s with four workers, sampled near1.1GB under8GB,134.78MiB combined. No optimizer disabled.
Matching-package IR target/bra-619-retirement-ir-product was132.88MiB,9,943blocks and7,099operations before completion
publication repair. Reuse evidence unless changed inputs justify another measurement.

## Execution and efficiency

Shared CARGO_TARGET_DIR=C:/Users/anton/source/repos/bray/target, CARGO_BUILD_BUILD_DIR=that target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Compiler tools use default Rust1.99.
Native provider packaging uses process-local RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc and TMP/TEMP=target/native/bra-505.
Native LLVM, linker and executable workflows run outside the sandbox. Worktree writes and Git need escalation. Never clean
shared storage. Drive C had41.7GB free before the current builds. No task-owned debuggee remains.

Use rg --files before unfamiliar paths. Never pass wildcard paths to rg in PowerShell. Do not guess file paths. Keep tool
output bounded, and parse large logs for selected facts. Python always uses explicit UTF-8. Stage codemods before writing.
After Python check LASTEXITCODE before Cargo. Ignored applied scripts include target/bra-619-output-relevance.py,
thread-report-results.py, callable-source.py and reuse-callable-references.py. Do not rerun applied scripts.
