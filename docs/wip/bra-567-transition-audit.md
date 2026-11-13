# BRA-567 current correction audit

Read this record and the [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction. The current target is less than500ms per fixture case. It is not met. Bounded diagnosis and
artifact cleanup are complete. Implementation of the performance plan is authorized. The indexed proof path passes68
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

## Proof performance contract

Target: every fixed tiny case below500ms, including caller construction, checking and lowering, excluding shared provider
setup and Rust builds. No full matrix runs. Retain every positive/negative expectation and actual failure boundary.

ExecutionProof borrows exact immutable trusted/ordinary facts. Shared preparation must never cross an owner, entry,
selection, snapshot or evidence mutation. Keep unknowns, trusted polarity, current referents, unavailable carriers,
overwrite/retirement and authoritative empty ownership records intact. Budget exhaustion cannot establish a claim.
Required cleanup-feasibility replay remains. Independent owner-support checks must stay independent.

Baseline: the cached implementation takes48 to 50s for the four preformed-task rows and117.5s for the independent-site row.
A bounded20s caller interval starts at least1,605,633 implications. The cause is repeated all-premise implication scans.
Alias proposal loops did zero work in that interval. Hot consumers are join source validation, per-owner predecessor
proofs and guarded-witness refinement. Facts include initialization and branch relationships, not just source requirements.

Checkpoint518014c0a replaces the scan with indexed Boolean structure shared with closure preparation. Negating a goal
updates connected nodes. Only contradicted published alternatives reach the existing bounded raw rule. Boolean keys
hash child IDs. Exact facts return before preparation. Empty-context values follow Boolean children directly.
Per-premise preparation maps are removed. The old scan remains only as a unit-test comparison oracle.

The consumer batch shares proof views across immutable resume, address-bound, cleanup-retention, qualification, return
inference and transfer phases. Reference-origin refinement republishes after each witness, so its view is per witness.
Resume reads captured facts while publishing current truth. Cleanup borrows pruned ordinary facts before trusted truth
changes. Branch/scalar reasoning uses ordinary-only facts. False conjunctions retain their actual polarity and use the
same elimination rules as true disjunctions, without reconstructing negations.

Last validated consumer batch:68 proof tests and109 guarantee tests pass. Four fixed task cases retain their outcomes
at3308 to3626ms. The independent-site case remains unfinished at10s. The target is not met. Formal-return mappings are
built once per actual exit, and Boolean lookup keys no longer allocate child vectors.
A bounded native stack sample attributes40/143 inclusive samples to repeated return mapping and30/143 to graph intern.
Inclusive counts overlap. Symbol names for folded native functions can alias and are not source attribution by themselves.

Next contract: assume_availability knows a Boolean guard correspondence but publishes numeric equality. Boolean closure
does not read that relation, and cleanup selection uses raw proof before forking. Publish the existing ordinary
ValueAlias instead. Closure copies Boolean truth only through established aliases. Query hypotheses may follow those
actual aliases but cannot invent alias identity from matching truth. Raw proof can disprove an alias from differing
Boolean values. Availability and cleanup selection must use ordinary-only deduction. Unknown conditional aliases stay
inactive. Keep current value/epoch identity, no reference/owner substitution, no qualified evidence from ordinary facts,
and stable alias cycles. Other existing ValueAlias producers use the same reader.

Current evidence: proof-indexed-current.json, proof-indexed-counts.json and proof-indexed-sample.json. Phase counts include
provider setup and nested durations must not be added. Remove superseded detailed outputs after retaining compact facts.
The original150-line production-growth target is exceeded by the indexed replacement. Final review must simplify it or
justify the remaining growth by required inference and removed machinery. Count production separately from tests/style.

1. Attribute expensive work to actual producers and consumers before further edits. Extend the rule inventory for new
   transitions. Reconsider whenever a fix leaves repeated reconstruction or irrelevant facts multiply query work.
2. Preserve owner, guard and current-value relationships through the existing witness and stored-value mechanisms.
   Check complete production, joins, deferred completion and consumption, including source/import replay.
3. Keep the Boolean graph as the shared deduction mechanism. Preserve comparisons, variant exclusions, negated-goal
   reasoning, qualifier polarity and budgets. Predicate-name filtering and unit propagation alone are insufficient.
4. Hold fixed the five formerly unfinished rows and existing ownership/loop controls. The generated comparison oracle,
   numeric/variant controls and irrelevant-fact work check protect the migration. An unexplained regression blocks it.
5. Require separate bounded optimized runs and repeats below500ms for all five rows. Report actual work and timing
   boundaries. Remove superseded machinery, audit maintainability, run style/format at the end and checkpoint/push.
   Passing these controls establishes performance evidence, not BRA-567 completion or full-matrix correctness.

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
