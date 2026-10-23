# BRA-619 remaining work

## Active sequence and baseline

Complete [BRA-505](https://linear.app/bray-lang/issue/BRA-505) in PR #612, then
[BRA-619](https://linear.app/bray-lang/issue/BRA-619) in a dependent PR.
Use [the active prerequisite record](BRA-505-retained-incidents.md) until it is complete.
The selected-scope implementation from #602 is included through merged #607.
The recovery branch `feature/bra-619-complete-selected-scope-native-cleanup` at `6bbcb4a3`
preserves subsequent admission and rollback work. Inspect its diff against the completed
BRA-505 branch and retain only work still needed. Do not replay old prerequisites.
BRA-502/554/558 are merged and BRA-561 is reviewed. BRA-505 must supply retained-error
quiescence, abandoned destruction and admitted payload transfer before this consumer connects them.
The native missing-role regression stays enabled and must pass before completion.

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
| `runtime/bootstrap/src/outgoing/` and `report/` | Existing outgoing report admission, record ownership, transfer, and retirement mechanisms to examine for reuse. |
| `crates/bray-compilation/src/compilation/lowering/tests/lifecycle.rs` | Existing source/MIR lifecycle coverage. |
| `crates/bray-compilation/src/compilation/product/codegen/implementation/tests/lifecycle.rs` | Native artifact regression and related cleanup coverage. |

Build one coherent compiler-to-runtime transfer path. Trace the actual error type, physical owner, admission/reservation, report representation, consumer lifetime, and destruction. Preserve ownership exactly once, including errors whose values need destruction. A bare pointer or discarded error is not a completed transfer. Do not invent a fixed native signature for arbitrary typed errors without following their existing representation and cleanup mechanisms.

Search structurally equivalent finalizer and cleanup-incident paths before choosing the implementation. Reuse existing lifecycle and reporting machinery. Keep established compiler phase invariants, and do not hide the missing role through fallback behavior or weaker validation.

## Transfer constraints

Retain the exact error payload, physical backing and provider/attachment dependencies until
run-boundary consumption. Reuse the existing report chain, concrete callbacks and outgoing
allowances. Admit size, alignment and callback obligations before failure. Preserve separate
reporting, destruction and release outcomes, including failures in each operation. Resolve child
runs and inactive captures before retaining an incident without replaying graceful finalizers.
No second arena, Rust payload registry, borrowed stack pointer, immediate disposal, allocation
on failure or weakened role validation satisfies this ownership contract.

## Validation and review

Start with the existing failing regression, running native workflows outside the Codex sandbox on Windows:

```text
cargo test -p bray-compilation --release scoped_selected_calls_emit_native_artifacts -- --nocapture
cargo test -p bray-compilation --release scoped_ -- --nocapture
```

Add meaningful native execution coverage for successful entry/exit, failed entry without exit, normal completion with exit failure, and pending return/control/panic/cancellation with exit failure. Check that exit runs once, the original pending outcome survives, the suppressed error remains owned, and its eventual disposal occurs once. Include asynchronous forms, source/imported contracts, stale scoped witnesses, and valid explicit authority transfer as required by the issue. These variants were still under audit in the earlier checkpoint.

Read AGENTS.md and the coding/debug/language skills before implementation. Reuse the configured shared Cargo and LLVM storage. After Rust changes run the mutating `cargo xtask style`, and finish with `cargo xtask format`. Run affected suites and native API/outcome audits when changes to shared transport warrant them. Do not repeat unchanged broad checks just to start review.

Follow the [Bray issue workflow](../../.agents/skills/work-on-bray-issues/SKILL.md) for
independent review, readiness and Linear status. Remove this handoff after implementation,
acceptance, review and any fixes are pushed. Keep durable behavior in specifications and tests.
