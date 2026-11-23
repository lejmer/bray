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

Pushed checkpoint `f64684d61` passes all 1,709 comparison cases, 139
bound-tree tests, 598 checker tests, and the nullable allocation regression.
Its 43-body check with lowering reports five diagnostic entries across
scheduler deallocation, the previous owner in scoped exit, and the view
owner in task cleanup. The complete native archive is still unverified.
Checkpoint `5c41737ba` builds the Windows x64 standard library and passes
its 1,188 affected unit tests. It checks and lowers all 43 packaged
bootstrap bodies with four diagnostics. Scheduler deallocation succeeds.
The previous owner and current view checks each still report twice. The
fixed comparison found one widened collection cleanup and one recursive
generic cleanup test that exceeded its time limit.

### Live owner preservation

The physical state resolves performed effects from admitted operands and
publishes the observations that survive. Framing the logical value and keeping
its witness must consume the same observations. A transferred or ended slot
supplies no live evidence. A separate copied address can preserve its bits,
but only a live owner with unaffected support can preserve allocation authority.

Result completion now preserves the staged descriptor and backing. The
logical capture fills unstaged data and adds support. This keeps the address
bound in `publish_driver` intact and prevents an escape from widening to the
unrelated acquisition loan. A lifecycle receiver write can have an older
descriptor in its slot, so it assembles its checked writes with the receiver
binding instead of treating the receiver as a staged result.

An opaque frame must independently establish the survival of a stable
address field and its allocation. The physical memory state has the reached
address alternatives, allocation incarnations, and reachable effect regions.
Use those records to publish the surviving allocation addresses with the
frame. Proof reads the corresponding live fields and retains only clauses
already attached to a surviving allocation owner. An unknown target, a
possibly retired allocation, or a copied pointer with no owner supplies no
authority. Effect-frame joins keep only evidence valid for both paths.
The arena creation call combines ambient and atomic effects. The atomic
frame must carry its contents-only classification into that merge.
The same surviving owner and backing can keep established spatial access
clauses carried by that owner. Use the existing pointer-argument and
contents-only predicate classification. The other operands must also survive.
The frame supplies no new permission and a copied address supplies no owner.

`nullable_allocation_publication_uses_its_declared_result_owner` exercises
the complete owner through an opaque call, outside the 1,709-case inventory.
The checker-state control distinguishes the live owner from a retained pointer
copy. `moving_raw_contents_preserves_allocation_ownership_until_deallocation`
covers a typed move followed by deallocation and rejects a second deallocation.

The scheduler's checked `take` row transfers the whole domain and its two
nested buffer references. The selected `SpinLock` destructor has the reached
receiver, but its resolved effect is ambient retirement on normal and abnormal
rows. Its generic source summary already contains ambient `Dispose(T)` for
the value moved from `Uninit<T>`. Specialization cannot recover the source
route at that point. Publish the carried value's cleanup scope while the
generic body still has the transfer. The caller must resolve effects through
the captured value without treating its former inline slot as the disposed
value.
Both selected cleanup phases capture the actual receiver before applying
effects. The summary reads the complete carried sources from that capture.
Imported specialization expands the cleanup type and binds external effects
through those routes. An open child reached across an indirection retains its
typed route for further specialization. Only a reached address can establish
a performed retirement or definite discharge.
An unresolved cleanup keeps possible retirement. If specialization has only
a region, a projected discharge contributes possible consumption instead of a
typed discharge.
The definite domain discharge and the retirement of a separate buffer backing
cannot retire the outer allocation. An unresolved retirement carries no new
authority and still loses proof within the region it can reach. Both raw
deallocation and buffer backing release are deallocation. A missing access
type does not give a backing retirement access to invocation-owned inline slots.

The generic `SpinLock<T>` source row now publishes one captured disposal.
The package build exposes two remaining consumers. Terminal checking must
compare the captured regions and inline writes with the dying receiver. A
complete symbolic route for an open type does not pass the receiver to an
opaque call. A capture with neither a complete bound nor symbolic sources
still requires proof that the terminal receiver cannot be reached. Retain
the opaque-call controls that reject actual receiver exposure.

The remaining cleanup obligations name the acquisition in the scoped previous
owner and the local current view. The normal task-disposal frame retains the
acquisition. Some panic frames omit it. In the start call the storage frame
omits the acquisition from the normal and exceptional transitions. Both
calls bind an escape through the returned task pointer and reach the inline
view owner. The admission summary already places that pointer in regions
reached through the view's stored context address. Locate the first read
that drops this descriptor between the admitted argument and the nested
backing. A possible region cannot substitute for a later unreadable pointer.
Keep the controls for opaque nested pointers and bounded borrow choices.

The selected `entry_current` rows publish exact borrows of two different
inline fields. Their combined row forwards either of those addresses.
Staging currently puts the forwarded choices in the same region form used
by merely possible pointees. The planned descriptor read needs the reached
choices when they are exhaustive and still current. Effects may use a
common operand or the union of their possible regions. A mutation needs
one agreed actual location before it can establish an ordered discharge.
This is independent of a pointer whose origin was only ever a region.

Collection storage prepares whole-value lineage for its generic element.
The stored-value transfer in `take_nullable` reads the payload component,
so entry preparation must install the same checked inline paths as ordinary
product and nullable storage. Cleanup of a moved value publishes its carried
source when the reached binding has complete provenance. Disposal without a
complete moved source uses the typed operand, including through recursive
calls. The collection probe confirms that the result carries its payload
source and that cleanup remains ambient if coverage is incomplete. The
two fixed regression cases and all 745 checker and bound-tree tests pass
with those changes.

Directory close publishes an unlocated `Retire` from the native directory
handle. Its independently supplied raw output is a typed entry cell with
no observed allocation of its own. Physical invalidation can mark a known
allocation or a reached retirement footprint. An unlocated summary instead
weakens proof for other raw cells. The output's certified `valid_write`
applies on each normal return. Keep the controls for explicit use after
deallocation and for possible retirement of a tracked allocation.

### Current regression contract

| Transition | Authoritative source and consumers | Required result and comparison oracle |
| --- | --- | --- |
| Cleanup escape | The checked effect route resolves an admitted receiver or capture. Physical escape, proof invalidation, deferred cleanup, and summary publication consume its region. | A resolved escape exposes precisely its reached roots. An unresolved escape cannot become an empty region. `retained_result` started then dropped must retire authority. A disjoint local owner must remain valid when a precise capture proves it unreachable. |
| Stored pointer observation | The reached memory value tree owns the exact field contents and returned-reference bounds. Raw read, effect targets, invalidation, and borrow summary consume its address. | Read exact current pointer contents directly when every possible container target has a compatible exact field. Otherwise retain every feasible input or ambient origin without inventing a single address. The recursive nullable result has two input origins and one empty origin. Opaque interference and raw reads after moves must reject unsupported authority. |
| Nullable payload | The planned discriminant and reached mutation determine whether a payload exists. Physical value merging, ownership transfer, postcondition proof, and call admission consume that result. | A promised mutation of `owner` must carry its optional-owner evidence through direct extraction. No promise or extraction from `other` must fail. The same rule applies to deferred and inactive cleanup routes. |
| Repeated producer site | Each reached frame and joined current value owns its actual producer and witness identity. Ordinary join facts, carrier publication, frame completion, and import consume those identities. | The first task from the loop can carry `held`. The independent second task cannot inherit it, in source or imported form. An index bound can survive only while its joined value represents the reached iterations. The loop exit at `index == 2` must remain reachable. |

Result effects read the staged output tree. Input effects read admitted
captures. Keep the same rule across deferred activation and cleanup.
Preserved discriminants belong to the reached value. For a borrowed value,
the reached referent determines whether its tag survives. A preserved
inline tag can coexist with a changed payload.

A joined slot takes the predecessor value at that named place. An older
capture inside another value keeps its own identity. At a loop header only
evidence that survives the fixed point can establish an invariant. At a
physical join a definitely absent parent makes its unlisted children absent.
An unknown parent supplies no such information.

A readable raw pointer prefix identifies a place where a later descriptor
can be read. The later descriptor carries its own origin bounds. This is
required in `admit_source_task`, where an opaque arena read can outlive the
caller-local `Current` view. A sound bound retains every feasible input and
ambient source. The fixed comparison and the bootstrap inventory exercise
both requirements. Reconsider the design if the reached values and effects
cannot distinguish those controls without maintaining another value tree.

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
