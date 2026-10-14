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

- Recovery checkpoint 6dfe51b4 was pushed and its remote hash verified on 2026-10-14.
- Broad compilation run at that checkpoint: 1,647 passed and one failed. The remaining failure is
  scoped_selected_calls_emit_native_artifacts, the separately tracked BRA-619 CleanupIncidentTransfer blocker.
- Imported union-payload projection regression passes, including rejection of an escaping local borrow. Native API
  compilation now gets past the prior declaration lookup panic.
- Native API acceptance still fails with 77 trusted-obligation diagnostics, all in channel.bray and sync.bray. It did
  not execute the native API or outcome cases. The source-level suite is not native acceptance.
- Some API helpers lacked the new authority requirements on their parameters. Those helpers now require and preserve
  their lock/barrier authority. The owning sender helper requires its endpoint authority. These changes remain to verify.
- A minimal real source check confirms generic identity<T> loses an incoming witness when returning the same owned
  value. This is a general forwarding boundary, not an import-only problem. Generic assert_ok/assert_present usages
  account for further native failures. Do not bypass them with local unwraps or new trusted assertions.
- The static-cycle audit reproduced a query cycle when a cleanup dependency helper also used trusted evidence. The
  repair makes static initializer bodies members of the existing result-dependency components. Checking prepares static
  carrier summaries from that component, and flow consumes them without queries. Shutdown graph validation keeps its
  existing owner. Both focused cycle regressions and 71 static-neighbor tests pass.
- Checker suite after the static-component repair: 319 passed. The source/import production guard regression also passes.
- The latest mutating style pass applied five fixes but failed because check_dependency_contracts has 260 production
  lines against the 250-line limit. Extract its static-summary preparation into the owning dependency module before
  final acceptance. The chained format command did not run. See step3-style-5.log.
- The user-requested expression_is_borrowed helper is shared by memory transfer and reference transfer.
- Final mechanical audit, formatting, remaining acceptance and published Step 3 review are still pending. PR #607 remains
  draft and Linear remains unchanged.

### Next actions

1. Resolve the dependency-check function-size failure and finish the static-component audit. Focused checker and
   source/import production guard checks have passed. Run final format after style succeeds.
2. Repair generic whole-owner forwarding at its owning analysis. The language already requires moving and returning a
   witness to preserve its value contract. Lifetime dependencies, field equality and borrowed-storage identity do not
   prove this. Infer only actual whole-value transport, invalidate mutated observations, preserve exact dependencies,
   and reject copying or reconstructing an owner from its fields. Use existing flow and exported dependency summaries.
   No new syntax or language feature is authorized.
3. Verify the corrected API helper contracts and all native diagnostics against the production declarations. Then run
   native API/outcome acceptance with Rust 1.98.1 and LLVM 22.1.8. Updating brayc alone does not update xtask's compiler.
4. Complete the mechanical audit of every Step 3 changed file and behavior. Audit condition-waiter publication/unwinding,
   static initializer dependencies, non-32-bit atomic role demand, source/interface transport, and production/test growth.
5. Run required style and format, push the finished change, obtain and verify a fresh published review on #607, fix valid
   findings and update Linear truthfully. Step 3 is not complete yet.

### Execution and retained decisions

- Use C:/Users/anton/source/repos/bray/target as CARGO_TARGET_DIR and its cargo subdirectory as CARGO_BUILD_BUILD_DIR.
  LLVM prefixes point to target/toolchains/llvm/active. Native workflows need RUSTUP_TOOLCHAIN=1.98.1.
- Retain logs in target/bra-620-authority. Relevant logs are step3-compilation-full-2.log,
  step3-native-acceptance-6.log, step3-imported-projection-2.log, step3-static-component-2.log,
  step3-static-neighbors.log and step3-owner-transfer.log. Scratch source reproductions are static-cycle.bray and
  owner-transfer.bray in the same ignored directory. The native workflow removed its temporary workspace on failure.
- No BRAY_TRACE_PROOF or RETURN tracing remains. Reuse shared artifacts and never clean shared storage.
- Automatic review previously rejected weakening channel core/lock requirements. The stronger contracts remain.
- Optional File conditional-cleanup changes were restored. Inactive HostLease and channel registration cleanup use
  existing checked when guarantees. Atomic waiting remains blocking, nonpure and nontotal.
- BRA-619 remains a separate final-stack blocker. It does not excuse any Step 3 native acceptance failure.
