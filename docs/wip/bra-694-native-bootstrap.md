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

Draft PR checkpoint `a04650e79` passed all 1,109 compilation controls and
938 symbols, bound-tree, and checker tests on one optimized binary. The old
call preparation, expression target, and independent normal storage promise
have no remaining callers. The controls cover host exit, observer panic
cleanup, channel cleanup, indexed protected writes, nested buffer and raw
address publication, imported witnesses, deferred frames, and destruction.
The [architecture map](bra-694-architecture.md) records the resulting contracts.

Candidate `guarded-result-publication` passed the same 1,109 controls and 938
unit checks. On Windows x86-64 it checked and lowered 18 of 25 selected
bodies. `owner_entry_scope` now passes. `start_frame` reaches scoped entry
with the token and execution witnesses. Its later selected-view obligation
still fails. `commit_start` captures and returns the exact token slot, but
its admitted witness set is empty. Direct `dispatch_task` and worker
`acquire_ready` checks expose failures within those callees. Direct
`create_native_provider`, `install_binding`, and `retire_driver_resources`
pass. The previous candidate passed 14 of the original 20 across all four
targets, including `prepare_start` and `register_task`.

The production decimal loop passes. The current optimized Windows standard
library took 29.1 s to plan, 26.8 s to emit, and 63.1 s wall time with 24
workers. The previous candidate took 28.9 s, 26.8 s, and 62.9 s. Its
result-dependency solving used 150.7 worker seconds against a 60.0 reference.
An earlier cleanup profile attributed 2.5 s to proof invalidation and 0.06 s
to both state copies across 3,354 actions.

## Active batch

An admitted callable's authored outcome promise applies to the result of
normal completion. Binding a guarded result clause to each individual row
simplifies its result test away. The `prepare_start` trace shows seven
`Error` rows with a true clause and two `Ok` rows with the active owner
clauses. Scoped entry then receives the correct `Ok` projection without
the producer witness. Bind value-dependent guarded declarations to the
published result and reached input observations. Keep checked portable
carriers and physical support on the rows that establish them. A failed
admission, an inactive guard, an unrelated payload, or a replaced subject
must confer no authority. Use the same completion for a selected local
deferred producer. Keep raw-address promises with their selected bounds.

Check guarded owner transport in the actual bootstrap and a smaller
source and imported contract with a positive selected payload and a
negative replacement. Remove duplicate row binding of the declaration.
Reconsider this approach if it weakens portable support or transfers a
fulfillment fact to an unproven owner.

At the physical effect boundary, repeated addressless non-atomic writes
with identical footprints perform the same weakening. Keep the typed
effects available to proof and export. Apply only the first consecutive
weakening when neither effect can write a complete cell, follow a cleanup
receiver, or retire backing. Verify the existing exact-write, backing
retirement, and alias controls alongside native timing.

| Affected transition | Reached evidence and expected behavior |
| --- | --- |
| Dispatch recovery | Escape behind `Current.pointer` now starts at the reached raw region. Some `dispatch_task` and wait rows still retain an escaped inline `Current` and unbounded retirement. The direct task body exposes an earlier obligation. Trace the callee's actual captures and cleanup prefixes before the later `context(selected)` call. Real raw mutation and retirement must still revoke proof. |
| Provider installation | `create_native_provider` publishes a nullable provider within `Ok`. The selected payload now carries its proof through the consumed match and `install_binding` to the absence-or-owned requirement. Source and imported controls reject a replaced payload. |
| Start and rollback | The guarded `Ok` payload of `prepare_start` now enters the scope with the token and execution witnesses. Compare the selected `Current` identity through scoped entry, lane selection, `register_task`, `commit_start`, and rollback. Direct `commit_start` extracts and returns the same nullable input identity. Its admission captures no associated witness. Trace the entry requirement and the reached payload carrier before the write. Registration presence remains an independent condition. |
| Worker acquisition | Compare the pool reference and guard's acquisition at `locked_state`. Their `acquisition_for` clause depends on both endpoints. Follow the outer scope marker across acquisition and the pending write. An unrelated pool or guard must still fail admission. |
| Driver retirement | Some `retire_driver_resources` rows already publish an untyped unbounded retirement, and the later acquisition observation becomes a post-state value without its marker. Its direct check passes. Trace the summary effect to the first reached cleanup action or provisional recovery that exports it. Retirement must revoke precisely the retired backing without preserving old generations. |
| Convergence | The array write selects element zero before the first join. The next evaluations agree on an unresolved index. Publish the enclosing logical observation when the physical write names only a region. Keep independently reached exact selectors and disjoint fields. Check the production decimal body and the existing indexed siblings and loop controls. |
| Planning | Consecutive unbounded effects with identical physical scope repeatedly weaken the same memory. Apply the physical transition once and keep typed effects for proof framing, summaries, and diagnostics. Leave precise writes and retirements ordered. Empty pointee preservation entries carry no evidence and need no mask allocation. Measure the unchanged native query and emission boundaries with the completed batch. |

Reuse the same optimized binary for bootstrap and the fixed inventory. The
current investigation uses actual checked captures and publication records.
A finite alias bound cannot prove one exact target, initialization, ownership,
or a performed retirement. Reconsider a storage change if it equates a live
owner with an unrelated or historical carrier.

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
