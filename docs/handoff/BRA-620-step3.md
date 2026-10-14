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

## Current checkpoint, 2026-10-15

Branch: `feature/trusted-storage-integration`. Worktree:
`C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray`. Published recovery checkpoint: `a4691a63`.
PR #607 remains draft. The next recovery checkpoint preserves the cleanup/import certification repair and
formatted source. Native acceptance and independent review are still pending, so it does not complete Step 3.

Implemented candidates include synchronization markers and protected control state, Once attempt identity and rollback,
native atomic32 wait/notify roles, channel ring detachment and cleanup outside its lock, host/environment/thread
contracts, shared call/cleanup effects, exact result propagation, source/interface witness carriers, and observation
and reference transport. No new Bray syntax or language feature was introduced.

### Established checks

- Checker: 320 passed, and package interface: 190 passed, in `step3-checker-interface-final.log` after the final Rust extractions.
- Compilation: 1,650 passed in `step3-compilation-full-3.log`. Three failures were BRA-619's existing
  CleanupIncidentTransfer blocker, production channel cleanup, and an outdated exact-origin projection expectation.
  The latter two now pass in focused checks.
- Package interface: 190 passed in `step3-interface-2.log`.
- Whole-owner forwarding, imported forwarding, static instance identity and static initializer component tests pass.
  Forwarding publishes only an unchanged owned input common to normal exits. It does not mint authority from field equality.
- Production channel bodies and four production guard regressions pass. Invocation reachability consumes physical
  receiver targets before falling back to lifetime anchors.
- Native acceptance 10 successfully compiled the standard library/runtime, then failed API compilation with ten
  cleanup-proof diagnostics. No native API or outcome cases executed. Log: `step3-native-acceptance-10.log`.
- Mutating style pass 6 passed before the latest changes. Style and final format must run again.

### Active cleanup repair

Storage analysis now publishes cancellation and opaque cleanup mutations alongside lifecycle mutations. Downstream
execution checking consumes those results. Opaque cleanup retains its known receiver and type, and prepares observation
footprints even when ordinary calls have no effects. A small task/PanicReport cleanup regression passes.

Nullable/union cleanup prepares the same typed presence tests used by ordinary patterns, limited to its invocation
branch. Temporary cleanup reads its evaluated value. The nullable temporary production case now passes, and the focused
positive/forged nullable owner regression passes. Conditional cleanup still cannot publish unconditional guarantees.

Retired reference slots were being treated as external reachability roots. Excluding retired slots from automatic
roots preserves explicit raw exposure and traversal from live references. Its focused regression passes.

Discard bindings now have execution places and committed values. `PatternDiscard` follows the same pattern source
transfer, reference contents, invalidation and scope expiry as named owners. Positive and forged discard cases pass.
Pattern-to-source discovery is shared with the existing pattern analysis instead of repeating its expression traversal.

Cleanup owners retain loans after their last ordinary read. Storage flow consumes the established value-retention
summary and current owner liveness/move state when ending loans on operations, edges and scope exits. Consuming or
leaving the owner releases its loans. The focused task regression covers named, discarded, moved, joined and scoped
owners. It passes in `step3-retained-task-3.log`. Conditional cleanup tests pass in `step3-conditional-cleanup-1.log`.

All four focused production API roots pass in `step3-api-cleanup-10.log` (50.42 seconds): combined lock/Once owners,
discarded mutex guards, reader/writer task ordering and reused barriers. Temporary cleanup tracing has been removed.
Native API/outcome acceptance 11 compiled the production API but hung during execution. The runner's precise JSON
cause is `forced_termination`, rather than a source diagnostic. The batch was stopped after the shared failure was
isolated, retaining the unchanged native generation `ab3ad3f5380091a8de0b0c0df6a6ee4aae678c0318c0f6fd490dab3557d2713f`.
Sampling that executable and matching its object relocations established recursive acquisition of RECORD_POOL:
SpinLock.lock admits outgoing backing after acquiring its ticket, then outgoing_admission reacquires the same lock.
Imported cleanup capacity rejected a valid total destructor certificate because it was trying to authorize the caller.
The candidate repair shares certification on every legal input across source/imported declarations, while ordinary
invocation proof continues to require caller evidence. Conditional promises retain their entry conditions.

The completed broad compilation run has 1,655 passes and two failures (`step3-compilation-full-4.log`, 1,027 seconds).
One is the established BRA-619 CleanupIncidentTransfer blocker. The other is nested-reference transport after moving
a scoped resource. Liveness was attaching the implicit scoped receiver loan to its returned capability even when the checked
entry contract returned only a nested reference. It now consumes the call's result dependencies. Scoped result
instantiation also uses the existing value-origin projection used by ordinary calls. The regression passes in
`step3-scoped-transport-4.log`. Shared pattern inventories are deduplicated before single-destination selection.
The corrected source/imported certification regression passes in `step3-imported-cleanup-5.log`. It covers
unconditional and conditional generic destructors, ordinary caller evidence, and rejection of forged owners.
Grouped checks also pass: returned values 28, outgoing capacity 5, exported guarantees 18, and task cleanup retention 1.
Rust formatting exposed oversized analysis functions. Existing cohesive blocks were extracted without changing
transition order. Mutating structural style now passes. The final Cargo style/format pass also passes.

### Audit map for the publication review

Search the mapped producers and consumers together. The audit uses the Step 3 baseline `839ab811`, including its
published recovery checkpoints, rather than only the latest uncommitted diff.

| Behavior | Owning implementation and searched equivalents | Evidence and disposition |
| --- | --- | --- |
| Preserve complete acquisition carriers | `analysis/guarantee/trusted/{carriers,borrows,completion}` and ordinary/scoped result transfers | Production guard forgery, extraction, reconstruction, copied fields and cross-owner cases remain in `trusted_contracts/sync.rs`. Immutable markers carry no separate runtime identity. |
| Transport unchanged owned results | `DependencyContractTemplateData::returned_input`, recursive result publication, generic substitution, interface codec/validation/remapping, structural identity | Source/imported forwarding and recursive dependency tests. Ordinary field equality never establishes whole-owner identity. Unknown or conflicting normal exits remove the summary. |
| Invalidate storage observations | `storage_flow/check/analysis/{effects,observations,cleanup}`, `memory/references`, `guarantee/flow/invalidation` | Explicit calls, opaque calls, cancellation and lifecycle mutations publish through `StorageMutation`. Retired slots are not automatic external roots. Explicit exposure and surviving references still traverse them. |
| Carry loans until owner cleanup | Liveness value-retention fixed point, `StorageFlowInput::prepare_cleanup_retention`, `StorageFlowState::retains_borrow_for_cleanup` | Named/discarded/moved/joined/scoped task regression passes. Operation, edge and scope expiry consult the same owner index. The broader retirement/composition assessment is the next authorized step, not inferred complete from these tests. |
| Apply conditional cleanup | `guarantee/cleanup/{plan,transition}`, shared pattern-value normalization and published cleanup effects | Nullable/union guards apply only to the cleanup invocation branch. Conditional completion does not become unconditional evidence. Positive and forged owner cases pass. |
| Certify cleanup capacity | `certified_entry_property` and shared imported proof graph, `CallableExecutionDomain::requires_only` | Source/imported unconditional and conditional generic destructors pass. Caller verification without authority remains rejected. This removes the source/import capacity disagreement that recursively reacquired RECORD_POOL. |
| Project returned scoped references | Existing call instantiation and value-input projection, selected call dependencies in liveness | All 28 returned-value tests pass. A scoped entry receiver loan is not automatically the returned capability's dependency. |
| Run native atomic waits | Existing core atomic operations, MIR role demand, LLVM word bridge and platform bootstrap modules | Cross-target declaration and role tests are retained. Wait remains blocking, nonpure and nontotal. BRA-568 tracks non-32-bit helper over-demand. |
| Preserve library protocol ownership | Guard families, Once attempt/claim, condition waiter list, channel storage/endpoint/lifecycle, host attachment/report pool | Existing native API/outcome cases cover publication, caught Once reentry, lock cleanup, waiter release and channel transfer. Native acceptance is still required on the current tree. |
| Avoid repeated utility behavior | Shared ring index, `replace_value`, `expression_is_borrowed`, pattern binding sources, execution-domain subset helper | Search results use the domain helper. Trusted raw-pointer authorization stays local to the body holding the actual owner contract, so extraction cannot create an unrestricted authority factory. |

The formatting audit split existing global invalidation, normal-entry assumptions, result-evidence restoration,
storage-location preparation, dependency preparation and execution-proof iteration into their owning modules.
It adds no analysis pass or state representation. The checker and interface suites pass after these extractions.
The native acceptance must finish before publication review. Source/imported proof groups passed before the
behavior-preserving function extractions, and the release compiler builds with those extractions.

Physical-line accounting against `839ab811`, after Rust formatting, is production +9,824/-2,802, tests +4,479/-218,
documentation +172/-2, generated +37/-37 across 265 files. Inline test modules are counted separately. Formatting
expansion accounts for part of the increase from the earlier unformatted counts, not additional behavior. The added
production capabilities are the shared summaries/transfers, native atomic bridge and repaired owner protocols above.
No reduction is claimed from moving or reformatting code. No new solver, runtime witness registry or runtime witness
allocation was introduced.

### Remaining acceptance

1. Finish the focused production cleanup repair and its adjacent negative cases.
2. Run native API/outcome acceptance with Rust 1.98.1 and LLVM 22.1.8, then resolve any actual execution failures.
3. Complete the every-file mechanical audit and behavior matrix. Audit source/interface transport, conditional cleanup,
   discarded owners, raw exposure, condition wait publication/unwinding and production/test growth separately.
4. Run required checks, mutating style and final format. Commit/push, obtain and verify a fresh published review on #607,
   fix valid findings and update Linear truthfully. Step 3 is not complete.

### Retained decisions and tracking

- Cargo target is `C:/Users/anton/source/repos/bray/target`, build directory is its `cargo` subdirectory. Reuse shared
  artifacts and LLVM under `target/toolchains/llvm/active`. Native workflows run outside the sandbox.
- Logs and temporary reproductions remain under `target/bra-620-authority`. Do not clean shared storage.
- The user-requested `expression_is_borrowed` helper is shared by memory and reference transfer.
- Strong channel requirements remain. Optional File conditional cleanup was restored. Atomic waiting remains blocking,
  nonpure and nontotal. Existing checked `when` guarantees describe inactive HostLease/channel registration cleanup.
- BRA-619 remains a separate final-stack blocker and does not excuse any Step 3 failure.
- Non-32-bit atomic wait/notify helper over-demand belongs to active BRA-568's existing R27-R31 artifact-publication
  contract. Native code uses the word helpers only for encoded 32-bit storage, while MIR demand currently selects them
  for every width. BRA-568 must unify demand with representation and verify source/imported feature subtraction.
  The user explicitly approved posting this finding. Linear comment `9309c31b-9168-4118-aa52-d10ae1112abb` is published.
  Issue: https://linear.app/bray-lang/issue/BRA-568. No Linear status changed.

### Authorized follow-up after Step 3 publication

The user requires finishing Step 3 correctness, acceptance, review and push first. Then insert a bounded Step 4 before
the previously planned backing/runtime continuation, and shift the later steps. The tracked plan must map existing
implementation and evidence for pending-cleanup retirement, surviving-owner retention, shared guarded cleanup
operations/outcomes, explicit missing operation handling, and composition through construction, moves, mutation,
branches, cancellation and destruction. Cover temporary/discarded owners and imported contracts.

Map before editing. Change code only for concrete gaps, in the owning analysis, remove superseded interpretations,
and reuse established results downstream. Add no syntax, language feature, general framework or downstream validation.
Batch fixes before tests and formatters. Step 4 requires evidence for every obligation and resolution of every identified
gap. Step 3 production success alone does not establish Step 4. Record findings and completion evidence in the tracked plan.
