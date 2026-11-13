# BRA-567 current correction audit

Read this record and the [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction. The current target is less than500ms per fixture case. It is not met. Bounded diagnosis and
artifact cleanup are complete. Implementation of the performance plan is authorized. The indexed proof path passes67
execution-proof and109 guarantee tests. Performance remains above target. Next repair repeated context construction
in immutable consumer loops and the independent branch-feasibility scan. No full matrix runs are authorized.
The issue remains incomplete. Do not merge, change issue status, edit develop or create another worktree.

## Current admission contract

Compatible cleanup retains historical pending admission only from the exact existing private carrier listed in that
entry's witness_inputs. The witness must own the clause, prove it from the captured entry, and require trusted evidence
rather than ordinary truth alone. Expand through the existing assume operation and retain only already-existing facts.
This never publishes current truth or restores witness availability. Unknown conditional guards do not grant their
consequence. Address-only, unproved and vacuous clauses do not qualify.

ExecutionState::invalidate_cleanup owns this retention boundary. ExecutionWitness::owning_conditions generalizes the
existing single-clause reader, and the former consumers select its first clause. Immutable Arc views permit filtering
the existing ledger without another stored representation. Resume still requires the surviving exact carrier and
captured proof. Precise invalidate_referents, retire_entry_context, retired-root and discarded-capture pruning remain
authoritative, including external observations of a private witness. No borrowed-prefix ownership inference is added.

## Frozen comparison baseline

Fresh uninstrumented optimized tooling binaries and source diff identity are recorded in
correction-admission-current.json and correction-current.json. Cargo reports opt_level=3. The build command is
`cargo test -p bray-compilation -p bray-checker --lib --no-run --profile tooling --message-format=json`.
The owning command exited zero and its JSON build-finished record reports success.

- All28 guarantee-state tests pass, including admitted-carrier selection, conditional guards, actual observation mutation
  and discarded-carrier revocation.
- All6 deferred contract tests pass, including panic and failing cleanup boundaries.
- private_formal_task_admission_preserves_only_its_carrier passes all8 source/import rows:4 real tuple/array positives
  and4 fabricated-carrier negatives. The real paths include Box capture, Task.start, join and cancellation.
- Exact frozen30 is29 pass/1 fail, with no ignored or incomplete tests. The remaining failure is reference-bearing
  RawBuffer payload reachability, which still accepts the invalid live-owner observation after opaque buffer transport.
- drop_selected reaches and passes exact Direct Parameter0 / OwnedTarget / Retire with no Unknown effect.

The matrix harness now exposes248 owned and148 deferred rows in each of source/import mode:792 named tests.
Catalogue assertions protect registration completeness. Original row bodies, outcomes and diagnostic checks remain intact.
Each process checks and caches its provider once, then checks each revised caller. Provider wait, initialization,
check and case start/finish events include elapsed times and panic status. The bounded runner resets one live progress
log per run. CPU availability determines concurrent test threads across its existing process workers.

Use `cargo xtask test compiler --profile tooling --filter owned_transfers:: --timeout-seconds 900` or replace the filter
with `deferred_completion::`. A row name selects that row in both modes. Existing shard selection remains available.
Both catalogue checks,6 runner tests and the8-row admission oracle pass. The optimized build succeeds
with opt_level=3 and debug assertions disabled.
The complete owned selection finishes in122s:484/496 matrix rows pass and12 fail, comprising six source/import pairs.
The deferred selection reaches its900s bound:281/296 rows pass,10 fail and5 remain unfinished. Across both selections,
15/16 additional related tests pass. These are observed outcomes, not newly established regressions against a full prior run.
Deferred failures cover task address/frame and returned-future authority. Owned failures cover conditional result authority.
The unfinished deferred rows are both modes of preformed_task_selected_after_completion and
preformed_task_fabricated_after_completion, plus source same_site_independent_second. Provider checks finish below1s.
Other slow same-site rows finish after555–794s, locating expensive analysis inside individual cases.
The deferred case phase currently ends before its final aggregated assertion, so panicking=false in its progress log
does not establish success. Use the final test report for outcomes. All runner-owned processes terminate at the bound.
Fixed920 and issue-wide source/native gates remain unverified. No full matrix rerun is authorized for this repair.

## Proof preparation repair

A bounded30s native stack profile of source preformed_task_selected_after_completion captures118 samples.
Conditional cleanup selection occurs in99, implication checking in82 and trusted-condition closure in62, with overlap.
The sampled path is storage-flow analysis, conditional cleanup, merge, guarded-fact proof, implication and closure.
flow/witnesses.rs checks candidate clauses and guards against predecessor evidence. condition/proof.rs then scans existing
disjunctions and calls implies, which rebuilds trusted_condition_closure for each premise. Nested proposal/implication
loops repeatedly construct structurally keyed proof graphs and clone/compare/drop conditions. Per-proof work limits do
not bound that repeated total work. This is the measured bottleneck, not evidence that fixed-point iteration counts are
healthy. A repair must share preparation within the exact immutable evidence context without widening authority or
discarding required clauses. All profiled children are terminated at their bounds.

Repair contract: ExecutionCondition proof owns Boolean derivation. conditional_facts and conditional_aliases consume
it against each original predecessor and exact owner evidence. Reuse may preserve preparation and fixed-budget query
results, never across changed fact sets. Borrow immutable trusted/ordinary sets, prepare each premise and each requested
clause's negation once per context, group candidates by owner and discard the context before evidence changes.
Equivalent groups include witness refinement, call requirements, result-carrier selection and postcondition exit checks.
Explicit-budget mixed proofs still charge each visited node. A decisive first operand skips an unnecessary Boolean
subtree, retaining unknown/false/true logic. Keep unknowns, ordinary/trusted separation, unavailable carriers, snapshots, negative
implication fallback and existing budgets unchanged. Replace repeated preparation without a global cache or stored flow
state. Existing condition/guard/cleanup tests and the real positive/fabricated task cases are the fixed oracles.
Comparison reasoning must reject noncomparison operators before constructing ordering terms. This removes work whose
result was always None, retaining all applicable comparison reasoning and Boolean derivation rules.
The existing provenance test covers context isolation/reuse. The former120s bound proved completion only and is superseded
by the500ms target, including consumer construction, checking and lowering, excluding shared provider setup and Rust builds.
Do not run either full matrix. Target less
than150 added production lines. Reconsider if proof outcomes change or preparation sharing leaves the cases pathological.

Current implementation shares premise closures, negated requests and fixed-budget results through borrowed proof
contexts, groups merge candidates by owner, and uses those contexts in the equivalent immutable query groups.
Comparison proof skips noncomparison operators and both Boolean proof engines short-circuit decisive operands.
The completion baseline passes all173 execution-guarantee/guarantee-analysis tests and all five formerly unfinished
cases under the former120s bound. Task cases take48.3–49.5s and the loop117.5s, with117.8s total. Positive cases pass
and fabricated-owner/second-owner negatives reject. The loop remains expensive. This is not full-matrix verification.
Temporary counters are removed. The198-block loop graph showed expensive individual passes, not runaway update counts.
Required cleanup-feasibility replay remains intact. Prepared facts retain raw/mixed subproof results keyed by condition
and incoming budget, restoring the exact remaining budget on hits. Raw/mixed facts and policies stay separate.
The fixed63/62-step oracle checks repeated prepared queries. No extra memoization policy remains.

A hash-verified15s individual-case leaf profile at bf27eafca collected401 samples:175 structural condition comparisons,
34 memo-helper samples,28 condition clones and26 drops. Leaf sampling does not identify callers. conditional_aliases
can propose128 values ×2 predecessors ×128 guards =32,768 guarded clauses per merge, each checked on both predecessors.
This upper bound is not the observed cause. A subsequent instrumented20s individual run completed76 caller alias merges
with zero alias proposals, zero guarded clauses and zero alias predecessor checks. Nevertheless the caller started at
least1,605,633 implication attempts. Counts are partial-run lower bounds. The alias-proposal hypothesis is ruled out for
this interval. A second20s instrumented invocation observed72,733 top-level requests,13,923 completed disjunction scans,
2,757,965 premise visits and1,572,864 implication attempts, including provider setup. A hot caller context made509
requests and219 scans, visiting385 entries and attempting382 implications on every scan:83,658 attempts total.
Its326 deterministic1-in-256 pair samples were all distinct. This describes the sample, not full pair uniqueness.
The observed cause is the all-premise search per unresolved disjunction. Hot-context constructor sites and the sources
of their expanded premises remain unidentified. More blind caching does not address the observed search breadth.
Temporary instrumentation is removed. Current evidence is scanner-counts.json, actual-counts.json, proof-unfinished.json and
proof-500ms-sample.json. Superseded build streams, traces and duplicate outputs were removed. Frozen comparison evidence remains.

## Replace repeated implication scans

The bounded attribution run locates hot contexts at conditional_facts source validation and per-owner predecessor proofs,
plus ExecutionWitness::refine. Goals include nested initialization and branch guards. The common query scan now uses
the closure graph for a reverse dependency query under the negated goal, then validates
falsified published premises with the existing bounded raw proof. This preserves single-premise authority and numeric,
variant, exact-occurrence, wrapper and trusted-polarity rules. Direct goal evaluation uses the current Boolean closure,
which already contains each premise's forward consequences. Remove pairwise preparation maps and the all-premise loop.
The graph owns no flow evidence and cannot outlive or mix its immutable owner/snapshot view. Budget exhaustion never
validates a candidate. Keep moves, overwrite, retirement and unavailable-carrier handling unchanged in this replacement.
Boolean keys use child IDs. Closure preparation retains its graph. Exact evidence returns before preparation and
empty-context values follow Boolean children without recursive reproving. The old scan exists only as a unit-test
comparison oracle. Numeric, variant, qualifier and unrelated-fact scaling controls pass.
Latest bounded phase counts are proof-indexed-counts.json. The selected source case takes3539ms. The independent-site
case remains unfinished at10s. Its preparation falls from5.5s to40ms, but repeated closure construction remains4.75s
in that partial interval. Counts include provider setup and nested phase times must not be added.
Reuse immutable views in resume, address bounds, cleanup retention, qualification, return inference and transfer loops.
Recreate views when facts mutate between requirements or ownership cases. Branch feasibility and scalar joins must use
indexed alternative reasoning, including false conjunctions. Independent owner-support checks must stay independent.

1. Attribute hot contexts and proof requests to constructor sites and clause producers in one bounded single-case run.
   Count original and derived clauses, duplicate facts, distinct goals, rule work and repeated equivalent contexts.
   Inventory supported rules, including negated-goal reasoning, numeric comparisons, unknowns and trusted polarity.
2. Trace the measured obligations through production, joins, deferred completion and consumption. Preserve owner,
   guard and current-value relationships through the existing witness and stored-value mechanisms. Transfer established
   relationships directly. Keep overwrite, retirement, unavailable carriers and authoritative empty records intact.
3. Extend the existing Boolean closure graph. Share immutable condition structure using existing identities or local
   node IDs. Keep evidence views scoped to exact owners, dependencies and snapshots. Index rule inputs and conclusions,
   propagate changed facts through a work queue, and resolve each distinct goal through its transitive relevant slice.
   Preserve indirect proofs and contradictions. Predicate-name filtering and unit propagation alone are insufficient.
   Count total query work, including every implication attempt. Limits must not become a way to reject valid fixtures.
4. Migrate the measured async composition through both source and imported contracts. Compare existing and replacement
   results temporarily, investigate differences against the contract, then remove the superseded scan and redundant
   preparation caches. Do not ship parallel proof engines or an old-engine fallback.
5. Preserve the five formerly unfinished cases and their negative failure locations. Reuse focused rule and ownership
   controls. Add only missing controls for irrelevant-fact scaling, owner isolation and loop stabilization. A regression
   blocks expansion. Measure production growth separately from tests and remove temporary detailed outputs.
6. Require each of those five cases to complete below500ms in separate bounded optimized runs, including caller setup,
   checking and lowering, excluding the shared provider and Rust build. Report repeats and actual rule-work counts.
   Reconsider the design if irrelevant facts multiply implication work or equivalent evidence is rebuilt on replay.
   Do not run the full matrices during this performance work. Broader BRA-567 verification remains a later gate.

Frozen30 membership remains comparison.txt minus verification.json comparison_repairs, with fixed identities in
correction-frozen30.json. correction-current.json preserves original-to-current mapping. Fixed920 is907 comparison.txt
names plus13 verification.json comparison_added keys. Count tests separately from matrix rows.
For comparison, each former `source_owned_transfers_preserve_complete_carriers_and_dependencies` or imported counterpart
now means all248 rows under `owned_transfers::source::` or `owned_transfers::imported::`. Each former
`source_deferred_completion_follows_future_ownership` or imported counterpart means all148 rows under the corresponding
`deferred_completion::source::` or `deferred_completion::imported::` prefix. Group success requires every row to pass.

## Checks and maintainability

The mutating style and format commands run through the repository's optimized xtask-incremental alias. Format passes.
Style changes only Rust blank lines and still reports the same7
structural errors outside the admission change:

- cleanup/selection.rs and flow/invalidation.rs have wildcard test imports.
- flow/merge.rs, storage_flow/check/analysis/cleanup.rs, storage_flow/check/analysis/core.rs,
  execution_guarantees/portable.rs and bray-tooling output/diagnostic/json/argument.rs exceed the function limit.

This checkpoint is bounded to diagnosis and cleanup, then stop. These7 errors remain gate failures.
The end audit rereads AGENTS.md, reviews every changed file and searches equivalent proof-preparation and budget-cache
behavior. Raw/mixed engines share one memo helper. Contexts retain exact predecessor/owner evidence and original proof
budgets, with no persistent flow-state cache. Net nonblank growth is101 production lines and36 test lines. Formatting
growth is separate. git diff --check passes.

## Remaining contracts after the pause

Borrowed generic tuple/array transport lacks current-data completion: exact typed reference leaves have no prepared
reference subjects or corresponding result completion relation. Replay must use actual stored-value paths and current
completion without preserving entry referents. Conditional ownership alternatives cannot become unconditional borrow
relations. Protected/raw negatives need a clean forwarding prefix and failure at the actual mutation boundary.

Inactive conditional result shape remains incomplete for unknown nullable/nested/optional extraction. Reuse ordinary
current-shape publication and known_variant/known_presence. Do not infer shape or ownership from a witness. Recorded
anonymous wrapper and payload ownership stays authoritative after borrowed/scalar rejection, including explicit empty
records. Absent terminal snapshots cannot erase live result ownership. Unknown storage remains conservative.

RawBuffer Push lacks generic stored payload metadata and allocation-payload exit publication. Close producer/export/
import/staged replay before schema edits. Whole-value source edges belong inside the existing stored-value graph,
separate from physical pointer targets. Distinguish resolvable open-T contents from unknown pointer origins. Preserve
formal value origin after moving from a source slot without borrowing it. Conditional stores keep unchanged alternatives;
AnyElement append unions prefix contents and pop retains possible remaining contents. Reuse canonical clear/retirement
and generic raw Write. Cover two referents, scalars, nested owners and source/import replay.

Keep original opaque raw-pointer, replaced-reference, actual mutation and fabricated-owner negatives. Preserve actual
root/main authority, lexical cleanup before terminal publication, normal/panic/cancellation, retained services, sync
control and native archive/link requirements. Focused compiler evidence is not issue-wide completion or mergeability.

Keep only current compact outcomes, active build evidence and frozen identities. Remove obsolete task-owned detailed
logs by verified explicit paths under target/audit. Own all processes. Run native toolchains outside the sandbox.
Checkpoint coherent changes on feature/bra-567-async-root-orchestration and verify remote equality.
