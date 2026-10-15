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
host ABI version 2 covers the changed callback arguments. The obsolete paired allocation mapping
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
