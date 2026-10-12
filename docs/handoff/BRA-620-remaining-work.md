# BRA-620 remaining work

Recorded on 2026-10-10. This handoff covers the remaining trusted synchronization authority defect and review requirements.

Updated on 2026-10-11: use the [current stack completion plan](stack-completion.md) for work order and status.
This document retains BRA-620's evidence and acceptance details. The earlier boundary before BRA-553 is retired.

The user accepted the architectural groundwork on 2026-10-12 and requested self-contained Linear issues.
Use the [Linear execution guide](runtime-stack-linear-execution.md) for the exact recovery checkpoint and ordered work.
The [rule coverage and replacement plan](runtime-stack-rule-coverage.md) and [closed inventory](runtime-stack-coverage-inventory.md)
replace the earlier proposal. The [architectural assessment](stack-architecture-assessment.md) and the instructions below
retain dated evidence and acceptance requirements. Issue preparation did not begin implementation.

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
