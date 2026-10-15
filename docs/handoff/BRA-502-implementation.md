# BRA-502 implementation checkpoint

## Contract and baseline

Base: `f188b2b6226b0457211e16f092369c8e2570003a`, the squash merge of #607 into develop.
Branch: `feature/bra-502-secure-physical-local-cleanup-backing-before-ownership`.

Finish Step 5 as BRA-502 -> BRA-554 -> BRA-558, one reviewed PR per issue. The user authorized
stacking dependent PRs without waiting for merges. BRA-502 targets develop, BRA-554 targets the
BRA-502 branch, and BRA-558 targets BRA-554. Preserve predecessor commits and review each issue's
own diff. No new language feature, syntax, authority weakening or downstream proof pass is allowed.

BRA-502 must secure concrete local cleanup storage before accepting an obligation. Moves preserve
allowances without rebinding. Child owners bring their own allowances. Current completion proofs
cannot shrink a uniform type allowance. Activation and retained results/incidents keep their backing
through final use. Rejection preserves initialized inputs. Mandatory cleanup must not allocate.

## Existing implementation map

| Obligation | Owning implementation | Concrete gap / required change |
| --- | --- | --- |
| Selected local requirements | lowering synthetic/lifecycle and compilation product/realization/outgoing | outgoing currently counts records independently of the emitted selected calls; concrete storage is absent |
| Admission and discharge | lowering/outgoing, LLVM translation/unit/outgoing, bootstrap/outgoing | existing owner operations pass scalar counts only |
| Physical report backing | bootstrap record_pool and pool | reuse existing report ownership; do not add a second report arena |
| Typed static error retention | LLVM mapping/static_storage/finalization | allocates payload after finalizer failure |
| Native completion storage | runtime/native/frame | allocates after completion and rejects alignment above u128 |
| Native observation storage | runtime/native/task_observation | independently allocates result storage with Rust allocation policy |
| Entry and sink retention | BRA-554 and BRA-558 | consume the same admission/transfer contract; do not implement separate backing services |
| Full async/task/provider consumers | BRA-505/506/507/555/560/561 | remain their issue owners; shared storage contract must support their stated requirements |

## Design commitments

Extend existing selected lifecycle and outgoing-storage ownership, not a second cleanup walker or
universal planning pass. Concrete target layout is realized after semantic certification. Separate
local requirements from structural forwarding and retain multiplicity. Reuse enclosing/coallocated
storage where its lifetime is adequate. Storage retained beyond its producer must be independently
owned. Allocation policy belongs in Bray, with Rust bridges only for separately unfinished consumers.

Production growth must buy an implemented ownership or storage invariant. Remove replaced scalar-only
interpretations, post-failure allocation and duplicate storage policy as their consumers switch.
Reconsider the implementation if it introduces hidden per-move allocation, duplicate descriptor walks,
a new authority registry, a global maximum slot size, or caller-name special cases.

## Acceptance matrix

- Source and imported/generic selected cleanup derive the same local requirement and concrete layout.
- Over-aligned and large payloads, zero-sized values and checked overflow.
- Nested/recursive owners and structural wrappers do not charge child allowances twice.
- Moves, partial construction, failed admission and mutation after completion preserve ownership.
- Activation rejects mismatched shape/identity/epoch without consuming the reservation.
- Results and incidents outlive their producer without premature backing reuse.
- Admission followed by denied allocation still permits activation, capture, destruction and release.
- Native selected cleanup consumes the new backing through final release.
- Link evidence excludes replaced Rust policy for migrated consumers.
- Record allocations, reserved bytes/alignment and compiler work separately from final initiative cost gates.

No acceptance claim yet. Batch changes before compilation, then run focused and broad relevant checks,
required style/format commands, the mechanical maintainability audit and independent published PR review.

## First implementation batch (not issue completion)

The existing Bray heap allocator now supplies the closed cleanup-storage admission/release roles.
The native bridge owns a validated concrete layout and moves that allocation into retained completion
or incident ownership. Native frame completion no longer allocates after execution or imposes u128
alignment. Task observation reuses that bridge instead of maintaining a separate Rust allocator.
Static finalizer callbacks receive payload backing admitted with their static owner; generated error
capture no longer allocates. The generated destructor releases the same admitted layout. Product
host ABI uses its current version 1 with the changed callback arguments, with no compatibility shim. The obsolete paired allocation mapping
and allocation-failure cleanup branch were removed.

Evidence so far:

- `cargo test -p bray-runtime --lib`: 147 passed, including concrete alignment/overflow, retained
  incident ownership, actual over-aligned root completion and status propagation.
- `cargo test -p bray-compilation static_lifecycle --lib`: 12 passed, including asynchronous fallible
  finalizer generation and static/imported ownership.
- `cargo xtask runtime-artifact smoke-test`: passed, including allocator denial after acquiring
  65,537 bytes, exact release without fresh allocation, native symbol partitioning and synchronous
  link-map checks. Bootstrap startup remains one allocation / 96 bytes.

The shared symbolic/concrete requirements projection, source allowance activation identity/epoch
contract, complete denied-allocation native acceptance and independent PR review remain outstanding.
Neither this batch nor those passing tests completes BRA-502 or Step 5. Do not change the issue to
In Review based on this checkpoint. BRA-554 and BRA-558 have not started.

Linear delivery instructions for all three Step 5 issues now explicitly allow dependent PR stacking.
The existing acceptance requirements and dependencies are unchanged.

### Admission timing still to close

The native completion bridge now allocates before execution, but `NativeFrame::try_new` receives an
already transferred protected frame. That is not proof that every inactive/source-owner admission
failure preserves its original input. Preserve the existing root ABI's explicitly consumed-frame
failure behavior, while connecting source cleanup and inactive-frame acceptance to their already
admitted concrete requirements. Do not present the bridge alone as the complete admission contract.

### Audit of this batch

The changed behavior categories are concrete backing acquisition/release, pre-execution result
reservation, static typed-error transfer, native status preservation and callback ABI propagation.
Repo-wide searches found the two old result allocators and the static post-failure allocation path;
they were removed or switched to the single bridge. Layout acquisition is shared in `backing.rs`,
while payload destruction remains with each initialized-value consumer. The selected static release
helper now stands alone; no paired allocation wrapper remains. Product host ABI versioning, generated
callbacks and in-repo callback fixtures agree. No source-language or compiler authority rule changed.
Required style and format passes succeeded; existing structural-size warnings remain unchanged.

## Shared expansion checkpoint

Local invocation requirements are now recorded at `push_selected_call`, the point that emits
execution. `LoweredLifecycle` retains that list with the raw MIR. A demand-driven fact caches
both by concrete lifecycle identity, target and MIR unit identity; admission and code emission
consume the same result. The old independent `LifecycleAction` capacity match is removed.
Requirements retain selected callable, receiver, completion and execution kind, with repeated
local calls preserved and structural children excluded. Concrete byte descriptors and their
allowance-service consumers still need implementation.

Evidence: the five outgoing-capacity/production-host tests passed; the owned-indirection test
compares all three actual emitted calls with the requirements and verifies cache reuse across
admission and emission; all four synthetic lifecycle tests passed, including constant-size array
traversal and no repeated child allowance. A new production-declaration authority test accepts
exact allocation release and rejects wrong byte size, wrong alignment, duplicate release through
a pointer copy, and stale pointer use after release followed by another allocation.

The behavior audit covered shared expansion, query identity/reuse, source diagnostic proofs and
profiling/error context. Searches found the existing FactCellMap mechanism and selected-call
emission point, which are reused; no second action selector or downstream validation was added.
The new retained result buys reuse of execution plus requirements; the empty local-call vector
on heap-only lowering does not allocate. Existing immutable query dependency tracking provides
invalidation. Required structural style and source format passes completed. This remains a
checkpoint, not BRA-502 acceptance or completion of Step 5.

Broader checkpoint verification: all 42 realization support tests and 12 static-lifecycle tests
passed, covering imported/generic storage, repeated constant owners and asynchronous fallible
static finalizers. This batch changes no native calling convention or execution body semantics.


### Shared cancellation review repair

Review 5527130283 identified caller cancellation being cached as a lifecycle expansion failure
while another request still needed the same result. The expansion now uses FactCell's shared
request token. The structural audit found the same wrapped-result pattern in optimized MIR and
native product planning, and both now use that token as well. Other inspected query callbacks
return cancellation through FactQueryError rather than publishing it as a successful cached value.
The existing cache owns retry and publication policy. No new cancellation mechanism was added.

A deterministic two-request lifecycle regression blocks computation until a waiter has registered,
cancels the initiating request, and verifies successful reuse by the waiter and a later request.
The product test batch finished with 249 passes and the existing BRA-619
MissingRole(CleanupIncidentTransfer) failure in scoped_selected_calls_emit_native_artifacts.
That failure remains enabled and owned by plan Step 6. This is not a clean whole-product test claim.


## Physical lease work in progress

The current uncommitted batch replaces raw payload allocation with shaped slabs owned by the
existing ReportPool. Each native lease carries the service identity, shape ordinal, slot ordinal,
monotone epoch and payload address. Admission reserves bytes. Activation checks the descriptor
and RESERVED state. Release checks the live epoch before returning a slot. Retained incidents
carry the original lease through their final destructor. Slabs amortize allocation and alignment
padding without attaching runtime tokens to ordinary source moves.

This batch is not accepted yet. The first integrated Rust/runtime build is running with its log
at target/bra-620-authority/bra-502-storage-tests.log in the shared repository target. The C smoke
fixture now covers wrong-shape activation, repeated activation, stale leases after slot reuse,
over-alignment, allocation denial after admission and unchanged output on failed admission.
Those new cases have not run yet.

Outstanding integration remains concrete source allowance descriptors and atomic admission,
shared protected-frame layout emission, epoch availability secured at fallible admission,
physical activation consumers, native ABI role retention, production declaration authority tests,
full acceptance, the required audit/style/format passes and a fresh published review. The
BRA-554 and BRA-558 branches must wait for this contract. Step 5 remains incomplete.


### Concrete descriptor integration in progress

The uncommitted source allowance now carries a typed borrowed descriptor slice and record count.
Admission reserves the complete bundle under the existing pool lock and rolls back acquired
credits on failure. Lease activation and release use the same shape/epoch checks as independent
native backing. Async descriptors use the backend's existing protected-frame layout rather than
reconstructing its fields in the frontend. The static error resolver selects the admitted requirement of its source finalizer after
the generated helper retires its temporary activation. It transfers the claimed lease directly
into the incident instead of allocating a second payload region.

The integrated runtime check exposed a borrow conflict in an admission helper and unused
capability declarations. Those are corrected. The production declaration regression now checks
all function bodies in the storage and outgoing modules, including helpers, because checking
only native entry bodies missed those errors. The contextual yielded-construction fix has a
passing five-variant regression. None of this uncommitted batch has passed full native acceptance.

Remaining: verify the generated static activation and retained incident path, connect actual
cleanup frame/result consumers, establish rejection/retention and cost evidence, run the audit,
style/format and batched tests, then push and request a fresh PR review. Step 5 is still incomplete.

Zero-byte synchronous completions require their report record but no payload region. Bundle
reservation, activation and discharge skip payload slots for these descriptors. Nonzero payloads
and asynchronous frame descriptors retain the shape/epoch path.

The all-body production contract test and lease-forgery test both passed after qualifying the
moved bootstrap names. The source fixture keeps the original source ordering so existing
source-identity assertions retain their meaning. The current runtime build began before the
final static resolver change, so its result alone cannot validate that change.

Each bundle secures epoch budget for activation and one retained-result transfer per requirement.
The slots can be reused between those disjoint lifetimes without reserving a second payload.
The static callback's temporary activation runs inside StaticFinalize, so claiming an incident
lease before invoking that helper would double-charge it. Capture now occurs in the generated
resolver after the helper returns. The abandoned early-claim callback mode was removed.


### Observed large-aggregate compilation cost

The first large generic error fixture used a repeated 4,097-element byte array. Its focused
compilation test exceeded 60 seconds and reached approximately 7.4 GB working set before it was
stopped. A reduced 17-element aligned array also exceeded 60 seconds at approximately 1.1 GB.
A single-field, 131,072-byte-aligned aggregate reached approximately 42.7 GB before it was stopped.
The latter rules out attributing this solely to source array expansion. LLVM aggregate lowering
represents padding as byte arrays, which is a candidate for the native-emission cost. Attribution
still needs profiling under existing BRA-542. No production workaround or unsupported claim of
large aggregate object-emission performance is included here.

The regression covers generic selected requirements with a 64-byte-aligned emitted object and
a 4,096-byte-aligned backend IR artifact. The native allocation-denial case separately retains
65,537 bytes at 4,096-byte alignment. These establish different boundaries, not a completed
large-aggregate native compiler performance result. Automatic approval review blocked the
attempted Linear performance comment. This tracked document retains the finding and its owner.


### Current validation and audit checkpoint

The contextual propagation repair passed all 321 checker unit tests. The current runtime and ABI
suites passed 147 and 18 tests respectively. The generic selected-requirement regression passed
with a native object at alignment 64 and backend IR at alignment 4096. Production storage bodies
and forged lease evidence passed their two regressions. Five outgoing regressions cover the source
fixture ordering repair. Previous product coverage remains 249 passing cases plus the enabled
BRA-619 missing-role failure, before this final native integration batch.

The production native smoke reached all storage retention/epoch checks under allocation denial.
The rejected bundle test used the wrong expected tag, 1 instead of the ABI panic tag 2. The
assertion now also checks the allocation-failure cause. Its first two-payload admission measured
9 allocations and 148,280 bytes for 131,074 payload bytes at alignment 4096. Final smoke completion
is still required. Generated native static cleanup execution and fresh PR review remain required.

Repo-wide searches covered concrete descriptors and leases, allowance accounting, layout emission,
incident transfer/destruction, callback signatures, contextual expectation propagation and buffer
growth. Buffer growth reuses heap.reserve. Frame layouts reuse the backend frame type. Static
incident capture reuses the outgoing requirement and backing service. The obsolete scalar source
admit/discharge Rust helpers and static result-layout metadata are removed: generated callbacks
now own their storage and Rust no longer allocates from those sizes. ABI fixtures must be checked
after that removal. No new language feature or downstream authority check was added.


The corrected native bootstrap fixture passed against the retained production archive. Both new
storage checks run after the original cold-start tests, because warming the shared pool first
invalidated the existing deliberate cleanup-registration allocation failure. Final shutdown
asserts zero live allocations. The corrected executable and map are retained under
`target/bra-620-authority/bray-runtime-artifact-smoke-9nTWAk/runtime-bootstrap-smoke-corrected.*`
in the shared target. This run reused the original package's extracted inputs and native links.
The production bootstrap source was unchanged. The 13 static-lifecycle compilation tests passed
again after removing unused native result-layout fields. Runtime tests, style/format, the complete
artifact smoke workflow, generated static execution and independent review remain in progress.

Storage choice: ordinary synchronous result destinations stay in their enclosing call. A retained
incident can outlive that call and its source owner, so its bytes require independent ownership.
The existing pool owns geometrically grown slabs rather than one allocation per move or error.
The backend uses its actual protected-frame type plus completion layout for asynchronous
requirements. Full async frame activation remains with BRA-505/BRA-560 and is not claimed here.


The final ABI cleanup batch passed 13 static-lifecycle tests, 147 runtime tests and 18 ABI tests.
`cargo xtask style` and `cargo xtask format` passed, followed by Rust formatting and diff whitespace
validation. The corrected bootstrap map retains all four Bray storage symbols and excludes Rust
allocator/personality symbols and host/callback/scheduler archives. Before final formatting, the
branch relative to develop contained approximately +1,851 net production-file lines, +804 test/
fixture lines and +270 documentation lines, separating inline test modules. Production growth
implements physical slab ownership, lease epochs, requirement realization and native consumer
transfer. Moved implementation is included in gross additions/deletions, not counted as a benefit.

This is a recovery checkpoint, not BRA-502 completion. Still required: full artifact smoke after
fixture corrections, generated static execution using the updated ABI, independent PR review and
its fixes. Then continue BRA-554 and BRA-558 without waiting for merges.


### Native acceptance follow-through

The complete runtime artifact smoke passed at `7a6deb12`, including the storage admission,
allocation-denial, rollback, occupied destination, shape/epoch mismatch and final zero-live-allocation
checks. Independent review [5528190374](https://github.com/lejmer/bray/pull/608#pullrequestreview-5528190374)
was published and verified on that commit with no actionable findings.

Native execution passed static storage, retained typed static cleanup errors, cleanup panics,
repeated products, independently loaded libraries and imported archive storage with the changed ABI.
The broader run exposed three legacy readiness fixture problems. Linked PE images have no COFF
symbol table: archive selection now inspects the final consumer's relocatable references and retains
the native `MF` output check, proving selected cleanup runs while hidden cleanup does not. Both
debug and release pass, producing 2,266,624-byte and 1,366,016-byte executables respectively.
The buffer push fixture inspected its owner while holding an exclusive slice; owner checks now
precede that borrow, preserving every assertion. All sibling buffer variants were inspected.

The raw-memory fixture used raw allocation and offsets without their required access contracts,
and assumed memory-clobbering assembly preserved initialization automatically. It now uses
initialized typed owners for access, an explicit preservation contract for its fixed byte increment,
and bounded pointer projections for the overlapping and non-overlapping copy cases. The original
raw allocation/deallocation, volatile, memory-copy, pointer, fence and assembly operations remain
covered. The previous final raw allocation no longer leaks. These are fixture corrections, not
compiler authority exemptions. Native execution of the corrected full batch remains in progress.

The large-aggregate cost finding is now linked from active BRA-542 in Linear comment
`720fe6c0-e5d4-4064-8254-b76c29a12710`. Publication of a link to the pushed finding succeeded; the
earlier blocked detailed comment is no longer an outstanding action.


The corrected raw-memory fixture passed native execution, followed by the atomic fixture. The
next layout stage exposed a stale partial `std.memory` fixture missing `null`, now required by
production allocation. That duplicate declaration fixture is removed; layout, buffer and format
checks use the same production memory source. The existing source-authority test covers this
loader contract. No production memory or authority implementation changed for these corrections.


### Completed native verification

`cargo xtask readiness native-execution` passed the full workflow in 130.1 seconds after the
fixture corrections. This includes semantic rejections; static storage and typed cleanup;
transitive native libraries in debug and release; hello world; formatting; all byte-buffer cases;
text iteration; startup; ranges; nullable queries; guarded root and represented-part cleanup;
heap ownership; child panic shutdown; value replacement; reborrowing; pattern conditions; entry
results; raw memory; atomic operations; production memory layout; synchronous and asynchronous
host behavior; and the runtime artifact smoke workflow. The final log is
`target/bra-620-authority/bra-502-native-execution-verified.log` in the shared target.

The end audit re-read AGENTS and checked every changed acceptance file. Object discovery reuses
`native_execution::core::object_files`; no PE-specific fallback was introduced. The production
memory source has one fixture constant and no partial declaration copy. Pointer projections in
the memory fixture are restricted to its fixed initialized arrays and native instructions, with
real bounds and non-overlap authority; no reusable production mechanism was added. The bounded
acceptance correction still needs publication and independent re-review before readying #608.
