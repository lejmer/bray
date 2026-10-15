# BRA-619 remaining work

Recorded on 2026-10-10. This is an implementation handoff, not a change to the language contract.

Updated on 2026-10-11: use the [current stack completion plan](stack-completion.md) for work order and status.
This document retains BRA-619's evidence and acceptance details. The earlier boundary before BRA-553 is retired.

## Current integration, 2026-10-15

PR #602 is closed as superseded by [#607](https://github.com/lejmer/bray/pull/607). Its implementation and history
are already included in `feature/trusted-storage-integration`. Keep the original branch. BRA-619 remains In Progress.
[Plan Step 6](runtime-stack-rule-coverage.md#6-finish-bra-619-scoped-execution-and-native-acceptance-in-this-pr)
explicitly completes this work in #607 after Step 5's BRA-502 -> BRA-554 -> BRA-558 prerequisites. The typed incident,
pending-outcome and provider/backing lifetime contracts below constrain the current Step 4 analysis and Step 5 design.
The missing-role regression remains enabled and blocks final PR acceptance. Do not treat separate issue ownership
as exclusion from this PR or as permission to declare the stack complete.

## Original issue and stack checkpoint

- [BRA-619](https://linear.app/bray-lang/issue/BRA-619/implement-selected-scope-enter-and-exit-invocation): implement selected scope enter and exit invocation.
- [PR #602](https://github.com/lejmer/bray/pull/602), draft at this original checkpoint. Linear remains In Progress.
- Branch: `feature/bra-619-implement-selected-scope-enter-and-exit-invocation`.
- Head: `3f6e93090ec11968a6df225f927353d444f6239e`.
- Base: BRA-609 branch at `a96404055a81bfa7e9d0db2ba35d8df893f5fe21`.
- Later stack entries are BRA-620 / PR #604 and BRA-621 / PR #605. The inspected worktree is on BRA-621 at `b4f6a587291d692cd8fe9874522558075a58d471`.

These branch heads describe the original investigation. Revalidate live heads before editing and preserve dependent changes.

## What is implemented

Selected enter and exit calls flow through semantic selection, storage and dependency checking, trusted contracts, asynchronous execution, and lowering. Scoped bindings use the entered capability. Lowering retains a protocol activity guard and clears it before invoking exit. Existing source/MIR tests cover distinct entry/exit inputs, borrowed receiver lifetime, asynchronous exit shielding, fallible result types, and preservation of pending control outcomes.

The original implementation recorded passing affected Rust suites and formatting/style checks. These results do not establish native completion of fallible scoped cleanup.

## Functional gap

The missing path transfers an owned error from a fallible exit to the native runtime when another outcome is already pending.

For example, the body starts returning a value, then exit returns an error. The original return must continue, and the exit error must be retained as a suppressed incident for later consumption and disposal. The same rule applies to break, continue, propagation, panic, and cancellation. Normal body completion or yield instead lets the exit error contribute to the `with` expression's failure result.

This behavior is already specified in [with expressions](../language/lifecycle/with-expressions.md). Also read [scoped use](../language/lifecycle/scoped-use.md), [scope exits, panics and cancellation](../language/lifecycle/scope-exits-panics-and-cancellation.md), and the trusted caller, witness, and propagation contracts under `docs/language/contracts-and-trust/`.

The recorded native failure is:

```text
InvalidExecutableHost(IncompatibleRuntime(MissingRole(CleanupIncidentTransfer)))
```

The native regression `scoped_selected_calls_emit_native_artifacts` exercises ordinary and fallible enter/exit. The fallible form requires the missing role during compilation even when its example exit body returns success. A potentially reachable error branch still needs a valid native implementation.

## Where to resume

| Path | Relevant behavior |
| --- | --- |
| `crates/bray-lowering/src/lowering/scoped.rs` | `push_scope_exit` emits `TransferCleanupIncident` for an exit error that cannot replace the pending outcome. |
| `crates/bray-ir/src/operation.rs` | Defines the incident-transfer operation. |
| `crates/bray-ir/src/unit/validation/operation.rs` | Validates its runtime role. |
| `crates/bray-runtime-abi/src/catalog/execution.rs` | `CleanupIncidentTransfer` currently has no native binding and is marked compiler-owned. |
| `crates/bray-codegen/src/mapping/demand.rs` and `mapping/model/validation.rs` | Demand and validate the transfer mapping. |
| `crates/bray-codegen-llvm/src/translation/unit/effect/body.rs` | Translates the incident operand and invokes its runtime mapping. |
| `runtime/bootstrap/src/outgoing.bray`, `records.bray`, and `record-pool.bray` | Existing outgoing report admission, record ownership, transfer, and retirement mechanisms to examine for reuse. |
| `crates/bray-compilation/src/compilation/lowering/tests/lifecycle.rs` | Existing source/MIR lifecycle coverage. |
| `crates/bray-compilation/src/compilation/product/codegen/implementation/tests/lifecycle.rs` | Native artifact regression and related cleanup coverage. |

Build one coherent compiler-to-runtime transfer path. Trace the actual error type, physical owner, admission/reservation, report representation, consumer lifetime, and destruction. Preserve ownership exactly once, including errors whose values need destruction. A bare pointer or discarded error is not a completed transfer. Do not invent a fixed native signature for arbitrary typed errors without following their existing representation and cleanup mechanisms.

Search structurally equivalent finalizer and cleanup-incident paths before choosing the implementation. Reuse existing lifecycle and reporting machinery. Keep established compiler phase invariants, and do not hide the missing role through fallback behavior or weaker validation.

## Scope and dependencies

The earlier Linear checkpoint identifies [BRA-502](https://linear.app/bray-lang/issue/BRA-502) and [BRA-558](https://linear.app/bray-lang/issue/BRA-558) as active owners of admission/report consumption work that overlaps this path. Their host work depends on BRA-553. Re-read those issues before deciding where the missing transfer belongs.

The [subsequent dependency investigation](BRA-619-native-transfer-dependency.md) established that the existing transport lacks the physical typed backing and consumer contract. BRA-502 and BRA-558 own those prerequisites, with BRA-554 preceding BRA-558. BRA-553 has since completed implementation and review. Follow the shared completion plan and do not claim BRA-619 complete merely by deferring its native acceptance case.

BRA-621 repaired physical report storage and callback ownership. At its head, the full normal native API audit passed 159 tests across seven plans and the outcome audit passed all 24 expected outcomes. Those suites do not cover the missing scoped-incident binding and do not clear this gap.

## Validation and review

Start with the existing failing regression, running native workflows outside the Codex sandbox on Windows:

```text
cargo test -p bray-compilation --release scoped_selected_calls_emit_native_artifacts -- --nocapture
cargo test -p bray-compilation --release scoped_ -- --nocapture
```

Add meaningful native execution coverage for successful entry/exit, failed entry without exit, normal completion with exit failure, and pending return/control/panic/cancellation with exit failure. Check that exit runs once, the original pending outcome survives, the suppressed error remains owned, and its eventual disposal occurs once. Include asynchronous forms, source/imported contracts, stale scoped witnesses, and valid explicit authority transfer as required by the issue. These variants were still under audit in the earlier checkpoint.

Read AGENTS.md and the coding/debug/language skills before implementation. Reuse the configured shared Cargo and LLVM storage. After Rust changes run the mutating `cargo xtask style`, and finish with `cargo xtask format`. Run affected suites and native API/outcome audits when changes to shared transport warrant them. Do not repeat unchanged broad checks just to start review.

PR #602 had no published reviews at the original checkpoint. Once implementation and validation are complete, prepare the behavior review matrix and follow the [Bray issue workflow](../../.agents/skills/work-on-bray-issues/SKILL.md) for review, readiness and Linear status. Use the shared completion plan for sequencing.
