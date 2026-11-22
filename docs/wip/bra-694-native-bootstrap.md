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

The guarded owner publication and typed pointee changes pass the audited
inventory of 1,709 cases, 584 checker tests, and 139 bound-tree tests.
The latest package-backed check and lowering covers 43 bodies and leaves
9 diagnostic lines at seven sites: five scoped cleanup sites and two call
sites. The raw retirement and both observer errors cleared. The four
native targets check the 32 selected owner, registration and dispatch
bodies, including `dispatch_ready` and `start_frame`. A complete runtime
archive and executable link must check the remaining bodies and backend.

In an isolated optimized run with 24 workers, the Windows x64 standard
library took 28.3 s to plan and 26.9 s to emit. Platform providers took
18.3 s, and the full package invocation took 80.3 s. The earlier
private-root candidate planned in 23.7 s and emitted in 22.5 s. Compare
pinned binaries with the same prepared providers and one workload at a
time before attributing the difference to a compiler change.

## Current repair contract

The checked flow publishes each reached mutation and retirement to proof and
the current value tree. At a join, `common_components` keeps changed fields
over a joined value. `conditional_facts` pairs that value with each actual
predecessor snapshot, reads the witness ledger, and admits a clause only when
both predecessor proofs establish it with owning carriers. Its consumers
include direct call requirements, `Current` and `Reference` observations, and
scoped cleanup requirements. The observer's `preparation_owned` claim is
published after `prepare` and the mutating `reserve_*` calls.

The current 43-body package check with lowering reports 9 diagnostic lines
at seven source sites: five cleanup sites and two call sites. Both observer
sites now pass. Snapshot pairing reads the logical owned or nullable shape
inside a physical capture and projects through that captured value. It pairs
implicit joined fields with actual predecessor snapshots. The focused test
covers repeated joins with and without the marker on each path. Keep the
two-predecessor carrier proof and the ordinary forward map unchanged. A
candidate must never gain authority from one path, an inactive payload, or a
borrowed or copied address. Reconsider if an allocation control gains
authority or the fixed inventory regresses.

`destroy_domain` moves the `RawAllocation` and arena owner out of the domain.
The allocation destructor retires the domain backing, while `release_arena`
later updates dynamically registered host storage through an ambient effect.
Ending the allocation's lexical scope before releasing the independent arena
cleared the caller's E7086 on the fixed inventory. The strict raw retirement
check remains. A row without a proved typed discharge must still reject
retirement.

The remaining execution cases need separate producer-to-consumer proofs.
`current()` guarantees `view_owned` for its result. In `task_entry`, the
`dispose_reference` call at expression 52 frames that live result into a
post-state, and some reached paths drop the witness before `Current` cleanup.
Trace its stable storage paths and actual effect target. The `Current` view
protects the context while `dispose_reference` can release an execution pin,
so moving its destructor earlier is not established as safe. `root_scope`
loses the borrowed `Reference` claim at a later loop join. Its selected
candidate has no retained witness carrier on either predecessor, although a
direct premise exists before the join. The join must not manufacture ownership
for this borrow. Check whether the live counted view supplies permitted
support, or whether the runtime source should reacquire the reference after
the scheduler call. `resolution` also
fails one scoped view cleanup and one task-storage call. Do not infer that
these share a cause from the diagnostic category alone. A broad effect must
not preserve a claim on storage it may change, and a copied address must not
gain owning authority.

## Acceptance

1. Finish replacing old paths and search for their callers and equivalents.
   Check value identity, retirement, and guarded transport as complete paths.
   Include round-trip import and source behavior, defaults, and cleanup.
2. Run the fixed inventory of 1,709 cases against the candidate. Use the same
   audited test inventory and reuse only exact-binary evidence.
3. Check and lower the actual bootstrap owners, registration, and dispatch
   on Windows and Linux for both x86-64 and AArch64. Build the complete
   native runtime archive and link an executable against it. Run native
   toolchain steps outside the sandbox. BRA-695 and BRA-696 cover execution
   and shutdown.
4. Measure an optimized build on 24 cores without tracing. Planning should
   take 17–25 s. The full build must produce a finished binary within 60 s.
5. Remove temporary probes and stale artifacts. Complete the repository
   audit, mutating style check, formatting, and independent debug review.
   Push verified checkpoints to the existing draft PR.
