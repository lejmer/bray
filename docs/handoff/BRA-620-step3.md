# BRA-620 Step 3 task record

## Contract and baseline

Authorized on 2026-10-14. Continue on feature/trusted-storage-integration from 839ab811.
Steps 1 and 2 are complete. Implement the production migration in the agreed rule coverage plan, without new syntax
or language features. PR #607 includes the five original PRs, now closed as superseded. Keep their branches and
Linear statuses. Finish Step 3 with verification, mechanical audit and published review.

Baseline compilation evidence is 1571 passing and 18 failing tests. Seventeen failures belong to this production
migration. BRA-619's CleanupIncidentTransfer remains outside this unit and blocks final stack acceptance.

## Design and removals

The complete guard carries acquisition authority. An immutable owned empty marker is the predicate observation.
Keep the original lock and initialization epoch through checked dependencies. No independently issued token,
field-privacy assumption, runtime witness registry or weaker invalidation. Remove whole-guard acquisition predicates.
Protect concurrent protocol fields behind existing Uninit storage. Ordinary helpers cannot grant control-word access
or mint authority from scalar state. Once attempts must retain exact instance/thread authority, detect caught reentry,
roll back once and publish initialized storage before ready. Preserve consuming condition waits.

## Coverage matrix

| Domain | Required checks |
| --- | --- |
| SpinLock, Mutex, RwLock | Actual declarations, payload writes then cleanup, nullable acquisition, moves, generic/imported use, fabrication, field extraction, copied fields, cross-lock pairing, early release, reader overflow before commit. |
| Once/Lazy | Valid instance and attempt producers, protected state, fabricated state/attempt rejection, caught reentry, rollback, retry, publication ordering and destruction. |
| Host/report pool | Production lease and anchor contracts, rollback, retained cleanup, total destruction, layouts and existing native failure checks. |
| Memory/foreign/thread/environment | Output/InPlace/anchored views, buffers, channels, owner formation, successful foreign outputs, actual source/imported bodies and rejection of forged owners. |
| Other synchronization | Events, semaphores, barriers and word helpers need only their real protocol invariants. No protected-data authority from atomic or thread identifiers. |
| Costs and structure | Empty markers add no runtime storage, allocations, lookups, atomics or helper calls. Reuse established compiler summaries. Separate production/test growth and remove superseded mechanisms. |

## Work constraints

Prefer cohesive production changes over compiler additions. Reconsider any need for another analysis, new language
feature, library-specific compiler rule or substantial new infrastructure. Keep meaningful negative tests.
Reuse C:/Users/anton/source/repos/bray/target/cargo and target/toolchains/llvm/active.
Retain logs under C:/Users/anton/source/repos/bray/target/bra-620-authority. Do not clean shared artifacts.

## Current checkpoint, 2026-10-14

The user requested an immediate off-machine recovery checkpoint. This commit deliberately preserves incomplete
Step 3 work. It is not an acceptance or review milestone. Branch: feature/trusted-storage-integration. Worktree:
C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray. PR #607 remains draft and includes the five prior PRs.

Implemented candidates include synchronization markers and protected control state, Once attempt identity and rollback,
native atomic32 wait/notify roles, channel ring detachment and cleanup outside its lock, host/environment/thread
contracts, shared call/cleanup effects, exact result propagation, source/interface witness carriers, and observation
and reference transport. No new Bray syntax or language feature was introduced.

### Evidence and remaining acceptance

- 319 checker tests passed before the latest dependency-projection repair.
- Last broad compilation run: 1,638 passed and nine failed. Eight failures now pass focused checks: three static
  dependency cases, three assembly pointee-layout cases, production channel declarations, and the native replacement
  fixture. The ninth is the separately tracked BRA-619 CleanupIncidentTransfer blocker.
- All five opaque Future/Task driver capture variants reject stale authority. Future creation retains deferred effects
  and captured regions separately from completion dependencies. Actual guard local/static source and import tests pass.
- Scalar mutation tests now cover custom indexing and writes through a local borrow. All nine variants pass. Unresolved
  writes must not inherit the unexposed-local exemption of an opaque call with no arguments.
- Native acceptance builds standard-library and runtime archives and reaches API product compilation. Its current
  failure is an imported union payload declaration lookup in dependency instantiation. The candidate repair uses the
  existing source/import declaration provider during preparation and retains variant relationships in the checked
  dependency table for query-free flow. A new source/import regression is still being finalized. The latest run had
  an incorrectly unqualified imported name, corrected after that run. No passing result is claimed yet.
- Mutating style passed three times before the latest Rust edits. Final style and format remain required.
- No Step 3 published review has run. The immediate checkpoint is not ready to merge and Linear remains unchanged.

### Next actions

1. Verify imported_union_payload_dependencies_resolve_at_the_owning_boundary. Recheck scalar mutation, replacement,
   returned-reference, and authority neighbors with the current test binary.
2. Run cargo xtask style to rebuild xtask itself, then native API/outcomes acceptance with Rust 1.98.1 and LLVM 22.1.8.
   Updating brayc alone does not update xtask's in-process compiler library.
3. Complete the mechanical audit of every changed file and behavior, including condition-waiter publication/unwinding,
   static initializer query dependencies, non-32-bit atomic runtime-role demand, and production/test growth. Do not use
   implementation agents. Move the static helper above tests and remove temporary tracing, both done at this checkpoint.
4. Rerun affected broad checks after repairs, then final style and format. Push the finished change, obtain and verify a
   fresh published review on #607, fix valid findings, and update Linear truthfully. Step 3 is not complete yet.

### Execution and retained decisions

- Use C:/Users/anton/source/repos/bray/target as CARGO_TARGET_DIR and its cargo subdirectory as CARGO_BUILD_BUILD_DIR.
  LLVM prefixes point to target/toolchains/llvm/active. Native workflows need RUSTUP_TOOLCHAIN=1.98.1.
- Logs are in target/bra-620-authority. Relevant logs: step3-native-acceptance-5.log, step3-projection-build-2.log,
  step3-imported-projection.log, step3-replacement-repair.log. Broad results are in step3-compilation-full.log.
- All task-launched native/test commands have finished at this checkpoint. Do not rebuild a running Windows test binary.
- No BRAY_TRACE_PROOF or RETURN tracing remains. Reuse shared artifacts and never clean shared storage.
- Automatic review previously rejected weakening channel core/lock requirements. The stronger contracts remain.
- Optional File conditional-cleanup changes were restored. Inactive HostLease and channel registration cleanup use
  existing checked when guarantees. Atomic waiting remains blocking, nonpure and nontotal.
- BRA-619 remains a separate final-stack blocker. It does not excuse any Step 3 native acceptance failure.
