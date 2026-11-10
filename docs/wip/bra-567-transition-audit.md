# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on draft [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. Independent review, PR ready and Linear In Review follow completion.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively, with invalid variants.
3. Check production before consumption, moved authority retirement, identity versus contents, and loop convergence.
4. Preserve comparison identities/expectations. Explain and repair regressions before expanding migration.
5. Name traced paths, evidence and exclusions. Source/MIR success does not certify native composition.

Retain producer-owned facts. Reconstruction is a last resort. Keep one current-value tree, alias model, witness ledger
and composite storage flow. Distinguish owners, loans, addresses, pointees and contents. Preserve specific diagnostic
causes. Remove duplicate mechanisms rather than adding parallel mappings or new dependency kinds without evidence.
Keep records short/current and delete stale task-owned logs/probes by exact path. Proper Bray formatting applies to fixtures.
No style/format until semantic completion. Then perform the architectural/test-pruning pass requested by the user,
mechanical AGENTS/DRY audit, mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcome,
incident and selected static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance and costs. PreparedStart retains admission/registration through
publication/rollback. EntryRestoration retains displaced context through abnormal exits. RetiredCell disposes control,
pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence and cause inventory

Current checkpoints are pushed to the PR branch. verification.json holds executable identities and results.
All907 frozen test outcomes remain762/46/99. The new allocation import case passes separately. Checker485,
bound135, symbols221 and interfaces216 pass. Cleanup61, predicates30 and focused18 retain their recorded outcomes.
Native Source64a4e5490, complete233-file Windows request:39 unchanged diagnostics,24 E7123 and15 E7135.
Ten E7135 diagnostics explicitly depend on other checking failures. Do not treat these as independent defects.
The async-ownership-composition fixture and3 invalid variants pass source/MIR. Native execution remains unverified.

Implemented invariants remain fixed: publish actual planned result slots before cleanup, read current contents at
completion, keep initialization inventory out of joined data slots, distinguish owner identity from contents and loans,
and retain checked reference origins through projection and guards. Allocation returns now retain producer identity
and alias groups through substitution and interfaces. Freshness grants no initialization authority. Independent returns
remain conservative. Shared snippets shorten code operands only. Conditions, messages and JSON remain complete.

Current unresolved families, with proof distinguished from hypotheses:

- Raw retirement and framing. driving197 loses Current's view witness at wait_execution Attempt. Its referent is known
  and current. The first rejected effect is raw retirement through a returned task pointer bounded by the execution
  owner's stored address. Whether the effect is too broad or the contract lacks a necessary bound remains unresolved.
  Do not add preservation exceptions. Related cleanup/view failures are candidates for this family, not proved duplicates.
- Guarded owner transport. Source and imported witness matrices both first fail on the nullable case with actual Unknown.
  Native entry_scope400 also loses an optional returned owner before consumption. Trace production, join, pattern
  extraction and call-input capture to find the first missing fact before changing implementation.
- Expired address dependencies. Source/import address tests accept access after the backing scope ends. Trace pointer
  stores and input postconditions through physical and witness dependency retirement. This is a safety defect candidate.
- Post-state meaning. Ordinary completion/mutation postconditions and prepared_start175's assigned nullable registration
  are unproved. Inspect their publication/invalidation boundaries. A common cause has not been demonstrated.
- Extraction and lifecycle composition. Native retirement205 loses the extracted SpinLock cleanup predicate. Existing
  nullable raw-extraction, represented-child cleanup and owned-transfer checks provide related boundaries to compare.
- Verification visibility. The46 failed Rust tests include14 first-case false accepts,14 false rejects and18 other
  assertions or blockers. These are observations, not46 root causes. Some tests contain large fail-fast matrices.
  Three first failures explicitly report MissingEvidence for bodyless execution promises. Do not generalize that
  explanation to the remaining failures or change expected verdicts without actual controls.

The complete source/import witness matrices now run with unchanged case names/verdicts. The same46 positive cases
fail in each mode. All invalid controls, including returned-address expiry, pass. This narrows the separate stale-address
family to mutation/input-postcondition paths. Deferred provider diagnostics identify held evidence lost through an if
selecting try-await cancel/join results. Their consumer matrices remain unverified. Next trace the nullable join and
pattern extraction against the matching source/import cases before changing production semantics.
No temporary trace hooks or review agent. Style/format remain deferred.

## Remaining

Repair native proof/execution failures by transition class, then execute the complete async fixture. Trace Bound and
PatternObservation, ResultBegin/Production/Completion, explicit/default/operator/construction inputs, attempt/completion,
iterate/next/scoped enter/exit, await/yield, cancellation, outcomes/recovery and cleanup through all consumers.
Verify Source/import certificates and native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal
and nested foreign restoration. Audit from_core admission/refcount ownership. Do not weaken initialized-memory policy to
hide task/result318 copy<Uninit<u8>> failures.
The retained tuple-context-projection source is the partial-carrier control.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its negative opaque call has no owner input. Add a mutable-authority control before changing expectations. Do not
implement the disproven normal_effects hypothesis. Keep a fresh review agent for the committed PR only at semantic completion.

## Execution

One CPU-heavy operation at a time, default Cargo parallelism, no clean. Share original target and target/cargo build dir.
BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX use target/toolchains/llvm/active. Tooling uses default stable. Compiler/linker and
produced-executable workflows run outside sandbox. Cargo uses the default24 logical processors and16 tooling codegen units.
Native uses Rust1.98.1, LLVM bin on PATH, TMP/TEMP worktree target/audit.
Native: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
After a build, capture executable identities in verification.json and remove the consumed build log. Cargo feature selection changes test hashes.
The compilation test binary accepts the7 comparison filters together. Assert all907 identities/outcomes match.

Keep comparison.txt as the fixed907 identities/outcomes. verification.json holds current results and executable paths.
native.log holds the current unresolved diagnostics. tuple-context-projection.bray is the one
unresolved source probe. Consume successful logs and old baselines immediately. Build logs exist only while active.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.

