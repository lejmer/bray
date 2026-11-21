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

## Current implementation batch

- Ordinary effect targets read admitted descriptors and typed routes.
  Inline effects read the plan's storage coordinate. Static and result
  routes read their own actual storage. Opaque calls, frames, intrinsics, and
  await get their possible reach from actual captured operand information.
  An incomplete erased value can supply symbolic dependency support while
  remaining an incomplete region bound.
- Assignment resolution uses the destination captured before the right-hand
  side for conflict checking, replacement cleanup, and installation.
  Select the evaluated source once for staging and escape. Compound
  installation selects the private operator result. An unresolved destination
  still contributes possible escape regions and stages no exact write.
- Requirement checks use reached entry evidence after the runtime guard and
  before effects. Staged raw memory completion reads the initialized or
  empty-copy decision from that admitted entry. It requires initialization
  on all joined paths, with empty copies vacuously valid. Scope, edge, and
  last-use loan retirement consult actual descriptors and shared ancestry.
- The generic iterator escape trace reaches a published value with a loan
  in its actual carried support and only a conservative address. Check
  that loan at return and default publication. Check the legitimate generic,
  imported, and raw-address controls for retention and false positives.
- The retained incident producer publishes a known union tag with an opaque
  base. Establish the base identity from the actual publication and preserve
  that tag, fields, and all admitted owners. Verify both forged and repacked
  variants still fail. A deterministic scalar publication uses its defining
  call term and its actual copied data. Values read through live loans
  cannot establish an immutable data correspondence.

Use exact test names from the fixed inventory and the grouped checks.
The known relevant controls include delayed argument writes, overlapping
storage, compiled/imported owners, default conversions, committed failure
writes, nullable absence, raw-pointer retirement, revived compared operands,
generic iterator escape, and protected sibling fields.

## Open evidence

| Area | Current evidence and next boundary |
| --- | --- |
| Storage and proof publication | Candidate `owned-values-and-cleanup-evidence` passed 1,099 of 1,109 grouped compilation checks, plus all 574 checker and 139 bound-tree checks. Compared with `terminal-storage-and-bounds` at 1,102, it restores channel cleanup and proves the bounded scan and imported whole nullable witness. Four optional transfer cases, nullable acquisition, and the host exit regress. |
| Guarded authority | The broad `Present` wrapper distinguishes the whole value but loses payload authority in extraction and transfer. Use reached ownership and carrier subjects to bind whole and payload identities together. Trace host predicate normalization separately because the attachment exit compares reference contents. The exact product-static descriptor still loses its trusted fact at opaque invalidation. The observer still loses `view_owned` before the selected view's block destructor. |
| Bounds and cells | Both captured slice-bound controls, returned raw ranges, and absent-buffer cleanup pass. Indexed protected writes preserve sibling fields. The selected write still loses its evaluated index at summary publication. The nested-buffer trace reaches the typed inner buffer descriptor from the outer view. Entry demand closure alone did not make the inner backing available at the second view. |
| Native owner composition | The previous four-target comparison passed `prepare_start`. Once the refactor paths are replaced, trace `dispatch_ready`, `start_frame`, and `acquire_worker_ready` through the actual acquisition, resident task slot, callback cleanup, and backing. A retirement follows its captured allocation even if its former descriptor changes. |
| Planning and emission | The current candidate emitted the Windows standard library in 45.7 s planning, 40.8 s emission, and 98.5 s wall time with concurrent desktop use. The previous completed run took 32.3 s planning and 32.4 s emission. At unchanged query and code-generation counts, result-dependency solving rose from 153.5 to 194.6 worker seconds, and code generation rose from 109.7 to 145.0. Compare under similar load before attributing the increase. An earlier cleanup profile attributed 2.5 s to proof invalidation and 0.06 s to both state copies across 3,354 actions. Trace why finite effects still visit unrelated physical observations. |

The optimized compilation test binary is
`target/audit/comparison-owned-values-and-cleanup-evidence-compilation.exe`
with SHA-256 `592D868B1B5A0CCB39636CE65E0CB5CD4A41BF53655F4DBE6B1ADB825AE593E3`.
The comparison receipt records its source diff. The three test fixture reuse
edits after that build still need validation. No build is running.

Resume by reviewing the six regressions together with the four remaining
failures. Follow the reached inputs through outcome materialization, scope
retirement, proof invalidation, and portable summary binding before building
the batch. Review the remaining independent effect setter on
`DependencyContractTemplateData` during the old-path audit. Keep the native
bootstrap failures deferred until the replacement is complete.

Use the unchanged production fixture for tests that query only declared
production bodies. Their empty caller does not need a revised compilation.
For repeated imported witness cases, encode the provider interface once and
reuse the imported inputs while changing the caller source. Keep each case's
diagnostic boundary and target checks.

### Cleanup proof work

Scope and replacement cleanup already pass their reached mutation and
observation mask into proof. Each transition must apply the same revocation
rules to ordinary facts, trusted facts, current witnesses, allocation owners,
and admitted frames. A borrow of an address slot retains its descriptor when
the pointee changes. Known stable observations and preserved predicates keep
their existing exemptions. Cache traversal answers only inside the immutable
transition inputs. Remove repeated walks of the same subtrees and defer
retirement-only work until a lifetime actually ends. Loan retirement needs
only the current candidates, and reads support from the real live values
and reports. Check scoped and replacement cleanup, raw address aliases,
protected cells, conflicting loans, and private frame witness retirement.
Reuse the paired native profile and exact control group. Reconsider the
change if repeated walks shrink too little to affect planning or the cache
requires copying current state.

One cleanup transition can reach the same admitted evidence through current
values, private outputs, and several deferred producers. Evaluate revocation
once for each shared immutable entry within that transition. Reuse the
updated entry wherever the original was shared. Entries whose support
survives keep their existing allocation. Different captured entries and
different transitions still apply their own cleanup rules. Check deferred
witness survival and revocation, nested producers, and the native cleanup
profile alongside the observation traversal changes.

Opaque cleanup checks each pending entry's private owners and admission
facts when deciding whether it needs revocation. Pass that same prepared
evidence to the update. Evaluate it only when pending proof needs it. Use
the same immutable value tree and support rules for the check and update.

Terminal states already own the reached physical bindings and execution
values. Read them while encoding each normal outcome. The cross-outcome
absence check needs only the set of observed result slots. Borrow unchanged
bindings and stored payloads, and materialize only returned-origin
expansions. Remove the copied exit storage and its deferred publication
record. Confirm source and imported relations, complete and absent payloads,
reports, and allocation anchors retain the same meaning. Measure the change
with the same result-dependency and native profile.

### Joined scalar data

The joined-index trace reaches the checked selector but has only the original
parameter bound in the proof ledger. The join must propose the scalar bound
using the copied contents of each incoming slot and of the joined slot. The
same observation is used for proposing a condition and rebinding it against
each predecessor. A literal provides evidence in its own predecessor and
cannot replace all equal literals in a condition. Never transfer a witness or
loan by comparing copies. Check the bounded index, the out-of-bounds sibling,
guarded output scalars, and loop stabilization.

Postcondition proof applies its typed reflexive comparison to the normalized
condition as well as the original condition. A copied call result can become
the same integer term after its recorded data correspondence is substituted.
Machine arithmetic must still establish exactness before publication, and
floating equality retains its existing NaN behavior. Check the guarded
deterministic call product and the overflowing control.

### Returned address lifetime

The positive and expired slice-prefix callers admit the actual range of a
local array. The producer publishes `address_of` with an incoming-borrow
target and a fixed first-element projection. Normal exit reads that binding,
but the portable reference conversion drops the fixed projection. The caller
therefore sees a result with no formal address source. Admission separately
requires an exact cell to record an incoming lifetime, even though the range
still identifies its containing storage.

Publish fixed selectors from the reached targets through the same projection
conversion used by stored values. The prepared call route must consume these
selectors for source and imported summaries. Use the captured address
alternatives directly for lifetime support even when they describe a range.
Bounded regions supply neither an exact cell nor a new loan. Guarantee
completion must follow that returned address source, and scope retirement must
expire its guarantees with the containing storage. Shared conversion also
keeps mutation, cleanup, and copied-value dependency paths from dropping the
same fixed selector. Unknown and displaced selectors keep their existing
conservative handling.

Check live and expired slice prefixes, returned raw ranges across imports,
protected indexed writes, and raw exposure after a borrow ends. Reconsider
if a prepared route cannot bind the published selector or if backing
retirement revokes an unrelated address. This change replaces the narrower
projection conversion and uses the existing reached value and proof ledger.

### Reference contents at completion

The operation's resolved `StorageInvalidation` determines whether execution
changed observations. Retain its unchanged decision on the reached call
entry. Refresh reference contents from the current typed target when it is
available. When the target is abstract, preserve admitted contents only if
that same reached transition established no change. Cleanup and abnormal
outcomes use their own transition and cannot use the normal decision. An
await reads the decision at activation or observation, where its effects
were applied, rather than at frame formation.

Remove the completion-time purity query and its separate preservation
decision. Keep the certification dependency for use of an opaque callable's
declared effect bound. Check source and imported calls, delayed captures,
deferred receiver refresh, and failure-prefix cleanup. Reconsider the
handoff if a completion cannot identify its actual attempt without a source
lookup or a second effect resolution.

Compiler-known leaf producers author the same normal rows as checked source
bodies. The allocator and generator publish reference relationships together
with the effects of that row. Keep opaque leaves without an established
result relationship opaque. Remove the separate reference setter so a caller
cannot replace the relationships independently of their outcome. Use the
existing generated interface, specialization identity, and recovery controls.

### Slice extent

The normal slice producer has the evaluated receiver and bounds. Publish the
immutable extent on the resulting descriptor using the compiler-known
sequence length term and the existing copied-data correspondence. Use the
receiver's declared array extent or reached descriptor length when the upper
bound is omitted. An index reads the evaluated receiver and selector. It
must not query the syntax or retained operand slots of an earlier slice.
The length fact belongs to that descriptor incarnation and conveys no
initialized cells, access authority, or ownership. Loop reuse and storage
replacement must retire the corresponding old observation. Check captured
bounds after argument evaluation, joined index controls, nested slices with
omitted bounds, guarded access, and an out-of-bounds sibling.

The current comparison passes descriptor lifetime and joined-index checks.
Length equality resolves the later index's operand to subtraction. Check the
resolved expression with the same integer range proof used for source
arithmetic. A strictly ordered pair gives the exact difference a minimum of
one. Preserve the separate precondition check of the slice itself. The
early-return scan's back edge has already folded the next value to one.
Its incoming facts constrain the old counter to zero. Prove strict progress
from the actual two values and the back-edge facts when the result has no
retained arithmetic form. Continue requiring the same direction on every
feasible cycle. Verify bounded early returns, aliased counters, and overflow.

### Protected storage operand

The indexed protected write admits a typed borrow whose evaluated selector
does not identify a statically exact cell. Apply the payload projection to
its captured address alternatives. Use the same projected descriptor for
possible effects, the value transfer, and the returned view. Only an exact
target can supply a strong write or move. Unknown regions remain unknown
and do not recover the body or initialize other elements. Check protected
writes, move-out, raw payload address formation, and source and imported
consumers that retain independent fields.

The indexed writer now completes without recovery and its caller preserves
the sibling marker. The selected element's initialization is still absent at
the caller. The portable effect widens before the selector because it only
recognizes fixed ordinals. Trace the evaluated selector through portable
term publication, call admission, and the typed cell observation. A selector
that cannot be bound leaves only the containing region. Neither that region
nor the opposite selector establishes initialization.

The outer buffer's view provides the bounded element slot through its actual
backing. The entry planner closes demanded field paths through inline types.
It must also carry a demand from the typed slice to its buffer's explicit
backing route. Install that binding when reading the entry snapshot. View
production then reads the stored descriptor. Keep an unknown descriptor after
an opaque replacement and keep region bounds separate from exact addresses.
Constrain cyclic type paths to the explicit demands already present. Check
nested and nullable buffers, wrapper transfer, and opaque replacement.

### Nested reference observation

The attachment exit compares the same reached field, descriptor, and
post-state data on either side of admission. The caller's restored reference
adds an owning layer with the same identity as the reference descriptor.
Canonicalize that observation when constructing the predicate argument.
The stored value keeps its ownership structure for cleanup and retirement.
The trial that canonicalized the stored value passed the attachment exit
and regressed channel cleanup. Check both, along with nested witnesses and
replaced, crossed, or stale owners.

### Whole-value identity during publication

A completed nullable value uses its own producing storage for whole-value
clauses. The active payload may have separate ownership and a presence
guard. Bind both from the produced row. Keep any admitted ownership
transfer on the value it names. Source and imported callers must preserve
that distinction through aggregate publication, joins, and extraction.
Check whole nullable ownership, payload-only ownership, and forged or absent
alternatives. A missing root must never be repaired by upgrading a payload
witness to the whole value.

A returned static descriptor retains the provider's actual selection and
pointee type even when the caller's plan has no static access expression.
Use its reached shared view and the existing typed preservation decision.
Check raw exposure, mutable or unknown backing, and exact-thread lifetime
before preserving observations. Keep the negative input-holder controls.

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
