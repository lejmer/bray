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
New ordinary locals retain normal checked cleanup and cannot transfer obligations into
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
certificate is lost. A teardown marker can change without introducing work, while acquiring
a new descriptor or appending buffered output can introduce work without replacing the
whole receiver. Do not silently equate these cases or make abandonment mutation-sensitive.
A checked preservation inference must establish its claim from operations and contracts,
including abnormal outcomes. An unknown or ordinary may-write effect cannot establish it.

The existing `destructor_receiver_move_admits_before_transfer` test has only a destructor,
not a finalizer. This boundary does not justify rejecting it or deleting its admission
machinery. Nonescaping consuming helpers retain their ordinary checked contracts.

No source implementation of terminal preservation has been added. The next implementation
decision is how existing operations and foreign contracts establish absence of new graceful
work without interpreting author intent, a new language contract, or a blanket mutation ban.
Keep that proof obligation explicit before connecting abandoned disposal.

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
