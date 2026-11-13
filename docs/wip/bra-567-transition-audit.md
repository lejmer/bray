# BRA-567 current correction audit

Read this record and the [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction. The user requests pausing after this checkpoint is committed and pushed. Do not start another batch.
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

## Current verification

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
Focused owned and deferred source/import probes pass. Their providers checked once per process in812ms and267ms,
respectively. Both catalogue checks,6 runner tests and the8-row admission oracle pass. The optimized build succeeds
with opt_level=3 and debug assertions disabled. This does not establish where the previous fifteen-minute stall occurred.
The complete owned selection finishes in122s:484/496 matrix rows pass and12 fail, comprising six source/import pairs.
The deferred selection reaches its900s bound:281/296 rows pass,10 fail and5 remain unfinished. Across both selections,
15/16 additional related tests pass. These are observed outcomes, not newly established regressions against a full prior run.
Deferred failures cover task address/frame and returned-future authority. Owned failures cover conditional result authority.
The unfinished deferred rows are both modes of preformed_task_selected_after_completion and
preformed_task_fabricated_after_completion, plus source same_site_independent_second. Provider checks finish below1s.
Other slow same-site rows finish after555–794s, locating expensive analysis inside individual cases.
The deferred case phase currently ends before its final aggregated assertion, so panicking=false in its progress log
does not establish success. Use the final test report for outcomes. All runner-owned processes terminate at the bound.
Fixed920 and issue-wide source/native gates remain unverified. No compiler fixes or second broad run follow this checkpoint.

Frozen30 membership remains comparison.txt minus verification.json comparison_repairs, with fixed identities in
correction-frozen30.json. correction-current.json preserves original-to-current mapping. Fixed920 is907 comparison.txt
names plus13 verification.json comparison_added keys. Count tests separately from matrix rows.
For comparison, each former `source_owned_transfers_preserve_complete_carriers_and_dependencies` or imported counterpart
now means all248 rows under `owned_transfers::source::` or `owned_transfers::imported::`. Each former
`source_deferred_completion_follows_future_ownership` or imported counterpart means all148 rows under the corresponding
`deferred_completion::source::` or `deferred_completion::imported::` prefix. Group success requires every row to pass.

## Checks and maintainability

`cargo xtask format` passes. Mutating `cargo xtask style` ran and applied deterministic formatting. It still reports7
structural errors outside the admission change:

- cleanup/selection.rs and flow/invalidation.rs have wildcard test imports.
- flow/merge.rs, storage_flow/check/analysis/cleanup.rs, storage_flow/check/analysis/core.rs,
  execution_guarantees/portable.rs and bray-tooling output/diagnostic/json/argument.rs exceed the function limit.

The user bounds this checkpoint to the matrix harness, then stop. These7 errors remain gate failures.
The end audit rereads AGENTS.md and reviews all changed files. Source revision reuses the canonical fixture updater.
Named registration and live progress are shared between matrices. The existing runner owns filtering, CPU scheduling,
timeouts and reports. Concurrent completion gaps are labelled accurately rather than reported as individual durations.
The compiler implementation is unchanged. Most growth registers named tests. git diff --check passes.

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
Checkpoint this tested batch on feature/bra-567-async-root-orchestration, verify remote equality, then stop.
