# BRA-505 ordinary finalization and retained incidents

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
Remove this temporary record after BRA-505 acceptance and independent review are complete.
Carry only still-needed inputs into the BRA-619 record. No handoff should survive its stack.

No new language features, syntax, parallel cleanup walker, post-failure allocation,
or downstream authority proof. Reuse selected lifecycle expansion, existing
cleanup outcomes and BRA-561 same-run activation. Remove replaced paths when
switching consumers. A helper's organizational role does not justify a separate
reservation. Measure production and test growth separately. Reconsider a design
that duplicates structural traversal or introduces hidden blocking execution.

## Pull request description contract

Use a lower-case Conventional Commits title without a parenthesized scope.
Descriptions contain `## Change Summary` with imperative summary items and
`## Related Issues` with links displayed as issue IDs only. Keep verification,
test steps and agent workflow details in comments or this tracked record.

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

## Normal ownership-end checkpoint

The checker now records represented finalizers and distinguishes normal from
abnormal cleanup paths. One immutable cleanup publication certifies completion
and reports unresolved normal finalization or asynchronous cleanup in synchronous
scopes. Diagnostics and lowering consume that publication. Lowering does not
repeat the proof. Omission still requires pure, total, successful execution.
Successful verified postconditions also establish infallibility without total,
while retaining invocation. Explicit calls and implicit finalizers share the
existing normalized completion-contract producer.

The pre-repair broad compilation run completed 1,734 tests in 1,007.562 seconds:
1,732 passed, two failed. The last caught-input storage stress case accounted for
727.067 seconds and passed. Do not repeat this unchanged broad run for every
checkpoint. The separate diagnostic JSON regression passed.

- `scoped_selected_calls_emit_native_artifacts` still reaches the planned missing
  `CleanupIncidentTransfer` integration. Keep it enabled through BRA-619 acceptance.
- `library_preserves_main_thread_cleanup_without_selecting_a_runtime` exposed a
  missing direct dependency on an admitted static finalizer's frame constructor.
  The current repair preserves concrete outgoing callees until static dependency
  collection, then maps them to existing codegen requirements. The audit also
  found discharge operations using those frame descriptors without declaring
  their constructor dependencies. Admission and discharge now retain the same
  required definitions. The existing native planning regression passes.

The combined batch passed 339 of 340 tests before the final discharge fix. It
covers source/imported completion, guarded successful results, postcondition-only
infallibility, mutation, moves, wrappers, partial and discarded owners, retained
child finalization, static mappings, diagnostic output and query profiling. After
the discharge fix, all 18 affected native planning and outgoing-ownership tests
passed. The existing compiler-defect diagnostic lacked readiness coverage. Its
producer now has a passing quality/source-context regression, and diagnostic
readiness passes with the freshly built xtask. Rust formatting, the mutating style
command and its Bray formatting stage pass. The explicit cargo format/readiness
sequence is still finishing its redundant release rebuild. Ordinary async
activation, inactive abandonment and retained incident integration remain
outstanding in the acceptance map.

The maintainability audit searched the changed behavior categories across the
repository: normal cleanup classification, finalizer proof and postconditions,
query publication and snapshot reuse, structured diagnostics, concrete outgoing
requirements and native frame dependencies. Normal classification and requirement
construction each have one shared implementation. Explicit calls and implicit
finalizers share normalized completion contracts. The old pure/total wrapper was
removed after its logic moved into the three-way finalizer disposition. Static
requirements retain existing concrete callees until dependency collection rather
than reconstructing them from a key. No language syntax, ABI version, cleanup
engine or runtime allocation was added.

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

## Remaining implementation

Ordinary generated finalizer calls still assume synchronous results. Connect their
asynchronous completion to the admitted same-run activation, retaining the cleanup
shield and all source continuations. Planning must use selected cleanup semantics
rather than generate MIR solely to discover admission needs.

Quiescence exists in checked selection, MIR, serialization, realization and LLVM
dispatch, but no production incident producer uses it yet. Erased futures,
generators and raw-buffer initialized prefixes need complete retained-child
resolution. Source destructor field cleanup must not replay abandoned graceful
finalizers. Connect concrete backing transfer and the existing report chain only
after those obligations are handled. Do not use the older static bridge's separate
activation or immediate disposal as the implementation of ordinary cleanup.

After the production acceptance matrix passes, complete the audit, push, independent
PR review and any fixes. Update BRA-619's remaining inputs, then delete this record.

## Checkpoint cost and publication

The normal ownership-end batch adds about 374 net production lines and 445 net
test lines. It adds checked legality and reusable completion proofs, structured
diagnostics and missing constructor dependencies. It removes the old pure/total
wrapper and repeated outgoing requirement construction. Documentation deletion
is recorded separately and is not counted as compiler simplification.

Obsolete handoffs were deleted in `b67ec488`. The six remaining documents are active
inputs only. Their removal is part of stack completion, as recorded in the execution
guide. PR descriptions follow the user's imperative summary and issue-link format.
