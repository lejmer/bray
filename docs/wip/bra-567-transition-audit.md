# BRA-567 active performance correction

Read this record and the work-on-bray-issues contract gate after compaction. Continue after checkpoints.
Work and pushes to feature/bra-567-async-root-orchestration are authorized. No full matrices.

## Acceptance contract

All five fixed formerly unfinished fixtures must take less than 500 ms each for caller construction, checking
and lowering, excluding provider preparation and Rust builds. same_site_independent_second must also stay
at or below 10,000 combined condition comparisons and interner lookups, recursive work included. Never hide
or relocate counted work. Timing passes. The operation-count gate remains unmet.

Preserve outcomes and rejection at observe(&value), source/import behavior, exact owner, dependency, current-value
and entry-snapshot scopes, qualified polarity and original budget behavior. Unknowns, overwritten contents,
borrowed prefixes and unavailable carriers grant nothing. Preserve authoritative empty ownership alternatives
and cleanup-feasibility rounds. Published structures remain immutable. No reconstruction when retained results
can serve consumers. No cache lock spans recursive inference. Style only at completed-batch finalization.

## Current implementation and evidence

ConditionIndex shares structural identity independently of authority. Each exact fact-ID/polarity vector owns
one raw query memo. Its ordered trusted/ordinary pair identifies a mixed scope. Physical aliases retain every
immutable allocation. Query memos retain original semantic work costs and exhausted-limit results. Owning queries receive a proof view pairing their exact trusted and ordinary
inputs. Memo misses retain query IDs through computation. Budgeted memo indexing holds no result-map lock.

Solved flow outputs and actual cleanup parts/phases are retained. Wrapped futures preserve cancellation captures,
part-specific mutation identity, report dependencies and lifecycle retirement. Keep these regression controls.

RetainedSnapshots now owns actual roots and one lazy candidate index. Insert invalidates it in one place.
Owner matching and root/path indexing reuse the same definitions as direct comparison. Nodes reached through
loans remain distinguishable from directly retained nodes. Owner queries exclude loan traversal. Data queries
may follow actual referents. Prefix matching only selects actual components under the original WORK_LIMIT.
The original role/contents predicate still decides retention. Unknown contents cannot prove survival.
Constructed bases are lineage, not separately retained data. Before/after checks compare actual selected contents.
Retirement, scalar preservation, private inputs, cleanup, mutation masks and borrowed carriers reuse these indexes.
The old static query API and repeated graph walk are removed. No proof rule or authority publication changes.

BooleanGraph consumes the ConditionIndex's existing child IDs and shares immutable nodes. It no longer recursively
re-interns children. Node truth, published polarity, ordinary/trusted roles and query results stay scope-local.
Child projection/order and relational fallbacks are unchanged. No index lock spans graph recursion.

OrderedFacts now prepares ordered edges once per exact proof scope. Edge positions preserve every old scan budget
charge, DFS order, strictness, reversed equality and integer shortcuts. False comparisons provide no ordered edges.
False inequality can establish an ordered domain. Empty graphs retain the initial integer shortcut and original scan
cost without interning endpoints. Trusted and ordinary baseline memos stay separate. An implication hypothesis shares
its memo until actual alias consequences change its facts, then resets every result while retaining structural IDs.
The production scan is removed. One differential test retains it as an independent oracle over 128 graphs and budgets
0-64, checking results and remaining budgets, including cycles, missing edges, negative evidence and literal bounds.

All 552 checker controls pass. Fixed five twice with counters compiled but disabled: task cases 123-130 ms,
loop 171/178 ms. Additional source/import real-owner loops and imported fabricated-owner loop pass at 169-180 ms.
Frozen30 remains 29 pass and the same RawBuffer invalid acceptance in 12.2 seconds. No full matrices ran.
Style retains six existing errors: wildcard flow/invalidation and oversized flow/merge, storage_flow cleanup/core,
execution_guarantees/portable and tooling output/diagnostic/json/argument. Format and diff checks pass.

Complete count is 2,222,361: ordering 1,087,749, equality 543,442, identity 270,575, interning 320,595.
Top-level comparisons are 850,288 ordering and 494,518 equality, with maximum depths 20 and 12. Recursive
comparison tuning alone cannot approach the target. Repeated root operations dominate.
The caller has 198 CFG blocks and two required storage solves with 271 transfers each. There are 6,219 indexed
nodes across indices, 612 mixed scopes over 327 raw inputs and 13,032 public proof queries. Witness refinement
performs 856 computations, usually over five claims. Only 12 new witness clauses result. Join publication adds
344 clauses. Refinement costs 546K and Boolean derivation 286K operations. Stage costs overlap. Do not sum them.
Raw input sharing cut ordered queries from 28,264 to 14,769 and ordered-index constructions from 2,896 to 1,599.
The count target is not met. Correctness is unchanged, and more lookup tuning alone is insufficient.

## Current optimization model

Production baseline c043523c2 is pushed, with no probe hooks. A diagnostic-only bounded loop run passes in
410 ms with counters enabled. It measures 2,213,955 operations. This is build variation, not an optimization.
Retained timing and regression evidence still applies to unchanged production code.
The Boolean prover already has parent edges and a change-driven queue. Optimize the enclosing dependencies.
Code tracing gives this computation graph:

- incoming_state -> propagate -> merge -> join_observations -> conditional_facts -> witness/proof queries
- transfer_block -> apply_operation -> current-value rewrites, witness refinement and retirement
- refine_witnesses -> every available witness and pending deferred producer -> exact proof scopes
- retire_values -> retirement_dependents plus scans of facts, witness clauses, private entries and selections
- final block entries -> publication apply_operation replay -> diagnostics, effects and completion products

ConditionIndex already shares structure. retirement_dependents already uses a worklist, but rebuilds reverse
edges per mutation. Edge membership depends on current values, mutation kind and retained owners.
Exclusive costs partition 2,222,361: proof/refinement 723,963, retirement/retention 663,517, rewriting 235,471,
scope/reuse/normalization 144,633, joins 68,266, unclassified 386,511. Inclusive stage costs overlap.
Eliminating recursive comparisons below the root saves at most 286,385, about 13 percent. Eliminating all caller
publication saves at most 503,967, about 23 percent before replacement costs. The target requires over 99.5 percent.

Model a typed dependency hypergraph because rules can require several inputs together. Separate structural
conditions, qualified facts, carrier authority, contents snapshots, availability and query results. Contents
equality never merges owners. Captured entries differ from current values. Lifetime support does not establish
truth. Preserve predecessor/selector requirements and distinct loop incarnations at the same source expression.

Minimize graph construction plus sum(evaluation count * exclusive evaluation cost), with bounded memory,
unchanged results, rejection sites, budgets and determinism. All indexing and recursive work counts. Replace
whole-collection discovery with producer-owned dependency edges. Lazily schedule affected consumers in stable
order, retaining immutable results and the existing cyclic join/convergence rules. Reuse existing graph nodes.
The candidate spans retirement selection, witness guards and join substitution. Remove their repeated scans.
Do not add another truth representation, manual index repairs at every mutation, or a general incremental engine.

Two inventories must close before edits. Dependencies include contents, ownership, addresses, borrowed paths,
selectors, availability, private evidence and independent supporting carriers. Structural child IDs alone are
insufficient. Also account for failed proofs and budget costs. New evidence can change an absent result, and
ordered scan positions can change exhaustion without changing truth. Successful premises alone cannot justify
reuse. Preserve costs through indexed accounting, as OrderedFacts does, rather than replaying unrelated scans.
The dependency probe measures 88 indices, 6,219 structural nodes and 9,633 child edges. Outer index requests
repeat heavily: scope preparation 21,318 requests/91 distinct nodes, proof queries 65,414/587, refinement
21,682/153, rewriting 25,523/256, retirement 120,098/2,536. Distinct means index plus node, not proof scope.
Retirement has 92,546 pure-cache hits and 27,552 misses. It removes 142/1,864 ordinary facts, 276/716 trusted
facts and 1,148/2,604 witness clauses. Thus many changes are real. Broad retirement skipping is unsound.

Next close the join rule contract in flow/witnesses.rs::conditional_facts. It enumerates proposed clauses and
proves each against both predecessors. Trace whether actual incoming support and selectors can supply those
transport edges directly, preserving inactive alternatives, unavailable carriers and exact source ownership.
Combine that with direct traversal of retained condition nodes. Do not add per-query ID adapters or caches.
Structural edges alone cannot encode this transport. Semantic changed-edge counts remain unresolved, and the
current data does not predict a 10,000 result.

Publication reuse is secondary. Its open contract includes intermediate checks, diagnostic deduplication,
final-input identity, cancellation and publication-only computations. A final block output is insufficient.
Keep fixed-five/source/import/checker/frozen30 controls. Do not repeat eager retirement fusion or witness-key
experiments. Both increased work. Unchanged carriers or empty invalidation sets do not imply unchanged guards.
Keep one current count report and probe patch. Remove hooks after bounded runs. Build without hooks for final
acceptance, not every checkpoint. Retain the checker-only non-incremental override and shared Cargo storage.

## Fixed evidence and broader work

Use optimized tooling, shared target/cargo and pinned target/toolchains/llvm/active. Select products from successful
proof-build.jsonl. Native runs stay outside the sandbox. Each fixed case is bounded to 10 seconds. Stop on changed
outcomes. proof-unfinished.json freezes five identities. correction-frozen30.json freezes membership and
correction-current.json maps names. Fixed 920 is comparison.txt plus verification.json additions. Keep one current
counter, timing and regression result. Overwrite stale task-owned logs. Never clean shared Cargo storage.

BRA-567 still needs generic borrowed-field completion mappings, inactive conditional result shapes and RawBuffer
payload dependencies. Keep d302124's deferred-admission control: exact private carrier and nonvacuous qualified
clause through captured entry evidence, without publishing truth. Then finish maintainability, owned/deferred
matrices, frozen 30, fixed 920 and source/native gates. Verify root/main authority, cleanup before publication,
normal/panic/cancellation, rollback once, services through static disposal, synchronous control and native archive/
link removal of Rust orchestration. Independent draft-PR review precedes ready/In Review. Closing follows merge.
