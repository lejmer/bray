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

The earlier guarded-owner checkpoint passed the audited 1,709-case inventory,
584 checker tests, and 139 bound-tree tests. The current optimized candidate
passed the fixed package-backed 43-body bootstrap check with lowering and zero
diagnostics. The two focused Rust regressions for physical parent absence and
nested pointer prefixes also pass. The complete native archive has not yet been
rerun on this candidate.

The exact-binary 1,709-case run on checkpoint `f7039e981` completed with
seven failing cases and none missing. Removing the three latest repairs
together leaves all seven failures, so they predate the checkpoint. All pass
at `d7f12fb8a`. The first failing revisions are `477a945c9` for retained
task cleanup authority, `cd6c582a5` for recursive pointer bounds and two
raw-memory authority controls, `c2d3b0c6f` for nullable-owner extraction,
and `9d9486481` for both same-site deferred authority controls. The 43-body
success alone is insufficient to close the contract.
The batched candidate passed all 594 checker tests and 1,708 of the 1,709
exact-binary comparison cases, with none missing. The remaining recursive
nullable-pointer case exposed duplicate exact and `InputRegions` relations
for the same inputs in the unselected summary. The shared summary join now
retains the weaker region-only relation for each input; its next exact-binary
comparison is pending. A newly built standard-library bundle and the full
43-body bootstrap check with lowering report 14 proof diagnostics. They are
`owned_allocation` and `view_owned` obligations across initialization, call
admission, and cleanup. The previous 41-body probe omitted two owners, and
its older bundle was not a valid current comparison. A targeted storage-flow
trace is checking which retained owners and physical observations reach proof.
The focused trace shows the allocation pointer's producer value still in the
live owner tree when its trusted fact is dropped. For the execution view,
storage flow marks the acquisition field stable in the producer, temporary,
and local owner paths, but proof does not match any of them to the borrowed
producer-expression observation. A copied scalar cannot repair that gap:
the negative raw-memory controls require authority to expire when its
referent may change. Trace only the mapped owner and field paths through
`preserved_storage_values`, current-value framing, witness retention, and
cleanup. The broad transitive trace was discarded; the temporary source
probe has been removed.

### Current regression contract

| Transition | Authoritative source and consumers | Required result and comparison oracle |
| --- | --- | --- |
| Cleanup escape | The checked effect route resolves an admitted receiver or capture. Physical escape, proof invalidation, deferred cleanup, and summary publication consume its region. | A resolved escape exposes precisely its reached roots. An unresolved escape cannot become an empty region. `retained_result` started then dropped must retire authority. A disjoint local owner must remain valid when a precise capture proves it unreachable. |
| Stored pointer observation | The reached memory value tree owns the exact field contents and returned-reference bounds. Raw read, effect targets, invalidation, and borrow summary consume its address. | Read exact current pointer contents directly when every possible container target has a compatible exact field. Otherwise retain every feasible input or ambient origin without inventing a single address. The recursive nullable result has two input origins and one empty origin. Opaque interference and raw reads after moves must reject unsupported authority. |
| Nullable payload | The planned discriminant and reached mutation determine whether a payload exists. Physical value merging, ownership transfer, postcondition proof, and call admission consume that result. | A promised mutation of `owner` must carry its optional-owner evidence through direct extraction. No promise or extraction from `other` must fail. The same rule applies to deferred and inactive cleanup routes. |
| Repeated producer site | Each reached frame and joined current value owns its actual producer and witness identity. Ordinary join facts, carrier publication, frame completion, and import consume those identities. | The first task from the loop can carry `held`. The independent second task cannot inherit it, in source or imported form. An index bound can survive only while its joined value represents the reached iterations. The loop exit at `index == 2` must remain reachable. |

The repair hypothesis is to use existing reached state and admitted captures
at these four producer boundaries and preserve their identity through the
listed consumers. Remove any fallback that asserts a known empty region when
resolution failed, any replay of pointer contents from a container region,
and any join rewrite that equates independent captured producers. Do not add
a parallel state representation or grow production code merely to encode
another copy of the same fact. Reconsider the hypothesis if a complete
direct read cannot distinguish the positive and negative controls, or if a
join still fabricates authority after the frame identities are kept separate.
The nullable producer is identified: `opaque_observations_with_escapes`
added discriminant preservation for every prepared logical path. A borrowed
input's logical slot was judged disjoint from its reached `Entry` referent,
so `mutate(owner)` incorrectly preserved the borrowed tag. Owned tags already
enter `opaque_values`; active borrowed tags use `preserve_active_borrows` with
physical targets. Remove the redundant generic tag block and its now unused
candidate expansion. The `cd6c582a5` pointer-summary producer is the removed
direct field read from complete returned-container bounds. Restoring it
removes the extra recursive input-region relations without affecting the two
raw-memory controls. Those controls fail because the opaque cleanup path
injected every live scalar copy into physically preserved snapshots. Removing
that injection while retaining the copied-address clause rule passes both the
disjoint write and the opaque or moved-read controls. The 477 escape route is
now traced: all unresolved effects in the failing owned-computation case name
fields of `Result`. Output staging precedes effect binding, but `call_effects`
routes every non-static indirect subject through an admitted input capture.
A `Result` has no such capture, so the fallback publishes an empty escape.
Resolve result effects against the staged output value tree, using its reached
descriptor and physical bounds. If that direct route is unavailable, the
escape remains unknown. Check deferred and cleanup effect producers for the
same result-root routing. The consumer that turns valid loop bounds into
same-site authority remains unresolved. A probe at the first failing commit
found no trusted admission proof at the negative `observe` call. The join
makes the post-loop branch unreachable. The predecessor trace shows that
`index <= 1` is correctly rejected when the incoming value is 2, then
reintroduced on later joins using assumptions already attached to the reused
loop-header identity. Suppressing only unchanged-name candidate republication
does not restore the path. Test the structural rule that a loop header cannot
promote an ordinary fact observed in finitely many iterations into an
invariant. The isolated candidate passes the source and imported positive
first-task and negative second-task controls. Ordinary joins and carrier
transport still retain their facts. Validate this rule with the other repair
boundaries on one release candidate.

The current repair follows three shared boundaries. At a conditional proof join,
the named current place supplies the predecessor snapshot for its joined
identity. Another value can contain an older capture of that place and must
retain its own identity and proof obligations. At a physical binding join,
an explicit no-payload parent
makes its unlisted children absent on that predecessor. The join retains a
present child from the other predecessor, while a missing child under an unknown
parent still loses precision. At an output route, a readable raw pointer prefix
identifies where a later descriptor is read. It does not by itself bound that
descriptor's pointee. An unresolved nested pointer does not gain an exact
target or owning authority. Result staging must retain every feasible origin.

These boundaries meet at `admit_source_task`. Its present rows publish the
context pointer relation. Field `2027` is `Current.pointer`; field `2026` is
`Current.acquisition`. The first raw address read has a physical region but
cannot resolve the later context and arena reads. Using only that region in
the current candidate keeps the caller-local `Current` out of the result
bound, but the broad controls show that this narrowing is not generally valid.
The 43-body check retains both `task_entry` cleanup proofs and the
`start_frame` path. The controls must continue to reject a genuinely unknown
pointer, an actual escaped local, and unsupported authority on copied or
borrowed addresses. No runtime contract was relaxed.

An isolated 24-worker Windows x64 standard-library run of an earlier candidate
took 28.3 s planning and 26.9 s emission, with 80.3 s for the package
invocation. The previous private-root candidate took 23.7 s and 22.5 s.
Compare pinned binaries with the same providers and no competing workload
before attributing that difference to the compiler. The target remains
17–25 s planning and a complete binary within 60 s.

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
