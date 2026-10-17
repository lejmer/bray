# BRA-505 retained incident prerequisite

## Active agreement and baseline

Complete BRA-505, then BRA-619, with one reviewed stacked PR per issue. BRA-561
is implemented and independently reviewed in PR #611 at `7a254388`. The current
branch is `feature/bra-505-admitted-ordinary-finalization`, based on that head.
Only the BRA-505 checkpoint `b3022d87` was transplanted, as `eb91736c`. The old
branches remain recovery history. BRA-619 admission and rollback are preserved
at `6bbcb4a3` on `feature/bra-619-complete-selected-scope-native-cleanup`.

The active contract includes ordinary async and fallible finalization, completed
whole-value omission, synchronous-scope legality, inactive Future cleanup, retained
error quiescence and disposal, admitted backing transfer and run-owned incidents.
The old prerequisite slice below is not the full issue.

No new language features, syntax, parallel cleanup walker, post-failure allocation,
or downstream authority proof. Reuse selected lifecycle expansion, existing
cleanup outcomes and BRA-561 same-run activation. Remove replaced paths when
switching consumers. A helper's organizational role does not justify a separate
reservation. Measure production and test growth separately. Reconsider a design
that duplicates structural traversal or introduces hidden blocking execution.

## Current acceptance map

| Obligation | Owning mechanism | Evidence or work remaining |
| --- | --- | --- |
| Completed whole finalizer | Execution candidate receiver evidence and verified pure/total result proof | Implemented shared source/imported completion for sync and async finalizers, including guarded Ok(unit). Mutation invalidation and missing imported evidence are covered. |
| Normal ownership end | Checked cleanup transitions | Reject unresolved possibly fallible finalizers and async obligations in synchronous scopes. Preserve represented children after whole-value completion. |
| Async ordinary cleanup | Shared lifecycle expansion, source continuations, protected frames | Replace immediate-call assumption with admitted same-run completion under the cleanup shield. |
| Retained error ownership | Quiescence expansion, concrete outgoing backing, report chain | Connect production transfer only after child quiescence and abandoned destruction are complete. |
| Inactive captures and partial owners | Existing frame state and initialized-part cleanup | Never execute an inactive body, never lose accepted captures or initialized prefixes. |
| Composition and cost | Source/imported MIR and generated native products | Cover normal/abnormal exits, ordered incidents, large/aligned errors, denied allocation, exact release and link maps. |

BRA-561 driver tests establish activation behavior, not these unconnected consumers.
The original BRA-619 native transfer regression remains enabled and is required
when completing its consumer PR. Run tests in coherent batches, then rustfmt,
`cargo xtask style`, `cargo xtask format`, audit and independent PR review.

## Completion proof checkpoint

The async body proof now distinguishes pre-entry cancellation from cancellation
inside the body. Only the former is excluded from execution guarantees. Ownership
and unwind analysis retain both. Empty pure/total async finalizers can therefore
be discharged without creating a Future. Await and panic still invalidate the
corresponding body guarantees.

Source and generated lifecycle selection share one completion predicate. It
checks the exact applicable entry domain, including a guarded Ok(unit) proof that
refines an unconditional total guarantee. Export carries inferred result tags as
existing certified postconditions. Imports require that evidence and retain its
dependencies. There is no new language syntax, interface field or ABI version.

The focused execution, export and lifecycle realization batch passed 153 tests.
This proves completion selection, not ordinary async cleanup execution or retained
incident transfer. Those remain outstanding below. The next change will publish
normal-exit legality with the same checked receiver evidence and make lowering
consume that result.

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

## Historical dependency closure correction

The shared slice cannot yet be completed independently of BRA-561. In
`native/run.rs::NativeRun::compose`, composition admits six records, calls
`NativeFrame::try_new`, and allocates a boxed activation. `NativeFrame::try_new`
also allocates completion backing. Reusing that path after failure would violate
post-admission allocation denial. The generated finalizer helper in
`synthetic/lifecycle/calls.rs::push_lifecycle_call` uses an immediate call result
rather than driving an asynchronous finalizer. The static path has a separate
activation bridge, not the missing ordinary admitted activation.

The user approved BRA-561, then BRA-505, then BRA-619. BRA-561 is now complete
through independent review, as recorded in the active baseline above.

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