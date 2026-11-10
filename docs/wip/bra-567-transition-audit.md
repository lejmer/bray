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

## Current evidence and blockers

The current semantic checkpoint retains live predecessor snapshots, projected aliases, nullable wrapper/payload
separation and composed overlays. Earlier producer-owned result and allocation/reference transfers remain fixed.
No reconstruction or parallel persistent state was added. The admitted-entry and diagnostic checkpoint is verified. The return-carrier simplification remains uncompiled. No review agent or goal is active.

Frozen907 identities:766 passed,42 failed,99 ignored, four explained repairs and no new failures. One extra allocation
import case passes. Checker492 pass, including the direct collector and live-Unknown controls. Bound135, symbols221 and interfaces216 pass.
Cleanup57/4, predicates26/2/2 and focused18 are unchanged. Source/MIR composition and3 invalid variants pass.
Native composition has not run. verification.json holds executable identities and current evidence.

Accepted native baseline remains37 diagnostics,22 E7123 and15 E7135, Source64a4e5490, complete233-file request.
The admitted-entry repair removes the added worker/entry170 failure. All37 original identities remain unchanged.
The unexplained-regression gate is clear.
Both added call failures at activation/run/entry95 and worker/entry179 are repaired. Their actual conflicting snapshots
came from retired call-input slots marked Absent/Absent. The collector now excludes absent storage with Option snapshots,
while live Unknown still blocks proof. The direct collector test reproduces before repair and passes after it.

Pending batch also preserves same-region predecessor snapshots, derives projected-alias contents from the containing
owner's phi, composes nested Constructed overlays at construction, and retains Unknown backing for anonymous overlays.
Six nullable-union predecessor orders pass, including same/distinct regions and fabrication controls. The alias matrix
covers Owned/Inline/Loan/Reference and retired snapshots. Reference descriptors gain no pointee ownership.
All7 nested boundary tests and32 condition tests pass. A Present wrapper now records its value snapshot separately
from its payload owner. Previously both snapshots conflicted at the payload identity. The48-case matrix covers all
orders, both nesting directions, same/distinct regions and fabrication controls. Only an explicitly Present payload
collapses its identical overlay. Never erase component structure merely because a projected identity matches. No temporary snapshot hooks
remain. The added worker postcondition depends on Pure for scheduler_reference(pool) at entry179. The captured entry has
trusted_boundary=true but no retained(pool.marker) fact. Completion publishes admitted ordinary conditions and omits
accepted trusted conditions. The verified repair records those conditions only in the completed immutable entry and
publishes that snapshot. Pre-admission and false ordinary controls pass, as do492 checker tests and the fixed907 comparison. No live authority is
created. Rejected selection now reports MissingEvidence at the invocation, with a passing guard cause/span regression.
All temporary hooks are removed. Inferred return witnesses still reconstruct a second value/identity map.
The pending simplification reuses returned_condition, already used by ownership metadata, deleting that parallel map. Cargo builds compiler, driver and tests together. Always capture executable identities
from Cargo JSON because feature unification changed their hashes. verification.json owns the current paths.

Source/import witness matrices have the same2 positive failures each, down from46, with every negative control passing:
optional_claim and new_owned_tuple_inferred. All Result<Nullable> permutations and provider variants now pass.
Deferred provider held evidence still fails after if selecting try-await cancel/join. Consumers remain unverified.

Other unresolved families: driving197 loses Current's view at wait_execution Attempt through returned-pointer raw
retirement. Whether its effect or bound is wrong remains unproved. Address expiry fails specifically through mutation
and input postconditions, while returned-address negatives pass. prepared_start175's registration postcondition and
retirement205's extracted SpinLock cleanup remain unproved. Do not add preservation exceptions or weaken initialization.
Three Rust first failures cite MissingEvidence for bodyless execution promises. That does not explain the other failures.

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

