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

Candidate `checked-continuation-demands` passed all 1,109 compilation controls,
the selected and unrelated nullable extraction cases, and 938 symbols,
bound-tree, and checker tests. The old call preparation,
expression target, and independent normal storage promise have no remaining
callers. The [architecture map](bra-694-architecture.md) records the resulting
contracts.

On Windows x86-64 the candidate checked and lowered 21 of 25 selected
bodies. The remaining bodies are `dispatch_ready`, `dispatch_task`,
`start_frame`, and `retire_driver`. The checked continuation paths reduced
unresolved effect routes in the scheduling source from 31 to two. The
caller traces show which actual descriptors still lose their addresses.

The production decimal loop passes. The optimized Windows standard library
took 28.3 s to plan, 26.0 s to emit, and 61.6 s wall time with 24 workers.
On the previous `live-enclosing-referent` profile, result-dependency
solving used 151.5 worker seconds against a 60.0 reference. Cleanup
completion used 22.7 worker seconds against 0.93.
An earlier cleanup profile attributed 2.5 s to proof invalidation and 0.06 s
to both state copies across 3,354 actions.

## Active batch

| Affected transition | Reached evidence and expected behavior |
| --- | --- |
| Dispatch recovery | `dispatch_task` has the original task borrow when it acquires the dispatch claim. After the earlier worker call, a feasible path produces a symbolic service view where another path reaches the actual execution domain. The later call reads fields through the stored task and services borrows. Trace the reached writes before these reads, including failure prefixes. The selected inline marker and the pointees reached through the other fields have independent support. |
| Start and rollback | `prepare_start` publishes exact selected and task borrows in `Ok`. Its execution pointer has several input-region origins and an independent alternative. `register_task` admits a loan to the actual inline execution field, then cannot read the pointer inside it. The first failed `commit_start` condition reads the selected status after this call. Trace the pointer through the task cell, pinning, publication, and scoped transfer. Registration presence remains independent. |
| Driver retirement | Host progress and thread cleanup move nullable buffers out of host storage. Later cleanup has the receiver but cannot read the reached backing. The summary loses the binding to the receiver and reports unbounded retirement. Trace the written destination at the swap and the moved owner's backing at cleanup, including when the callback changes the selected row. |
| Planning | Compare the storage flow preparation, solving, and certification for the same optimized build. Reuse exact proof inputs across reached consumers when their evidence and authority roles agree. |

The checked route owns the descriptor read and its target type. Entry
preparation makes each selected component available at its actual location.
A result, write, or move carries that binding into its new owner. Call,
activation, and cleanup use the same binding while traversing effects.
Maintain exact addresses, complete typed regions, and opaque region bounds
through the entire route. Only a checked address can prove initialization,
ownership, or a performed retirement. An inactive payload contributes no
target. An active payload with an unresolved descriptor still needs a
conservative effect. A raw pointer field can reach across ownership
boundaries. Distinct field names alone cannot establish disjointness.

Use the nested buffer and imported reference controls for the positive
cases and the unrelated extraction, overlapping write, and old-generation
controls for the negative cases. Verify the same checked route with normal,
abnormal, and deferred completion. Compare the Windows bootstrap and
fixed compilation inventory with the same optimized binary. Reconsider
any proposed representation that requires replaying previous writes or
inferring a live pointer from a type or a former owner.

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
