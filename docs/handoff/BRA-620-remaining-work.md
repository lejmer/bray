# BRA-620 remaining work

Recorded on 2026-10-10. This handoff covers the remaining trusted synchronization authority defect and review requirements.

Updated on 2026-10-11: use the [current stack completion plan](stack-completion.md) for work order and status.
This document retains BRA-620's evidence and acceptance details. The earlier boundary before BRA-553 is retired.

The user accepted the architectural groundwork on 2026-10-12 and requested self-contained Linear issues.
Use the [Linear execution guide](runtime-stack-linear-execution.md) for the exact recovery checkpoint and ordered work.
The [rule coverage and replacement plan](runtime-stack-rule-coverage.md) and [closed inventory](runtime-stack-coverage-inventory.md)
replace the earlier proposal. The [architectural assessment](stack-architecture-assessment.md) and the instructions below
retain dated evidence and acceptance requirements. Issue preparation did not begin implementation.

## Prepared reference paths, 2026-10-13

This continuation follows `66700ac0`. The user authorized publishing subsequent checkpoints to the existing repair branch.
The protected-payload checkpoint and its preceding local checkpoints are now pushed.

Body accesses and call contracts now retain the same checked reference-path representation. It pairs physical component
projections with typed dereference positions. Entry seeds share those immutable paths. Selected call inputs no longer
convert portable projections during every flow visit, and one memory-state resolver replaces the two path walkers.
Ordinary body accesses remain scalar. Calls collect the possible roots supplied by the actual argument.

This removes duplicated preparation and traversal without changing authority, witness preservation or generated code.
The type belongs to checked bound-tree storage and grants no access by itself. The remaining nested-reference identity,
finite-summary, carrier and production protocol work described below is still required.

Verification after the scalar resolver change: `prepared-path-final-libraries.log` has 119 bound-tree and 300 checker passes.
The final returned-value, protected-payload, reborrow and storage-flow logs have 24, four, 13 and 15 passes respectively.
`cargo fmt --all`, mutating `cargo xtask style`, `cargo xtask format` and `git diff --check` pass. The audit finds one
reference-path walker and no old tuple-path tables. About 60 production lines retain the complete checked paths and
share their resolution. There are no new test-only implementations or generated-code operations in this consolidation.
The broader authority suite remains at its preceding 135 passes and 15 outstanding acceptance failures.

## Protected payload checkpoint, 2026-10-13

This continuation follows `d541b0dd` on the same repair branch. BRA-620 remains incomplete.

Protected contents now occupy an inline payload path beneath their physical owner. Taking that address proves neither
initialization nor authority. Whole-value transfers carry the payload's contents and obligations through the shared memory
state. Raw reads and writes use the same transfer operation. Old pointers keep their original storage incarnation when
an owner moves or its storage is reused. Last-use pruning removes value facts, while the owning scope ends physical storage.
The separate protected pseudo-allocation identity is removed.

The production guard's returned borrow now has the exact protected payload origin. Receiver calls use their selected input
types. Calls that have no abnormal edge prepare their reference transfers and effects through the same transition as split
calls. Entry reference fields demanded only by checked callee summaries are seeded before flow analysis. Mutations still
revoke those facts. Unconditional replacement updates the returned origin, and conditional replacement cannot claim an
exact origin. This is an internal representation of existing Uninit semantics, with no new syntax or runtime representation.

Evidence in the shared primary checkout's `target/bra-620-authority/` directory:

- `payload-summary-seeds.log`: all four source, imported and production protected-payload origin checks pass.
- `payload-seeded-checker.log`: all 300 checker tests pass, including whole-value transfer, storage reuse, physical lifetime,
  must-joins, and bounded unlocated writes.
- `payload-seeded-returned_values.log` and `payload-seeded-memory_operations.log`: 24 returned-value and 15 memory tests pass.
- `payload-seeded-storage_flow.log`, `payload-seeded-publication.log` and `payload-seeded-default.log`: 15 storage-flow,
  35 publication and 44 default checks pass. The refinement cache test now checks prerequisites through their owning
  callable component, consistent with the established publication contract.
- `payload-final-libraries.log`: 119 bound-tree, 300 checker, 69 messages, 189 package-interface and 202 symbol tests pass.
- `payload-seeded-trusted_contracts.log`: 135 pass and 15 fail. The failure set is the preceding checkpoint's set minus
  `production_guard_borrow_origin_reaches_the_protected_payload`. No previously passing authority case regressed.

After the audit, `payload-audited-tooling.log` and `payload-audited-protected.log` pass the JSON projection test and
all four origin checks. `payload-split-checker.log` passes all 300 checker tests after separating input preparation,
state and solver into cohesive modules. `cargo fmt --all`, mutating `cargo xtask style`, `cargo xtask format` and `git
diff --check` pass. The change adds approximately 650 production lines and 630 test lines. The production growth
implements inline contents transfer and its physical lifetime, prepares missing call inputs, and transports the
protected projection across interfaces and diagnostics. It replaces the protected pseudo-allocation model and shared
transfer duplication. Performance acceptance remains outstanding.

The remaining authority failures are still acceptance work. Entry reference demand currently covers direct formal roots.
Nested reference identities, alias demand closure, finite recursive paths and must candidates, specialization demand closure,
deferred effects, and the production acquisition carrier remain unfinished. The broader std.memory initialized-prefix
failure also remains active. Passing the protected origin proof does not prove witness preservation or complete BRA-620.

## Default component checkpoint, 2026-10-13

This continuation follows `5a13b1eb` on the same repair branch. Runtime-default providers now participate in the same
source summary components as callable bodies. Both publish their checked body and retained dependencies together.
The default-expression summary consumer reuses that publication instead of running a separate inference pass.
Selected arguments supply their already substituted types, removing the full default query and repeated substitution
from storage-borrow classification. One callee iterator covers explicit calls, selected defaults and implicit lifecycle calls.

Every component member receives a provisional entry before inference. Specializing a portable generic result can select
additional witness implementations beyond the direct selected-call inventory. These use ordinary summary queries, while
members of the active component use its approximations. Query-cycle errors still propagate. Demand-graph closure for
specialization, membership reuse, finite recursive paths and recursive must candidates remain required by the plan.
This checkpoint does not claim those broader summary requirements are complete.

Evidence in `target/bra-620-authority/`:

- `default-component-resolved-defaults.log`: all 44 default-related tests pass, including the previously failing recursive
  runtime default, source/imported storage rejection, generic and trait results, and construction defaults.
- `default-component-publication.log`: 35 pass. The new concurrent test crosses a callable/default cycle and proves
  that both public body queries share the component's checked allocations.
- `default-component-recursive_dependencies.log`, `default-component-returned_dependencies.log` and
  `default-component-returned_values.log`: seven recursive, 13 imported dependency and 21 returned-value tests pass.
- `default-component-checker.log`: all 289 checker tests pass.

`default-component-authority.log` confirms 134 passes and the same 16 production authority failures after formatting.
`cargo fmt --all`, mutating `cargo xtask style`, `cargo xtask format` and `git diff --check` pass. The audit covered
summary ownership, source/imported default selection, trait witness resolution, generic type reuse, diagnostics and
concurrent publication. Production code grows by about 80 lines, replacing duplicate default analysis with the common
component and adapting the three imported default kinds. The new concurrency coverage adds about 100 test lines.

The 16 production authority failures remain acceptance work. Protected payload identities,
nested references, retirement, deferred effects and the production acquisition representation are still unfinished.
No new language feature, runtime operation or interface encoding is introduced here.

## Shared target and observation checkpoint, 2026-10-13

This continuation follows `6d8a4a6a` on `feature/bra-620-repair-synchronization-authority`. BRA-620 remains incomplete.

Ordinary reference slots and references stored in addressed memory now use one sparse target-keyed map. The original
reference supplied at entry is distinct from the parameter slot and from addresses projected through that reference.
Different input references may alias. Only slots owned by this invocation are proven separate from original inputs.
Typed projection preparation is shared by body accesses and selected-call summaries. Returned field addresses retain
their target projections through export, selective import, substitution and specialization identity. Reading a reference
field and taking that field's address produce different relationships.

The shared analysis resolves mutation observations once. Refinement and guarantee consumers use the published result.
It preserves a sibling field only when the reached storage proves separation. Unrepresented protected payloads and
allocation bytes retain conservative invalidation. They cannot publish an empty effect merely because they lack ordinary
observation paths. Dynamic array indices invalidate their enclosing observation. Literal indices are normalized by
storage planning using already checked literal values, replacing the downstream invalidation conversion.

Scoped-call suspension now records live storage without evaluating the invocation or transferring its inputs again.
Aggregate mutation invalidates stored reference fields even when they have no separate addressed-memory metadata.
Subtree transfers and old-epoch targets share one ordering, so old entries cannot interrupt traversal of current fields.
The removed test-only referent algorithm is replaced by calls to the production target resolver.

Evidence is in the primary checkout's shared `target/bra-620-authority/` directory:

- `target-final-libraries.log` and `target-final-checker.log`: 119 bound-tree, 289 checker, 189 package-interface and
  202 symbol tests pass, including the aggregate-reference regression and the final test-helper simplification.
- `target-aggregate-returned_values.log` and `target-aggregate-memory_operations.log`: 21 returned-value and 15 memory
  tests pass after the aggregate invalidation fix.
- `target-final-` import/publication logs: 13 returned-dependency, 34 publication, eight reborrow, eight default-borrow
  and seven storage-plan tests pass.
- `target-final-authority.log`: 134 pass and 16 fail after the final audit and formatting, compared with 131 pass and
  18 fail at the preceding checkpoint. The protected double-move, overlapping/dynamic selector and
  synchronous/asynchronous scoped-authority regressions all pass.
- Broader coverage found acceptance work that narrower filters had not exercised. `target-verified-runtime_default.log`
  has 17 passes and one recursive-default query cycle. Default providers still reenter body analysis outside the callable
  component. `transition-scoped_.log` has nine passes and two failures: borrowed scoped storage lacks its physical
  capability plan, and native scoped execution reports missing `CleanupIncidentTransfer` in the runtime host contract.
  These are unfinished stack work, not waived checks.

`cargo fmt --all`, mutating `cargo xtask style`, `cargo xtask format` and `git diff --check` pass. The audit reused
projection typing, target overlap, subtree transfer, input ownership classification and metadata transport. It removed
the mirrored stored-reference state and obsolete test-only target resolution. This checkpoint adds roughly 850 production
lines and 380 test lines, including tests colocated with production. The production growth supplies distinct entry and
reached identities, shared observation publication and portable projected-address contracts. It introduces no runtime
witness state or generated-code operations. Compiler cost still needs the plan's performance acceptance.

Next, complete the internal protected-payload and nested-reference representation, finite effects and retirement, and
shared component ownership for default providers. Keep the distinct slot, reference value, reached bytes, epoch and
acquisition authority throughout. The accepted recursive-path, must-summary, deferred-effect and production protocol
requirements below remain active. No new Bray syntax, runtime representation or LLVM alias assertion was introduced.
Native performance, binary size and the final production guard layout have not yet met acceptance.

## Shared reference producer checkpoint, 2026-10-12

This continuation follows `88bd8ca4` on `feature/bra-620-repair-synchronization-authority`. It is an incomplete BRA-620
checkpoint. The production authority requirements below remain active. Read this section before the older checkpoints.

Callable components now publish the checked body and its dependency summary together. Summary consumers reuse that body.
The shared storage flow supplies returned reference origins, observable input-field replacements, returned lifetime
requirements, and formal mutation effects. Runtime defaults use the same body publication. Selected default-provider
contracts are prepared once and reused across their requirements and effects.

The separate `dependency/result/borrows.rs` syntax analysis and `storage/expression/result.rs` lifetime-as-referent planner
are removed. The execution checker no longer resolves a second set of returned-reference summaries. Storage planning
provides expression storage independently of inferred results. Ordinary calls and assignments snapshot reference values
before mutation and publish their outputs on normal completion through the same pending transition. Nullable absence
participates in the existing reference join. A missing, moved, or unknown present value cannot prove exact identity.

Portable mutation summaries distinguish direct slot changes, changes through an entry reference, and unknown effects.
Export, import, substitution, validation and specialization identity preserve those summaries. An absent summary means
unknown mutation, not purity. Package interfaces retain revision 1 because this repository requires no compatibility
with the superseded development encoding.

Cleanup preserves an external reference only when its entry and exit subjects are identical. A replacement relation does
not justify a postcondition about the old reference. Address formation transfers the actual borrowed target independently
of its lifetime anchor. Content writes retain intermediate lifetime dependencies even when they revoke all old content
facts, so a newly established pointer guarantee still expires with the original owner.

Evidence is in the shared primary checkout's `target/bra-620-authority/`:

- `normal-exit-address-checker.log`: 283 checker tests pass.
- `normal-exit-data-contracts.log`: 119 bound-tree, 189 package-interface and 202 symbol tests pass.
- The `normal-exit-final-` logs record 19 returned-value, 15 memory-operation, 34 publication, 12 imported-result and eight
  default-borrow tests passing after the final address correction. Five substitution tests and two specialization-identity
  tests also pass after moving dependency logic into focused modules.
- `normal-exit-fix-consuming_cleanup_retains.log` and `normal-exit-fix-default_borrows.log`: cleanup replacement rejection
  and eight default-borrow tests pass.
- `normal-exit-address-trusted.log`: 131 pass and 18 fail. Seventeen failures are retained production/authority work.
  The additional `production_guard_borrow_origin_reaches_the_protected_payload` failure must be repaired in this issue.
  The earlier deferred-producer failure was a stale pure/opaque fixture and is corrected.
- `address-fix.log`: nine address tests pass, including the owner-lifetime and distinct-origin regressions. The broader
  `raw_allocation_address_copies_preserve_whole_owner_transfer` test fails while checking production `std.memory` bodies.
  Its moved-storage, initialization, trusted-obligation and totality diagnostics remain BRA-620 work.

The mandatory mutating `cargo xtask style`, `cargo xtask format`, `cargo fmt --all` and `git diff --check` pass.

The maintainability audit removed duplicate body/default summary construction and shares reference classification,
projection typing, subtree transfer and join operations with the existing domain owners. Reference implementation roots
are thin modules. Dependency substitution and specialization encoding live in focused sibling modules under their existing
owners. The diff against `88bd8ca4` adds approximately 1,800 production lines and 1,000 test lines, excluding documentation.
The added production code supplies the checked target state, normal-output transitions, portable effects and shared body
publication that replace the two deleted analyses. Moving functions between modules is not counted as simplification.
The next representation work must resolve symbolic borrowed targets and protected payload views in the owning shared
flow. A formal reference identity is not evidence of a disjoint physical allocation. Do not reinstate the deleted syntax
inference to restore the guard summary. The current physical resolver cannot express the full path through `Uninit<T>`.
Preserve the distinction between a reference slot, its reached storage, a protected payload, and the owner supplying
lifetime/authority. Check parameter aliasing, nested borrowed fields, payload epoch retirement, callee replacement, and
source/imported summaries together. No new source syntax or field-privacy assumption is authorized.

Other consolidation still required by the accepted plan: finite cleanup effects and actual retirement transitions,
invocation versus deferred effects, demanded-component graph reuse, finite recursive path closure, recursive must candidates, and replacement of the
remaining separate lifetime inference. Finish these and the production protocol matrix before treating BRA-620 as ready.
No binary-performance, binary-size or native guard-layout acceptance claim is made by this checkpoint.
## Shared storage transition checkpoint, 2026-10-12

Implementation is in progress in `feature/bra-620-repair-synchronization-authority`, after the published recovery and plan
commits named in the execution guide. This checkpoint records the changes below in the existing repair worktree. They do
not establish completion of BRA-620 or replace the production acceptance requirements.

Moves, initialization, absence, replacement, cleanup and mutation effects now use shared storage paths. Ordinary borrows
and raw addresses resolve against the same target state. A derived path does not need a matching source expression to
publish a move or mutation. The former access-ID mutation representation and conversion back to source IDs are removed.
Unknown reference targets remain conservative. Borrow-value moves retain their checked type so they transfer the borrow
slot without marking its referent moved. Paths share immutable projection arrays instead of copying them on each lookup.

Content mutation retains only compiler-defined spatial facts whose validity does not depend on the changed contents.
Overwriting a pointer or witness carrier and ending its owner lifetime still revoke the dependent evidence. Arbitrary
opaque predicates receive no preservation exemption. The old global invalidation and witness reconstruction path remains
outstanding and must be replaced with the formal effect model.

Current checks, with logs under the shared primary checkout's `target/bra-620-authority/`:

- Bound-tree and checker unit tests: 118 and 273 pass respectively.
- Raw-memory transitions: 12 pass, including an effect whose physical target has no source access ID.
- Reborrowing: 8 pass. Returned values: 14 pass. Condition refinements: 5 pass.
- Storage flow: 14 pass. Cleanup: 9 pass. Pattern lowering: 4 pass.
- Scoped mutable alias postconditions and imported trusted-contract propagation pass.
- `cargo test -p bray-compilation --lib trusted_contracts`: 129 pass and 20 fail, matching the preceding failure set.
  These remain active BRA-620 failures, including production synchronization and host checks. See
  `path-effects-trusted_contracts.log`. Earlier production successes below are historical evidence.
- `cargo xtask style` passes. The storage-retention stress test passes in 82.23 seconds versus an earlier 75.50-second run.
  These single runs do not establish a performance improvement. No current native or production guard-layout claim is made.

Next, derive guarded formal reference, mutation and carrier summaries from the shared flow and solve demanded recursive
components before publication. Remove the separate `dependency/result/borrows` syntax walk and whole-body exclusions.
Storage planning and structural cleanup preparation currently request inferred reference summaries. Resolve that phase
dependency as part of the owning component analysis, without a later validation pass or a second reference analysis.
A current observation named `Input(place)` is not by itself proof that the entry reference survived mutation. Reference
identity, changed contents and lifetime dependencies must remain distinct through calls, cleanup and joins.

Continue with the production protocols and full rule-derived acceptance in the Linear issue after that consolidation.
No new language syntax, field-privacy assumption or weaker authority contract is part of this checkpoint.

## Call dependency publication checkpoint, 2026-10-12

The shared storage transition work above is committed locally as `dca2312c`. The following continuation separates
structural cleanup selection from inferred relationships and removes repeated selected-call checking.

Structural cleanup preparation no longer requests inferred lifecycle reference summaries. Dependency checking
instantiates each selected lifecycle once per callable instance and publishes the relationships in the existing
checked dependency result. Body flow and execution-property consumers borrow that result. A regression walks the
structural preparation dependency graph and rejects an inferred-result component anywhere in its prerequisites.

Ordinary and scoped calls now publish their separate invocation, returned-value and deferred contracts once. Liveness
consumes those contracts directly, including thread-result retention. The duplicate call instantiation and its error
paths are removed from liveness. Body checking prepares value-source data once and transfers it to liveness after
dependency checking. The unused independent dependency-checker service is removed.

Checks pass for 273 checker tests, 119 bound-tree tests, and 78 focused compilation tests covering publication,
returned values and assignments, reborrowing, async retention, recursive and trait result dependencies, cleanup and
memory operations. The authority suite remains at 129 passing and the same 20 failing tests. Logs use the
`call-publication-` prefix in the existing evidence directory. The mutating style check passes.

This completes the cleanup-preparation separation and shared call-contract consumption. It does not yet replace the
reference-summary producer. Storage planning still requests returned-reference summaries. The syntax-based result
inference and the ordinary execution-flow reference adapter also remain. The next unit must replace those dependencies
with symbolic reference slots and summaries projected from shared flow, covering input roots, mutation, normal exits
and recursive component publication together. Do not infer entry identity from a refreshed `Input(place)` observation.

## Original issue and stack

- [BRA-620](https://linear.app/bray-lang/issue/BRA-620/complete-standard-library-trusted-storage-and-foreign-contracts): complete standard-library trusted storage and foreign contracts.
- [PR #604](https://github.com/lejmer/bray/pull/604), currently draft. Linear remains In Progress.
- Branch: `feature/bra-620-complete-standard-library-trusted-storage-and-foreign`.
- Head: `31ee6a0025c3da4f7b95a4cf3fdcc41be1e6b150`.
- Base: BRA-619 branch at `3f6e93090ec11968a6df225f927353d444f6239e`.
- BRA-621 / PR #605 follows this PR. The inspected worktree is on BRA-621 at `b4f6a587291d692cd8fe9874522558075a58d471`.

These branch heads describe the original investigation. Revalidate live heads and ownership of local changes before editing, and preserve dependent changes when updating the stack.

## Completed work and validation

The PR establishes storage and foreign-call contracts, source/imported authority propagation, initialization evidence, and indexed witness invalidation. Its first review found unconditional Windows TLS output initialization and scalar initialization invented by an empty copy. Both findings were fixed in `81a6837d50dee69d095d32a1713b2f86e5aa5fde`, their threads were resolved, and [the next review found no additional actionable issues](https://github.com/lejmer/bray/pull/604#pullrequestreview-5471182245).

That review required keeping the PR draft until BRA-621 repaired bootstrap storage and the blocked native API/outcome tests ran. BRA-621 has now done that: at `b4f6a587291d692cd8fe9874522558075a58d471`, the normal native API audit passed all 159 tests across seven plans and the outcome audit passed all 24 expected outcomes. Retained local evidence is in `target/bra-621-outcomes/full-native-api.log` and `native-outcomes.log`. These results were obtained on the combined stack, not on isolated BRA-620.

The audit blocker is cleared. The authority defect below and the later unreviewed refactor remain.

## Confirmed authority defect

The recorded investigation checked both actual source and imported standard-library interfaces. An ordinary caller can use internal access to construct a `SpinLockGuard` with a lock reference and `active = true`, then call `get()`, without acquiring the lock.

`spin_lock_guard_pointer` is trusted and publishes initialized, exclusive, synchronized storage guarantees. Its implementation checks only the ordinary `active` field before returning the lock storage address. That Boolean check does not prove that this guard acquired the lock. Consequently, safe code can obtain authority over protected storage through fabricated ordinary fields.

Internal access acknowledgement is not trust acknowledgement. A copied flag, lock reference, exposed address, declaration spelling, or current atomic state must not establish a caller's ownership of an acquisition. The source still has this shape at the inspected BRA-621 head. The source/imported acceptance result is retained investigation evidence, not a new reproduction run performed while writing this handoff.

The issue's [Linear investigation comment](https://linear.app/bray-lang/issue/BRA-620/complete-standard-library-trusted-storage-and-foreign-contracts) records this defect and requires auditing mutex, read/write lock, and once-cell producers as equivalent paths.

## Where to resume

| Path | Relevant behavior to inspect |
| --- | --- |
| `standard-library/std/src/sync/spin_lock.bray` | Guard construction after acquisition, nullable try-acquisition, `get`, pointer guarantees, unlock, and destruction. |
| `standard-library/std/src/sync/mutex.bray` | Equivalent ordinary `active` field and trusted mutable-storage pointer producer. |
| `standard-library/std/src/sync/rw_lock.bray` | Read and write guard acquisition, shared versus exclusive authority, try-acquisition, pointer access, and release. |
| `standard-library/std/src/sync/once.bray` | Initialization attempt ownership, successful publication, initialized reads, failure/abandonment, and pointer producers. |
| `crates/bray-compilation/src/compilation/trusted_contracts/` | Source/interface contract publication and witness handling. |
| `crates/bray-checker/src/analysis/guarantee/trusted/` and `execution_guarantees/` | Existing authority propagation, conditional proof, movement, and invalidation mechanisms. |

Read the [trusted caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md), [trusted witness values](../language/contracts-and-trust/trusted-witness-values.md), [obligation propagation](../language/contracts-and-trust/obligation-propagation.md), and [lifecycle and foreign boundaries](../language/contracts-and-trust/lifecycle-and-foreign-boundaries.md) contracts.

Establish live acquisition evidence at the successful acquisition boundary. Require matching live evidence when protected storage authority is produced. Preserve it through legitimate guard movement, borrowing, nullable successful acquisition, and public access. Revoke it when the guard or relevant owner changes, is released, or is destroyed. Distinguish shared read authority from exclusive write/initialization authority.

Use the existing trusted predicate and dependency mechanisms. Audit producers and consumers together so normal standard-library calls retain their authority. Do not introduce compiler exemptions for standard-library names, grant authority through ordinary guard fields, or weaken caller checking to make bootstrap compile. Add new representations only when they enforce the actual acquisition invariant and cannot be expressed clearly through existing mechanisms.

Mutex, read/write lock, and once-cell findings have not all been independently reproduced in this handoff. Treat them as mandatory sibling audit targets, not as four already proven identical defects. Cover the acquisition, access, release, and failure paths that each API actually implements.

## Earlier compiler repair checkpoint

The active, uncommitted repair is on `feature/bra-620-repair-synchronization-authority`, based on the combined BRA-553 tip
`cca4bd4429d0648cae0493db72045b7d4a7a3e73`. It retains the earlier BRA-620 and BRA-621 work. The primary checkout has separate
handoff changes that must be preserved.

Execution propagation, cleanup evidence collection, and trusted validation now share one prepared cleanup transition.
Replacement partitions, definitely moved parts, and conditional cleanup use existing storage-flow facts. The earlier
232-test result remains in `target/bra-620-authority/cleanup-availability-broad.log`. Later checked dependency inference
carries exact returned-reference origins and unchanged stored-reference identities through generic export/import.
The owning inference also publishes whether each reference is a language borrow or a raw address. Execution flow consumes
that category without reconstructing result projection types.

The previous ordinary SpinLock acquisition failure is resolved. The current six-test production selection passes actual
synchronization declaration bodies, SpinLock/Mutex acquisition and forged-guard rejection, nullable SpinLock acquisition,
ConditionVariable release/reacquisition, and the production payload-origin check. Seventeen origin-related tests pass,
including an encoded package round-trip carrying both borrow and address origins. Evidence is in
`target/bra-620-authority/reference-kind-production.log` and `reference-kind-origin-tests.log`. These focused results do not
replace the remaining native and full standard-library checks.

BRA-620 remains incomplete. Mutation through a guard still loses acquisition authority, and RwLock writer-overflow unwind
still loses initialization evidence needed for cleanup. Exact origin alone does not establish preservation of an opaque
predicate. A new negative regression confirms that an opaque carrier predicate must be revoked when its referenced payload
changes. The user authorized investigating an acquisition witness using existing language features, with adoption conditional
on proving authority and acceptable performance. No new language syntax or replacement production representation has been added.
Do not infer permission to preserve arbitrary opaque predicates from the successful origin tests.

Once and the bootstrap report pool still need their authority audit. Full source/imported acceptance, native validation,
the complete maintainability audit, integration-stack reconciliation and published review remain required.

## Acquisition witness investigation, 2026-10-12

An empty `Permit` with an opaque `held(&permit)` predicate can be stored beside an immutable owner reference in a guard.
A whole-value destructor requiring the permit prevents detachment from leaving a valid original guard. This uses public
fields and existing mutation/lifecycle rules, not field privacy. The retained synthetic regression checks nine cases:
acquisition and whole-guard movement succeed; fabrication, fabrication beside a live witness, detachment, whole-source
replacement, fresh-result detachment, immutable-field restoration, and consuming-pattern detachment are rejected with
specific authority, mutation, or lifecycle diagnostics. This is evidence for the candidate shape, not a completed proof
against production declarations, imported contracts, and every acquisition path.

The retained codegen layout comparison gives the empty permit size 0 and alignment 1. Both the baseline owner-reference
wrapper and the wrapper with a permit have size 8 and alignment 8 on the tested baseline target. Evidence is in
`target/bra-620-authority/acquisition-permit-verified.log`, with two passing tests. This establishes no added storage for
that shape. It does not establish compile-time cost or optimized/unoptimized instruction and cleanup cost.

Two positive experiments still fail, so the candidate was not adopted:

- An ordinary `get()` returning the exact mutable payload reference followed by `value += 1` loses the independent permit
  obligation. `analysis/storage_invalidation.rs` currently classifies writes through local borrows as global invalidation.
  Evidence: `acquisition-permit-payload.log`.
- A const helper mapping `&Guard?` to `(&Permit)?` is accepted, but the acquired condition on that helper result does not
  prove `held(&guard.permit)` after consuming the present nullable guard. Evidence: `acquisition-permit-nullable2.log`.
  The first nullable experiment used the wrong nullable-borrow spelling and is not evidence for this conclusion.

The unsuccessful experimental test sources are retained locally in
`target/bra-620-authority/acquisition-witness-probes.rs`, outside the passing regression suite. Addressing these findings
requires compiler work in the owning mutation and contract analyses. Do not add downstream revalidation, preserve all
opaque conditions, or treat the measured empty layout as proof of complete performance neutrality. The next discussion
should distinguish that compiler work from introducing an explicit preservation contract. Actual production/export/native
proof and performance comparisons remain gates before adoption.

## Required regression coverage

Add negative source and separately compiled/imported tests for fabricated guards or attempts, ordinary internal field construction, copied or replaced authority fields, owner mismatch, stale authority after release, and mutation that invalidates a live witness. Verify that supplying only an ordinary condition cannot discharge a trusted acquisition obligation.

Add positive coverage for ordinary acquisition and access, successful nullable try-acquisition, legitimate movement and borrowing, release/destruction, read sharing versus write exclusion, and once initialization/publication. Failed acquisition must not publish success authority. Include tests that preserve valid bootstrap use of the report-pool lock.

Prefer actual standard-library producers and interfaces to synthetic lookalikes. A negative test must reach the trusted authority boundary and assert the specific rejection, rather than merely failing on unrelated syntax or visibility.

## Review gap

The last clean review covers `81a6837d50dee69d095d32a1713b2f86e5aa5fde`. The current head adds `31ee6a0025c3da4f7b95a4cf3fdcc41be1e6b150`, `refactor: remove checker infrastructure failures`, which has no subsequent published review. Its diff spans 193 files, with 2,814 insertions and 7,139 deletions. Do not treat the earlier clean review as covering this refactor.

Review the full PR and the later refactor, especially invariant enforcement at producers, typed operational/input failures, preservation of specific diagnostic causes, source/imported boundaries, and removal of recoverable errors representing impossible compiler states. Avoid restoring broad infrastructure-error plumbing as a shortcut. Extend the behavior review matrix with the acquisition-authority audit and regression results before requesting review.

## Validation and completion

Read AGENTS.md, coding conventions, and the Bray debugging/language skills before implementation. Reuse configured shared Cargo and LLVM storage. Run focused source/imported contract tests during the repair and affected compiler/checker suites after the sibling audit. Native LLVM/linker/executable workflows must run outside the Codex sandbox on Windows.

After the systemic repair, verify the normal combined native pipeline:

```text
cargo xtask standard-library test --part api
cargo xtask standard-library test --part outcomes
```

After Rust changes run the mutating `cargo xtask style`. Finish with `cargo xtask format` and the repository compliance/maintainability audit. Preserve known independent structural style warnings outside this functional repair unless the user extends the scope. Record exact heads and distinguish focused results from broader audits.

The user replaced the original PR #604 publication plan on 2026-10-11. Finish this repair on the combined source, then
reconcile the stack through `feature/trusted-storage-integration` as described in the [shared completion plan](stack-completion.md).
Publish the combined draft PR into `develop` and obtain review there under the [Bray issue workflow](../../.agents/skills/work-on-bray-issues/SKILL.md).
The reviewer must publish its summary and genuine actionable inline findings on that integration PR. Verify those artifacts
and repeat review after material fixes until clean or concrete disagreements are recorded.

Only after the authority defect, sibling audit, validation, and current-head review are complete should BRA-620 move to
In Review. Do not mark it Done. Keep old PRs available until the integration lands, then reconcile their superseded status
without closing incomplete issues. BRA-619's missing native scoped-incident transfer remains documented in
[its handoff](BRA-619-remaining-work.md).
