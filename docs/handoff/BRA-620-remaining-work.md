# BRA-620 remaining work

Recorded on 2026-10-10. This handoff covers the remaining trusted synchronization authority defect and review requirements.

Updated on 2026-10-11: use the [current stack completion plan](stack-completion.md) for work order and status.
This document retains BRA-620's evidence and acceptance details. The earlier boundary before BRA-553 is retired.

## Issue and stack

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

Commit and push coherent fixes, update the stacked draft PR, then obtain a fresh subagent review under the [Bray issue workflow](../../.agents/skills/work-on-bray-issues/SKILL.md). The reviewer must publish its summary and all genuine actionable inline findings directly on PR #604. Verify the artifacts and repeat review after material fixes until clean or concrete disagreements are recorded.

Only after the authority defect, sibling audit, validation, and current-head review are complete should PR #604 become ready and BRA-620 move to In Review. Do not mark it Done. BRA-619's missing native scoped-incident transfer remains documented in [its handoff](BRA-619-remaining-work.md). Use the shared completion plan for sequencing.
