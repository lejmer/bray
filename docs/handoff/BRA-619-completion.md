# BRA-619 completion

## Contract and baseline

Start at develop `055f1d4f`, after #608, #609 and #610 were squash-merged.
Use `feature/bra-619-complete-selected-scope-native-cleanup`. Preserve the old
implementation branch and its history. BRA-619 remains In Progress until its
complete acceptance and published independent review pass.

Retain selected entry/exit checking and the shared cleanup transitions. Complete
native typed exit-error ownership without new syntax, language features, a second
cleanup walker, post-failure allocation, discarded errors or weaker authority.

## Implementation map and gaps

| Obligation | Existing owner | Finding and required evidence |
| --- | --- | --- |
| Selected entry/exit and activity | lowering/scoped, shared ordinary/abnormal cleanup | Retain exactly-once guards. Cover failed entry, panic and cancellation. |
| Pre-entry exit allowance | outgoing MIR and concrete requirement realization | Value allowances exclude scoped exit. Represent the selected invocation in the existing admission operations and register rollback through existing input cleanup. |
| Error backing | LLVM outgoing activation/retirement, bootstrap storage leases | Call retirement currently releases completion backing before incident capture. Preserve ownership through capture and discharge only the scope reservation. |
| Typed transfer | TransferCleanupIncident, static finalizer descriptor/callbacks | Operation currently forwards arbitrary values to an unbound role. Reuse typed descriptors and the existing admitted report owner. |
| Pending outcomes and dependencies | shared cleanup outcomes and dependency retirement | Retain the pending transfer independently. Incident payload and providers must survive until their last consumer. Do not substitute immediate disposal for retained ownership. |
| Consumption | Bray report consumer and incident consumer | Reuse output, exact-once destruction and backing release, including callback failures. |
| Source/imported contracts | semantic selection, exported executable MIR | Preserve stale-witness rejection and valid explicit transfer. Cover serialization and generic realization. |

## Acceptance matrix

Synchronous and asynchronous entry/exit. Failed entry without exit. Normal and
yield exit errors. Pending return, break, continue, nullable/result/run-result
propagation, panic and cancellation. Exit and payload destruction exactly once.
Allocation denial after admission. Large and over-aligned errors. Borrowed/provider
dependencies. Source and imported trusted contracts, stale witnesses and valid
explicit transfer. Preserve original failing native artifact regression.

Batch coherent changes before tests. Run affected compiler and native compositions,
then style and format. Audit each changed behavior against repository equivalents.
Publish the PR before independent review and push all review fixes. No completion
claim is supported by the original artifact regression alone.

## Design limits

Extend existing admission, temporary rollback, descriptor and report ownership.
Delete replaced interpretations when their consumers switch. Reconsider any design
that adds another arena or registry, per-move allocation, repeated selected-call
analysis, or a downstream authority validation pass. Track production and test
growth separately and keep cost evidence distinct from correctness.

## Shared incident ownership findings

The first implementation batch adds selected-exit admission before entry and guarded
rollback through the existing input-temporary cleanup. It does not complete native
incident transfer. The original native regression remains enabled.

The native consumer cannot be connected soundly by forwarding the error operand:

- `dependency/call/contract.rs::selected_scoped_contracts` supplies no result access
  for ScopedExit. `storage_flow/.../references/invocation.rs::invocation_result`
  likewise returns None. Exit-result dependencies therefore need publication and
  guarded transfer in the owning analyses before native retention is enabled.
- `synthetic/lifecycle/storage.rs` offers task cancellation broadcast and lifecycle
  resolution. Resolution also destroys a task during cleanup. There is no separate
  payload quiescence operation that leaves the error owner retained.
- `synthetic/lifecycle/representation.rs::child_lifecycle_operations` finalizes
  represented children during destruction. Reusing that operation alone does not
  establish that an incident's graceful finalization was abandoned and all its
  asynchronous obligations were resolved before retention.
- `report/incident.bray::dispose` reports and destroys a static incident immediately.
  Scoped incidents instead need the existing report-record chain to own their
  concrete payload and backing through the surrounding run boundary. A thread-only
  queue would incorrectly mix suspended tasks or nested synchronous roots.

The required shared correction is to represent the returned error, its guarded
transfer and its retained dependencies in the existing operation graph. Quiescence
must resolve child runs and inactive captures without disposing of the retained
error. Terminal disposal must consume that established state. Retention must use
admitted concrete storage and existing report records, with the current run owning
those records across suspension and nested entry. No new syntax, parallel cleanup
walker or downstream proof pass is proposed.

BRA-505 is the existing open owner for shared async/fallible abandonment. The user approved completing the shared prerequisite in a BRA-505 PR first,
then stacking BRA-619 on it. BRA-505 is In Progress and blocks BRA-619 in Linear.
Its broader activation migration remains outstanding until its own prerequisites
and acceptance are satisfied. The unproved retained-payload path has not been enabled.

## Checkpoint evidence, 2026-10-15

The scoped batch passes 16 tests, excluding only the unchanged native artifact
regression already reproduced as `MissingRole(CleanupIncidentTransfer)`. The full
`bray-ir`, `bray-lowering` and `bray-package-interface` suites pass 44, 39 and 190
tests respectively, with doc-tests passing. `cargo xtask style`, `cargo xtask format`
and `cargo +stable fmt --all` completed. This is admission and rollback evidence,
not completion evidence for typed incident retention.

The audit reused the existing input-temporary rollback and shared admission,
discharge and concrete invocation requirement paths. It removed the superseded
recoverable invalid-cleanup-target diagnostic and its now-unused formatting helpers.
The executable template version changes because outgoing owners now have tags.
The remaining native regression stays enabled for BRA-505/BRA-619 acceptance.
