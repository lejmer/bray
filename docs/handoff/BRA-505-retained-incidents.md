# BRA-505 ordinary finalization and retained incidents

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
Required formatting, final source checks and independent re-review follow before this fix is
considered reviewed. The remaining implementation matrix above is unchanged.
