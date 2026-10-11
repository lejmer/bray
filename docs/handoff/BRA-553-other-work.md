# Work outside BRA-553

Use the [current stack completion plan](stack-completion.md) for work order, ownership gaps and status. This document retains the investigation evidence. The earlier BRA-553-only task scope explains why findings were recorded here and does not restrict later authorized stack work.

Recorded on 2026-10-10 while implementing [BRA-553](https://linear.app/bray-lang/issue/BRA-553/establish-bray-host-ownership-and-bootstrap-attachment) on the BRA-621 branch at `fb69346e31b225c39105d095e9f900c9cebad97c`.

The original task was limited to BRA-553. The synchronization defect below belongs to [BRA-620](https://linear.app/bray-lang/issue/BRA-620/complete-standard-library-trusted-storage-and-foreign-contracts) and [PR #604](https://github.com/lejmer/bray/pull/604). It was not repaired in this task.

`standard-library/std/src/sync/spin_lock.bray` still permits an internal caller to construct `SpinLockGuard` from an ordinary lock reference and `active = true`. Its trusted pointer producer checks that flag and publishes exclusive, initialized, synchronized storage authority without requiring evidence of acquisition. The existing investigation in the primary checkout's `docs/handoff/BRA-620-remaining-work.md` records acceptance through source and imported interfaces.

Resume the synchronization-authority repair on its owning stack branch. Audit the equivalent mutex, read/write-lock, and once-cell paths. Preserve acquisition evidence through valid movement and access, require it for release and destruction, and reject authority fabricated from copied or mutated ordinary fields. Do not grant compiler exemptions based on standard-library names or internal visibility.

BRA-553 established its own host acquisition contract while this defect remained. Product membership and ordered thread-static cleanup belong to BRA-557. Source and foreign-entry wrappers belong to BRA-554. General cleanup admission belongs to BRA-502.

## Compiler-owned cleanup incident role

This is the same unresolved native transfer tracked by BRA-619, with BRA-502 and BRA-558 prerequisites. See the [dependency investigation](BRA-619-native-transfer-dependency.md). It is not a separate repair.

The broader lifecycle test run on 2026-10-11 passed 63 tests and failed `scoped_selected_calls_emit_native_artifacts` with `InvalidExecutableHost(IncompatibleRuntime(MissingRole(CleanupIncidentTransfer)))`. The role catalog marks `CleanupIncidentTransfer` as compiler-owned with no native symbol. The fixture supplies every native role, while `RuntimeContract::validate` requires a runtime binding for every requested role. Trace compiler-owned role demand through executable-host planning before changing the fixture or inventing a native symbol. This concerns scoped cleanup incident transfer, outside primitive host attachment. The full failure is retained in `target/bra-553/lifecycle-3.log` in the primary checkout.

## Evidence invalidation across sibling fields

A minimal compiler probe also rejects a valid acquisition lease after assigning an unrelated sibling field. The probe returns `Access { mut payload: u64; lease: Lease; }` with `ensures(trusted live(&result.lease))`. Observing the lease succeeds before `access.payload = 1` and fails afterward with E7123. The lease, its immutable lock reference, and its active flag do not change. Implicit lease destruction also loses the same evidence.

The matching compiler was built from the BRA-553 worktree on the base above. The standalone probe and source-check logs are retained in the primary checkout under ignored `target/bra-553/lease-check.bray` and `target/bra-553/source-check-6.log`. The probe uses the cached `x86_64-pc-windows-msvc` standard-library bundle and one compiler worker.

Repair sibling-field evidence preservation in the owning guarantee-flow and storage-dependency implementation. Preserve evidence for unchanged disjoint fields while still invalidating evidence when its owner, named storage, or dependency changes. Do not exempt host or standard-library names. BRA-553 keeps the lease and payload anchor in separate locals to avoid depending on this repair.

A related execution-proof limitation rejects two owned inputs with the same conditionally total destructor, even when both entry guards hold. Cleanup of the first input conservatively erases observations needed for the second. The initial two-receiver probe is recorded in `target/bra-553/execution-matrix-1.log`. BRA-553 now captures receiver evidence for the actual implicit cleanup invocation and intersects repeated evidence conservatively. It does not preserve arbitrary facts across a preceding destructor.

## Contract pattern export and lost export diagnostics

An interface export rejects a checked `matches none` pattern in a callable contract with `invalid_compilation_checked_template` and `unsupported_structured_expression`. The same contract passes source checking. Preserve the checked pattern through interface templates, or reject it at the owning semantic boundary with a specific diagnostic. BRA-553 removed a redundant field condition already covered by its whole-owner inactive witness and successfully exported the actual bootstrap interface.

The `check --emit-interface` path also reduces this failure to E9005 with an `io other` cause. The equivalent `build --artifact package-interface --format json` preserves the template failure. Repair cause preservation in the check/export command separately. Evidence is retained in the primary checkout under `target/bra-553/export-3.json` and subsequent export logs.

## Owned result proof after mutation through a local borrow

A focused lambda probe with a mutable owned `Flags` parameter, `ensures(result.ready)`, and `executes(total)` cannot prove its postcondition after assigning `borrowed.ready = true` through a scoped `&mut Flags` alias and returning the owner. Direct assignment through the owned parameter proves the same postcondition. The failure is conservative, with `CheckingExecutionGuaranteeNotProven`, and does not admit false authority. The source and failure are retained in `target/bra-553/local-postconditions-2.log` in the primary checkout. Investigate propagation of observed field values from a local alias back to the returned aggregate. BRA-553's host operations do not require that proof. Its regressions check mutable borrow authority and direct result-field proofs separately, including their negative cases.
