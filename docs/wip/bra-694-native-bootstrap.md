# BRA-694 working record

Track [BRA-694](https://linear.app/bray-lang/issue/BRA-694/complete-native-bootstrap-owner-and-registration-contract-checking)
in [draft PR 717](https://github.com/lejmer/bray/pull/717). Follow
`StoragePlan` and `StorageFlowState` through the
[architecture map](bra-694-architecture.md).

## Refactor contract

The existing CFG determines evaluation order and phase boundaries. The plan
owns typed storage accesses, uses, definitions, and cleanup shapes. Backward
liveness reads those structural facts. Forward flow supplies the actual
evaluated operands, addresses, access permissions, and observation transitions.
The checked flow publishes them by CFG operation and reached outcome. Use the
same publication for proof, summaries, and lowering. Lowering consumes the
decisions needed for code generation.

| Boundary | Producer and consumers | Contract |
| --- | --- | --- |
| Input evaluation | Checked access routes and the paired current state select the value. Admission, effects, ownership, and proofs read that capture. | Preserve the evaluated value through argument conversion, defaults, parent-to-child frame capture, later operand evaluation, and replacement of its former slot. A route that crosses a descriptor must read the descriptor it follows. |
| Admission | The incoming composite state supplies actual arguments after the runtime guard. Consumption, requirements, terminal access checks, and certification use this admission. | Check requirements and conflicts before the operation invalidates their subject. Distinguish checked initialization from other requirements. Acknowledgement permits execution at its declared boundary. Certification still requires evidence. |
| Effects | The selected outcome and checked route resolve against current memory or an admitted snapshot. Physical memory and guarantees consume the same transition. | Keep actual addresses and conservative footprints separate. An escape contributes reachability. A write changes contents. Descriptor replacement, atomic payload updates, consumption, and backing retirement each revoke their own support. A finite bound requires complete captured dependencies. |
| Private output | The reached row produces a private result, writes, and transferred duties. Cleanup and the normal completion read those values. | Keep the result available to cleanup. Publish only after required cleanup. A whole-value move transfers its carried information, including opaque generic storage. A former slot retains no usable ownership. |
| Normal completion | Publish the surviving result, actual current writes, references, witness carriers, and guarantees from the same rows. | Retain value identity and guard when the data or layout is abstract. Keep code-generation activity decisions tied to that publication. Always distinguish explicit absence from unknown contents. |
| Cleanup and failure | A reached action uses its receiver, actual activity, and retained prefix. Ordered continuations retain their failure, effects, reports, and next action. | One owner owes each duty on each feasible path. Only performed effects and writes survive a failure. A conditional or possible address can bound invalidation. It cannot establish an exact cleanup target. |
| Deferred execution | Formation captures actual arguments and moves duties into the frame. Activation retains the resolved rows and cleanup. Observation and cancellation consume the selected producer and its support. | Pointees can change before activation. Captured descriptors stay the same. Keep competing producers separate even when they use the same row number. An inactive frame cleans up its capture. A running frame also cleans up obligations already acquired. |
| Joins and summaries | Existing current-value trees and fixed points merge reached outcomes. Export serializes the callable-observable terms, effects, and duties together. Import remaps identities and uses the same instantiator. | Possible information grows conservatively. Guarantees must hold on every path covered by their guard. Keep allocation recency and stable loop identities. An opaque region or shared source definition cannot establish unique allocation or authority. |

Scope retirement uses the prepared storage identities and the actual descriptor,
backing, and borrow ancestry. Ending a descriptor slot can leave the referent
alive. Retiring the backing ends all dependent access. Live stored loans,
cleanup, and reports can retain support. A raw address captures address bits
and bounds without forming another language loan. Escape checks read the
carried support of the published value even when its address is opaque.

Each transition determines which observations survive. Preserve independently
live sibling contents, pointers, and owners. The witness ledger retains the
logical subject, owning path, reached address support, and any associated
allocation. A live shared view can sustain an already supported clause.
Only ownership can transfer its cleanup duty. Retire support at the same
reached write or lifetime boundary in public proof and admitted frames.

Keep root, product-static, and thread-static identities explicit. A possible
target region cannot turn into an exact address, initialized cell, live
allocation, or ownership. Keep structured diagnostics when the program lacks
evidence. Impossible internal states fail at the violated contract.

### Required removals

- Shared value bindings replace separate reference and stored-value forms.
  Guarded outcome rows share ownership, reports, effects, references, and
  witnesses. The interface must encode these connected rows.
- Resolve actual input captures, effect targets, assignment destinations, and
  cleanup receivers at their owning phase. Remove later operand and address
  lookup from effect binding, output binding, and certification. Separate
  formal route selection from path traversal of the captured descriptor.
- Reuse checked per-operation mutation and retirement records. Remove source
  occurrence indexes that independently classify phases, inferred survival
  from declaration purity alone, and repeated settled-block transfers.
- Compare production and test code separately. Retain additional code when it
  represents a required invariant across phases. Remove superseded branches
  and conversion layers without using an arbitrary growth ceiling.

## Verified checkpoint

Candidate `captured-inline-witnesses` passed all 1,109 compilation controls,
the selected and unrelated nullable extraction cases, and 938 symbols,
bound-tree, and checker tests. The old call preparation,
expression target, and independent normal storage promise have no remaining
callers. The [architecture map](bra-694-architecture.md) records the resulting
contracts.

On Windows x86-64 the candidate checked and lowered 21 of 25 selected
bodies. `commit_start`, `worker.acquire_ready`, and `acquire_worker_ready`
pass in addition to the 18 bodies verified at `244c673a8`. The four
remaining bodies are `dispatch_ready`, `dispatch_task`, `start_frame`,
and `retire_driver`. The traced check completed in 138.6 s.

The production decimal loop passes. The optimized Windows standard library
took 28.6 s to plan, 26.6 s to emit, and 62.7 s wall time with 24 workers
on the previous `live-enclosing-referent` candidate. Result-dependency
solving used 151.5 worker seconds against a 60.0 reference. Cleanup
completion used 22.7 worker seconds against 0.93.
An earlier cleanup profile attributed 2.5 s to proof invalidation and 0.06 s
to both state copies across 3,354 actions.

## Active batch

| Affected transition | Reached evidence and expected behavior |
| --- | --- |
| Dispatch recovery | Escape behind `Current.pointer` now starts at the reached raw region. Some `dispatch_task` and wait rows still retain an escaped inline `Current` and unbounded retirement. The direct task body exposes an earlier obligation. Trace the callee's actual captures and cleanup prefixes before the later `context(selected)` call. Real raw mutation and retirement must still revoke proof. |
| Provider installation | `create_native_provider` publishes a nullable provider within `Ok`. The selected payload now carries its proof through the consumed match and `install_binding` to the absence-or-owned requirement. Source and imported controls reject a replaced payload. |
| Start and rollback | The guarded `Ok` payload of `prepare_start` enters the scope with the token and execution witnesses. Direct `commit_start` checks and lowers. At its call in `start_frame`, the later selected `Current` obligation and rollback still fail. Compare the same selected view across lane selection, `register_task`, and normal and abnormal completion. Registration presence remains an independent condition. |
| Driver retirement | Some `retire_driver_resources` rows publish an untyped unbounded retirement, and the later acquisition observation loses its marker. The trace shows the same broad effect first appearing when reached cleanup cannot bind a prepared receiver backing route, including nested owned buffers. Trace the checked receiver, backing route, and actual captured descriptor at that boundary. Retirement must revoke precisely the retired backing without preserving old generations. |
| Convergence | The array write selects element zero before the first join. The next evaluations agree on an unresolved index. Publish the enclosing logical observation when the physical write names only a region. Keep independently reached exact selectors and disjoint fields. Check the production decimal body and the existing indexed siblings and loop controls. |
| Planning | Consecutive unbounded effects with identical physical scope repeatedly weaken the same memory. Apply the physical transition once and keep typed effects for proof framing, summaries, and diagnostics. Leave precise writes and retirements ordered. Empty pointee preservation entries carry no evidence and need no mask allocation. Measure the unchanged native query and emission boundaries with the completed batch. |

Reuse the same optimized binary for bootstrap and the fixed inventory. The
current investigation uses actual checked captures and publication records.
A finite alias bound cannot prove one exact target, initialization, ownership,
or a performed retirement. Reconsider a storage change if it equates a live
owner with an unrelated or historical carrier.

The next batch also addresses repeated proof preparation in cleanup
certification. The completed build spent 22.0 worker seconds in cleanup
completion against 0.93 in the reference. Source property selection,
imported obligation selection, and finalizer success verification read the
admitted arguments, assumptions, and implementation witnesses. Their common
instantiator currently walks the complete entry, including normal outputs,
failure rows, extraction records, and carried authority. Give certification
a borrowed view of its actual proof inputs. Keep full captured-value
remapping for storage and deferred flow. The new view preserves the existing
controls, but cleanup completion is still at the same cost. Inspect repeated
proof preparation within the selected invocation and cleanup phases.

For entry witnesses, an ordinary conjunct in a checked trusted requirement
establishes its fact on entry. A trusted predicate or unresolved disjunction
does not establish an ordinary result by itself. The entry ledger must bind
forwarded contract views to the same selected referent that admission and
extraction read. `commit_start` returns the admitted nullable token with its
original identity, yet neither extraction nor the next call receives its
producer witness. Normalize checked entry reference origins when they
identify one complete referent. Register the resulting clause with that
carrier before any write. Keep unresolved origins conditional. A move may
carry the selected contents and their proof, while the emptied source and
an unrelated optional slot gain no permission.

## Acceptance

1. Finish replacing old paths and search for their callers and equivalents.
   Check value identity, retirement, and guarded transport as complete paths.
   Include round-trip import and source behavior, defaults, and cleanup.
2. Run the fixed inventory of 1,709 cases against the candidate. The last
   complete inventory passed 1,694 and failed 15, with no missing cases.
   Use the same audited test inventory and reuse only exact-binary evidence.
3. Check and lower the actual bootstrap owners, registration, and dispatch
   on Windows and Linux for both x86-64 and AArch64. Run native toolchain
   steps outside the sandbox. BRA-695 and BRA-696 cover execution and shutdown.
4. Measure an optimized build on 24 cores without tracing. Planning should
   take 17–25 s. The full build must produce a finished binary within 60 s.
5. Remove temporary probes and stale artifacts. Complete the repository
   audit, mutating style check, formatting, and independent debug review.
   Push verified checkpoints to the existing draft PR.
