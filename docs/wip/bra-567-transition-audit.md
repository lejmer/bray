# BRA-567 current work

Read this record and the work-on-bray-issues contract gate after compaction. Update in place.
Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Commits and pushes are authorized. Continue after checkpoints. The user has now authorized the full owned
and deferred source/import matrices for final verification, with bounded execution.
Current priority: finish BRA-567 correctness, required verification and review. Further performance
optimization is deferred to a separate PR at the user's request. It is not a gate for this PR.
Checkpoint c298e3b7a replaces manual host/sync source inventories with shared deterministic discovery.
Current uncommitted batch preserves tagged payload owners, branch guards, returned nullable shape,
compiler-only conversion values, and authoritative moved-path absence during cancellation selection.
The shared owned-storage preservation mapper replaces both duplicated conversions without a temporary vector.
Mutating style, Bray format and optimized build pass. Current bounded checks: checker559/559,
frozen30/30, owned492/496, deferred292/296, production sync and native-thread ownership pass.
Remaining matrix failures: conditional_optional_join, select_nullable_inputs_join_both,
task_frame_borrow_ends and task_returns_observation_future, each in source/import mode.
transition-validation.json records exact identities, binary hash and results. Production report is
running with a 240s bound. A 90s timeout did not identify a regression because this full-provider
fixture previously exceeded 60s. Namespace filters avoid Windows' argument-length limit.

## Status and applicable evidence

The stored-value batch builds in optimized tooling. All 558 checker, 221 symbol and 217 interface tests pass.
The four RawBuffer rows pass in both source and import modes, including copied scalar independence and
projected-borrow rejection at inspect. The fixed 48 borrowed-field and 12 conditional-owner controls pass.
Frozen30 passes all 30 cases in one final run, including the RawBuffer source/import fixture.
The fixed five pass at 140-154 ms, with the loop at 154 ms.
Full comparison on the structural-cleanup build: 1,582 pass, 59 fail, 74 incomplete after one batch's
180-second bound. This includes the 916 mapped fixed identities and all owned/deferred namespace tests.
Thirty matrix failures concern guarded owned selection and six concern deferred result/frame transport,
symmetrically in source/import modes. Another 23 failures include the owned aggregate and production
contracts. Diagnose shared boundaries before semantic edits. comparison-result.json retains identities.
Compact evidence: stored-validation.json, proof-frozen30.json and proof-indexed-current.json in the audit output.
Earlier nine activation controls remain valid for their unchanged code. Final BRA-567 gates remain open.
Mutating style passes after seven-file structural cleanup. Its build and 558 checker tests pass.
The ABI correction passes 203 checks: tooling 115, runtime interface 29, native ABI 35,
test protocol 23 and backend identity 1. Native smoke execution remains pending.
All unreleased compiler-owned ABI/protocol/backend versions stay at 1 or 1.0. Reset declarations and
native fixtures together. Deliberately incompatible test versions and upstream versions remain distinct.
Format and diff checks pass.
Stored paths must name physical Receiver, Parameter, Result or
static roots. Capability, witness and EvaluationStorage roots cannot be replayed. Reject them at import and
assert the producer invariant, for destinations and sources. Result remains forbidden as an entry source.
The final results include this validation repair and its external-input rejection control.

## Active stored-value contract

RawBuffer Push/Pop must transport stored payload dependencies through generic calls and normal returns.
Effects describe possible events, not final contents, so they cannot reconstruct this information.
Use the existing State.values, Transfer and ReferenceExit graph. No parallel persistent graph.
Whole sources are immutable entry Target snapshots, separate from reference targets and ownership.
They are possible stored data, never equality, borrow, ownership or trusted-witness authority.

- Complete output records describe all possible final sources. Incomplete records retain uncertainty.
- Exact overwrite removes old contents. Conditional writes retain surviving entry alternatives.
- AnyElement stores are weak. Pop retains possible remaining elements unless emptiness is established.
- Explicit complete-empty differs from missing, unknown and uninitialized state.
- Capture sources before mutation or consumption. Publish only on normal completion.
- Resolve output allocations before replaying their payloads. Abnormal paths install nothing.
- Actual scalar type information can establish reference freedom. Missing graph nodes cannot.
- Unknown reachability uses the existing unknown_reference_may_reach rule. Private unexposed inline
  storage remains independent. Stored sources never become alias edges merely because they exist.
- Ordinary current tags and NoPayload records remain authoritative. No historical entry resurrection.

DependencyStoredValuePath carries a subject and terminal borrow depth. Nonterminal loads come from
checked StorageReferencePath preparation. Raw AddressTarget remains a raw-pointer operation.
Entry.layer is a load ordinal at its reference prefix, not a global depth. Descriptor sources refer
before the final load. Whole-value sources refer after it. Preserve this distinction across projections.

The portable per-destination record carries source endpoints and completeness. Normalize by full endpoint.
Carry it through semantic validation/substitution, source recursion summaries, bound ordinary/hidden/
conversion/deferred calls, lifecycle calls, export, codec, selective import/remap and specialization identity.
CURRENT_FORMAT_REVISION stays 1. There are no released formats to preserve or increment for this work.

Per-exit publication maps actual allocation targets to observable output anchors before merging exits.
Inactive NoPayload alternatives are neutral. Missing active outputs make the record incomplete.
RawBuffer payload paths use owner.OwnedTarget.AnyElement. Replay must follow the actual owner pointer
into its allocation. pointer.AddressTarget.AnyElement has the wrong typed shape.
Ordinary descriptor relations retain their separate authority. Possible payload replay must not weaken
an independently established exact descriptor or publish unobservable by-value input writes.

## Integration state and next action

Stored-value transport and version alignment are pushed in 51336bd5b and cf0bf217a. No probe hooks
remain in the verified source. Next: trace the four remaining matrix patterns before further semantic
edits, then run fixed920 and native gates. Keep current positive and negative identities unchanged.
The current batch removes duplicated forward preservation mapping and keeps reverse export validation
separate. Independent inspection found no concrete authority widening. Matrix evidence remains the gate.

Boundary contracts and unresolved evidence:
- Initial direct/nullable joins retain precise guarded alternatives. selection-trace.log establishes
  the first loss in conditional cleanup: frame stores a branch-simplified transfer, then selection
  joins unconditional alternatives. Key collisions occur later. Preserve proven-true Case guards
  at the ownership selector instead of reconstructing them downstream. False alternatives still
  prune and explicit empty records remain authoritative. Existing selected-empty, overwrite, loop,
  source/import selector and fabrication controls are mandatory. Nullable empty correlation remains
  unresolved in the two named failing owned patterns. No new state representation is needed.
- Deferred union now passes after tagged snapshot repair. Two other deferred cases complete with
  known effects, but their first witness loss remains unresolved. OwnedTarget resolution was not involved.
- Cancellation cleanup part 1 at the native-thread fixture is false in every reachable storage publication
  and true in guarantee replay. Both cancellation and lifecycle now consult the existing definite-absence
  exit decision. A missing selected mutation must still panic. No parallel move/skip registry.

Compiler-only conversion contract: Identity, CallableContract and NullablePresent execute no callee.
Their selected input conversion already has an exact evaluated snapshot. Preserve that transferred
source during completion instead of applying an opaque-call contents mask a second time. Keep call
entry/ownership bookkeeping, nullable output paths and loan roles. Default providers, user conversions
and ordinary calls still apply their checked masks. Empty effects alone do not authorize preservation.
The sync trace shows initialized Uninit<T> evidence surviving, but input conversion changes its argument
to PostState. Existing production sync and mutable/atomic/borrow rejection controls bound the repair.

Returned-value shape contract: completion must preserve the actual source ownership role. A lifted
Present payload is not a same-shaped whole nullable input. Use with_current_contents instead of
recreating Owned from owner_identity, which strips the wrapper. True nullable formals keep root lineage,
borrowed outputs keep their loan path, scalars cannot acquire owned authority, and changed payload masks
remain effective. Existing nullable fabrication/overwrite controls and production create_execution
exercise source/import transport and unwrap. Avoid a second ownership-composition implementation.

Snapshot repair contract: a checked initialized aggregate keeps its supplied destination owner while
reanchoring actual children. A tag-only constructed base describes contents and cannot erase that owner.
Unknown children stay unknown, borrowed children retain their loans, and inactive alternatives contribute
no fabricated ownership. RunResult/Stored currently loses the nested future at this transition before
consume matching. Repair owned_snapshot using its existing identity, with no new state representation.
The repair passes all 559 checker tests and the deferred union in source/import modes. Four other
deferred positives still fail. Existing nullable and fabrication controls remain regression oracles.
Reconsider if destination identity is not authoritative at any caller.

Cleanup-selection contract: cancellation and lifecycle must consume the same checked definitely-absent
paths. Lifecycle replay already uses StorageExitDecision.definitely_absent, but cancellation omitted it
and reconstructed initialization from its weaker execution state. Exact moves populate those paths,
joins intersect them, and broadcast preserves them before the directly following lifecycle snapshot.
Share the part-presence check using live storage facts during collection and the published exit during
replay. Unknown/maybe-moved paths must still select conditional cleanup. Preserve part identities,
mutation lookup assertions, normal/abnormal paths, replacements and sibling cleanup. No new move-event
registry, reconstructed path or missing-mutation fallback. Existing native-thread partial move,
production storage, conditional cleanup and frozen source/import controls bound this change.
Conditional stores, multiple referents, pop/remaining contents, overwrite and retirement have memory-level
controls. Real source/import composition for conditional stores and pop with another retained owner remains
unverified. Keep that exclusion visible while closing the contract.

Regression oracles: scalar independence, reference-bearing payloads, multiple distinct referents,
conditional stores, pop with remaining contents, overwrite and retirement. Negative cases must reject
at the intended mutation/observation boundary. Preserve existing test identities and meaningful bug controls.
Reuse fixtures and helpers. Inline Bray formatting is mandatory and is not fixed by Rust style commands.

The memory/staging and portable/publication reviews are complete. The batch adds about 1,700 production
lines and 350 test lines, including import/style changes. It extends the existing stored graph and portable
contract transport rather than adding a second state graph. Raw logs are removed after recording results.
Do not treat checkpoint completion as BRA-567 done.

## Deferred performance objective and current graph

For the later performance PR, all five fixed fixtures should take under 500 ms for caller construction, checking and
lowering, excluding provider/build time. same_site_independent_second must also reach at most 10,000 combined
condition comparisons and interner lookups, including work moved to producers and maintenance.
The last count is 1,471,543 comparisons and zero interner lookups. The count goal is paused, not achieved.
Current fixed-five times are 140-154 ms, including 154 ms for the loop. While the user shares the computer,
prioritize counts and treat timing as informational. Larger-program scaling remains part of the objective.

The expensive dependency graph crosses scope versions:

- predecessor values/captures -> carrier selection -> predecessor proof scopes -> joined ordered clauses
- transfer/current-value rewrite -> witness refinement and retirement -> next proof scopes
- final block entries -> publication replay -> diagnostics, effects and completion products

The inner Boolean prover already uses a graph and change-driven queue. Preserve typed roles and barriers:
owner/value substitutions differ, captured entries differ from current contents, and absence differs from
unknown. Joins use actual predecessor evidence. Equal contents never imply ownership. Deferred evidence
is keyed by owner, execution site and captured entry. Retirement considers writes, raw mutation, cleanup,
root expiry and all independent qualified supports. Failed/absent probes and exhausted budgets are dependencies.

Current retained representations:

- Immutable ExecutionCondition nodes own structural hashes. Child-slot identity preserves observation
  occurrences. Pair memoization bounds shared-DAG comparisons. No condition interner or global factory.
- ProofMemo uses exact ordered fact/polarity scopes. Raw membership and filtered answers remain separate.
- BooleanGraph/OrderedFacts retain source precedence, ordered DFS, short circuits and original budget costs.
- RetainedSnapshots keeps owner, contents and inline-contents queries separate. Insertion invalidates derived
  answers. Between-snapshot queries depend on both actual sets. Unknowns and loans grant no ownership.
- Comparison forms cache prepared syntax on immutable operations, never scope-dependent truth.
- Retirement subtree queries share traversal/pruning rules. Their larger-DAG scaling benefit matters even
  when a small fixture gains little. Count lookups, hashing, maintenance and memory as well as comparisons.

Optimization must remove repeated work across scope versions without changing publication order, assume
expansion, branch limits, observation occurrences, capture boundaries or exhaustion behavior. Rebuilding an
equal normalized formula can change budget consumption. Allocation IDs cannot replace semantic ordering.
Existing measurements rule out edge-result caching or a retirement reverse-graph rewrite as the main next
step. Joins, substitution, scope preparation and equality construction remain candidates, not proven solutions.
Do not introduce another evidence representation or stack caches without removing superseded machinery.

## Fixed evidence and remaining BRA-567 gates

Use optimized tooling, shared target/cargo and pinned target/toolchains/llvm/active. Keep checker compilation
non-incremental. Native toolchain runs must be outside the sandbox. Never clean shared Cargo storage.
Frozen identities live in proof-unfinished.json and correction-frozen30.json. Fixed920 comes from
comparison.txt plus verification.json. Keep one current count, timing and regression result and the reusable
counter patch. Remove stale logs and JSON streams once their useful result is retained. Final performance
evidence must come from a build without temporary hooks.

Final gates: fixed920, full owned/deferred source/import matrices, relevant source/interface/native checks
and maintainability review. Verify root/main authority, cleanup before terminal
publication, normal/panic/cancellation, rollback exactly once, services through static disposal, synchronous
control and native archive/link evidence that Rust orchestration is removed. Preserve deferred-admission
control d302124. Independent draft-PR review precedes ready/In Review. Close the issue only after merge.

Native coverage uses runtime-artifact smoke-test: runtime-root-smoke.c covers main-lane identity,
suspension/resume, success/panic/cancel, cleanup-before-terminal, retirement and rejected admission.
async-ownership-composition.bray covers real source await/child cancellation/cleanup/static disposal.
The same gate audits native archive/link provenance and synchronous controls. It has not run on this code yet.
