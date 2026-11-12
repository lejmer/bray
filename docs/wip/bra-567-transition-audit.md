# BRA-567 active audit

## Resume gate

Read this record and the active worktree's [contract gate](../../.agents/skills/work-on-bray-issues/SKILL.md#close-the-contract-before-changing-it)
after compaction or restart. Resume at the unresolved contract below, not the last command. Do not patch the next
diagnostic until the affected mechanism's producers, consumers and preservation limits are traced. A regression
reopens that contract and equivalent paths. Tests verify the reasoning, they do not replace it.

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Branch feature/bra-567-async-root-orchestration in target/bra-567-audit. Leave original develop untouched.
Checkpoint pushes authorized. After semantic completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.
Questions and corrections steer this work. Before any final response check the remaining gates and continue any
authorized work. A checkpoint, resolved subproblem or workflow question is not a stopping point.

Inventory actual CFG transitions and their ownership, storage, liveness and lowering consumers.
Prove native async-ownership-composition.bray and invalid variants. Check production before consumption,
authority retirement, identity versus contents and loop convergence. Preserve fixed identities/outcomes.
Unexplained regressions block expansion. Name traced paths, evidence and exclusions.

Keep one current-value tree, alias model, witness ledger and composite flow. Carry producer-owned identities.
Reconstruction is a last resort. Specific structured diagnostics. Internal defects panic at their owning boundary.
Batch independent checks. Proper Bray formatting. Delete consumed probes. Keep records short and current.
Write and inspect correctly formatted Bray fixtures before committing. Never use ad hoc whitespace rewrites.
Only the automated style/format pass is deferred until semantic completion. Then simplify production code, prune redundant tests, audit AGENTS/DRY,
run mutating cargo xtask style and cargo xtask format, and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown returns
unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust. Verify worker/deadline
omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence and pause

Paused for the user's app update. No task-owned processes remain. All code and user corrections are pushed through
9fab0d127. The latest combined tooling build succeeds for that code, including the restored ordinary shared-loan rule.
Do not start from the last diagnostic. Read the contract gate before changing the unresolved mechanism.

Accepted baseline: fixed920=791 pass/30 fail/99 ignored. Original907 identities/SHA are frozen with16 explained repairs
and13 added controls. Owned248/deferred148 on each source/import surface and witness matrices retain outcomes.
Last candidate before restoring ordinary shared-loan protection:786/35/99, checker511/bound136 pass. One repaired
negative and six regressions. That candidate is unaccepted. Its complete diagnostics are in comparison-candidate.log.
Latest source and fixture changes have compiled but have not run through the comparison again.

verification.json holds binary paths/digests, frozen expectations and372 relocated test-name mappings. Normalize current
names through that mapping, never rewrite the frozen comparison. The source-item moves retain all tests. Inline fixtures
now live in contract_tests topic files. Preserve the user's formatting and path simplifications, including when blocks,
template clauses, tuple layout and array types. A brace-pair check did not establish correct formatting. Never use ad hoc
whitespace rewrites. Write and review fixtures as Bray source. Style commands do not format embedded Bray.

Accepted runtime source23 =6 E7123 +17 E7135, seven local failures and sixteen cascades. Local groups are driving197,
start230 producer and224 cleanup, prepared_start175 registration postcondition, polling105, withdrawal185/199. Three
start.bray span relocations from the consuming receiver are explained. Native last25 remains stale for current semantics.
Async composition source/MIR and3 invalid variants pass at the accepted checkpoint. Native composition has not run.

## Unresolved mechanism

Shared-view forwarding restored evidence through an overwritten reference from its old pointee. The pending repair
requires descriptor-slot preservation before forwarding, removes marker-derived claims about containing pointer fields,
and records nullable tags as Discriminant rather than descriptor identity. Ordinary opaque effects cannot grant mutation
through a retained shared loan. Their original protection is restored. Actual raw operations retain their alias scope.
Do not accept the batch until every regression is explained and repaired. No additional fixture expansion.

Next deduction to trace: storage/observation.rs::prepare special-cases shared root borrows, while mutable-root borrow
fields can enter owned_observations. transfers.rs then uses Target::storage(logical_path), although the actual bytes can
be an Entry borrow referent. Opaque framing also uses those owned observations. Establish the physical boundary for all
borrow modes and nested loads before changing this producer. Reuse prepared checked routes and current memory identities,
never recover an old value from a witness or manufacture a narrower alias bound.

Escape-summary regression: exposing owner.pointer and address_of(owner.value) in Frame produces2 roots where the fixed
control expects3. Frame.value is &Value and Value contains a raw pointer. Determine whether a reachable pointer was lost
or only a redundant derived incoming root was omitted. Inspect source and import together. Do not change expected counts
to hide a missing dependency. The other five candidate regressions concern shared scalars, borrowed completion, nullable
adaptation, raw entry owners and observer protocol changes. Their statuses after restoring ordinary protection are unknown.

Closed rules remain protected: consuming @copy receivers move, ordinary copies cannot transfer authority, nullable call
inference permits one outer construction, raw AddressTarget paths carry finite entry demands, checked physical effects
replay for every obligation, moved nullable owners retain extracted values and retire old slots, deferred joins stabilize,
and diagnostics retain specific conditions/causes. Missing internal proof causes panic. Keep these rules coherent.

## Execution and remaining gates

One CPU-heavy job at a time. Cargo jobs24, tooling profile, same combined targets, shared target/cargo and LLVM toolchain.
Run native tooling outside the sandbox. Never edit Rust during a Rust build. The combined build includes brayc, xtask,
bray-symbols, bray-package-interface, bray-bound-tree, bray-compilation, bray-checker, bray-messages and bray-tooling with
--profile tooling --all-targets. Reuse binaries in verification.json for tests. No cargo clean or new target directory.
Stale incremental caches were pruned by explicit user instruction. Delete consumed task probes and logs.

Remaining gates: fixed-suite convergence, all runtime source/native failures, native async success/panic/cancel/startup
rollback/blocked shutdown, result/static disposal and foreign restoration. Then simplify production code and redundant
tests, perform the AGENTS audit, run mutating cargo xtask style and cargo xtask format, validate affected behavior,
obtain independent PR review, mark PR ready and Linear In Review. Never merge, mark Done or claim complete composition early.
