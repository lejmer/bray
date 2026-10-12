# Runtime migration stack: architectural assessment and consolidation plan

Current execution is recorded in the [Linear execution guide](runtime-stack-linear-execution.md).
The user accepted the replacement groundwork and requested issue preparation on 2026-10-12.
Pause and authorization statements in the dated assessment below describe that earlier task, not a new implementation gate.

The [rule coverage and replacement plan](runtime-stack-rule-coverage.md) and [closed operation inventory](runtime-stack-coverage-inventory.md)
supersede the earlier implementation proposal. This assessment retains its dated evidence. Implementation remains paused.

Assessment date: 2026-10-12. **Proposal for discussion. Implementation is paused by the user.**

The [earlier implementation proposal](runtime-stack-implementation-plan.md) remains historical evidence. Use the replacement plan linked above for current design and sequencing. Implementation is still paused for discussion.

## Scope and baseline

This assessment covers the combined BRA-609, BRA-619, BRA-620, BRA-621 and BRA-553 stack, including the uncommitted BRA-620
repair. It examines the changed compiler mechanisms, their source/interface/native boundaries, and known production
consumers. It is not a claim that every compiler path has been exhaustively verified.

- Local comparison base: `468dab0dab6a825235b2652275365dd087960113`, the merge base with local `develop`.
- Combined committed tip: `cca4bd4429d0648cae0493db72045b7d4a7a3e73`.
- Current branch: `feature/bra-620-repair-synchronization-authority` in the existing BRA-553 worktree.
- Existing uncommitted changes are part of the assessed implementation, not new changes made by this assessment.
- Remote PR heads have not been freshly reconciled. That is an integration gate, not an assumption of this report.
- No implementation, test-source, branch, commit, PR or Linear mutation is authorized by this plan. The user must discuss
  the plan before implementation resumes. The existing master-feature-branch publication decision remains in force.

The [stack completion record](stack-completion.md) and [BRA-620 evidence](BRA-620-remaining-work.md) retain earlier results.
Issue contracts were read from Linear for [BRA-609](https://linear.app/bray-lang/issue/BRA-609),
[BRA-619](https://linear.app/bray-lang/issue/BRA-619), [BRA-620](https://linear.app/bray-lang/issue/BRA-620),
[BRA-621](https://linear.app/bray-lang/issue/BRA-621), [BRA-553](https://linear.app/bray-lang/issue/BRA-553), and the downstream
[BRA-502](https://linear.app/bray-lang/issue/BRA-502), [BRA-554](https://linear.app/bray-lang/issue/BRA-554),
[BRA-558](https://linear.app/bray-lang/issue/BRA-558) contracts. Reading downstream contracts does not authorize starting them.

## Main conclusion

The stack does not need another general framework. It needs a consistent account of references, mutation and lifecycle
transitions at the existing semantic boundaries, followed by complete production integration.

There is already substantial reusable architecture: immutable query results, selected calls, storage identities, portable
dependency contracts, a shared body graph, selected cleanup parts, and a shared authority transition implementation.
The remaining fragmentation is in how those components consume facts and in the ordering of their analyses. Introducing
another witness representation before settling those boundaries would make the implementation harder to assess.

Some growth implements genuinely missing behavior. BRA-609 introduced caller-obligation enforcement; BRA-619 introduced
selected scope entry/exit execution. Other growth is tests and necessary source/interface plumbing. That does not justify
all growth. The strongest simplification opportunities are repeated analysis preparation, repeated cleanup expansion,
and competing approximations of reference and mutation effects. Their exact removable line count is not established.

## Where the growth occurred

Physical line counts include blank lines and comments. These are snapshot differences, not sums of commit churn.
Historical checkpoints are local commits, not assertions about current remote PR heads.

| Checkpoint | Cumulative net lines | Increment since preceding row |
| --- | ---: | ---: |
| BRA-609, `a9640405` | 10,010 | 10,010 |
| BRA-619, `3f6e9309` | 14,080 | 4,070 |
| BRA-620, `31ee6a00` | 19,166 | 5,086 |
| BRA-621, `fb69346e` | 21,776 | 2,610 |
| BRA-553 combined tip, `cca4bd44` | 29,085 | 7,309 |
| Current repair, including new untracked files | 34,315 | 5,230 |

The committed stack adds 43,570 lines and removes 14,485. The repair adds 2,668 and removes 4,350 tracked lines, but also has
6,912 lines in new files. Ignoring untracked files would misleadingly describe the repair as a reduction.

A structural attribution of the combined net growth gives:

| Category | Net lines |
| --- | ---: |
| Dedicated test and fixture paths | 9,518 |
| Inline test-module tails, estimated | 3,107 |
| Rust production or mixed files, excluding the preceding categories | 15,045 |
| Bray production or mixed files | 5,581 |
| Documentation | 642 |
| Other configuration/data | 422 |

This is not an AST-accurate production/test split. The inline estimate uses conventional trailing `cfg(test)` modules;
mixed buckets can still contain tests, including test-only modules with non-test filenames. Roughly 12.6k lines are
attributed to tests by this method, and 20.6k remain in production/mixed code. The total is exact for the assessed snapshot;
the category interpretation is deliberately conservative. No claimed savings are derived from these categories.

Local measurement details are retained in `target/bra-620-authority/assessment-line-attribution.json` in the primary
checkout. Code hashes before this assessment are in `assessment-code-baseline.json`. These are local investigation
artifacts, not new repository infrastructure or implementation files.

## Keep these distinctions and existing mechanisms

Consolidation must reduce duplicate work without collapsing different guarantees into one Boolean.

| Fact | Existing owner / representation | Required distinction |
| --- | --- | --- |
| Selected operation and occurrence | Bound selections and `BoundExecutionSite` | Ordinary call, scoped enter and scoped exit have different execution identities. |
| Storage identity, paths and borrow capabilities | `StoragePlan` in `bray-bound-tree`; checker storage planner | A lifetime anchor is not necessarily the physical referent. |
| Portable callee relationships | `DependencyContractTemplateData`, including exact reference relations | Required lifetime, exact origin and unchanged stored reference are different facts. |
| Initialization, partial moves and absence | Checked storage flow | A moved value differs from an observed absent nullable payload. |
| Ordinary truth and trusted authority | `ExecutionCondition` and provenance-bearing execution state | A true flag or copied address cannot create authority. |
| Cleanup selection and order | Checked async/lifecycle storage results | Conditional invocation, partial cleanup and whole-value cleanup must remain distinct. |
| Unwind and termination | `CheckedUnwind`, phase behavior, execution certification | A non-unwinding function may loop forever. A current completion condition is not unconditional `total`. |
| Native cleanup capacity | Concrete realization from selected cleanup | Authority to invoke an action and backing sufficient for it are separate obligations. |
| Host backing and acquisition | Resident owner/anchor and a live lease | Keeping storage alive does not grant current exclusive access. |

Keep immutable, demand-driven publication and source-independent interface identities. Keep decoding validation at the
external interface boundary and assertions for established internal invariants. Keep negative authority regressions.
Do not make one monolithic compiler pass, universal effect object, new storage type, new report arena, or generic lock
framework merely to reduce the number of files.

## Findings and consolidation candidates

### A1. Exact reference facts are consumed inconsistently

**Confirmed source finding.** The storage planner's `returned_borrow_access` uses a sole direct dependency requirement to
retain a returned borrow. Execution proof separately consumes exact `borrows()` relations and reconstructs referent
observations. Mutation classification then falls back to global invalidation for local borrowed roots.

Evidence: [storage result planning](../../crates/bray-checker/src/storage/expression/result.rs),
[proof preparation](../../crates/bray-checker/src/analysis/guarantee/flow/prepare.rs),
[returned-reference completion](../../crates/bray-checker/src/analysis/guarantee/trusted/borrows.rs), and
[mutation classification](../../crates/bray-checker/src/analysis/storage_invalidation.rs).

A retained dependency is not proof of the address reached. The real `borrow_from` contract deliberately allows a lifetime
anchor different from the pointer's physical origin. The current repair has a regression covering that distinction.
This finding does not establish that every planner approximation is unsound; it establishes that consumers have different
information and that the broad invalidation loses valid authority.

**Proposal.** Use the existing inferred relation as the source of physical-origin facts in storage/access analysis and
contract observation. Keep lifetime retention separately. Track changes to local references and joins explicitly; retain
unknown aliasing when exact identity is unavailable. Remove downstream inference of physical identity from lifetime-only
requirements and redundant syntax rediscovery once the owning result supplies the fact. Projection/adaptation between
representations may remain, but must not become another proof algorithm.

The current exact-origin inference also conservatively declines bodies containing anonymous callables and some implicit
lifecycle cases. [Result inference](../../crates/bray-checker/src/dependency/result/borrows.rs) needs an explicit supported
boundary. Do not turn those conservative limits into unconditional promises or silently add unrelated inference features.

### A2. Mutation semantics are split between broad invalidation and proof-specific exceptions

**Confirmed source finding.** `storage_invalidation.rs` classifies operation effects. The authority flow then interprets
global invalidation again, with rules for spatial predicates, raw operations, empty copies and spare-buffer headers.
These are meaningful semantic distinctions, but their placement encourages another exception for each consumer.

Evidence: [classification](../../crates/bray-checker/src/analysis/storage_invalidation.rs),
[authority invalidation](../../crates/bray-checker/src/analysis/guarantee/flow/invalidation.rs), and
[existing memory proof rules](../../crates/bray-checker/src/analysis/guarantee/trusted/proof.rs).

**Proposal.** Establish the operation's conservative storage effects in the owning analysis, using checked references and
existing compiler-known operation semantics. Share closed primitive classification; keep extent- or evidence-dependent
refinement in the stateful domain that can actually prove it. Consumers should apply the resulting effects rather than
repeat primitive classification. Replace global invalidation for proven local referents, but retain it for genuinely
opaque effects. Remove individual preservation exceptions only after their exact semantics are represented by the shared
rules. There is no permission to preserve arbitrary opaque predicates.

### A3. Authority flow is solved both before and after cleanup becomes available

**Confirmed repeated work, with a real dependency reason.** `check_body_semantics_with_graph` computes early trusted memory
evidence, storage flow and dependency/cleanup results, then performs trusted completion. The early authority solve uses
`Cleanup::default()`; the later solve uses selected cleanup. Both invoke `analyze_execution_flow`, which rebuilds immutable
indexes and returned-dependency preparation. Execution-property candidates also use that engine.

Evidence: [body stage](../../crates/bray-checker/src/body_semantics.rs),
[early/final trusted checking](../../crates/bray-checker/src/analysis/guarantee/trusted/check.rs), and
[flow preparation](../../crates/bray-checker/src/analysis/guarantee/flow/prepare.rs).

**Proposal.** First share immutable per-body preparation where inputs are identical. Then make the storage/authority/cleanup
dependency explicit inside the existing owning body stage. Determine whether the early approximation can be eliminated or
whether genuinely mutually dependent facts require a coordinated fixed point. No extra final validation pass is proposed.
Do not simply delete the later solve: the earlier one does not establish its cleanup-sensitive conclusions. Likewise,
replaying converged block states to emit diagnostics is normal dataflow publication, not automatically redundant checking.

The exact coupled-stage design remains a discussion item. Adding another independent query that requests a completed body
would reproduce the existing ordering problem. Preserve narrow immutable outputs for downstream consumers.

### A4. Shared cleanup transitions still coexist with separate cleanup-property expansion

**Confirmed consolidation opportunity.** `transition_cleanup` is already the common authority transition for scope exits
and replacement. Its prepared steps come from selected cleanup parts. Separately, `check_cleanup` calls
`execution_cleanup_dependencies`, which walks type structure and selects lifecycle dependencies. Early unwind inference
also needs structural cleanup information before final storage availability is known.

Evidence: [prepared cleanup](../../crates/bray-checker/src/analysis/guarantee/cleanup/plan.rs),
[shared transition](../../crates/bray-checker/src/analysis/guarantee/cleanup/transition.rs),
[cleanup property checking](../../crates/bray-checker/src/analysis/guarantee/cleanup/check.rs), and
[structural dependency walk](../../crates/bray-checker/src/asynchronous/execution.rs).

**Proposal.** Have one source of structural lifecycle expansion, with occurrence-specific availability and property
projections. Reuse it for trusted cleanup, property dependencies, lowering and concrete capacity where their stage inputs
permit. Remove repeated recursive type expansion and repeated lifecycle selection for the same prepared occurrence.
Keep the distinction between early structural possibilities and final selected/available actions. Do not create a second
cleanup plan or replace the already-shared transition implementation.

### A5. Effect summaries have separate transitive traversals

**Confirmed separate traversal; performance benefit unmeasured.** Early unwind inference walks callable dependencies.
Later body-behavior inference also walks dependencies, but has different inputs and includes body semantics. They cannot
be merged by making the early query depend on the late one.

Evidence: [unwind query](../../crates/bray-compilation/src/compilation/execution_guarantees/unwind.rs) and
[body behavior](../../crates/bray-compilation/src/compilation/behavior.rs).

**Proposal.** Reuse selected callee/default/conversion edge data where it is identical and available early. Profile chains,
diamonds and recursive components before adding a shared solver or another cache. Remove repeated closure work only with
a demonstrated reuse boundary. Keep non-unwinding distinct from purity and termination. Lowering already consumes checked
unwind results; preserve that producer/consumer boundary.

### A6. Predicate preservation is still an unresolved semantic boundary

**Retained failing experiments, not a completed design.** Exact referent identity does not prove that an opaque predicate
on an entire guard is independent of payload contents. An empty acquisition permit separates those observations, but a
write through a local borrow still loses its authority under A2. A const helper returning an optional permit is accepted,
yet its result condition does not establish the unwrapped guard condition.

Evidence: [BRA-620 investigation](BRA-620-remaining-work.md#acquisition-witness-investigation-2026-10-12) and
[const-call normalization](../../crates/bray-checker/src/execution_guarantees/normalize/calls.rs). Const calls currently
retain a callable/argument term; this inspection and the failed experiment do not prove that one local normalization edit
would solve all conditional relationships.

**Proposal.** Discuss the observable state and authority contract first. Prefer existing representations only if the
production pairing, nullable, mutation and cleanup requirements are expressible and verified. A checked relation for a
const helper must be published by its owning analysis, not inferred by a downstream exception for a helper's name. Do not
add general symbolic evaluation or new preservation syntax merely to make the next test pass. Neither the permit nor an
explicit preservation contract is adopted by this assessment.

### A7. Production integration is incomplete, beyond the failing guard tests

**Confirmed source mismatch.** The [report-pool static](../../runtime/bootstrap/src/record_pool.bray) still initializes
`SpinLock<ReportPool>` with `next_ticket`, `serving_ticket` and a plain payload. The current
[SpinLock declaration](../../standard-library/std/src/sync/spin_lock/lock.bray) has `storage: Uninit<State<T>>` instead.
This is an incompatible consumer left behind by the uncommitted representation change. No native build was run for this
assessment, so this is source-level evidence, not a freshly observed native diagnostic.

**Confirmed authority-contract gap requiring a reproduction.** [Once](../../standard-library/std/src/sync/once.bray)
still exposes ordinary fields and `OnceAttempt.active`. Its pointer producers publish initialization/exclusive authority
based on ordinary atomic state, the active flag and a thread marker, without an acquisition-witness requirement. That is
the same structural risk as the original lock defect. A safe-caller reproduction and source/imported rejection remain
necessary before calling the specific exploit verified. Once failure/reentry/rollback needs its own acceptance cases.

Other consumers include the bounded channel and ConditionVariable. Search found the old SpinLock aggregate initializer in
the report pool; constructor-based consumers still need mutation/lifetime validation. The production synchronization test
fixture uses a manual source list and numeric source-ID filtering, so its passing body subset does not certify the full
standard library or bootstrap.

**Proposal.** Maintain one inventory of actual acquisition and initialization producers, paired with all consumers. Provide
one legitimate initialization route for each intended use, including static report-pool storage where language rules
permit. Replace the stale literal and any repeated authority-bypassing construction. Retain distinct lock algorithms;
sharing a contract model does not justify a generic lock framework. Replace numeric test-source selection with stable
module/declaration identity and require the real package/native pipeline before reporting production integration complete.

### A8. Native scoped failure transport is genuinely missing

**Confirmed catalog gap plus retained failing native evidence.** Lowering emits `TransferCleanupIncident` when a fallible
scope exit must preserve an already-pending outcome. The runtime catalog's `CleanupIncidentTransfer` has no native binding.
The existing report records do not by themselves provide admitted storage and destruction for an arbitrary typed error.

Evidence: [scoped lowering](../../crates/bray-lowering/src/lowering/scoped.rs),
[runtime catalog](../../crates/bray-runtime-abi/src/catalog/execution.rs), and
[BRA-619 dependency investigation](BRA-619-native-transfer-dependency.md).

**Proposal.** Specify one admission-to-final-release ownership path using BRA-502 physical backing, BRA-554 entry lifetime,
and BRA-558 incident consumption; BRA-619 then connects its selected exit result to that path. Error destruction and
backing release remain separate required actions. Do not add a scoped-only report arena, erase the typed error, use scalar
credits as physical backing, or turn an existing missing-role failure into success through a fallback.

These downstream issues are prerequisites, not work started by this plan. Decide explicitly whether the integration will
wait for them or retain incomplete issues outside its completion claim. An integration claiming all current stack
acceptance is complete cannot omit BRA-619's native requirement.

### A9. Some apparent duplication must remain until its owning migration is complete

The host anchor and lease carry different obligations. Bootstrap also has reentry and formation constraints that prevent
blind replacement with an ordinary standard-library lock. Preserve the verified host protocol while auditing its use of
the same compiler facts as other callers. Avoid generalizing host-specific predicates into compiler policy.

Likewise, Rust callback/product/TLS cleanup code still serves separately unfinished consumers. Removing all Rust code
with similar names would exceed BRA-553's migration boundary and break those consumers. Each migration must switch its
actual native demand and remove the superseded implementation in the same change, with link-map evidence. Do not keep a
second implementation of an already migrated consumer as a compatibility fallback.

Portable export/import conversions are also not automatically duplication: they cross identity and trust boundaries.
Keep one semantic definition and checked remapping. Remove re-proving of an established fact downstream, not the encoding,
external-input validation or necessary substitution itself.

## Proposed target ownership

This refines the existing [compiler architecture](../design/compiler-architecture.md) and
[checker design](../design/checker.md), rather than adding a new architectural layer.

| Owning stage | Establish once | Consumers |
| --- | --- | --- |
| Selection and callable-summary inference | Selected operations; lifetime dependencies; exact reference relationships; supported effect facts | Storage planner, contract observation, interfaces |
| Storage/access analysis | Per-occurrence reference targets and conservative mutation effects, retaining unknown cases | Refinement, initialization, authority invalidation |
| Existing body analysis | Consistent storage and authority transitions, joins, scope expiry and applicable cleanup | Diagnostics, execution certification, lowering |
| Lifecycle expansion | Structural actions and dependencies; occurrence selection/availability remains explicit | Body analysis, lowering, capacity realization |
| Portable publication | Proven relationships and execution properties with stable identities | Imported checking and specialization |
| Lowering and realization | Execute selected actions, preserve checked ownership, realize target-dependent backing | MIR, LLVM, native adapters |
| Bray runtime | Own admitted bytes, epochs, leases and terminal disposal for migrated consumers | Generated/native callers |

Lifetime retention, reference equality, predicate truth and trusted provenance stay distinct within that model. Backward
liveness and forward initialization need not become one solver. Source and imported code must consume the same meaning.
Ordinary calls, scoped calls and cleanup share transition semantics while keeping their different ordering and outcomes.

## Consolidation sequence proposed for approval

1. **Set the semantic boundary before implementation.** Agree on the fact ownership above and resolve A3's stage coupling
   and A6's predicate observation boundary. Write the required relationship for each failing production case. Do not choose
   a new language feature or add a general effect framework without demonstrating the missing expressive capability.
2. **Consolidate reference/effect inputs.** Address A1/A2 using existing result and storage representations. Cover aliases,
   reassignment and joins, source/imported helpers, nullable returns, distinct anchors and referents, and opaque calls.
   Keep negative opacity tests. Remove superseded inference/classification in the same change.
3. **Consolidate prepared cleanup and body proof work.** Address A3/A4 with identical-input preparation shared once. Resolve
   actual mutual dependencies in the owning stage. Keep selected transitions and remove repeated structural expansion.
   Measure before considering A5's additional graph-sharing changes.
4. **Finish one authority protocol across actual consumers.** After discussion of A6, apply the chosen protocol to SpinLock,
   Mutex, both RwLock guard kinds, ConditionVariable, Once/Lazy and bootstrap report-pool use. Include static construction,
   recursive entry and abnormal exits from the start. Retain real overflow/reentry failures and allocation behavior.
5. **Complete or explicitly defer the native ownership path.** If full current-stack acceptance is required, complete the
   BRA-502 -> BRA-554 -> BRA-558 prerequisites, then BRA-619 transfer. Each remains its own existing issue contract. No
   special scoped-error transport or duplicate arena. A deferral must leave affected issues explicitly incomplete.
6. **Reconcile and verify the integration.** Revalidate remote heads and include their work exactly once on the agreed
   `feature/trusted-storage-integration` branch. Run the combined matrix, performance checks and published review before
   declaring readiness. No individual stack PR merges into `develop`; final merge remains the user's decision.

Steps 2 onward are proposals, not authorization to execute. Do not continue them automatically after writing this report.

## Removal and simplification ledger

| Candidate | Remove or simplify | Preserve / condition before removal |
| --- | --- | --- |
| Reference reconstruction | Repeated inference from syntax or lifetime-only metadata | Exact checked origin plus independently retained lifetime obligations |
| Local borrowed writes | Global invalidation when the owning analysis proves a narrower affected region | Conservative treatment of unknown aliases, joins and opaque calls |
| Primitive preservation rules | Repeated hook classification and consumer-specific restoration branches | Closed operation semantics, extent-dependent evidence and initialization distinctions |
| Proof preparation | Rebuilt literals, observation indexes and callee relation maps for identical body inputs | Different assumptions and graph policies remain explicit |
| Early/final authority solves | Redundant solve only after the coupled stage is designed and proven | Cleanup-sensitive final conclusions cannot be taken from the preliminary solve |
| Cleanup dependencies | Repeated recursive expansion/selection of already prepared lifecycle actions | Conditional and partial availability, child ownership, release and cancellation duties |
| Transitive effects | Repeated closure traversal if profiling establishes common reusable inputs | Early unwind cannot depend on late body behavior; no speculative cache layer |
| Production fixture selection | Numeric source-ID subsets and duplicate inventories | Exact source/imported/native boundary coverage and useful negative regressions |
| Runtime migration | Rust policy only for a consumer actually switched to Bray | Distinct unfinished callback, provider and thread-static responsibilities |

This is an actionable removal list, not a promised net-line reduction. Pure consolidation changes should reduce repeated
mechanisms and explain any production growth. Missing functionality can require growth, but each addition must identify
its invariant, consumer, replacement and performance cost. Moving code into more files does not count as simplification.

## Acceptance and cost gates

Use a coverage matrix of the following dimensions, with concrete applicable cases rather than every meaningless Cartesian
combination:

- Authority source: real producer, forged aggregate, copied fields, detached/re-paired witness, stale/replaced owner.
- Access path: direct, returned borrow, reassigned alias, nested field, nullable, generic, imported, const helper.
- Transition: acquire, mutate, move, replace, consume, destroy, scope enter/exit, failed entry, return, panic, cancellation.
- Storage: ordinary payload, protected Uninit state, static report pool, typed incident backing, retained callback/provider.
- Boundary: source body and caller, encoded interface, MIR, native execution, supported target ABI/layout.

Existing valid tests must stay valid and authority-forging cases must keep reaching the intended rejection boundary.
In particular, retain the opaque-carrier mutation negative, unconditional/conditional/invalid `total` distinctions,
different lifetime-anchor/physical-origin checks, partial lifecycle checks and real production declaration tests.

Correctness gates include the currently failing mutable-payload and RwLock abnormal-cleanup cases; Once publication and
rollback; report-pool initialization through the real bootstrap; and native fallible scoped exit under denied allocation
with exactly-once typed destruction and backing release. The permit's synthetic tests are not substitutes for these.

Compiler cost: record prepared-analysis construction counts and retained state, plus cold/warm complete compilation,
encoded-library reuse, and scaling on alias-heavy bodies and call chains/diamonds/recursive groups. Compare matched
configurations using existing profiling/xtask facilities. A5 gets no new infrastructure without measured benefit.

Generated cost: compare guard and host layouts, optimized instructions and unoptimized cleanup, allocations, atomics/TLS
accesses, outgoing reserved bytes and linked native reachability for the relevant consumers. The empty-permit experiment
only established size 0/alignment 1 and an unchanged 8-byte test guard; it did not establish full performance neutrality.

There is no basis yet for a numeric speedup or removal estimate. Establish matched baselines before implementation and
report production and test changes separately. Stop for discussion if a proposed consolidation introduces a general
alias/effect language, a new runtime ownership framework, or cannot identify which existing mechanism it replaces.

## Decisions for the discussion

1. Agree on consolidation of existing reference/effect and cleanup machinery before choosing an acquisition representation.
2. Decide A3's handling of genuinely coupled storage/authority facts; shared immutable preparation is the first bounded
   improvement, but removing a whole solve requires a semantic dependency argument.
3. Decide A6's authority observation contract from concrete production requirements. Neither new syntax nor the permit is
   preselected by this report.
4. Confirm whether the final integration must satisfy BRA-619 native acceptance now, which entails the already-recorded
   downstream prerequisites, or whether it will explicitly leave that issue incomplete. The latter changes the completion
   scope and must not be described as finishing the whole stack.

## Assessment validation and limitations

This was a read-only implementation assessment with documentation updates. No compiler, runtime or test implementation was
changed. No builds or test suites were run, and no prior result is represented as freshly rerun. Source-level findings,
retained reproductions and proposed simplifications are labeled separately above. The implementation remains uncommitted
and incomplete. The full implementation compliance audit and native acceptance still belong to the resumed work.

The assessment used repository-wide searches for equivalent reference handling, mutation rules, cleanup expansion,
proof traversals, portable mappings, lock consumers and incident roles. It deliberately does not recommend merging
semantically distinct mechanisms merely because their names or loops are similar.

Final documentation checks: all relative file links in the assessment and completion plan resolve, `git diff --check`
is clean, and SHA-256 comparison of all 2,940 tracked/non-ignored non-documentation files shows no implementation changes.
