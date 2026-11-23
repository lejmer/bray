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

## Verification

Checkpoint `a262dd85f` passed all 1,709 fixed comparison cases and all 1,192
bound-tree, checker, package-interface, and symbols unit tests. It checked
and lowered all 43 packaged bootstrap bodies with zero diagnostics on Windows
x64 and ARM64. A later Windows x64 standard-library build took 24.3 s to
plan, 23.0 s to emit, and 53.0 s overall. Full archive planning slows inside
numeric dependency checks reached by real power and complex division.
Within numeric bodies, the long transfers include lifecycle resolution
even with a single reached state. Checkpoint `a71fda5de` uses direct root
lookup and skips by-value conditions that have no live observations.
It still needs an uncontended timing run.
The archive and executable link remain unverified.

Linux x64 standard-library planning reports one missing read guarantee at
the `copy_spare_raw_bytes` call in `copy_native_text_entry`. The producer
returns both the address and the length. The guard and extent must refer to
the same published result and retain the pointer's underlying support.
The null test reads the admitted descriptor. This removed the branch that
treated a definitely null pointer as non-null. The producer still combined
a non-null physical descriptor with the zero-length return when the check
knew the pointer was null only in its logical facts. The taken edge must
refine the observed inline descriptor before the pair is transferred.
The timing target remains 17–25 s for planning and 60 s to produce the binary.

The null-edge refinement checks and lowers the Linux source regression.
The subsequent Windows build planned the x64 standard library in 25.9 s and
emitted it in 23.3 s. The 67.6 s total included a Rust provider rebuild.
The ARM64 run took 26.3 s to plan, 23.8 s to emit, and 60.1 s overall.
Both package runs used Rust 1.98.1 for the native providers because the
pinned LLVM could not link Rust 1.99 provider bitcode.
The publication candidate passed 615 checker tests, 139 bound-tree tests,
and six selected compiler checks. The optimized Ubuntu host compiler built
from the same source changes. The packaged bootstrap matrix and fixed
comparison are still required for this candidate.
Checkpoint `e5a62e742` planned the Windows x64 standard library in 24.0 s
and emitted it in 22.1 s. Its 62.0 s total included 12.5 s for providers.
The ARM64 build took 24.3 s to plan, 22.5 s to emit, and 55.9 s overall.
The allocation-incarnation batch passed all 615 checker tests and all 221
package-interface tests. Its source and imported forwarding regression both
retain separate historical and current allocation subjects and lower without
diagnostics. The private retirement test reads duties and discharges from the
same staged payload map as the performed effect.
The direct-bounds batch passed all 615 checker tests and both allocation and
escaping-lifetime compiler regressions. Its exact optimized binary passed all
1,709 fixed comparison cases with no failures or missing cases. The packaged
bootstrap matrix and native archive remain to be verified for this candidate.

### Current storage contract

The task admission output's checked result route ends in owning backing.
When its captured descriptor cannot supply an address, the route still
excludes invocation-owned inline storage. Carry that region through result
staging, ordinary forwarding, and package summaries, including descriptors
stored inside another allocation. The region may overlap other external
storage and needs no logical slot. Only a reached address provides a
definite mutation target. Unresolved ordinary pointer reads retain their
conservative fallback.

Complete borrowed alternatives keep actual addresses in the returned-input
record. Read each projected descriptor from its current, type-matched
target. A common descriptor can supply its address. Divergent reads keep
their regions. A missing alternative leaves the read opaque. A staged result
uses its own type, values, and liveness through the entire effect route.
An admitted operand keeps its committed incarnation while a repeated call
stages another producer at the same slot. Direct forwarding, projected reads,
effects, and commit must resolve the corresponding incarnation. A reached
physical destination can hold the backing region without a logical slot.
A write with several possible locations must retain the prior contents at
each location alongside the new alternative.
Keep exposure and lifetime effects on the private result until publication.
Writes through an input can reach a previous allocation at the same producer
slot. Retain their possible contents on the historical allocation without
letting their typed updates affect the new allocation.
Imported historical allocations use the dependency-bound channel. A bounded
ordinary borrow still carries its lifetime relation and cannot escape its
source storage.

Generic cleanup uses complete carried lineage through all feasible transfers.
Incomplete caller-visible sources retain conservative effects. Typed entry
preparation seeds inline buffer element paths only when they read no
descriptor and cross no backing or inactive payload. Unknown recursively
selected effects stay unknown, so expanding a generic cleanup cannot add
another layer of its own typed disposal on each fixed-point pass.

### Current regression contract

| Transition | Authoritative source and consumers | Required result and comparison oracle |
| --- | --- | --- |
| Cleanup escape | The checked effect route resolves an admitted receiver or capture. Physical escape, proof invalidation, deferred cleanup, and summary publication consume its region. | A resolved escape exposes precisely its reached roots. An unresolved escape cannot become an empty region. `retained_result` started then dropped must retire authority. A disjoint local owner must remain valid when a precise capture proves it unreachable. |
| Stored pointer observation | The reached memory value tree owns the exact field contents and returned-reference bounds. Raw read, effect targets, invalidation, and borrow summary consume its address. | Read exact current pointer contents directly when every possible container target has a compatible exact field. Otherwise retain every feasible input or ambient origin without inventing a single address. The recursive nullable result has two input origins and one empty origin. Opaque interference and raw reads after moves must reject unsupported authority. |
| Nullable payload | The planned discriminant and reached mutation determine whether a payload exists. Physical value merging, ownership transfer, postcondition proof, and call admission consume that result. | A promised mutation of `owner` must carry its optional-owner evidence through direct extraction. No promise or extraction from `other` must fail. The same rule applies to deferred and inactive cleanup routes. |
| Repeated producer site | Each reached frame and joined current value owns its actual producer and witness identity. Ordinary join facts, carrier publication, frame completion, and import consume those identities. | The first task from the loop can carry `held`. The independent second task cannot inherit it, in source or imported form. An index bound can survive only while its joined value represents the reached iterations. The loop exit at `index == 2` must remain reachable. |
| Repeated allocation site | The physical exit owns current and historical allocation incarnations. Summary export, package encoding, import staging, forwarding, and raw-memory checks consume their portable identities. | Outputs that reach different incarnations use different allocation subjects. A historical output carries only a region bound. Initializing the fresh allocation cannot initialize the historical allocation. Source and imported forwarding must retain the same distinction. |

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
