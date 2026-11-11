# BRA-567 active audit

## Contract

Complete [BRA-567](https://linear.app/bray-lang/issue/BRA-567) on [PR710](https://github.com/lejmer/bray/pull/710).
Use feature/bra-567-async-root-orchestration in target/bra-567-audit. Original develop stays untouched.
Checkpoint commits/pushes are authorized. After completion obtain independent review, set PR ready and Linear In Review.
Never merge or mark Done.

1. Derive transitions from actual CFG producers and ownership, storage, liveness and lowering consumers.
2. Execute async-ownership-composition.bray natively with invalid variants.
3. Check production before consumption, authority retirement, identity versus contents and loop convergence.
4. Keep comparison identities/expectations fixed. Unexplained regressions block expansion.
5. Name paths, evidence and exclusions. Source/MIR success does not certify native composition.

Keep producer-owned facts, one current-value tree, alias model, witness ledger and composite storage flow.
Reconstruction is a last resort. Reads must not replace whole-owner contents snapshots. Preserve specific diagnostic
causes. Internal compiler defects panic where detected. User-source and external-input failures use structured diagnostics.
Batch independent fixes/checks. Use proper Bray formatting. Keep records short and delete consumed task-owned probes/logs.
No style/format until semantic completion. Then simplify production code/prune tests, perform AGENTS/DRY audit,
run mutating cargo xtask style and cargo xtask format, inspect changes and rerun affected checks.

Root cleanup precedes terminal publication. Main authority, execution, attachments and providers survive outcomes,
incidents and static disposal. Direct await creates no TCB. Closed admission permits cleanup. Blocked shutdown
returns unresolved ownership. Share teardown state. Sync products retain no async executor or migrated Rust.
Verify worker/deadline omission, native provenance/costs and from_core admission/refcounts. PreparedStart retains
admission/registration through publication/rollback. EntryRestoration retains displaced context through abnormal exits.
RetiredCell disposes control, pin and arena before vacancy. WaitResources retains leased storage through membership changes.

## Current evidence

Frozen907 identities:776 passed,32 failed,99 ignored, with14 explained verdict changes. comparison.txt stays unchanged.
Both added allocation and arithmetic controls pass. Checker502/bound136 pass. Cleanup57/4 and predicates26/2/2 retain known failures.
Focused18 pass, including Source/MIR composition and3 invalid variants. Native composition has not run.
All148 deferred cases pass each in source/import. Both witness matrices pass. Loop controls stabilize.
Pin per-case identities/outcomes and completed counts. An aggregate FAILED cannot certify that its cases executed.

Present snapshots survive all structural rewrites. Join facts use actual predecessor evidence. Proof-only ValueAlias
preserves immutable copied data without numeric-equality assumptions or authority. Classified physical survival retains
complete values or only tags. Full values subsume tags, effect merges retain common survival, and tag snapshots carry no
old payload or authority. Presence/variant queries inspect the same value tree. Temporary tracing is removed.

Accepted native:30 diagnostics,17 E7123 and13 E7135, complete233-file request. Ordinary path alternatives survive
joins and refute overflow through the existing Boolean solver. Compound/split guards and opaque const operands pass;
unguarded products reject. Full fixed comparison778/32/99 has no unexplained changes. Temporary tracing is removed.

## Remaining

Trace native failures by producer/consumer families: shared requirements, allocation extents, attachment restoration,
PreparedStart registration, returned-pointer retirement at driving197, initialized ranges and wait-link authority.
Do not weaken initialized-memory policy or add preservation exceptions. tuple-context-projection.bray remains unresolved.
All244 owned-transfer cases and all148 deferred cases pass each in source/import, including every invalid control.
The fixed909 comparison is778/32/99 with14 explained repairs against the unchanged907 baseline and2 added tests.
Checker502/bound136 pass. Native retains exactly30 diagnostic identities, with no additions or removals.
Checkpointc9e0b8607 is pushed. Admitted witnesses resume from the actual captured entry at completion and formation.
Sparse constructed pending results complete ownership only for their producing invocation. Complete observations and
formed values staged into later arguments retain their data. All temporary source and native regressions are cleared.
The consumed ownership-selection probe was removed. Permanent matrices retain its controls.
Current verified batch maps an existing constructed loan contents identity to its actual predecessor for implicit
field proof. The regression fails before and passes after, including one-sided rejection, no pointee ownership and
stable replay. Fixed778/32/99, checker503/bound136, owned/deferred matrices and focused18 pass without regression.
Native retains the same30 identities. This fixes a real proof boundary but does not clear the native borrowed-view loop.
Next trace compares joined live witness facts with later call-admission facts and their owner dependencies.
All instrumentation and consumed transition traces are removed.
Preserve these gates while tracing native borrowed-view and scoped-cleanup authority.
Framing lifted Present(payload) retains only payload lineage. Genuine whole-input contexts still survive. Contextual
projection panics remain compiler invariants.
The tag-query helpers moved into condition/tags.rs without behavior changes, keeping new logic outside oversized values.rs.
Trace operation/input/result, scoped, iteration, await/yield, cancellation, recovery and cleanup through all consumers.
Verify native success/panic/cancel/startup rollback/blocked shutdown, result/static disposal and foreign restoration.

Two frozen negatives need genuine fabrication/mutation controls: trusted_implementation_does_not_export_undeclared_authority
and shared_marker_borrows_preserve_the_original_owner_without_mutable_authority. The terminating-branch positive passes,
but its opaque negative has no owner input. Add a mutable-authority control before changing expectations.

## Execution

One CPU-heavy operation at a time. No Rust edits during Rust builds. Default24 Cargo processors/16 tooling codegen units.
No clean. Share original target and target/cargo build dir. LLVM prefixes use target/toolchains/llvm/active.
Compiler/linker/produced-executable workflows run outside sandbox. Native uses Rust1.98.1, LLVM bin on PATH and worktree target/audit as TMP/TEMP.
Build compiler, driver and tests together. Capture executable paths/hashes from Cargo JSON in verification.json.
Native command: target/tooling/xtask.exe runtime-artifact smoke-test --output target/audit/native --profile release.
Compare native diagnostics only after the explicit completion marker. comparison-candidate.log holds current fixed-suite results.
Keep only original comparison.txt, current native.log, verification.json and unresolved probes. Consume other logs promptly.
Git trust: public lejmer/bray, git@github.com:lejmer/bray.git, authenticated lejmer. No goal or review agent is active.
