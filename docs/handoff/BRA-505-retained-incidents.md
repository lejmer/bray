# BRA-505 retained incident prerequisite

## Agreement and baseline

Implement the shared prerequisite for BRA-619, then finish BRA-619 in a stacked PR.
Start at `5f1141e4`, the shared sequencing document above develop `055f1d4f`.
BRA-619 admission and rollback are preserved and pushed at `6bbcb4a3` on
`feature/bra-619-complete-selected-scope-native-cleanup`.

No new language features, syntax, parallel cleanup walker, post-failure allocation,
or downstream authority proof. The broader BRA-505 activation migration still
requires BRA-561. This slice alone cannot close BRA-505.

## Ownership contract

An initialized error transitions through cancellation broadcast, quiescence,
retained incident ownership, and synchronous destruction. Broadcast visits all
initialized task obligations before any wait. Quiescence resolves child tasks and
inactive captures without running the error's graceful finalizers or destroying
its retained representation. Incident transfer moves concrete payload, backing,
provider and attachment dependencies. Disposal runs destruction once and releases
backing even when a callback panics. Non-panic incidents preserve cancellation.

## Implementation and acceptance map

| Contract | Existing implementation | Required correction and evidence |
| --- | --- | --- |
| Local action selection | checker/lifecycle and synthetic/lifecycle | Add the missing quiescence action to the same selected structural expansion. Task resolution must retain terminal handles. |
| Inactive captures | protected frame descriptor and state cleanup | Resolve initialized captures without executing an inactive source body. Retain terminal state through later destruction. |
| Abandoned destruction | generated destruction and checked destructor receiver cleanup | Do not replay graceful finalization of a retained incident or its represented parts. Preserve ordinary destruction behavior. |
| Physical backing | outgoing admission, storage leases | Transfer a pre-admitted concrete payload region before invocation retirement can release it. No fresh allocation on failure. |
| Run ownership | report records, frame and entry ownership | Retain ordered typed entries across direct await, suspension and nested independent roots. Reuse the existing report chain. |
| Disposal | report/incident and report/consumer | Reuse concrete callbacks and iterative consumption. Preserve independent reporting, destruction and release failures. |
| Dependencies | checked cleanup transitions and exported contracts | Publish retained error obligations in their owning analysis. BRA-619 owns its selected exit-result transfer wiring. |

Tests must cover source and imported contracts, nested and partial represented
owners, task completion/cancellation/panic, inactive captures, large/aligned errors,
allocation denial, exact-once release and root isolation. The original BRA-619
native regression remains a completion requirement of the consumer PR.

## Design limits

Reuse lifecycle selection and structural expansion. Add only missing ownership
states and concrete operations. Do not implement retention through a thread-only
queue, immediate consumption, a fake panic or message, or an additional Rust payload
registry. Production and test growth are measured separately. Reconsider if an
implementation requires another lifecycle traversal or hides unsupported protected
ownership as no work.

## Current evidence

The implementation map confirms that lifecycle resolution currently combines
finalization and destruction. Static incident disposal also selects that combined
helper. Neither proves retained incident quiescence. Future type-wide lifecycle
needs currently report no work despite frame-owned captures, so that existing
interpretation must not suppress quiescence of an erased future.

## Dependency closure correction

The shared slice cannot yet be completed independently of BRA-561. In
`native/run.rs::NativeRun::compose`, composition admits six records, calls
`NativeFrame::try_new`, and allocates a boxed activation. `NativeFrame::try_new`
also allocates completion backing. Reusing that path after failure would violate
post-admission allocation denial. The generated finalizer helper in
`synthetic/lifecycle/calls.rs::push_lifecycle_call` uses an immediate call result
rather than driving an asynchronous finalizer. The static path has a separate
activation bridge, not the missing ordinary admitted activation.

The requested scope decision is therefore BRA-561, then BRA-505, then BRA-619.
That extension is pending user response. No BRA-561 implementation has started.

## Initial implementation checkpoint

Added an internal quiescence operation through checked local action selection,
MIR, helper realization, serialization, inspection and LLVM dispatch. It uses
existing structural expansion and task resolution, retaining terminal handles and
owner credits. It is not connected to a production incident producer yet.

Quiescence needs propagate independently of destruction and cancellation needs.
Erased futures must not be classified as trivial. Their still-missing admitted
capture-cleanup activation fails explicitly at lifecycle selection. Generator
quiescence also remains unimplemented and fails at the existing exhaustive-role
boundary. This checkpoint is not completion evidence for retained incidents.

Remaining work is admitted inactive-capture resolution, abandoned synchronous
destruction, concrete backing transfer, run-owned report retention and consumer
integration. Native and composition acceptance, formatting, audit and review must
precede any issue-completion claim.

## Checkpoint audit

Re-read AGENTS.md and checked every changed production file. Searched lifecycle
selection, helper identity/substitution, MIR operand and storage walkers, validation,
codegen demand/dispatch, executable encoding/decoding and tooling inspection for
all finalization/destruction siblings. Quiescence uses those existing paths rather
than a second visitor or generated-body cache. The executable operation tag is 25,
which is distinct from the host-content tag 24. Production changes add one internal
operation and phase, not source syntax or a new runtime owner.

The represented-owner audit also leaves explicit acceptance work: raw buffers carry
an initialized element prefix that declared-field traversal alone cannot visit.
Their quiescence need and initialized-prefix operation must be connected before a
producer can retain arbitrary errors. Generator and erased-frame work likewise
remain prerequisites, not test exclusions that establish completion. Existing
source-destructor field cleanup can still invoke graceful child finalizers, so
ordinary `Destroy` is not yet an abandoned-incident disposal operation.

No incident producer has been switched to this unfinished path. No PR is ready for
review or merge, and no issue is complete. Independent PR review remains required
after the implementation and acceptance are complete.

## Verification at the checkpoint

The focused lifecycle realization suite passed 47 tests. The full checker, MIR,
lowering and package-interface suites passed 321, 44, 39 and 190 tests respectively.
Their doc tests passed. `cargo +stable fmt --all`, `cargo xtask style`,
`cargo xtask format` and `git diff --check` completed successfully. Structural
style reported existing module-size warnings. No native retained-incident test
has passed because the production transfer remains unconnected.

Rust changes total 270 added and 26 removed lines. Of those, 172 added and 4
removed lines are tests. The remaining 98 added and 22 removed production lines
extend existing lifecycle paths. No new helper framework, runtime allocation or
second structural traversal was added. The new internal operation is necessary
to distinguish retaining a quiescent error from finalizing or destroying it.

Builds reused the shared `C:/Users/anton/source/repos/bray/target` and its existing
LLVM toolchain. No task-owned target or toolchain was created. The checkpoint's
build and test processes have exited.