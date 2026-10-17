# BRA-505 ordinary finalization and retained incidents

## Active agreement

Complete BRA-505, then BRA-619, with one reviewed stacked PR per issue. BRA-561
is implemented and independently reviewed in PR #611 at `7a254388`. Work only in
`C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray` on
`feature/bra-505-admitted-ordinary-finalization`, draft PR #612. Its base is
`feature/bra-561-compiler-described-activation`. No merge is authorized.

The old BRA-505 checkpoint `b3022d87` was transplanted as `eb91736c`.
BRA-619 admission and rollback remain at `6bbcb4a3` on
`feature/bra-619-complete-selected-scope-native-cleanup`. Preserve those branches.
Do not replay older stack work. Keep user edits in the main checkout untouched.

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

## Current implementation

- `14dc444c`: source/imported pure-total successful completion proofs, including
  async finalizers and guarded Ok(unit). Distinguish pre-entry cancellation from
  body cancellation. Omit whole finalization without losing represented children.
- `92ee8c35`: normal ownership-end legality and shared immutable completion
  publication. Reject unresolved fallible cleanup and async cleanup in synchronous
  scopes. Verified successful postconditions can establish infallibility without
  total, retaining invocation. Admission and discharge retain concrete async
  constructor dependencies. Diagnostic readiness coverage is restored.
- `d02f1b62`: admission and lane planning consume selected LifecycleAction calls
  without generating MIR. Remove LoweredLifecycle and duplicate call bookkeeping.
  Actual emission retains the generated-body cache and shared role-to-phase mapping.
- `9c148384`: buffer correction: cancellation and quiescence visit the recognized
  RawBuffer initialized prefix with the existing reverse cleanup loop. Lifecycle
  needs and source cleanup phases include element obligations. Preserve count, allocation and elements for
  later destruction. Source and imported declarations exercise the same selection.

- `c4337ede` result correction: generated finalizers transfer a completed `Result` error
  through the existing incident operation instead of discarding it. Source and
  generated cleanup share payload branching. Cleanup outcome edges carry the value
  only on successful completion, including scoped exits. Source lowering installs
  the branch through its ownership-aware terminator sink, preserving move guards.
  Incident transfer keeps its existing non-propagating contract. Quiescence and
  retained destruction still belong to the unfinished production consumer below.

- Current implicit-cleanup checkpoint: storage and liveness share selected cleanup
  suspension occurrences for scope exit and replacement. Publish exact finalizer
  signatures from the owning analysis, retain evaluated assignment inputs, and
  reuse existing shielded awaited-frame composition for direct async finalizers.
  Keep owner guards live until finalization finishes. Source, imported, moved,
  discarded and replacement owners pass MIR checks, and a native artifact test
  emits admitted frame composition. Shared receiver conversion and completed
  Result collection replace duplicate implementations.

## Remaining implementation and acceptance

| Obligation | Owner and required work |
| --- | --- |
| Ordinary async cleanup | Shared lifecycle expansion and protected source continuations. Replace generated immediate-call assumption with admitted same-run completion under the cleanup shield. Retain parent progress, pending outcomes and dependencies. |
| Inactive Future captures | Existing frame descriptor and abandonment entry. Never enter an inactive body, and resolve accepted captures before retaining a terminal representation. Future selection currently rejects quiescence explicitly. |
| Retained error quiescence | Shared selected expansion. Resolve child runs while retaining payload and terminal handles. Generator quiescence remains missing. Buffer prefix selection is now covered. |
| Abandoned destruction | Generated destruction and source destructor receiver cleanup must not replay implicit graceful finalizers. Preserve ordinary destruction behavior. |
| Physical incident ownership | Move pre-admitted payload backing, provider and attachment dependencies into the existing report chain. Discharge cannot release transferred storage. Do not use the static consumer's immediate disposal or separate activation. |
| Run ownership and disposal | Retain ordered typed entries across direct await, suspension and nested independent roots. Destroy and release exactly once, preserving independent reporting/destruction/release failures and cancellation. |
| Production acceptance | Source/imported contracts, normal/panic/cancel exits, temporary/partial/nested owners, inactive captures, large/aligned errors, denied allocation, exact release and link maps. BRA-561 driver tests are not proof of these consumers. |

The enabled `scoped_selected_calls_emit_native_artifacts` failure currently reaches
missing CleanupIncidentTransfer. Keep it required through BRA-505/BRA-619. Do not
publish a production transfer producer until quiescence and abandoned destruction
satisfy the retained-error contract.

## Integration facts

Source cleanup emits opaque Cleanup/Destroy operations. Synthetic lifecycle calls
currently assume immediate results. Selected scoped exits already use
lower_lifecycle_call, lower_awaited_frame, cleanup shields and CleanupOutcome.
Reuse these execution mechanisms without repeating checked receiver proofs.

CheckedAsync records explicit awaits, yields, scoped calls and selected implicit
finalizer suspensions. Storage snapshots precede cleanup transitions. Liveness
retains pending exits and evaluated assignment inputs. Direct async finalization
uses the existing enclosing frame and keeps its owner guard live through awaited
mutation. Protected frames retain all ordinary MIR storage.

Temporary and pending-result cleanup now carry their cleanup occurrence through the shared
abnormal suffix. Completion cleanup reuses the same broadcast implementation, including
while its finalizer is suspended. The owning storage analysis publishes unreachable
suspensions explicitly. Missing snapshots assert instead of becoming source diagnostics.
Selected input finalizers cover failed defaults and failures in later evaluated inputs,
with retained borrowed dependencies. Bodies without asynchronous finalizers skip this
selection. The diagnostic's retired failure category and duplicate completion broadcast
have been removed.

Unconditional pure/total successful finalizer proofs now share the existing completion
publication with occurrence-specific storage proofs. Temporary inputs and unconsumed
results consume the type-wide proof directly. Conditional completion still requires the
exact storage/exit proof. Input retention uses checked result dependencies, while implicit
receiver loans remain live through invocation and expire before assignment cleanup.
Nested argument blocks use the existing bound-tree walker.

The audit removes missing-suspension diagnostics, lifecycle-selection error wrappers and
invalid compiler-owned key diagnostics. Missing internal semantics panic with context.
Heap operation recognition is now infallible. No recoverable internal failure was added.

Next: complete retained-error quiescence and abandoned destruction through the shared
selected expansion. Synthetic helpers still assume immediate
ordinary lifecycle calls. BRA-506 owns broader represented-owner families. Arbitrary
retained error ownership and inactive captures remain required in BRA-505. No implementation
choice for that remaining expansion has been committed.

## Verification and review

The initial broad pending-input run passed 1,067 of 1,071 tests. Its four failures exposed
receiver-loan over-retention, missing invocation loan retention, and assertions that did
not distinguish entry abandonment or explicit post-finalizer destruction. Those are fixed.
All 488 affected checker, lowering, source/imported completion and native artifact tests
pass (`target/bra-505-completion-audit.log`). All 703 expanded unit-query and lifecycle tests
pass after the fail-loud selection correction (`target/bra-505-pending-final.log`). The final
216 storage, source/imported lifecycle, heap realization and production bootstrap checks
pass (`target/bra-505-final-audit.log`). Rustfmt, mutating `cargo xtask style`, explicit
`cargo xtask format` and whitespace checks pass (`target/bra-505-final-format.log`). No native
runtime execution is claimed by artifact-emission tests. This checkpoint adds
163 net production lines and 297 net test lines. It removes the second suspension-publication
traversal and unreachable set. The nested message rules now match the root fail-loud
contract and no longer instruct agents to render compiler invariants as diagnostics.
Commit and push this checkpoint before starting the remaining expansion work.

Regression cases cover failed defaults, direct and block-valued later inputs, retained
borrowed inputs, pending result cancellation broadcast, unconditional versus conditional,
effectful and error-returning finalizers, receiver mutation, missing-state assertions and
impossible lifecycle selection. The audit reuses selected operation suspension identities,
checked value dependency propagation, the bound-tree walker and existing completion proof.

The implicit-cleanup batch passes all 540 affected bound-tree, checker, lowering,
source/imported lifecycle, realization and native artifact tests. Evidence:
`target/bra-505-cleanup-composition.log`. The replacement regression proves the
old stored owner's loan and incoming evaluated owner's loan both survive cleanup.
The selected native test emits LLVM artifacts; it does not prove runtime execution.
The audit reuses receiver conversion, cleanup-type publication, pending-input
retention, task-operation publication and Result completion handling. It removes
the replaced helpers and impossible recoverable receiver error. The final 16 async
analysis tests pass after these refactors. Rustfmt, mutating `cargo xtask style`,
explicit `cargo xtask format` and whitespace checks pass. This checkpoint adds
360 net production lines and 556 net test lines. Further temporary/result regression
tests are being added separately; their acceptance remains unfinished.


The result correction passes all 72 focused lowering, lifecycle, propagation and
realization tests. Its regression requires valid MIR, one consuming error projection,
no payload destruction, no result inspection on panic/cancellation, and retained
completion omissions. Existing scoped-call regressions now require valid MIR.
The temporary validator probe was restored. Final evidence is in
`target/bra-505-completion-values.log`. Rustfmt, mutating style, explicit Bray formatting and the maintainability audit pass. This correction adds 198 net production lines and 169 net test lines.


The broad normal-legality run passed 1,732/1,734 tests. The static constructor
mapping failure was fixed and its affected 18 tests pass. The other failure is the
required native incident transfer above. Combined compiler/diagnostic/tooling
coverage passed after those bounded fixes. The compiler-defect diagnostic quality
regression and diagnostic readiness pass.

The selected-action batch passed all 294 bound-tree, MIR, lowering, realization
and static tests. Its observer rejects generated-body demand during admission.
The buffer analysis/expansion batch passed all 381 checker, lowering and affected
realization tests in 5.446 seconds after compilation. The source/imported regression
also proves the scope emits buffer task broadcast before generated prefix traversal.
Authoritative element recognition now has one implementation shared by checking and
realization. The common reverse-loop tests cover empty/nonempty prefixes and pending values.

All completed checkpoints passed rustfmt, mutating style and explicit Bray format.
The final buffer rustfmt, mutating style, explicit Bray formatting and whitespace checks pass. Logs use the shared target
`C:/Users/anton/source/repos/bray/target/bra-505-*`. Reuse that target, Cargo build
storage and its LLVM toolchain. Do not repeat unchanged broad tests at checkpoints.

After complete issue acceptance: audit all changed behavior categories, commit and
push, dispatch a fresh independent reviewer on PR #612, verify its published review
artifacts, fix findings and push. Then mark ready/In Review and continue BRA-619.
Do not review an unfinished checkpoint. PR descriptions use imperative Change
Summary bullets and Related Issues links displayed as IDs only. Verification and
agent workflow belong in attributed PR comments, not descriptions.

The buffer audit covered action selection, cleanup phase propagation, initialized
prefix traversal, authoritative declaration recognition and all query consumers.
It reuses the array loop, moves recognition out of codegen into its shared owner,
and removes the replaced recognition implementation. Invalid recognized generic
shapes now expose an internal invariant failure instead of a recoverable product
error. The change adds about 155 net production lines and 171 net test lines.
