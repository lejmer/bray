# BRA-505 ordinary finalization and retained incidents

## Resume after every context compaction

Read this section and the active agreement before resuming work after compaction or
an application restart. These are explicit user requirements, reaffirmed 2026-10-18.

- The project is moving the runtime entirely to Bray. Runtime ownership, activation,
  waiting, retirement and lifecycle policy belong in Bray. Do not add or extend a Rust
  runtime frame to complete BRA-505. Existing Rust runtime machinery is a migration
  target, not a foundation for another adapter. Rust compiler code is a separate concern.
  All project-owned linked support belongs in Bray, except the pinned temporal provider.
  Native system ABIs do not justify permanent project-owned Rust or C runtime shims.
- Types and invariants must hold across the entire pipeline. Do not compensate for a
  lost checked contract with consumer-specific adapters, repeated inference or validation.
  Preserve the established result at its owning boundary and remove competing meanings.
- Correctness is necessary but insufficient. Assess compiler time, generated execution,
  binary size and maintenance independently. Do not rely on LLVM to remove avoidable work.
  A helper extraction or file move is not simplification if competing mechanisms remain.
- The user has repeatedly rejected growth through local repairs followed by another
  architectural document. Before further implementation, identify the mechanism replaced,
  its existing consumers and the code to remove. Unexpected layers or scope growth require
  reconsidering that design, not adding another exception. No new language features.

Current correction: pause further implementation additions while examining the actual
phase boundaries. The earlier proposed extra Rust owner-waiter slot was withdrawn. It is
not an accepted solution. Do not revive it after compaction.

Confirmed implementation conflicts, not new language requirements:

- `LifecycleCallable` retains completion type and execution separately; generated
  `push_lifecycle_call` constructs an Immediate result even for an async selection.
  Source cleanup instead creates and awaits the checked Future.
- `MirUnitKind::GeneratedLifecycle` mixes provenance with execution classification and
  excludes a frame descriptor. `MirSourceOrigin` already retains generated provenance.
  Merely changing the enum does not complete shared resumable cleanup.
- Native `TaskObservation` owns a separate Rust frame, cleanup sequence and failure
  collection. Generated task cleanup uses blocking `resolve_task`. Native `observe`
  consumes the outcome, so it is not a non-consuming readiness check. Reusing these
  interfaces unchanged cannot preserve all quiescence and retained-owner invariants.
- Existing Bray `activation.run` already owns composed execution and retirement.
  Complete the compiler/Bray connection and remove superseded policy. Do not introduce
  a replacement Rust observation frame or another cleanup scheduler.

The governing designs already state this boundary: `docs/design/composed-async-execution.md`
and `docs/design/cleanup-storage-and-reports.md`. The deficiency is implementation
alignment, not permission to invent another runtime architecture.

Latest pushed checkpoint: `216886091c2734aa5cb835c468ab3e163193c776`, PR #612.
Independent review 5560583230 found no actionable checkpoint findings. Native acceptance
remains unfinished. Human architectural feedback is now the active review.
The report-pool simplification removes the C pop wrapper and retains typed callbacks across
pool detachment. Its three focused compiler checks, three ABI checks, all-target metadata
check, and eight packaging/archive checks pass. The initial failures exposed a duplicate
binding, an existing incorrect pure annotation on address exposure, and a stale literal
role inventory. All three are corrected. Logs are in the shared target directory:
`bra-505-report-boundary-tests.log` and `bra-505-report-inventory-tests.log`.
Rust formatting, mutating structural style and Bray formatting completed successfully.
Style reports existing module-size warnings. Native acceptance remains unfinished.

## Active agreement

Complete BRA-505, then BRA-619, with one independently reviewed stacked PR per issue.
BRA-561 is implemented and reviewed in PR #611 at `7a254388`.
Work only in `C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray`, on
`feature/bra-505-admitted-ordinary-finalization`, draft PR #612, based on
`feature/bra-561-compiler-described-activation`. No merge is authorized.

The old BRA-505 checkpoint `b3022d87` was transplanted as `eb91736c`.
BRA-619 admission and rollback remain at `6bbcb4a3` on
`feature/bra-619-complete-selected-scope-native-cleanup`. Preserve those branches.
Do not replay older stack work or modify the user's main checkout.

No new language feature, syntax, compatibility layer, ABI bump, second cleanup
walker, post-failure allocation or downstream authority proof. Reuse selected
lifecycle expansion, checked cleanup transitions, existing outcomes, BRA-561
same-run activation and the existing report chain. Structural traversal shares
retained enclosing progress where possible. An organizational helper does not
justify another reservation. Reconsider accumulating special cases or hidden
blocking execution. Measure production and test growth separately.

Delete this temporary record after BRA-505 acceptance and independent PR review.
Carry only unfinished BRA-619 inputs into its record. No handoff survives the stack.
Completed implementation detail and evidence belong in Git and PR history.

The resource completion and predicate checkpoint is pushed as `93420da3`.

## Established implementation

- Source and imported successful completion proofs share checked publication.
  Conditional completion requires exact storage/exit evidence. Unconditional
  pure/total successful finalizers can be omitted without losing child cleanup.
  Successful postconditions without total retain invocation.
- Admission and lane planning consume selected lifecycle actions without generating
  MIR. Actual emission retains the generated-body cache. RawBuffer cancellation and
  quiescence traverse the initialized prefix with the existing reverse cleanup loop.
- Source and generated finalizers branch on completed Result values through shared
  outcome handling. The error branch retains the incident operand. Physical transfer
  remains unfinished below.
- Storage and liveness share selected implicit cleanup suspension occurrences,
  including scope exit, temporaries, pending results, evaluated inputs and replacement.
  Direct async finalizers reuse shielded awaited-frame composition and retain owner
  guards until completion. Compound assignment publishes replacement at call completion.
- Native inactive composition carries abandonment through existing MIR, serialization,
  LLVM, adapter and NativeRun paths. Continuing parents retain cleanup panic outcomes.
  This connection still needs its pending native regression and source integration.
- File, Directory, DynamicLibrary, ChildInput, ChildOutput and ChildProcess implement
  completion predicates and checked is_complete observations. Borrowed close/wait APIs
  preserve retryable ownership on errors. File/pipe close share native status/disposition
  reporting. Unix descriptors retire before close errors, Windows failures retain them.
  Directory closure and successful process reaping publish the consumed terminal state.
- Ordinary predicate meanings use existing portable constant terms and exact generic
  substitution through source and imported interfaces. Trusted predicates remain opaque.
  Predicates are rejected outside contract contexts. Interface validation checks predicate
  argument scope, generic const parameters and nullable terms at the import boundary.
- Ordinary call postconditions observe borrowed receivers and arguments. Consumed inputs
  cannot establish caller witnesses. Boolean and propagation refinements preserve exact
  selected outcomes. Yield boundaries reuse existing control transfer handling.
- Loop merges assemble joined parent observations before renaming and intersecting facts.
  Unknown pointers cannot reach never-addressed, cleanup-free local inline storage.
  Borrowed, heap and implicit-cleanup storage remain conservative. Returned unknown
  references retain their existing opaque reachability bounds without asserting identity.
- Inferred finalizer completion posts are numbered and deduplicated while building each
  exported domain. They reuse established total-proof dependencies. Missing checked
  mappings and semantic results fail with developer context, never user diagnostics.

## Abandonment contract

The user approved the terminal destructor boundary in
`docs/language/lifecycle/destruction.md`. Enforce it in owning storage and dependency
analysis before claiming abandoned disposal is correct. This is an intentional semantic
restriction, not an existing implementation guarantee.

From destructor body entry, receiver destruction authority permits preservation and
consumption, but not introduction or reactivation of graceful-cleanup obligations in the
receiver, represented ownership graph, or relevant dependencies. Cover aliases, helpers,
foreign calls, imported contracts and abnormal outcomes. Installing a currently completed
finalizable owner is forbidden too: later sibling cleanup can invalidate its completion.
New ordinary locals retain normal checked cleanup and cannot transfer graceful obligations into
the dying receiver. Explicit operations retain their ordinary contracts.

Implement abandoned disposal through the shared selected expansion. Preserve per-owner
lifecycle ordering and keep receiver initialization guards with its body and tail. Normal
child finalizers remain ordinary until entry to their own destruction. Do not require total
destructors, run all child finalizers before destruction, or use an ambient abandonment mode
to change explicit operations. Incident quiescence resolves child runs while retaining the
payload, backing and provider dependencies for later synchronous destruction.

The boundary removes the mixed inherited/new owner case that required runtime provenance.
Do not add per-part abandonment bits, widened borrows, hidden ordinary-call state cursors,
transition logs, global address registries, or helper specialization for this distinction.
Costs must be acceptable before LLVM optimization. Existing may-effect summaries can prove
that forbidden transitions cannot occur, but cannot identify actual committed transitions.
`initialized_as` alone does not prove foreign preservation. Missing proof must reject the
operation in destruction, not assume preservation.

Implementation acceptance must cover direct and aliased replacement, nested represented
parts, completed replacement, imported helpers, normal and panic outcomes, forbidden
reactivation through dependencies, admissible ordinary locals, and ordinary child finalizers.
Reuse established effects downstream and remove superseded interpretations. Batch checks
after coherent changes, then format, audit, push and independently review. No new syntax.

The implementation mapping exposes two distinct checks. Existing resolved may effects and
physical target relationships can conservatively identify possible owner installation,
including aliases, `Uninit` payloads and heap storage. Current cleanup completion candidates
certify particular cleanup sites, not preservation across every destructor transition.
Neither existing mechanism alone proves the terminal boundary.

Independent read-only design review confirmed that completion-proof invalidation is not
obligation introduction. An inherited abandoned duty stays abandoned when its omission
certificate is lost. A teardown marker can change without introducing ownership. A newly
acquired foreign descriptor can introduce resource ownership without replacing the whole
Bray receiver. The reviewer corrected its earlier buffered-output example: the specification
does not introduce a separate compiler-owned obligation for every buffered write. Do not
silently equate completion invalidation with new ownership or make abandonment
mutation-sensitive.
A checked preservation inference must establish its claim from operations and contracts,
including abnormal outcomes. An unknown or ordinary may-write effect cannot establish it.

The existing `destructor_receiver_move_admits_before_transfer` test has only a destructor,
not a finalizer. This boundary does not justify rejecting it or deleting its admission
machinery. Nonescaping consuming helpers retain their ordinary checked contracts.

The precise structural interpretation is now recorded in the language rule. Checked
construction, installation and transfer must not add new obligated ownership to the dying
graph. Consumption/removal and same-owner teardown preserve its terminal disposition.
This gives an inductive tail proof: remaining owners are inherited, so the abandoned tail
needs no inherited/new runtime discriminator. Existing initialization guards still exclude
consumed parts. This proof is conditional on establishing the rule at every source/imported
and foreign transition, not evidence that the current compiler already does so.

The actual foreign-binding audit remains necessary. `std.os.unix.close` and Windows
`close_handle` currently expose scalar/pointer parameters and `uses(foreign_call)`, while
resource release is described in generated comments. Those comments are not checked effect
metadata. Do not infer preservation from a function name, lack of `raw_memory`, normal
initialization postconditions, or an assumption that all trusted functions preserve owners.
The Unix generated close ownership comments incorrectly said failure retained descriptor
ownership. The four target inputs now describe relinquishment of valid descriptors, with
Darwin's close-guard exception. Generated comments and input digests are regenerated from
those inputs. The evidence is the [Linux close documentation](https://man7.org/linux/man-pages/man2/close.2.html)
and [Apple's close implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_descrip.c),
where `close_nocancel` checks close guards and `fp_close_and_unlock` retires the descriptor
before returning the result of `fg_drop`. This corrects documentation, not compiler effects.

Terminal preservation is implemented in the uncommitted owning storage analysis. Focused
source and imported cases pass. Production effect publication and native resource checks
remain in progress. Complete that acceptance before connecting abandoned disposal.

The removed global graceful-phase experiment must stay removed. Existing shared awaited-frame
emission and internal invariant fixes remain at `74f77b59`, with 87 focused tests passed.
Native acceptance remains blocked by unfinished CleanupIncidentTransfer and enforcement of
the terminal boundary. Inactive abandonment never enters the body. A child's ordinary
cancellation must not cancel its abandoning parent, but cleanup incidents must survive.

## Report-chain ownership checkpoint

Reports now distinguish an absent inline panic from an explicit panic with an empty message.
The presence field occupies existing padding, retaining the 96-byte primary and 136-byte
report on all native targets. Rust transfers detached chains without popping a panic primary
or consuming another admission record. Chain disposal uses the existing Bray consumer.
Source report retirement, retention, reporting, native mirrors and field binding share this
representation. Typed pool entries and compiler incident transfer remain unfinished.

Review of `74f77b59` is verified at PR #612 review `5557613058`, with no findings.
The report-chain checkpoint `2fce6402` is pushed. Independent PR review `5557690835`
is verified with no findings. Neither review establishes whole-issue acceptance.

The subsequent callback audit found that the generated incident destroyer relied on callers
to initialize both output destinations, contrary to its write-only callback contract. The
producer now initializes destruction and backing-release outcomes before invoking cleanup.
The regression covers successful and panicking source destructors and LLVM aggregate stores
or their equivalent memset. Checkpoint `25b0bd5b` is pushed. Tests and required formatting
pass. Independent PR review `5557720344` is verified with no findings.

## Remaining implementation and acceptance

| Obligation | Owner and required work |
| --- | --- |
| Ordinary async cleanup | Shared lifecycle expansion and protected source continuations. Replace generated immediate-call assumptions with admitted same-run completion under the cleanup shield. Retain parent progress, pending outcomes and dependencies. |
| Inactive Future captures | Existing frame descriptor and abandonment entry. Never enter an inactive body. Resolve accepted captures before retaining a terminal representation. Future selection currently rejects quiescence explicitly. |
| Retained error quiescence | Shared selected expansion. Resolve child runs while retaining payload and terminal handles. Generator quiescence remains missing. Buffer prefix selection is covered. |
| Abandoned destruction | Generated destruction and source destructor receiver cleanup must not replay implicit graceful finalizers. Explicit operations keep ordinary checked semantics. |
| Physical incident ownership | Move pre-admitted payload backing, provider and attachment dependencies into the existing report chain. Discharge cannot release transferred storage. Do not use immediate disposal or a separate activation. |
| Run ownership and disposal | Retain ordered typed entries across direct await, suspension and nested independent roots. Destroy and release exactly once, preserving reporting/destruction/release failures and cancellation. |
| Production acceptance | Source/imported contracts, normal/panic/cancel exits, temporary/partial/nested owners, inactive captures, large/aligned errors, denied allocation, exact release and link maps. BRA-561 driver tests are not proof of these consumers. |

The enabled `scoped_selected_calls_emit_native_artifacts` reaches missing
CleanupIncidentTransfer. Keep it required through BRA-505/BRA-619. Do not publish
production transfer before quiescence and abandoned destruction meet the contract.

## Implementation map

- Source `push_cleanup_action` drives direct async finalizers through
  `push_async_finalizer` and `lower_awaited_frame`, using checked occurrences,
  retained storage, shielded composition and ordinary cleanup outcomes.
- Synthetic `push_lifecycle_call` still assumes an immediate result. Generated helpers
  currently have no persistent frame. Reuse enclosing progress and admission rather
  than silently introducing an additional activation for structural traversal.
- Future checking selects cancellation and lifecycle cleanup, but compiler-known
  lifecycle needs publish only QUIESCE. Ordinary erased future disposal is treated as
  trivial by code generation. Inactive capture disposal needs selected suspension
  occurrences and retained inputs, never fabricated downstream storage snapshots.
- Native composition already accepts abandoning. Admission still requires a destination
  sized and aligned for the exact child result. Source inactive disposal is not connected.
- Synthetic TaskQuiesce resolves the run and disposes its RunResult while retaining the
  TaskSlot. Only cleanup destroys its terminal handle. Native async observation consumes
  that handle and cannot be reused unchanged for retained payload quiescence.
- CleanupIncidentTransfer is compiler-owned in the runtime ABI catalog with no native
  binding. Its current codegen call path is unfinished. The static consumer immediately
  disposes and cannot implement retained incidents.

## Verification and audit

All logs below are under the shared target directory
`C:/Users/anton/source/repos/bray/target`. Failed intermediate runs are not acceptance.

- `bra-505-storage-export-audit-tests.log`: all 238 focused storage, authority,
  completion, export and lifecycle tests pass, including loop joins, opaque references,
  forged-owner negatives and source/imported conditional finalizer completion.
- `bra-505-predicate-boundary-tests.log`: all 227 package-interface/import tests pass.
  Malformed predicate result references are rejected. Valid generic predicates export.
- `bra-505-predicate-boundary-format.log`: Rustfmt, mutating `cargo xtask style` and
  explicit `cargo xtask format` pass. Existing nonblocking style warnings remain.
- `bra-505-resource-boundary-verified.log`: OS bindings, Unicode data, all six native
  target source checks and the native standard-library bundle pass after the latest
  predicate-boundary fix. Targets are x86_64/aarch64 Linux, Windows and Darwin.
- `bra-505-inactive-native-verified.log`: the native runtime test build stops at the
  known missing compiler-owned CleanupIncidentTransfer while planning the bootstrap
  standard library. No runtime test executes, and the chained resource API run does
  not start. Do not repeat this build before the production transfer path is ready.
- `bra-505-shared-continuation-tests.log`: 83 compilation and 4 lowering lifecycle tests
  pass after removing the unaccepted phase split. The known native-role blocker remains
  explicitly excluded from this focused batch, not counted as accepted.
- `bra-505-shared-continuation-format.log`: Rustfmt, mutating style and Bray formatting
  pass. The audit removed obsolete lifecycle error variants and diagnostic projections.
  The shared await emitter replaces the source emitter while retaining source guards,
  established suspension states, lanes, affinity and cancellation edges.
- `bra-505-report-chain-tests.log`: 4 report ownership/ABI tests and 2 LLVM tests pass.
  The layout test covers all six native targets without increasing report size.
- `bra-505-report-binding-tests.log`: the report runtime-binding regression passes.
- `bra-505-report-production-tests.log`: changed production report bodies check and lower
  on all six native targets. Rust runtime sources and tests type-check without bundled
  bootstrap. The new native exact-release/round-trip regression remains pending native
  execution, which is still blocked by compiler-owned incident transfer.
- `bra-505-report-chain-format.log`: Rustfmt, mutating structural style and explicit Bray
  formatting pass. The audit covers report creation, chain transfer, reservation retirement,
  consumer disposal, native field binding, C/Rust mirrors and exact-release regressions.
- `bra-505-static-outcome-tests.log`: 13 existing static lifecycle tests pass.
  `bra-505-static-outcome-regression.log`: the new output-initialization regression passes.
- `bra-505-static-outcome-format.log`: Rustfmt, mutating structural style and explicit Bray
  formatting pass. The callback audit covers both output producers, the source write-only
  contract, invocation behavior and backing-release status handling.
- Terminal-boundary documentation audit: the completion/ownership distinction is recorded.
  The four Unix close input comments are corrected and all six target outputs regenerated.
  `cargo xtask standard-library os-bindings generate --check`, `cargo xtask format` and
  `git diff --check` pass. Generated diffs contain only ownership comments and input digests.
  No executable behavior changed, so native execution and unit tests were not repeated.
- Native resource retry tests and inactive composition execution remain pending.
  Source and bundle verification do not establish retained-incident acceptance.

The mechanical audit covers joined observations, pointer reachability, exported proof
numbering and dependencies, predicate transport and validation, propagation and yield
boundaries, platform close disposition, terminal resource state and removed internal
failure wrappers. Shared existing mechanisms replace repeated interpretation. No C/C++
fixture formatting was changed in this checkpoint. No independent completion review
is claimed while the issue's acceptance remains incomplete.

The separate compiler-defect diagnostic mechanism outside this lifecycle work is tracked
in [BRA-625](https://linear.app/bray-lang/issue/BRA-625). BRA-505 still owns every lifecycle
invariant encountered here. Do not turn impossible states into recoverable diagnostics.

## Current native ownership transfer work

The user selected existing-language API restructuring for unproved foreign calls.
File acquisition admits its existing inactive File owner before the native open and installs
its handle immediately after success. Pipe transfer admits ChildInput or ChildOutput before
clearing the process's retained handle. Native FileClose and ProcessPipeClose ABI roles remain
required. A proposed generic release-only Owner was removed because it duplicated admission
and forwarding destruction without establishing native resource authority.

The broader scalar-resource authority audit is recorded in the open
[BRA-508](https://linear.app/bray-lang/issue/BRA-508). Completion flags and field visibility
are not ownership witnesses. That resource-family work must cover acquisition, transfer,
operations, release, abnormal outcomes and imports, including forged, copied and mismatched
handles. BRA-505 still owns the acquisition ordering and terminal-analysis acceptance here.

## Approved ownership implementation and cost bounds

The user approved this design and unattended completion through BRA-619. No new language
feature or syntax is authorized. Baseline is afa9a815 plus the native-resource preparation
changes recorded above. BRA-505 owns the shared repair and PR #612. Stack BRA-619 on its
reviewed head and retain the existing admission/rollback checkpoint.

1. Refine existing ownership/dependency effect production to describe possible introduction
   or reactivation of graceful ownership. Check relevant targets through the existing storage
   relationships. Include aliases, initialized protected payloads, dependencies, implicit
   cleanup and every normal/abnormal outcome. Joins retain possible introductions even when
   later consumption leaves the final state unchanged. Unknown relevant effects cannot prove
   preservation. Existing caller and imported-summary consumers reuse the established result.
2. Keep scalar mutation distinct from ownership introduction. Invalidate affected witness
   evidence, require complete-carrier provenance for resource operations, and reject forged
   aggregates, copied fields and mismatched resources where the active contract applies.
   Retain the acquisition-order repair and the resource-family authority obligations in
   BRA-508. Do not use field privacy, runtime tags, false pure/total claims or a universal
   untyped resource owner as authority.
3. Connect abandoned disposal and incident quiescence through the shared selected cleanup
   expansion. Resolve child runs without dropping retained payload/backing/provider ownership.
   Preserve ordinary explicit-operation and child-finalization semantics. Finish ordinary
   finalization and inactive Future acceptance, then the BRA-619 scoped-transfer acceptance.
4. Batch meaningful tests after coherent changes, then format, audit every changed behavior
   and equivalent path, push, and complete independent PR review cycles with all fixes pushed.
   Remove temporary handoffs when their work is complete. Keep PR descriptions in the required
   Change Summary / Related Issues format without agent or verification logs.

Performance constraints: proof summaries are compile-time metadata and require no runtime
registry, history, snapshot, identity counter or per-part abandonment flag. Extend owning
analysis rather than adding a pass or repeating callee analysis at call sites. Use finite
demanded subjects and conservative summaries, not enumerated execution histories. Transfer
existing cleanup allowance with ownership and avoid duplicate admission or forwarding
destruction. Check emitted operations before LLVM optimization. Transparent layout guarantees
representation, not zero call or cleanup cost. Record phase costs, backing/frame sizes and
linked runtime demand alongside native correctness evidence. No competitive performance claim
is established by source checking alone.

Reconsider any approach requiring new syntax, proof-state runtime storage, duplicate cleanup
interpretation, redundant reservations, path-history growth or a new general subsystem. Fix
bounded gaps in their owning analysis. A genuinely independent subsystem must have an active
Linear owner before proceeding. The existing coverage matrix remains required.

## Terminal analysis checkpoint

The uncommitted repair adds typed replacement effects to the existing dependency summary.
Types are substituted, exported, selectively loaded, validated and included in specialization
identity. The owning storage analysis rejects graceful-owner installation in terminal storage.
Ordinary locals and the inherited receiver tail retain their selected cleanup semantics.

Receiver storage is always protected. A lifetime borrow does not transfer the referent's
cleanup ownership. Existing cleanup shape decides whether supporting dependency edges matter
for the receiver's own graceful obligations, including protected Uninit payloads. Definite
moves remove transferred edges from its current ownership graph. Scalar observations do not
become pointer edges because a transfer entry is unknown. Positive and negative cases cover
independently owned referents and dependencies supporting a finalizable receiver.

Address formation is distinct from escape. Writes to externally reachable storage, foreign or unresolved
calls, deferred captures and integer exposure retain escaped addresses. Checked bodies
publish address capture separately from unknown memory mutation. Generic cleanup remains
conditional until instantiation, including its possible capture, rather than publishing an
unconditional escape from a type parameter. A write confined to an
invocation-local slot does not publish its address. Existing reference relations track later
publication. Invocation roots are computed once before mutation and reused by both checks.
The memory tests and source cases distinguish a local address, a local mutating helper, and
an address actually passed to native code. Source/imported authority negatives remain required.

The production host loop exposed a separate condition-publication defect. Substitution changed
the first iteration's recorded `index < count` into `0 < count`, so joins lost the common bound.
Branch analysis now uses substitution to decide truth while retaining the evaluated condition.
A merge regression also covers an alias that disappears while its observed value survives.
Temporary effect tracing was removed after identifying the capture producers. Verify that
no BRAY_TRACE instrumentation remains before committing.

The full compiler batch completed with 1776 passes and 15 failures in
`bra-505-terminal-compilation-suite.log`. Subsequent focused checks pass terminal-destructor,
source/imported authority, production host retirement and two native bootstrap regressions.
The native bootstrap repair consumes established unreachable-exit semantics before lowering
cleanup. Production synchronization now passes. The focused terminal and imported-summary batch
passes, including captured-address revocation. Production channel declarations, surviving-sender
use, forgery rejection and protocol-access rejection now pass. File and pipe acquisition source
checks pass on all six targets. Native acquisition and lifetime acceptance remains pending.
The missing compiler-owned CleanupIncidentTransfer remains an expected unfinished path.
No retained-incident native acceptance or independent completion review is claimed.

The unused RawBuffer source destructor is removed because compiler-owned lifecycle selection
already performs its element cleanup and release. A failed synchronization payload-extraction
trial was restored. Do not weaken complete-carrier rules or revive that trial.

HashTable now moves its buffers to ordinary locals before clearing them. Its pre-existing
sparse panic-cleanup defect is recorded in the active represented-owner issue
[BRA-506](https://linear.app/bray-lang/issue/BRA-506). A panicking Item can leave occupied
Uninit slots unresolved. That family must reuse admitted cleanup without copying every payload
or adding a second cleanup engine. Positive source checking is not panic-path acceptance.

Complete the production effect and acquisition checks before claiming terminal acceptance.
Then finish the shared continuation, retained quiescence,
abandoned destruction and incident-transfer work in the implementation matrix above. Preserve
performance bounds and do not infer progress from passing unrelated driver tests.

Deque, OrderedTree, Directory and ChildProcess teardown now moves the affected represented
storage to ordinary locals before user or opaque cleanup. These source adjustments pass all six native-target source checks. The standard-library
bundle verification also passes. OrderedTree shares the sparse Uninit panic-cleanup
obligation already tracked by BRA-506 for represented collection owners. Moving the buffer does
not itself resolve the initialized sparse slots on an abnormal exit.

The conditional-summary batch passes 191 package-interface tests. Its channel failure is
resolved by the reference-transfer repairs below. Nullable presence
queries now declare their existing pure/total behavior in the compiler-known catalog, so
ordinary call analysis consumes that contract. Concrete source cleanup summaries retain
conditional payload cleanup and separate capture from mutation. Raw stores now consider
references contained in the stored operand, not the address used as their destination.
Logs: `bra-505-conditional-interface.log`, `bra-505-conditional-summary.log`,
`bra-505-library-source-recheck.log`; the retained release-tool digest is recorded in
`bra-505-library-tool-identity.log`. The last library run reused that tool after two Bray-only
integration fixes. Later Rust changes are covered by the focused checks below. Repeat native source acceptance
with the final compiler before completing this issue.

## Reference transfer and checkpoint review matrix

Reference origins are prepared independently of possible memory effects. The existing type-shape
preparation supplies both mutation observations and finite reference slots. No second type walker
or runtime metadata is added. Returned aggregate fields keep their bounded origins through
anonymous factories and selected propagation. Atomic writes preserve ordinary reference slots.
Resolved effects are deduplicated after physical target resolution to avoid repeating identical
unknown and atomic invalidations.

Propagation must account for both continuations. The success path transfers the selected payload.
The failure edge publishes its selected error or absence before cleanup, using the existing
callable/yield boundary and conversion. Pending yielded values represent inactive payloads as
empty reference values in the existing transfer map. Joins can then retain a present branch's
exact origin without mistaking absence for unknown contents. Pending results do not become live
storage before their join. An attempted early-initialization fix broke the production channel
witness regression and was removed. Temporary tracing is removed before publication.

| Changed contract | Owning implementation and acceptance |
| --- | --- |
| Terminal ownership | storage-flow terminal checks, typed replacement effects, source/imported direct and aliased installation, consumption, ordinary locals, implicit receiver tail and abnormal exits |
| Capture and mutation | separate escape effects, raw stored-value capture, deferred capture, local-address and foreign-publication negatives, source/imported unknown-call contracts |
| Reference composition | shared observation/reference preparation, aggregate and anonymous return, selected nullable/Result propagation, callable and yield exits, error payloads and guarded pending joins |
| Cleanup specialization | selected lifecycle summaries, conditional generic cleanup, nested RawBuffer selection and recursive-wrapper growth regression |
| Witness preservation | branch truth and disappearing-alias joins, production locks/once/channel, forged and mismatched owner rejection |
| Contract transport | type substitution, interface encoding/decoding, selective loading, validation, specialization identity and diagnostic rendering |
| Native source adaptation | preadmitted File/pipe acquisition, moved teardown buffers, host retirement and report-chain local disposal. Native retained-incident transfer remains unfinished |

The final propagation matrix passes for nullable and Result values at callable and yield exits.
`bra-505-pending-values-final.log` records 47 passing compilation tests, including production
synchronization and channel acceptance. `bra-505-checker-messages-final.log` records 330 checker
and 70 message tests. The pending-retirement regression also passes independently in
`bra-505-pending-retirement.log`. After extracting the existing operation-completion phase to
meet the function-size rule, 43 focused compilation tests pass in
`bra-505-checkpoint-audit-tests.log`. Rust formatting, structural style and maintained Bray
formatting pass in `bra-505-checkpoint-audit-format.log`. All six native-target source checks
and bundle verification pass with the final compiler in `bra-505-checkpoint-library-final.log`.
Source checks take 18.0 to 18.8 seconds per target in this run. These timings are coarse evidence,
not a competitive compiler benchmark. A broader earlier name filter selected a long unrelated
runtime-role test and was interrupted. It is not counted as a passing batch.

Compilation-cost acceptance remains required. Reference shape discovery is once per body/type,
with bounded recursive paths and no execution-history enumeration. This checkpoint must not be
represented as completion of ordinary asynchronous cleanup, retained quiescence, abandoned
destruction, native incident ownership, or BRA-619.

## Terminal checkpoint review fixes

Checkpoint `9df53861` is published on PR #612. Independent review `5559944824` found
compound assignments dropping operator effects and nested pending payloads losing reference
origins. Inline comments `4304384191` and `4304384194` are verified.

The owning transition now checks and publishes assignment and invocation effects independently.
Invocation roots include the operator inputs. Assignment capture still considers only the stored
value. Observation invalidation combines both effects without broadening an ordinary direct
assignment. No downstream validation pass is added.

Pending absence stages the already prepared reference slots beneath an aggregate payload.
Preparation covers temporary results as well as calls. This uses the existing bounded type demands,
without treating an empty aggregate root as evidence about arbitrary unknown descendants.

The expanded source/imported operator matrix covers direct assignment and a called helper,
including the operator's possible abnormal exit. Nullable and Result propagation cover nested
aggregate references at callable and yield boundaries. The final batch passes 48 compiler tests,
including production synchronization, channel and host destructors, and 331 checker tests.
Logs: `bra-505-review-composition-final.log`, `bra-505-review-checker.log`.
The review fixes were published as `0aa96f86`. Formatting, structural style, all six native-target
source checks and bundle verification passed (`bra-505-review-format.log`,
`bra-505-review-library.log`). Fresh review `5560018217` confirms both findings are addressed;
threads `4304384191` and `4304384194` have replies and are resolved.

That re-review found unresolved assignment destinations losing RHS capture (`4304443313`).
Checkpoint `df473c0b` collects RHS origins before destination invalidation and
retains capture without an exact target. Source and imported conditional-destination regressions
pass, including a later opaque call. Independent review `5560121489` has no findings. Reply `4304541312` is published and the thread is resolved.


## Active destruction implementation checkpoint

The current uncommitted work carries abandonment as an explicit internal destruction-call
argument. It changes no Bray syntax or public ABI version. Source destructors apply the argument
only to implicit receiver-tail cleanup at checked scope exits. Ordinary locals and explicit
operations keep normal cleanup. Existing initialization guards remain in force.

Finalizer results carry one SSA disposition through existing outcome branches. Error, panic and
cancellation abandon that owner's inherited graceful tail. Success uses the returned disposition.
No per-part history or runtime owner metadata is added. MIR uses block-local values, so cleanup
outcome dispatch forwards the disposition through block parameters. Whole-owner resolution,
represented child cleanup, task-result disposal and native entry-error disposal reuse the selected
cleanup helper instead of independent finalize/destroy sequences.

The first focused batch passed 39 lowering and 191 interface tests. After correcting block-local
SSA forwarding and old sequence assertions, 59 compiler tests pass
(`bra-505-abandonment-compiler.log`). These include source/imported capture, source destructor
local/tail separation and generated aggregate/nullable/union traversal. LLVM nested cleanup then
exposed a branch-join error: each branch must collect its call outcome before joining. The owning
lowering correction is applied. The combined 63-test compiler/LLVM batch passes in
`bra-505-abandonment-compiler-final.log`, including native host error and nested cleanup emission.
Rust formatting, structural style and maintained Bray formatting pass in
`bra-505-abandonment-format.log` and `bra-505-abandonment-bray-format.log`.
These are emission checks, not native retained-incident execution acceptance.

Still required for this implementation: audit abandonment through RawBuffer release and storage
policies, direct helper/callback consumers, finalizer error and panic/cancel outcomes, native order
and no-double-destruction acceptance, final formatting/audit, push and independent review. The
ordinary synthetic asynchronous call assumption, inactive Future quiescence, incident payload
transfer and run/report ownership matrix above remain unfinished. General generator construction
belongs to active BRA-484 through BRA-506; do not implement a second generator engine here.


## Storage-policy simplification in progress

The buffer batch passes 48 lowering, lifecycle and LLVM tests, including actual standard-memory
source emission (`bra-505-buffer-tests.log`). The internal optional abandonment operand is retained
by traversal, remapping, type/helper demands, serialization and inspection. Ordinary explicit
buffer release retains its original cleanup. Generated release shares the existing reverse loop.

The storage-policy audit found duplicate target lifecycle ownership in `Storage.destroy`. The
compiler already obtains the exact target through `Storage.borrow_mut`, and `Storage.release`
already requires the target to have been destroyed or removed. Remove the redundant destroy hook,
its Heap implementation and selected-call demand. Resolve the target through shared child cleanup
with the enclosing outcome, then release storage even if target cleanup panics. No new syntax,
ambient mode, callable specialization or storage-state metadata is needed. Protected policy
representation such as `Uninit<T>` remains storage, and release must not destroy its retired target
again. The existing custom-policy native fixture must establish exact target cleanup and release.
The catalog's 62 tests, 44 IR tests, 39 lowering tests and 191 interface tests pass. The
64-test storage batch passed 63 tests and exposed a zero-sized custom-storage construction
result that LLVM had intentionally erased. Construction and conversion now reuse the ordinary
call's zero-sized materialization rule. The custom fixture, buffer, replacement and nested
cleanup native emission tests all pass after that correction. Exact native execution remains
pending the known incident-transfer integration, not established by emission.

The final audit removes unused finalizer-disposition forwarding from destructor helpers by
reusing the existing checked-call path. Generator destruction now forwards its inherited mode
and demands ordinary Cleanup plus abandoned Destroy callbacks, rather than the superseded
Finalize/Destroy pair. Traversal, remapping, serialization, demand, inspection and its runtime
signature carry the mode. Native generator implementation remains owned by BRA-484/BRA-506.
These final audit changes require the next focused test batch before publication.

Review matrix for this checkpoint: explicit versus implicit receiver cleanup, ordinary locals,
normal and failed finalizer disposition, aggregate/nullable/union/array traversal, buffer prefix
retirement, custom/heap storage target and backing release, task result and entry-error disposal,
generator callback contracts, source/imported MIR transport, and zero-sized returned values.
Searches covered Destroy/Finalize helper consumers, StorageDestroy/HeapStorageDestroy and
storage trait implementations, call-result materialization, and optional operand visitors.
No compatibility hook or duplicate storage destruction implementation remains. Non-executable
generator and retained-incident paths remain explicit outstanding acceptance, not passing rows.


The callback audit also covers product and exact-thread statics. Their native destruction callback
now receives the finalizer abandonment outcome and forwards it to generated destruction. Its C
fixture declaration matches, without changing the ABI version. Existing runtime tests assert
ordinary and failed-finalizer modes. Runtime sources/tests check with bundled bootstrap disabled.
The 85-test audit batch passed 84 tests, including all static LLVM emission. The corrected generator
dependency regression now passes separately (`bra-505-generator-audit-test.log`). The final IR,
lowering, package-interface and runtime ABI suites pass 44, 39, 191 and 21 tests respectively.
The callback audit found and corrected this producer/consumer boundary before independent review.


Checkpoint audit: the implementation delta adds about 590 lines across production modules including
inline tests, 331 lines in dedicated tests/fixtures, and removes 30 generated catalog lines. Most of
the cross-file changes carry one explicit invocation operand through existing MIR transport. Removed
mechanisms include Storage.destroy/HeapStorageDestroy, their helper body, split child finalize/destroy
sequences and unused destructor disposition accumulation. The remaining mode argument is a register
value, not retained per-owner metadata. No allocation or runtime history is added for it. Actual native
counts and denied-allocation acceptance remain pending the incident-transfer integration.

Rustfmt, mutating `cargo xtask style`, explicit `cargo xtask format` and whitespace checks pass
for this checkpoint (`bra-505-final-destruction-format.log`). Independent PR review remains next.


Destruction checkpoint `37ac358a` is pushed to PR #612. Independent review
[5560298009](https://github.com/lejmer/bray/pull/612#pullrequestreview-5560298009)
found no actionable defects. Its commit and posted review were verified.

The next coherent batch connects inactive capture cleanup to the existing parent
continuation. MIR absence of completion storage represents abandonment, replacing
its redundant mode flag and avoiding a result-sized parent slot. Checked cleanup
selection must include inactive Future ownership ends even when no declared
finalizer contributes a callee. The native path already selects cleanup admission,
marks the child abandoned before dispatch, and never invokes its result mover.
A suspended lifecycle callback changes CLEANUP to RESOLVING before the next dispatch,
so its new cleanup child is resumed normally. Expected child cancellation is consumed
as abandonment completion, while cleanup panics retain the enclosing outcome.
Remaining synchronous/generated helper integration and retained incidents stay open.

The first async-disposal batch exposed full cleanup expansion inspecting the source
fields of protected Future/Task/report/string representations. Full expansion now
retains their compiler-owned cleanup operation, as it already does for RawBuffer.
The next batch passes 30 of 32 lifecycle/source/LLVM tests. The failures are the
known CleanupIncidentTransfer acceptance and synchronous disposal incorrectly
entering the protected-frame-only path. Source composition is now selected only
for protected callers. Synchronous callers retain the selected lifecycle operation,
whose native inactive disposal remains unfinished as recorded above. This fixes the
lowering regression without claiming synchronous disposal acceptance. The new async
cases pass for locals, discarded expressions, replacement and early returns.

The final focused batch passes 55 compiler, source/imported analysis and native-emission
tests (`bra-505-inactive-final-tests.log`). IR, lowering and interface suites pass 44,
39 and 191 tests. The pending CleanupIncidentTransfer case was already observed failing
and excluded from this bounded rerun, not from final acceptance. The array-retention
regression now identifies actual entry-initialized flags rather than every Boolean local,
and permits only pending-future disposal to suspend outside the ordinary body. Frame
layout already retains outcome locals and temporaries, independent of the descriptor's
initialized-source-storage list. Protected opaque cleanup classification is shared with
shape selection. No runtime owner history or result-sized abandonment slot is introduced.


The executable import test now decodes every exported template through the actual
implementation artifact and verifies abandonment survives serialization and remapping.
The bootstrap contract suite passed 55 tests and found one failure in report callback
witness preservation. Tracing identified raw byte pointers used for native code addresses:
ordinary data writes conservatively invalidated their possible referents. Report transport
now stores those code addresses as usize, as other native address protocols already do.
Reconstruction still requires the exact native producer predicate and nonzero address;
field mutation and reconstruction from another report do not preserve that authority.
The native pointer-sized transport is unchanged. No new compiler preservation rule,
language feature, runtime allocation, or ownership metadata was introduced.

The focused correction passes the full production storage-effects audit, positive and
negative callback-authority cases, callback field-type rejection, and executable MIR
round-trip (four tests, `bra-505-callback-address-tests.log`). Temporary trace instrumentation
was removed. Native execution and complete stack acceptance remain pending.

Checkpoint audit matrix: cleanup shape and suspension selection share the existing
selected plan; source disposal reuses awaited composition and outcome handling; MIR
visitation, remapping, validation, serialization and tooling handle absent result storage;
LLVM emits a zero abandonment destination and no result load; native composition contracts
include the mode; native report transport keeps code-address authority at reconstruction.
Repo-wide operation-name, representation-role and callback-field searches covered sibling
consumers. No old abandonment Boolean remains alongside optional completion storage.
Synchronous Future disposal, generated async helper integration and retained typed incidents
remain explicit issue work, not claims of this checkpoint.

The checkpoint delta is 162 net lines in production files (including inline test edits),
346 in dedicated tests and 64 in this temporary record before this note. Growth
buys checked inactive-disposal suspension and portable optional result storage; it removes
the redundant mode representation and does not create a second execution path. Rustfmt,
mutating style, Bray format and whitespace checks pass.


Inactive-disposal checkpoint `faf16081` is pushed. Independent review
[5560479964](https://github.com/lejmer/bray/pull/612#pullrequestreview-5560479964)
found no actionable defects. Its commit and posted review were verified.

The next batch makes the incident transfer boundary explicit. One shared lowering
routine retains the returned error in owned storage, broadcasts cancellation over
its new child graph, quiesces that graph, collects its cleanup outcomes and then
transfers the still-owned payload. Generated finalizers, direct async finalizers
and suppressed scoped-exit errors use that routine. The MIR transfer retains its
selected producer, needed to select matching admitted backing. Its payload is a
place rather than an arbitrary operand, and helper demand includes its destructor.

A new error can be returned during outer lifecycle resolution. MIR therefore allows
an explicit nested BeginCleanup at that point. It still requires a cancellation
broadcast entry, and ContinueCleanup still requires a broadcast-to-resolution edge.
The lowering regression covers all quiescence outcomes, excludes original-call
failure/success paths from error transfer, and verifies broadcast precedes waiting.
Executable import covers producer identity, owned storage and reconstruction.

This establishes the lowering contract only. Native transfer remains unbound until
async quiescence and physical retained ownership are connected. The synchronous
TaskResolution helper and generated async helper assumptions are still outstanding.
No hidden blocking root, source feature, runtime ownership history or allocation
has been added by this batch. The 49 source/import/generated lifecycle tests pass,
as do the 44 IR, 39 lowering and 191 interface tests. Logs are
`bra-505-incident-boundary-tests.log` and `bra-505-incident-transport-tests.log`.


The transfer also carries the producer's checked invocation result. Outgoing
realization reuses invocation_outgoing_requirements, and the regression compares
the retained incident descriptor with the original call's descriptor. It does not
re-infer the signature or select backing from the error type alone. MIR type
collection shares its invocation-result helper with ordinary outgoing owners.

The final 49-test lifecycle batch and two LLVM emission tests pass after extracting
awaited composition into the existing task-effect module and scoped result replacement
into its owning lowering module. Tooling compiles. The final transport suites pass
44 IR, 39 lowering and 191 interface tests. Review matrix searches cover all
TransferCleanupIncident producers/consumers, storage and operand traversal, executable
encoding/decoding, helper demands, descriptor selection, inspection output and
nested cleanup edges. No second incident-lowering sequence remains. Native execution
and complete issue acceptance remain pending.


Checkpoint audit: 256 net production lines, 230 net dedicated test lines, and this
temporary record. Part of the production diff expands existing compressed scoped
lowering during formatting. The added capability is shared typed-error quiescence
and exact admitted producer identity. Extracted helpers retain their existing domain
modules. No runtime representation or allocation is added. Rustfmt, mutating style,
Bray formatting and whitespace checks pass (`bra-505-incident-format.log`).
