# BRA-619 native cleanup transfer dependency

Recorded on 2026-10-10 after investigating `BRA-619-remaining-work.md`.

## Status and scope boundary

[BRA-619](https://linear.app/bray-lang/issue/BRA-619) is incomplete. [PR #602](https://github.com/lejmer/bray/pull/602)
must remain draft and Linear must remain In Progress. No compiler or runtime fix is included in this checkpoint.

The input handoff requires an owned, allocation-independent transfer of an arbitrary typed exit error into suppressed
incident storage. It also requires stopping if that work depends on BRA-553. Existing transport does not supply the
necessary typed backing or consumer contract. Completing those shared contracts belongs to the active admission and
incident-consumption owners below. Their prerequisite host work crosses the explicit BRA-553 boundary, so this task stops
without starting it.

## Verified source and stack

The inspected compiler/runtime source is BRA-619 at `3f6e93090ec11968a6df225f927353d444f6239e`, on
`feature/bra-619-implement-selected-scope-enter-and-exit-invocation`. Its base is the BRA-609 branch at
`a96404055a81bfa7e9d0db2ba35d8df893f5fe21`.

GitHub and fetched branch heads matched the original handoff:

| PR | Head before this checkpoint | Base |
| --- | --- | --- |
| [#602](https://github.com/lejmer/bray/pull/602) | `3f6e93090ec11968a6df225f927353d444f6239e` | BRA-609 |
| [#604](https://github.com/lejmer/bray/pull/604) | `31ee6a0025c3da4f7b95a4cf3fdcc41be1e6b150` | BRA-619 |
| [#605](https://github.com/lejmer/bray/pull/605) | `b4f6a587291d692cd8fe9874522558075a58d471` | BRA-620 |

All three PRs reported `MERGEABLE` before this documentation change. Recheck their current heads and mergeability after
publication. The later stack branches have no source changes from this checkpoint.

## Concrete missing contracts

1. `push_scope_exit` in
   [scoped.rs](../../crates/bray-lowering/src/lowering/scoped.rs) emits `TransferCleanupIncident` with the actual
   selected exit error operand. LLVM's matching arm in
   [effect/body.rs](../../crates/bray-codegen-llvm/src/translation/unit/effect/body.rs) passes that value directly to its
   runtime mapping. The operation carries no retained payload allocation, concrete cleanup callback, or release owner.
   [The role catalog](../../crates/bray-runtime-abi/src/catalog/execution.rs) has no native binding for
   `CleanupIncidentTransfer`. Adding a symbol or one fixed argument signature does not establish the error's owner.
2. [Outgoing realization](../../crates/bray-compilation/src/compilation/product/realization/outgoing.rs)
   computes scalar record counts. [outgoing.bray](../../runtime/bootstrap/src/outgoing.bray) reserves those counts, and
   [record-pool.bray](../../runtime/bootstrap/src/record-pool.bray) allocates fixed-size `ReportRecord` slabs.
   [records.bray](../../runtime/bootstrap/src/records.bray) stores a `ReportPrimary` with panic-message callbacks.
   These records do not secure the selected exit error's concrete size, alignment, destructor-generated outcomes, or
   backing-release obligations. `outgoing_retirement` retains an action record for a panic outcome and otherwise
   recycles it. A successful native call returning `Result.Error` has no corresponding retained typed payload.
3. [Static finalization](../../crates/bray-codegen-llvm/src/mapping/static_storage/finalization.rs) derives the concrete
   error layout, allocates its payload after observing failure, copies the error, and publishes `NativeCleanupIncident`
   with reporting and destruction callbacks. It has useful representation and disposal machinery to reuse, but its
   post-failure allocation does not satisfy mandatory cleanup under denied allocation. Its callbacks and memory
   mappings are tied to the static finalization owner. Reusing that path unchanged would preserve the missing admission
   guarantee rather than repair it.
4. [NativeCleanupIncident](../../crates/bray-runtime-abi/src/product.rs) has a type-erased
   payload address, concrete type identity, source, and reporting/destruction callbacks. The existing
   [static cleanup bridge](../../crates/bray-runtime/src/product/cleanup.rs) consumes this representation.
   The ordinary [CleanupReportSink](../../crates/bray-runtime/src/shutdown.rs) instead accepts `RuntimePanic` through
   admitted panic records. [Report consumption](../../runtime/bootstrap/src/report-consumer.bray) understands panic
   primary/message release, not the separate typed-error destruction and backing-release outcomes. There is no existing
   native route connecting the scoped typed operand to that sink while retaining the physical payload and its provider
   through final use.

No second arena, Rust payload registry, discarded error, immediate disposal, borrowed stack pointer, allocation-on-failure
adapter, or weakened runtime-role validation was introduced. None would complete the handoff's ownership contract.

## Ownership and dependency order

- [BRA-502](https://linear.app/bray-lang/issue/BRA-502) owns compiler-derived physical cleanup allowances, including
  concrete typed-error size/alignment, activation, ownership transfer, and release after the last consumer.
- [BRA-558](https://linear.app/bray-lang/issue/BRA-558) owns retained typed incident consumption, separate destruction and
  backing release, suppressed ordering, and carried provider/attachment dependencies.
- BRA-502 is blocked by [BRA-553](https://linear.app/bray-lang/issue/BRA-553). BRA-558 is blocked by BRA-502 and
  [BRA-554](https://linear.app/bray-lang/issue/BRA-554), which is also blocked by BRA-553 and BRA-502.

Linear now records BRA-619's remaining native acceptance as blocked by BRA-502 and BRA-558, preserving its BRA-609
prerequisite. The opposite BRA-619-blocks-BRA-553 relation was removed to avoid a dependency cycle. The selected-call
implementation is already available in the stack. Its unresolved native incident acceptance cannot also be a prerequisite
of the host infrastructure needed to implement that acceptance. BRA-620 and BRA-621 have separate prerequisite relations
and remain unchanged.

Once those owners supply the shared contracts, resume the scoped adapter and native execution coverage on BRA-619.
Reuse the typed finalizer representation and callbacks where applicable. Cover entry success and failure, normal exit
failure, pending return/control/propagation/panic/cancellation, synchronous and asynchronous forms, imported contracts,
stale witnesses, and valid explicit authority transfer. Verify exit and disposal occur exactly once, the pending outcome
survives, and transferred backing remains live. Complete PR review before changing readiness or Linear status.

## Validation and retained work

Native checks ran outside the Windows sandbox against the BRA-619 source, using the existing repository Cargo target
and `target/toolchains/llvm/active`:

| Command | Result |
| --- | --- |
| `cargo test -p bray-compilation --release scoped_selected_calls_emit_native_artifacts -- --nocapture` | 0 passed, 1 failed. Build took 4m 20s, test took 0.26s. |
| `cargo test -p bray-compilation --release scoped_ -- --nocapture` | 8 passed, 1 failed. Test took 0.25s. |

Both failures preserve the original leaf error:

```text
InvalidExecutableHost(IncompatibleRuntime(MissingRole(CleanupIncidentTransfer)))
```

The first case of the native artifact regression, with infallible selected entry/exit, passed before its fallible case
failed. This proves artifact generation only. Native execution and disposal acceptance remains unverified.

Logs are `target/bra-619-resume/native-baseline.log` and `target/bra-619-resume/scoped-baseline.log`. The compact task
record is `target/bra-619-resume/checkpoint.txt`. These are ignored task artifacts, not committed source. No task-owned
Cargo or LLVM installation was created. Shared intermediate storage is `target/cargo/release`, approximately 5.45 GiB
during validation, and shared published release products are under `target/release`, approximately 0.83 GiB at startup.

The final AGENTS.md audit checked this handoff's evidence and all 12 local links. No executable behavior, public API,
serialized format, or language contract changed, so no additional behavior implementation needs extraction or testing.
The required final `cargo xtask format` passed and produced no source changes. Its log is
`target/bra-619-resume/format.log`. All task-started commands finished, and shared build storage was retained.

The source investigation found no independent defect to implement. The static adapter's allocation timing is evidence
for the existing BRA-502 ownership gap, not a new task. No language or design document changed.
