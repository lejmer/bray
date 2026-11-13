# BRA-567 current correction audit

Read this record and the active worktree's [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction. Resume from the unresolved contract.

## Scope and checkpoint

Correct the original thirty comparison failures for [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on
[PR710](https://github.com/lejmer/bray/pull/710). The user authorizes checkpoint commits and normal pushes to the existing
feature/bra-567-async-root-orchestration branch, then continued correction work. Do not merge or mark the issue complete.
Do not edit the original develop checkout or create worktrees.

Frozen30 membership is target/audit/comparison.txt minus verification.json comparison_repairs, also recorded in
correction-frozen30.json. Map relocated namespaces or deliberate renames only. Count tests separately from matrix rows.
Fixed920 membership is907 comparison.txt names plus13 verification.json comparison_added keys.

The current identity batch is verified on uninstrumented compilation binaryA058651C.
The live-return/absent-source collision control passes, including guarded absence, Unknown mutation and partial child
availability. The existing open-T choose/current-exit-tag source/import oracle passes. The16case select/nested oracle
executes all source/import rows: both exported possible-input assertions pass,8 positive held requirements still fail
and8 selected fabricated-owner negatives retain the expected diagnostic. This is2 passing tests and1 failing test,
not16 independent tests. correction-identity-current.json records exact source/binary identity and current failures.

The implementation now preserves selected input identity; shallow generic contents preservation remains unresolved.
Four required source/import deferred/owned wrappers, exact30, fixed920 and all final gates need fresh verification after
that repair. correction-cleanup-current.json retains only that fixed bounded scope, without obsolete outcomes. The two
proof-tier cleanup controls must run with --ignored. No temporary tracing or fixture filter remains. Checkpoint progress
is pushed to the existing feature branch, and work continues without a mergeability claim.

## Behavioral contracts implemented

Known local slots begin absent before evaluation and lexical retirement resets loop re-entry. Unknown external or
loaded storage remains unknown. Initialized-predecessor joins retain applicable data and ownership, while absent
terminal snapshots cannot erase live ownership. Cleanup reads retained storage rather than rebuilding skipped syntax.

Each cleanup part uses its prepared payload guard and actual availability. False skips the full action, true executes
it, unknown branches and merges the untouched alternative. Logical effects, completion and retirement share that action.
Conditional physical mutations use the existing MAY union. Unknown initialization cannot publish unconditional
completion. Duplicate lifecycle entry and initialized/present/moved completion gates are removed.

Ordinary Boolean closure refines selected and skipped branches without any witness. Ordinary occurrences never become
trusted evidence. Recorded ownership selects proven applicable Cases, removes false alternatives, retains unknown Cases
and distinguishes reachable no-transfer layouts from missing information. Explicit empty records survive frame,
current/result joins and deferred capture. A selected source layout remains authoritative after selector expiry.

Normal-result publication carries proved portable output shape even with no input transfer. Each ordinary Boolean atom
maps independently through returned_condition and strict portable encoding, then must inverse-substitute exactly to its
original current atom. Unsupported or stale atoms weaken under polarity. AND retains conjuncts, OR retains all
alternatives and becomes true if one is unconstrained. One per-clause cache charges the unchanged Boolean budget for
unique condition/polarity pairs. This replaces whole-clause budget coupling and solver rediscovery. No-transfer Cases
pass through template conversion, interface/remap and replay. Physical input-retention queries still require transfers.

Address provenance follows current borrowed data and existing field associations without ownership rights. Classified
raw keys use the same current descriptor meaning as predicate arguments. Conjunct normalization prevents unbounded
returned-marker duplicates. Address observations preserve copied pointer bits and physical targets, never reached-state
facts. Explicit aggregate Identity masks replace marker-to-parent inference. FunctionPointer completeness describes
immutable code while preserving provider dependencies. DataPointer reachability remains conservative.

Value/Reference modes share one Descriptor evaluator and survive substitution, constants, interface tags, hashing and
lowering. Raw-pointer value predicates read current stored bits. Sequence descriptor observations retain their view
identity separately. Original opaque raw-pointer, replaced-reference and actual raw-mutation negatives remain required.

Ambient physical and logical effects share external/escaped reachability. Active mutable loans preserve inline
Identity/Marker across Ambient only when their actual target remains unpublished. Explicit mutations and escaped
inputs retain their effects. Task last-use handling follows consumed-slot retirement and excludes Input, Attempt and
Await boundaries. Catch success consumes logical source availability after publishing its pending payload, retaining
the terminal data for existing physical transfer. Nullable Input and Projection forms share one recorded-layout mapper.

Staged destructor receiver moves use checked CallInput admission. Numeric callable conversion proves bounded target
entry implication. Defaults publish portable copied-value meaning. Corrected tests retain actual invocations and staging,
accept borrowed-marker guarantees without owned transfers, and preserve changed-pointee/fabricated-owner negatives.
Fresh RawBuffer provenance is allocation-backed. Private cleanup may have no external effect. drop_selected must retain
exact Direct Parameter0 OwnedTarget Retire with no Unknown. Reverify this reached assertion on the final binary.

## Remaining contract boundaries

The small two-input oracle executes8 rows in source/import modes: unknown choice with both acquired witnesses,
known choice selecting a real witness, selected fabricated-owner negatives and the equivalent nested wrapper. Its8
positive source/import rows fail and its8 negatives retain the expected trusted-obligation diagnostic. Decoded select<T>
incorrectly publishes unconditional Input0 and false contents preservation, including when Input1 is selected.
The initial CFG join is correct: Case(take,Input0 preserved)/Case(!take,Input1 preserved). Raw normal-return collection
already has Input0 in both Cases with false preservation. Common-transfer reconstruction faithfully consumes that
corrupt layout, so no change to it is justified.

The first corruption is established at invalidation. Callable result s0 is Present before cleanup and moved source s4
is Absent, sharing J but retaining different snapshot layouts. Live s0 frames selected Input1 into Owned(J,PostState),
then absent s4 frames a stale Input0 layout into the exact same next key and overwrites it. Boundary publication then
faithfully retains the wrong input. The repair gives all3 invalidation readers one private value reader using existing
resolved_availability. Only definite Absent is excluded. Unknown still changes, and partial-root/component policy remains
unchanged. The live result continues to frame genuine effects. This removes the three unconditional terminal reads
without a new value representation or pending-return mechanism.

The semantic control uses a live return and an absent moved source sharing identity but different layouts, covers
literal and proven guarded absence, and checks Unknown mutation plus an absent child under a still-possible root.
The existing16case source/import oracle exercises actual invalidation callers. The uninstrumented collision and exported-input controls pass. The consumed probe is removed. False physical preservation on the live open-T result is separate: determine
whether its Value observation is absent or whether joined MAY reachability overlaps it. Compare specialized Marker
effects at the caller before changing policy. No preservation exception is added. Deferred formal tuple/array still
loses its nested required marker through start/cancel/join and remains in scope.

RawBuffer Push still installs no stored metadata and exit publication excludes allocation payload slots. Close the
producer/export/import/staged replay contract before schema edits. Whole-value source edges belong inside the existing
stored-value graph, separate from physical pointer targets. Open-T source contents must differ from genuinely unknown
pointer origins. Formal value origin survives moving from the source slot without borrowing that slot. Capture concrete
subtrees before mutation and replay only normal-exit contents. Conditional stores retain unchanged alternatives.
AnyElement append unions prefix contents, pop captures possible contents and retains a possible remaining prefix unless
existing evidence proves empty. Reuse canonical merge/clear/retirement, including generic raw Write. Cover two different
referents, scalars, nested owners and source/import replay. Add no separate State map, graph or witness ownership rule.

## Completion requirements

Strengthen remaining typed replacement assertions, remove dead test branches and rename misleading corrected test
identities with explicit frozen mapping. Run fresh exact30, checker controls outside fixed920, adjacent controls and one
bounded fixed920 comparison. New Descriptor interface meaning also needs relevant source/interface/native gates. Keep
issue-wide root authority, lexical terminal cleanup, panic/cancellation, retained services, sync control and native
archive/link gates separate from focused comparison evidence.

Before finalization reread AGENTS.md, review every changed file, search outside changed files for equivalent behavior
and remove superseded machinery. Measure production and tests separately. Run mutating cargo xtask style and
cargo xtask format, inspect their changes, then meaningful required checks. Checkpoint validation is not a mergeability
claim. A partial repair does not complete this task.

Keep only active build evidence, compact outcomes, frozen membership and actionable failure logs. Delete superseded
task-owned logs by explicit verified paths under target/audit. Do not sweep other tasks' artifacts. Own all processes and
run native toolchain workflows outside the sandbox. One CPU-heavy job at a time.
