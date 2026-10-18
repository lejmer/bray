# BRA-505 ordinary finalization and retained incidents

## Resume after every context compaction

Re-read AGENTS.md, its applicable coding and crate rules, this section and the active
agreement before resuming after every context compaction or application restart.
The user explicitly requires this repeated reading. These requirements were reaffirmed
2026-10-18. A summary of the rules is not a substitute for reading them.

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

Current correction: replace the competing compiler contracts identified below without
adding runtime ownership machinery. The earlier proposed extra Rust owner-waiter slot was withdrawn. It is
not an accepted solution. Do not revive it after compaction.

Confirmed implementation conflicts, not new language requirements:

- The pushed code retains lifecycle completion type and execution separately. The current
  selected-invocation checkpoint replaces those fields with the existing BoundCallResult and uses its
  selection constructor for ordinary calls too. Source lowering, outgoing admission and
  static signatures consume it. Generated push_lifecycle_call still forces Immediate.
  Removing that override requires completing the shared continuation path, not merely
  changing the field type. Do not claim the representation batch completes this repair.
- The execution-kind checkpoint removes `MirUnitKind::GeneratedLifecycle`. Generated
  provenance remains in `MirSourceOrigin` and `MirUnitKey`, while bodies use the ordinary
  execution categories. Generated bodies still select synchronous execution until shared
  resumable cleanup is connected. This removal alone does not complete that connection.
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

## Current checkpoint and implementation boundary

Published baseline: `61a483957` on PR #612. Earlier evidence remains in Git and the PR:
`6306d9f33` establishes the shared enclosing collection, `1d15b68c9` fixes native boundary
and report ownership, and `61a483957` adds source allocation-denial acceptance. The PR is
still draft. Independent review 5560583230 preceded active human architectural review.
Follow the workflow's human-review rule. Do not launch another agent review unless asked.

Structural cleanup borrows `MirCleanupCollection` state/report places. It does not create
another shield or independent report group. Source invocations keep a separate incoming
failure destination. Collection states are Normal, Incidents, Cancelled and Panicked.
Only Normal has no initialized report. Ordinary source call outcomes remain completed,
panicked or cancelled. Headless completed incident chains belong to native callbacks.
Typed incidents consume admitted payload backing and retain the provider independently
of owner discharge. Fixed C platform-service boundaries report and terminate on failures
that have no caller outcome destination. Ordinary Bray calls forward their existing outcome.

Current continuation work extracts `MirFrameRetention` from `MirFrameState`. Executable
states and logical cleanup share broadcast entry, affinity, operation requirements,
completion-cleanup identity and initialized storage. Source await and yield use the same
producer. Failed-finalizer quiescence reuses its awaited state. Completed-result cleanup
carries its checked retention through generic executable imports. Existing MIR reconstruction,
optimization, storage walking and import validation preserve it. The wire encoder/decoder
share one retention contract. This is transport for the planned expansion consumer, not
completion of generated resumable cleanup.

The focused tests pass 44 MIR, 39 lowering and 191 package-interface checks. Source coverage
passes all 25 lifecycle and eight async lowering cases, plus imported completed-result
specialization. The replacement test now checks the shared collection's two failure exits
and exact report transfer, replacing its stale helper-local CheckCallOutcome expectation.
MIR inspection passes all 17 focused checks. The audit also verifies that omitting a cleanup
operation removes its exclusively retained callback and storage. Style, Rust formatting, Bray
formatting and diff whitespace checks pass. No native acceptance is claimed for this transport batch.
Logs in the shared target: bra-505-retention-tests.log, bra-505-retention-verified.log and
bra-505-retention-replacement.log. The latter is the passing rerun of that corrected assertion.

Native baseline: full runtime-artifact smoke passes development and release, including
ordinary finalization, started tasks, bootstrap allocation/lifetime/reentry checks and
activation depths 1, 32 and 2048 with constant measured native callback stack usage.
The ordinary-finalization fixture denies Bray backing allocation after owner admission and
observes zero attempts through suspension, retained typed-error transfer, owner destruction
and report disposal. It preserves a 2 KiB-aligned payload, skips its finalizer and destroys
it exactly once. This measures the Bray allocator boundary, not every process allocator.
The panic has an empty message because a nonempty source message snapshots bytes before
raising the requested panic, under the existing catchable allocation-failure contract.
No runtime implementation was changed to evade that test. See bra-505-allocation-native.log.
This evidence does not cover retained errors with child work or generated resumable traversal.

## Next implementation

Use the shared selected-action expander to lower concrete structural cleanup into its
owning continuation before layout and reachability. Remove the generated Immediate override
only when its actual continuation is connected. Keep the scalar call splicer's existing
eligibility checks. Do not allocate a frame merely because compiler code uses a helper.
Retain pending values, array counters, borrowed storage projections and the enclosing
collection across introduced suspension. Physical SSA retention belongs to MIR construction,
not a downstream semantic reproof. Avoid copying/rebuilding the whole unit per cleanup site.

Audit all cleanup producers while connecting expansion, including ordinary cleanup with no
preexisting collection, scoped incidents, input temporaries, completed results and imported
generic bodies. A missing checked suspension must not become guessed execution requirements.
If an error's child-work obligation is missing from cleanup suspension analysis, repair that
owning analysis. Do not conceal it by using the previous frame state or a blocking root.

## Remaining implementation and acceptance

| Obligation | Owner and required work |
| --- | --- |
| Ordinary async cleanup | Shared lifecycle expansion and protected source continuations. Replace generated immediate-call assumptions with admitted same-run completion under the cleanup shield. Retain parent progress, pending outcomes and dependencies. |
| Inactive Future captures | Existing frame descriptor and abandonment entry. Never enter an inactive body. Resolve accepted captures before retaining a terminal representation. Future selection currently rejects quiescence explicitly. |
| Retained error quiescence | Shared selected expansion. Resolve child runs while retaining payload and terminal handles. General generator work remains owned by BRA-484/BRA-506. Buffer prefix selection is covered. |
| Abandoned destruction | Generated destruction and source destructor receiver cleanup must not replay implicit graceful finalizers. Explicit operations keep ordinary checked semantics. |
| Physical incident ownership | Verify the implemented transfer of pre-admitted payload backing, provider and attachment dependencies into the existing report chain. Discharge cannot release transferred storage. Do not use immediate disposal or a separate activation. |
| Run ownership and disposal | Retain ordered typed entries across direct await, suspension and nested independent roots. Destroy and release exactly once, preserving reporting/destruction/release failures and cancellation. |
| Production acceptance | Source/imported contracts, normal/panic/cancel exits, temporary/partial/nested owners, inactive captures, large/aligned errors, denied allocation, exact release and link maps. BRA-561 driver tests are not proof of these consumers. |

Keep `scoped_selected_calls_emit_native_artifacts` required through BRA-505/BRA-619.
Incident transfer is now implemented, but full quiescence and abandoned destruction
acceptance must establish its contract before the PR becomes ready.

## Continuation proof still to implement

A source-owned Future is inactive: await and start consume it. Its native composition path
currently consumes and retires that token, and frame failure cleanup destroys captures.
Therefore calling await_frame with no completion destination cannot implement retained
error quiescence. That would destroy captured owners early and leave a stale token inside E.
See lowering/asynchronous.rs, continuation.rs and activation/run/step.bray and retirement.bray.
This is a concrete ownership mismatch, not a request for new syntax or a Rust frame.

Keep quiescence distinct from destruction using the existing activation and frame ownership.
A quiesced Future must retain its capture storage and descriptor until synchronous abandoned
destruction consumes it. Ordinary await/unused-Future cleanup must still retire exactly once.
Synchronous destructors cannot await or start a Future (asynchronous/check.rs owns the start
restriction), and they cannot let receiver-derived ownership escape. Do not use field privacy
or a fabricated valid token as a substitute for these contracts.

Source cleanup already emits array loops into its enclosing frame and materializes counters
in MIR storage. Concrete generic cleanup expansion must retain that same enclosing progress,
collection and checked lane/guard information before layout. Do not relax the scalar inliner
or allocate an activation for each organizational helper. Recursive or erased ownership must
use its already-admitted child allowance. This continuation work has not been implemented,
and the remaining table above remains the acceptance boundary.

## Resuming efficiently

The tracked stack plan remains in runtime-stack-linear-execution.md and its semantic-proof,
rule-coverage and coverage-inventory companion records. Preserve BRA-619-remaining-work.md.

No new runtime frame or source feature is authorized. The generated lifecycle call still
forces Immediate twice in synthetic/lifecycle/calls.rs, and generated lifecycle/body.rs
still selects Synchronous. Checker lifecycle selection explicitly rejects Future quiescence
pending admitted capture-cleanup activation. Resolve these through existing shared selection,
continuation and Bray activation. Recursive/erased owners and arrays prevent assuming every
graph can simply be inlined. Do not relax the scalar MIR splicer's eligibility checks.

Terminal analysis distinguishes possible introduction or reactivation of graceful ownership
from scalar mutation. It covers aliases, dependencies and abnormal outcomes, and imported
consumers reuse the established result. Resource operations require complete-carrier authority,
not reconstructed fields. Native resource acquisition ordering remains part of BRA-505.
The broader scalar-resource authority audit belongs to open BRA-508. BRA-625 owns the separate
general compiler-defect diagnostic mechanism. Do not reopen closed issues or duplicate those
subsystems. Current lifecycle invariant failures remain BRA-505 work.

Reuse C:/Users/anton/source/repos/bray/target for CARGO_TARGET_DIR, with its cargo subdirectory
as CARGO_BUILD_BUILD_DIR. BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX both point to
C:/Users/anton/source/repos/bray/target/toolchains/llvm/active. Never clean shared artifacts.
Worktree writes and native workflows require escalation outside the sandbox.

Native smoke uses the fresh target/release/xtask.exe runtime-artifact smoke-test with process-local
RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc. Rust 1.99 emits incompatible LLVM 23 bitcode while
this checkout uses LLVM 22.1.8. Do not change the global toolchain or rebuild the compiler just
to select the provider producer. Native TMP/TEMP use target/native/bra-505. Keep logs and temporary
products inside the repository target. No full native rerun until its known blocker changes.

Check the active task-owned process before launching another build. Use focused test filters,
not the broad unit:: filter. Avoid duplicate release xtask rebuilds. Find files with rg and read bounded
relevant sections without truncation. PowerShell does not expand directory wildcards for rg.
Preserve exact leaf failures instead of repeatedly running expensive workflows.
