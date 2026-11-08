# BRA-567 transition audit

## Completion contract and compaction guard

The objective is to implement BRA-567 and complete the agreed design in [the implementation handoff](bra-567-handoff.md).
The compiler repairs and regression reductions below are intermediate evidence. They do not replace that objective.
After compaction, reread this section before choosing more work.

1. Derive the transition inventory from executable CFG producers and the actual ownership, storage-flow, liveness,
   and lowering consumers. For every relevant operation and failure edge, record what exists before it, what it
   produces, what it consumes, and what survives failure. A category such as calls is not a completed inventory.
2. Prove one complete real async composition before expanding the migration. Trace the same owner across temporary
   production, calls, borrows or mutation, branches, loops, suspension, task outcomes, cancellation, and cleanup.
   Pair the valid program with invalid variants whose intended rejection is demonstrated. Native execution is required.
3. Test the architectural properties at their owning boundaries: no consumption before production, no authority left
   usable after owner movement, no overwritten contents preserved merely by retained identity, and stable repeated
   loop analysis. Passing incidental examples does not substitute for these properties.
4. Keep test identities, expected outcomes, and code versions fixed for comparison. An unexplained regression blocks
   migration expansion until explained and repaired. Investigate transition defects, not individual runtime failures.
   Correct a demonstrably invalid new fixture explicitly, with the specification reason and evidence retained.
5. Make every verification claim name its paths, evidence, and exclusions. Unit or Source passes cannot become an
   end-to-end claim. The agreed design remains unverified while required composition paths are blocked or unexecuted.

Continue on PR #710's branch and commit/push coherent checkpoints. Write inline Bray fixtures in Bray formatting.
Defer automated style and formatting passes until the audit and semantic repairs are complete.

The latest pushed source is c73547c02. Fixed comparison gates pass with no unexplained
comparison regression. Native planning has no remaining moved-value diagnostic, but still fails on trusted obligations
and execution guarantees. No native composition has executed. The copied-pointer owning-boundary regression and
all 441 checker tests passed at c73547c02. Native planning still fails. The active gate is the scalar-computation
repair for staged range extents and actual production buffer contracts. The temporary RANGE PROOF trace is removed.
The ledger remains incomplete. Source passes do not certify runtime execution.

### Executable ledger for the current blocking boundaries

These rows trace code paths, not completed native coverage. The failure edges are part of each transition.

| Operation and producer | Before and production | Consumption and failure | Actual consumers and evidence |
| --- | --- | --- | --- |
| Invocation Input, `execution.rs::begin_invocation` | Actual receiver and argument values exist. Borrow inputs are prepared. No result exists. | A failed precondition bypasses Attempt and Completion. Existing input owners remain cleanup obligations. | `AnalysisOperationKind::storage_plans` excludes self-produced accesses. Liveness `collect_edge_lifetime_ends` includes failed-invocation subjects on the bypass edge. Existing admission and cleanup controls still require final native evidence. |
| Invocation Attempt, `execution.rs::push_invocation` | Admitted inputs exist. Input transfers use Admission consumption. | Invocation failure cannot publish a result. Completion is only on the successful continuation edge. | Storage and trusted transfer checks use the shared phase selector. OutputDefinition plans never act as source reads. |
| Invocation Completion | Returned references are staged, then `memory.complete_transition` publishes them. | Result-reaching operations run after publication. Failure does not traverse this operation. | `apply_storage_operation` publishes before applying produced accesses. `TrustedChecks::after_operation` checks those transfers after semantic completion. |
| Custom index returned borrow, `expression/access.rs::custom_index_access` | Receiver and selector borrows are inputs. The indexer produces a borrow descriptor with its own stored borrow type. The reached item is a different value. | An ordinary item copy does not consume the reached owner. A live witness forbids that copy. | The existing InvocationResult binding identifies the descriptor. `invocation_result` stages its reference contract. `reference_target` resolves reached storage for the later copy. All 11 custom-index controls pass at 0e2e943d3. |
| Built-in field or index projection, `project_access` | A produced loan descriptor and its reached storage already exist. Projection extends the same source path. It produces no independent owner. | Retiring the descriptor at an intermediate projection destroys the source of subsequent projections. The repair defers retirement to the actual consumer. | Frozen `returned_borrow_survives_field_and_index_projection` had two baseline moved-value errors. It and all eight adjacent value-lowering tests now pass. |
| Actual consumer of a projected loan | The source path still identifies the producer's materialized borrow slot. The consumer first captures reached data or forms a new borrow. | Completion consumption retires the temporary descriptor, independently of the original referent owner. User parameter and binding storage are excluded by the producer/value-slot identity check. | `consume_expression_value` records the same consumption metadata read by storage flow and liveness `OperationEffects::from_storage_plan`. The frozen lowering regression passes. |

Audit baseline: `b90076c1f21f3853ebfd90954b3babe26f122eca`.
The behavioral contract remains [the implementation handoff](bra-567-handoff.md).
The fixed regressions below were established before semantic repairs. The user has now authorized repairing the
violations and continuing through the blocked composition. Do not change regression expectations. Completion
requires the whole composition, including lowering and native execution. This report does not yet certify it.

## Current implementation and evidence

Worktree: `target/bra-567-audit`, directly on PR #710 branch `feature/bra-567-async-root-orchestration`.
The original develop checkout is untouched. Latest pushed head is **c73547c02**.
The user explicitly authorized continued implementation, checkpoint pushes, and an independent review on the draft PR
once implementation and verification finish. Continue through resolved review findings until ready for human Review.
No review agent has been started. No style or formatting pass has run during this audit.

### Checkpoints

- `235686a17`: compile repair, shared access roles and phases, exact retirement membership, data/loan joins,
  scoped capability and produced-reference corrections, initial regression fixtures.
- `777ffb930`: ordinary scalar operator semantics, trust-boundary output transfer, real async composition extension.
- `0e2e943d3`: custom index returned-borrow publication before reached copies and witness checks.
- `4b39bfe38`: retain projected loan descriptors until their actual consumer captures data or a new loan.
- `c73547c02`: match copied pointer descriptors in closed spatial proofs, with async invalid variants and boundary test.

### Fixed comparisons and their limits

| Evidence | Code state and result | Meaning and exclusions |
| --- | --- | --- |
| `comparative-baseline-checker.log` | Original production with the same audit tests had seven failures | Controlled comparison, not a historical count from a different population. |
| `trust-result-checker.log` | 777ffb930, 440 passed | Checker properties, including exact revocation beyond proof budget, loan join stabilization and retired source ownership. Needs affected final rerun after later repairs. |
| `trust-result-trusted.log` | 777ffb930, 136 passed, 94 failed, 97 ignored | Frozen trusted-contract test identities. |
| `index-result-slot-repair.log` | 0e2e943d3, all 11 custom-index tests passed | Copy lowering, owned-witness rejection, borrowing conflicts, imported effects and const-contract control. |
| `index-result-trusted.log` | 0e2e943d3, 138 passed, same 94 failures, 97 ignored | Two added index tests passed. No changed failed identities. |
| `projected-loan-repair.log` | 4b39bfe38, all nine value-lowering tests passed | Includes the frozen returned field/index borrow regression, returned temporary and trust-boundary result. |
| `projected-loan-trusted.log` | 4b39bfe38, 139 passed, 93 failed, 97 ignored | Only transparent_predicate_getters_preserve_the_contained_owner changed from failure to pass. |
| `projected-loan-storage.log` | 41 completed passes, three failures, one uncompleted probe | Stopped only the task-owned operation_cleanup_native_fixture_lowers_its_checked_inputs process after about two minutes. It remains unverified. |
| `projected-loan-controlled-baseline.log` | Exact preceding production 0e2e943d3 with the same three failing tests | All three also fail before the projection repair. Sources and repaired executable restored in finally. |
| `projected-loan-adjacent-failures.log` | Repaired executable, same three failures | native callback report assertion, operator report borrowing, conversion reference-path panic. Pre-existing transition gaps still require classification where needed by the issue. |
| `projected-loan-native.log` | 4b39bfe38, standard-library native planning failed in 22.9 seconds | No moved-value diagnostics remain. 233 E7123, 123 E7119, 48 E7113, 24 E7087, 9 E7036, 4 E7125, 3 E7063, 2 E7127. These diagnostics are not source-body test counts. No native fixture ran. |
| `pointer-copy-boundary-repair.log` | Pointer repair, one checker boundary and eight source controls passed | Both formerly failing address cases, async positive and negative variants, witness movement/copy rejection and scalar-copy authority. |
| `pointer-copy-checker.log` | Pointer repair, 441 passed | All current checker properties pass. |
| `pointer-copy-trusted.log` | Pointer repair, 159 passed, 74 failed, 97 ignored | Exactly 19 old failed identities removed, no added failures. Includes address expiry, range bounds and invalidation controls. |
| `pointer-copy-native.log` | c73547c02, native standard-library planning still fails | 197 E7123, down from 233. Other diagnostic category totals unchanged. No fixture executed. First leaf remains byte_buffer_fill in std.memory.fill_spare_bytes. |

Earlier `live-join-trusted.log` had 123 passes and 103 failures. The nine removed failure identities at 777ffb930
are recorded by the comparison logs. Do not equate these test populations with runtime Source-body populations.
The older `live-join-import-lowering.log` completed 332 passes, 104 failures and two ignored before a generator analysis
was stopped after over four CPU minutes. `import-lowering-failed.txt` freezes the 104 completed failed identities.
Later checks blocked by that probe were unverified. Avoid an unchanged broad rerun.

### Current gate and owned process

The pointer trace confirms that a spatial requirement names a private input's PostState while available spatial facts
name the copied address's original field. The actual input is Scalar with that original field as contents. The boundary
test fails before the repair with Argument(1) versus Argument(0), while generic predicate subjects must remain distinct
and a scalar copy must retain no owning authority. `pointer-copy-boundary-baseline.log` freezes that failure.
The repair unwraps copied scalar descriptor contents only inside the existing closed spatial predicate normalization.
Do not broaden arbitrary trusted predicates or create ownership from pointer equality.

The pointer regression, selected source controls, all checker tests and fixed trusted comparison have completed.
Native planning was rerun and remains blocked. The next owning-boundary test freezes copied scalar range extents.
A production test lowers the actual fill_spare_bytes, copy_spare_bytes and copy_bytes_from_pointer bodies rather than
reimplementing their behavior in a fixture. A failure in the first body leaves later bodies unverified.
`range-staging-boundary-baseline.log` freezes failures in both the new extent-normalization unit test and actual
fill_spare_bytes. Later bodies were unverified after that first failure.

The production trace shows a Scalar staged count whose contents are the original count input. Available valid_write
evidence contains Subtract(Borrowed(capacity), Borrowed(initialized)), while the ordinary branch bound contains
Subtract(capacity, initialized). Built-in arithmetic was computing on loan identities instead of the reached values.
The repair shares computed_value normalization across built-in operations, const calls and spatial extent values.
It preserves the separate pointer descriptor normalization and generic predicate subject path. Superseded special
nullable-reference comparison normalization has been removed because the common computation read covers it.
An added unit property compares arithmetic and const calls over copied and borrowed views of the same scalar.

`range-staging-boundary-repair.log` passes both new checker properties, scalar-copy safety and both async composition
tests. The known Source/imported scoped continuation controls still fail. The buffer test passes fill_spare_bytes,
then fails copy_spare_bytes at copy_spare_raw_bytes. copy_bytes_from_pointer remains unverified by that fail-fast test.
`range-staging-checker.log` passes all 443 tests. `range-staging-trusted.log` has 160 passes, 74 failures, 97 ignored.
Against pointer-copy-trusted.log, conditional_memory_obligations_accept_smaller_proven_ranges is repaired, and the
newly added production buffer regression is the only added failed identity. No existing test regressed.
The unused superseded current_contents method was removed after the build. Its removal changes no behavior.
Next trace the newly reached production copy call before changing it. Keep the fixed comparison gate.

The temporary RANGE PROOF trace in `analysis/guarantee/trusted/proof.rs` was restored from pointer-proof-before-trace.rs.
No temporary trace remains in source.
The earlier pointer trace was removed before c73547c02. `pointer-copy-source-trace.log` retains that earlier evidence.
Both selected address tests failed before the repair. Both async composition tests passed, including invalid variants.

The new async invalid variants replace only one source fragment each. One fabricates ChildCleanup instead of calling
acquire and must fail trusted obligations in produce. The other observes owner after child(owner).start() and must
report moved storage in main. All callable and lifecycle bodies of the valid fixture are checked and lowered
by the fixed positive test. Both invalid variants are verified before the pointer repair.

### Remaining completion work

- Finish the executable ledger across remaining operation/failure transitions, including constructors/defaults,
  branches/loops, suspension and task outcomes, scoped and root cleanup, retained incidents, shutdown and imported facts.
- Preserve the fixed comparisons while resolving the next actual transition that blocks native planning.
- Execute async-ownership-composition.bray. Its assertions cover both transport return branches, loop suspension,
  genuine root Main authority, returned inactive Future disposal without executing its body, scoped child cancellation,
  exactly-once finalization/destruction, and lexical cleanup before static cleanup.
- Execute adjacent worker continuation, retained-error, panic/cancellation, partial startup, shutdown/disposal and
  imported/native acceptance paths. Synchronous ownership-transitions.bray is a focused control, not async completion.
- Complete the agreed runtime ownership protocols from bra-567-handoff.md, rather than assuming current runtime code
  satisfies them. Required Source-body/certificate runs must converge to the supported expected outcomes.
- Perform mechanical maintainability/DRY and AGENTS compliance review across the full active diff. Keep owned data,
  copied data, loan descriptors, physical references and lifetime dependencies distinct. Remove superseded mechanisms.
- Only at finalization run mandatory cargo xtask style and cargo xtask format, inspect their edits, and run required
  affected checks. Then push, dispatch the independent draft-PR review, resolve findings and move to human Review.

### Build and retained product rules

Use shared Cargo storage and LLVM. CARGO_TARGET_DIR points at the original repository target directory.
CARGO_BUILD_BUILD_DIR points at target/cargo. BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX point at
`target/toolchains/llvm/active`. Profile tooling uses optimized incremental code generation without a Cargo job cap.
The machine has 24 logical processors. Compiler frontend phases can still serialize.

Default Rust 1.99 produces LLVM 23 archives, incompatible with the Bray LLVM 22 toolchain. Build xtask with default
stable, then run its native command with RUSTUP_TOOLCHAIN=1.98.1 and LLVM bin on PATH. This compatibility selection
is for the native archive producer. Native LLVM, archiver, linker and produced processes run outside the sandbox.
Use ignored task-owned target/audit for retained logs and temporary files. Do not clean shared storage.

The compilation test executable is under shared target/cargo/tooling/deps. Preserved repaired and index baseline
copies are in target/audit. They need the original dependency directory on PATH to load their neighboring DLL.
All task-owned test/build processes before the current pointer-boundary run have ended.
