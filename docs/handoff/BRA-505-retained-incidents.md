# BRA-505 ordinary finalization and retained incidents

## Resume after every context compaction

The user requires an actual reread of AGENTS.md, docs/contributing/coding-conventions.md,
docs/contributing/crates.md and this task record after every compaction or restart.
A summary is not a substitute.

Continue BRA-505, then BRA-626, then BRA-619. Questions, corrections and checkpoints do not end the task.
No merge is authorized. Commit and push checkpoints are authorized. Delete this temporary
handoff once its obligations are accepted. No handoff survives the stack.

Use only release-built xtask, including style, formatting, profiling and native acceptance.
The user explicitly prohibits debug-built xtask. Keep the repository default release alias.
Never override CARGO_ALIAS_XTASK. Debug-built unit-test binaries remain ordinary Cargo tests.

Runtime ownership, activation, waiting, retirement and lifecycle policy belong in Bray.
Do not introduce or extend a Rust frame, waiter owner, scheduler or hidden blocking root.
Existing Rust runtime ownership is a migration target. Add no language feature, syntax,
compatibility layer, ABI bump, post-failure allocation or downstream semantic reproof.
Preserve the user's C Allman formatting, including their independent fixture corrections.

Correctness, compiler time, generated execution, binary size and whole-system maintenance
are independent acceptance criteria. Do not rely on LLVM to remove avoidable work.
Work top down, implement the connected producer/consumer boundary, then batch acceptance.
State what each mechanism replaces. Keep one owning analysis and reuse its checked results.

## Workspace and baseline

Only edit C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray.
Branch: feature/bra-505-admitted-ordinary-finalization. Draft PR #612.
Base: feature/bra-561-compiler-described-activation, reviewed PR #611 at 7a254388.
BRA-619 recovery: 6bbcb4a3 on feature/bra-619-complete-selected-scope-native-cleanup.
Preserve all branches and the user's main checkout. Do not replay older stack work.

Current pushed recovery checkpoint: 2463efb37. The subsequent body/tail implementation and
audit fixes are UNCOMMITTED. Independent review 5560583230 preceded active human architectural
review. Do not launch another review agent unless asked. PR #612 stays draft and BRA-505
stays in progress until connected native acceptance and the remaining boundary audit pass.

Push destination already verified: git@github.com:lejmer/bray.git, PUBLIC lejmer/bray,
authenticated lejmer. Reuse this evidence unless destination or authentication changes.
Worktree writes, Git mutations and native workflows require escalation. Main checkout is
shared build storage only. Never clean or remove shared artifacts.

## Connected implementation contract

1. Checking selects lifecycle actions and their suspension/execution requirements, including
   child work in a finalizer's error. Quiescence abandons graceful finalization while retaining
   initialized payload owners. Source, temporary, replacement and imported consumers agree.
2. Lowering expands selected structural operations into the owning continuation, sharing its
   cleanup collection. Arrays use a counter. Pending values/projections live in enclosing
   storage. Specialize before expansion and layout. Do not rebuild a unit per operation.
3. The existing Bray activation driver composes asynchronous children into the same run.
   Ordinary composition consumes the child. Quiescence detaches a terminal retained owner
   into its original slot, without entering its inactive body or destroying captures.
4. Attachment transfers admitted error backing after child work resolves. Later synchronous
   abandoned destruction consumes payload, captures, backing and provider dependencies once.
   Panic/cancellation retain collection order and exact execution/attachment requirements.

Replace synchronous structural-helper assumptions and consuming retained quiescence. Reuse
selected expansion, await emission, activation ownership and the existing report chain.
No reservation or activation per organizational helper. Recursive/erased/raw-buffer family
migration is BRA-506. Started tasks are BRA-560, waiter migration BRA-562, generators BRA-484.
Do not silently use consuming TaskObservation or blocking resolve_task as non-consuming wait.

## Implemented body/tail boundary, not yet fully accepted

Initialization bits alone cannot distinguish a pending initialized field from one whose
finalizer completed. The implementation therefore retains the exact checked MIR continuation,
including guards, completion choices, cleanup outcomes and source order. It adds no ownership
history or source contract. Whole destructor body still precedes remaining represented parts.

- One synchronous machine body executes source operations and ordinary local cleanup. At
  implicit receiver ownership end it publishes a tail entry and returns. Explicit source
  operations and nested callables keep their original semantics.
- A caller-owned stack/frame record carries selector, crossing guard/outcome/SSA storage and
  source storage retained through existing CheckedDependencyContracts lifecycle-consumer facts.
  The body borrows the actual consumed receiver and record. No callee-stack pointer escapes.
- MirCleanupContinuation carries handoff, ordinary/abandoned roots and initial reachability.
  Source and imported templates share this descriptor. Native realization prunes external roots
  through synchronous_body(). There is one body per specialization, no copy at cleanup sites.
- The selected graph is appended once per disposition, reconnecting later handoffs directly and
  dropping their redundant selector stores. One reconstruction retires replaced local slots.
  Existing MIR walkers and ID/place remapping handle capture transport and tail substitution.
- MirDestructionDisposition distinguishes Ordinary, Abandoned and Dynamic before realization.
  Known choices have no runtime bool argument or unused opposite branch. Dynamic remains for
  actual erased generator/raw-buffer callbacks. Incident transfer selects Abandoned.
- Demand, helper identity/hash, signatures, LLVM arguments, imports and incident adapters carry
  that same choice. Source destructor bodies no longer receive the old abandonment bool.
- CompilationSyntheticLoweringContext caches DestructionBody once per exact specialization
  through the existing FactCell engine, including provenance, cancellation and profiling.
- Caller dispatch uses existing BeginCleanup/ContinueCleanup when crossing from Ordinary to
  LifecycleResolution. Its zero-selector exit stays in the established phase. Invalid selectors
  are unreachable. Tail reports join the existing CleanupOutcome failure policy.
- The audit shares layout and SSA-result allocation between scalar and cleanup MIR splicing.
  Caller record preparation and tail dispatch are separate responsibilities. Selector projections
  come from the checked descriptor, not a second hardcoded tuple-field interpretation. Existing
  MIR validation owns the static path shape, including imported templates.

Review generic mapping for every actual tail operation, owned-indirection release and imported
protocol calls. The shared source body must resolve locals, preserve cleanup shielding and
transport abnormal-return outcomes before the selected receiver tail resumes. Keep no alternate
receiver-state interpretation or runtime descriptor-graph reconstruction.

## Retained activation and native boundary

MirFrameComposition distinguishes Completion, Abandon and Quiesce. The Bray header retains
that disposition, affinity and lanes across failures. The registry has a Retained state that
allows inspection and rejects reactivation. Later destruction uses activation.inactive and
existing callback containment, admission and report ownership. No new Rust frame was added.

BroadcastInactiveFrame and DestroyInactiveFrame identify their actual owner. Their walkers,
mappings, imports, inspection and LLVM consumers agree. Source selection has LifecycleAction::Future.
NativeRun masks composition authority during synchronous nested actions and still rejects
recursive resumable entry. Synchronous callbacks publish no task continuation.

The actual LLVM caller is translation/unit/effect/task.rs::cleanup_inactive_frame. It creates
an initialized token temporary and independent checked-call outcome. Broadcast borrows the
owner. Retained destruction consumes it through the checked MIR move. Registry epoch,
descriptor and inactive/retained phase are validated before selecting a header. Acquisition
executes no callback. Zero lookup aborts before dereference. The local cursor issuer gets only
the validated address under exclusive acquisition. Its requirement is the exact token/outcome
pair. Production negative cases reject absent authority and either differently paired slot.
Keep retained_address's exclusive-owner requirement, derived from entry, never lease fields.

Reports use one low-level report_records_append chain linker. suppress_report and
append_report_records assemble their ordered chain and call with_report_records for field
installation and consumer selection. The report remains unmodified until that boundary,
preserving native_report_callbacks. Fresh release runtime compilation accepts this repair.

## Remaining connected work

- Complete the execution-root and retained-provider audit before native acceptance claims.
  Native synchronous readiness currently reuses an actual task lane, otherwise grants no
  capabilities, admits movable capability-free actions, and checks exact attachment identity
  for origin affinity. This is correct for foreign entry but does not implement genuine root
  capabilities. entry/execution.bray owns root/foreign dispatch and attachment acquisition.
- Map root capability and retirement across every normal, failure and nested return. The sync
  executable root grants Blocking, Compute and Main. Async root grants Main only. A dedicated
  std.thread root grants Blocking and Compute. Foreign entry grants none by default. IDs are
  observational. Do not infer authority from OS-thread presence or an always-ready callback.
- Prove actual backing/provider/attachment ownership through retained reports, including reports
  returned beyond the entry scope and static cleanup. A live byte buffer or producer comment
  alone does not prove retention. host.attachment.Owner currently owns acquisition depth,
  finish can retire its row, and report.incident.Record has only a storage.Lease. Trace the
  actual native caller before choosing a representation. Use existing Bray owners, not a new
  Rust scope/frame. Do not weaken opaque whole-owner witnesses or rely on field privacy.
- Verify non-source TaskObservation descriptors quiesce and destroy exactly once. Preserve
  explicit migration boundaries. Complete all registry and native return-path ownership proofs.
- Run full native acceptance with fresh release xtask and both corrected invalid-exit checks.
  Include retained Future captures, aligned errors, denied allocation, no inactive body entry,
  exact capture destruction and no hidden blocking or extra runtime owner.
- Complete connected native acceptance, then finish the mechanical AGENTS audit. Basic MIR
  now owns synchronous-body preparation. Full optimization reuses Basic by its original key.
- Push checkpoints and fixes, keep PR/Linear accurate, finish the human review, then implement BRA-626 before completing BRA-619.
  Keep scoped_selected_calls_emit_native_artifacts required across both issues.

## Verification and costs, 2026-10-18

Before this body/tail batch, 331 checker, 111 trusted-contract source and 87 source-lowering
cases passed. Checkpoint 2463efb37 contains the callback-result and channel repairs.

The body/tail batch passes 45 MIR, 38 lowering, 191 interface, 25 source lifecycle,
10 destruction composition, imported generic receiver tails and all nine native lifecycle
emission cases, including scoped_selected_calls_emit_native_artifacts. The bool-argument
assertion now distinguishes actual Dynamic disposition from the known choices.
Logs remain under target/bra-505-handoff-*.log and target/bra-505-owning-boundary-tests.log.

Fresh release tooling plans the Windows standard library in 18.6s, within the user's normal
17-20s range. Total build is 32.2s. Report:
profiles/bra-505-release-stdlib/std-library-x86_64-pc-windows-msvc-standard-library-build.json.
Never use the debug xtask. Its earlier 103.6s/125.6s runs do not establish a release regression.
Always select --target x86_64-pc-windows-msvc here.

Release native acceptance passes bootstrap C smoke, foreign entry and async-values in both
profiles. It then exposed the ordinary-finalization fixture's invalid Future<unit>?> spelling
and initializer-evidence defect. Both C invalid-exit checks use 6 because 4 is valid Quiesce.
Preserve the user's formatting. Retained prior product:
C:/Users/anton/source/repos/bray/target/native/bra-505/bray-runtime-artifact-smoke-R6raB8.

## Current initializer repair and prerequisite

The corrected ordinary-finalization fixture imports std.memory and uses inferred replace_value
with distinct stored and pending bindings. Its explicitly prepared nullable owner avoids the
independent direct-construction conversion defect in BRA-626.

BRA-626 blocks BRA-619 in Linear. Complete BRA-505, then BRA-626, then BRA-619. BRA-626's
acceptance requires a repo-wide audit and removal of construction-conversion workarounds,
including fail() in xtask/fixtures/native-execution/ordinary-finalization.bray. Keep only
intermediates with independent ownership, ordering or witness purposes. Cover all conversion
consumers and dependency projections without new syntax. Restore direct construction in the
native fixture while preserving allocation denial and retained-capture assertions.

The initializer repair separates nullable destination expectations from actual initializer
evidence. Identity-only destinations and inferred aliases retain equality inference. Resolve
declaration evidence once per base component, classify nullable conversions, close the remaining
aliases once and reuse resolved types. This removes intrinsic-source guesses and repeated
per-initializer scans. The existing constraints owner supplies contextual nullable-present
numeric/aggregate types and contextual union-variant evidence. Callable argument selection
no longer owns a separate contextual-variant evidence path.

A fully directional first draft passed narrow cases but failed standard-library planning and
three broader source queries. Preserve those failures as evidence, not as accepted exclusions.
The narrowed identity/nullable distinction now passes all 312 source-query tests and 331
checker tests. Logs: target/bra-505-initializer-query-tests.log and
target/bra-505-contextual-checker-tests.log. The contextual variant regression covers defaults,
local constructors, payload calls, nullable construction and ordinary call arguments.

Earlier directional checks passed 32 returned-value and 111 trusted-contract cases. The initial
latest authority filter accidentally selected zero tests and supplies no evidence. The corrected
compilation::trusted_contracts batch selects 257 tests, including production host fixtures,
and is RUNNING in session64120. Keep its cost separate from the earlier 111-case subset.

The release rebuild completed in 3m47s. Rustfmt, release cargo xtask style and cargo xtask format
pass. Logs use target/bra-505-contextual-*.log. Fresh native acceptance is RUNNING in session95216,
log target/bra-505-contextual-native-smoke.log. Standard-library planning now passes in 30.8s
while the production host authority suite runs concurrently. This is not an isolated cost sample.
The prior native run at target/bra-505-directional-native-smoke.log failed contextual variants,
channel authority and thread borrows before this narrowed repair. Do not treat it as current proof.

The lifetime audit maps production root and foreign report consumption inside their entry
callbacks before return. See codegen effect/synchronous.rs and callback/foreign.rs. Product
cleanup consumes finalizer incidents before receiver destruction and thread-static detach,
under with_static_cleanup_context. Linked generated callbacks have process-image lifetime.
Finish exact origin/root and imported/native-provider acceptance. Do not add an attachment
owner simply because Record stores only its backing lease.

## Efficient execution

Shared CARGO_TARGET_DIR: C:/Users/anton/source/repos/bray/target.
CARGO_BUILD_BUILD_DIR: C:/Users/anton/source/repos/bray/target/cargo.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX: target/toolchains/llvm/active in that main checkout.
Never delete shared outputs or task logs. No parallel Cargo builds competing for this target.

Compile compiler tooling with default Rust1.99. For native provider work, invoke the fresh
release xtask executable with process-local RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc and
TMP/TEMP=C:/Users/anton/source/repos/bray/target/native/bra-505. Do not change global toolchains.
Do not cargo check bray-runtime with bundled-bootstrap under Rust1.99. Rust-only checks use
--no-default-features. Native LLVM/linker/executable workflows run outside the sandbox.

Use rg --files before unknown paths. PowerShell does not expand wildcard paths passed to rg.
Use bounded log reads and avoid lines containing the intentional 16KiB panic-message fixture.
Reuse compiled test binaries and retained products where faithful. Account for task processes.
