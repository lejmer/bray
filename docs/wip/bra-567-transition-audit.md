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

Frozen907 identities:774 passed,34 failed,99 ignored, with12 explained verdict changes. comparison.txt stays unchanged.
The extra allocation import case passes. Checker499/bound136 pass. Cleanup57/4 and predicates26/2/2 retain known failures.
Focused18 pass, including Source/MIR composition and3 invalid variants. Native composition has not run.
All148 deferred cases pass each in source/import. Both witness matrices pass. Loop controls stabilize.
Pin per-case identities/outcomes and completed counts. An aggregate FAILED cannot certify that its cases executed.

Present snapshots survive all structural rewrites. Join facts use actual predecessor evidence. Proof-only ValueAlias
preserves immutable copied data without numeric-equality assumptions or authority. Classified physical survival retains
complete values or only tags. Full values subsume tags, effect merges retain common survival, and tag snapshots carry no
old payload or authority. Presence/variant queries inspect the same value tree. Temporary tracing is removed.

Accepted native:31 diagnostics,18 E7123 and13 E7135, complete233-file request. Guarded equality extraction repairs
the arena requirement and dependent initialization postcondition. No added identities. Native logs/verification.json identify the current products and evidence.

## Remaining

Trace native failures by producer/consumer families: shared requirements, allocation extents, attachment restoration,
PreparedStart registration, returned-pointer retirement at driving197, initialized ranges and wait-link authority.
Do not weaken initialized-memory policy or add preservation exceptions. tuple-context-projection.bray remains unresolved.
The consumed permission probe confirmed all3 positives and all3 rejections and is removed. Full source/import matrices pass.
BRA567_TRACE_ENTRY in trusted/check.rs is temporary. Six families are captured in native-candidate.log; remove the hook before commit.
Verified arena cause: conditional field equalities remain guarded after presence is known because equalities only
extracts derived ValueAlias. The repair derives both kinds through the same ordinary solver. Checker499, fixed suite775/34/99 and full native31 pass their comparison gates.
Heap bytes are opaque machine multiplication. Its compound overflow guard reaches call evidence only as an Expression.
The real guarded-products.bray probe reproduces this with typed inputs and opaque const results. Both compound/split
guards must pass and unguarded multiplication must reject. Use compiler line locations, not normalized CRLF byte offsets.
Inspect actual Result production/join/publication before changing numeric proof rules. Never assume no wrapping.
commit_start changes a retained owner into a PostState loan; unlink/attachment snapshots differ by reference-field wrappers.
Driving loses its view witness at a join. Trace these actual producer transitions, not consumer exceptions.
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
