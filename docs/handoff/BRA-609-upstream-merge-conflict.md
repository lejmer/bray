# BRA-609 upstream merge conflict

Recorded on 2026-10-10 while checking the BRA-619 stack after pushing its dependency checkpoint, `bc33d054`.

## Finding and scope

[PR #601](https://github.com/lejmer/bray/pull/601) reports `CONFLICTING` against `develop`. This is independent of the
BRA-619 documentation change. The BRA-609 head and `develop` were not changed by that checkpoint. The user requires
separate findings to be handed off without expanding the requested implementation, so no ancestor source was edited.

[PR #602](https://github.com/lejmer/bray/pull/602), [PR #604](https://github.com/lejmer/bray/pull/604), and
[PR #605](https://github.com/lejmer/bray/pull/605) all reported `MERGEABLE` after publication. Git's merge simulations
between the changed BRA-619 head and BRA-620, and between BRA-620 and BRA-621, also passed.

## Reproduction

- BRA-609 branch: `feature/bra-609-enforce-trusted-caller-obligations-and-live-witness`.
- BRA-609 head: `a96404055a81bfa7e9d0db2ba35d8df893f5fe21`.
- Fetched `develop`: `468dab0dab6a825235b2652275365dd087960113`.
- Command: `git merge-tree --write-tree origin/develop origin/feature/bra-609-enforce-trusted-caller-obligations-and-live-witness`.
- Exit code: 1, one content conflict in
  [discovery.rs](../../crates/bray-compilation/src/compilation/export/build/tests/discovery.rs).
- Retained log: `target/bra-619-resume/bra-609-merge.log`.

The conflict is at the start of `parallel_interface_discovery_preserves_encoded_identity`. `develop` includes
[PR #603](https://github.com/lejmer/bray/pull/603), which makes its concurrency coverage deterministic with `Condvar`,
`Mutex`, `Duration`, and `ProfileOperation`. BRA-609 builds an expanded trusted source fixture with `std::fmt::Write`.
Combining only the import hunk is insufficient. Reconcile the fixture construction with the deterministic concurrency
mechanism and preserve both behaviors.

## Follow-up

Resolve on BRA-609 in a separately authorized task, run the focused interface discovery test and required style/format
commands, then propagate the ancestor update through BRA-619, BRA-620, and BRA-621. Recheck source conflicts and native
acceptance at the changed heads. Keep the unresolved BRA-619 incident-transfer dependency visible while updating the
stack. This handoff records a merge conflict and does not change any issue's implementation status.
