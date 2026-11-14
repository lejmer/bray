# BRA-567 current work

Read this record and the work-on-bray-issues contract gate after compaction. Update in place.
Branch: feature/bra-567-async-root-orchestration. PR: https://github.com/lejmer/bray/pull/710.
Commits and pushes are authorized. Continue after checkpoints. Do not run full matrices.
Current priority: finish BRA-567 correctness, required verification and review. Further performance
optimization is deferred to a separate PR at the user's request. It is not a gate for this PR.

## Status and applicable evidence

The stored-value batch builds in optimized tooling. All 558 checker, 221 symbol and 217 interface tests pass.
The four RawBuffer rows pass in both source and import modes, including copied scalar independence and
projected-borrow rejection at inspect. The fixed 48 borrowed-field and 12 conditional-owner controls pass.
Frozen30 passes all 30 cases in one final run, including the RawBuffer source/import fixture.
The fixed five pass at 140-154 ms, with the loop at 154 ms. No full matrices ran.
Compact evidence: stored-validation.json, proof-frozen30.json and proof-indexed-current.json in the audit output.
Earlier nine activation controls remain valid for their unchanged code. Final BRA-567 gates remain open.
Mutating style has six existing errors and no new errors. Format and diff checks pass.
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

Implemented in the working tree:

- StoredValue wraps existing descriptors plus whole sources, tracked state and completeness.
  Transfer, joins, clear, retirement and deferred relocation carry it. Source identities never relocate.
- The existing observation shape walker records opaque/generic leaf paths. Entry seeding resolves those
  actual paths and existing RawBuffer backing. Concrete scalar leaves receive no symbolic source.
- Push/Pop capture backing before effects. Pop preserves possible remaining contents. Intrinsic attempts
  reset their pending transition. Missing source capture downgrades completeness.
- Portable transport and per-exit publication are implemented. Ordinary/lifecycle reference preparation
  shares one helper. Composite conversions project endpoints, with conservative uniform-array handling.
- Call/lifecycle stored replay resolves all pre-effect sources before staging. Missing routes preserve
  uncertainty on the actual containing root. Concrete scalar destinations are skipped.
- Reachability consumers now carry or discharge unknown contents at eligible actual targets, including
  escape, ambient invalidation and terminal checks. Exported escape roots remain direct operands.

OwnedTarget replay follows actual backing pointers. Its overlay retains the original result boundary,
so staged physical slots take precedence without resurrecting old result storage. Concrete physical payload
references use DependencyBorrowRelation/InputRegions, retaining projections without alias authority.
Whole-source records carry actual value lineage only. Additive weak events differ from complete final source
sets. Fresh backing supplies an explicit empty summary. Portable dependency replay imports neither
initialization nor deferred-frame identity. Intrinsic Push/Pop retain actual value-transfer metadata.
Exact element reads include overlapping AnyElement summaries without weakening exact writes.
Imports belong at module scope by default, with test imports in test modules. Probes are removed.

Next: checkpoint this batch, then finish remaining source-composition and broader verification gates.
Conditional stores, multiple referents, pop/remaining contents, overwrite and retirement have memory-level
controls. Real source/import composition for conditional stores and pop with another retained owner remains
unverified. Keep that exclusion visible while closing the contract. No full matrices.

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

After contract correctness: fixed920 and relevant source/interface/native gates, maintainability review,
and owned/deferred matrices only when authorized again. Verify root/main authority, cleanup before terminal
publication, normal/panic/cancellation, rollback exactly once, services through static disposal, synchronous
control and native archive/link evidence that Rust orchestration is removed. Preserve deferred-admission
control d302124. Independent draft-PR review precedes ready/In Review. Close the issue only after merge.

Existing structural findings: wildcard flow/invalidation import and oversized flow/merge, storage_flow
cleanup/core, execution_guarantees/portable and tooling output/diagnostic/json/argument functions.
