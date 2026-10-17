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
- Current buffer correction: cancellation and quiescence visit the recognized
  RawBuffer initialized prefix with the existing reverse cleanup loop. Lifecycle
  needs and source cleanup phases include element obligations. Preserve count, allocation and elements for
  later destruction. Source and imported declarations exercise the same selection.

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

CheckedAsync currently records explicit awaits, yields and scoped calls. Existing
storage/liveness transitions retain pending cleanup dependencies. Source cleanup
owners already have initialization guards, which clear before resolution and
prevent repeated broadcast. Protected frames retain all ordinary MIR storage.
Synthetic helpers have no protected-frame continuation yet. No alternative
continuation implementation has been committed.

## Verification and review

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
