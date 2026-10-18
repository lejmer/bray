# BRA-505 ordinary finalization and retained incidents

## Resume after every context compaction

Re-read AGENTS.md, its applicable coding and crate rules, this section and the active
agreement before resuming after every context compaction or application restart.
The user explicitly requires this repeated reading. These requirements were reaffirmed
2026-10-18. A summary of the rules is not a substitute for reading them.

- The project is moving the runtime entirely to Bray. Runtime ownership, activation,
  waiting, retirement and lifecycle policy belong in Bray. Do not add or extend a Rust
  runtime frame to complete BRA-505. Existing Rust runtime machinery is a migration
  target, not a foundation for another adapter. Rust compiler code is a separate concern.
  All project-owned linked support belongs in Bray, except the pinned temporal provider.
  Native system ABIs do not justify permanent project-owned Rust or C runtime shims.
- Types and invariants must hold across the entire pipeline. Do not compensate for a
  lost checked contract with consumer-specific adapters, repeated inference or validation.
  Preserve the established result at its owning boundary and remove competing meanings.
- Correctness is necessary but insufficient. Assess compiler time, generated execution,
  binary size and maintenance independently. Do not rely on LLVM to remove avoidable work.
  A helper extraction or file move is not simplification if competing mechanisms remain.
- The user has repeatedly rejected growth through local repairs followed by another
  architectural document. Before further implementation, identify the mechanism replaced,
  its existing consumers and the code to remove. Unexpected layers or scope growth require
  reconsidering that design, not adding another exception. No new language features.

Current correction: replace the competing compiler contracts identified below without
adding runtime ownership machinery. The earlier proposed extra Rust owner-waiter slot was withdrawn. It is
not an accepted solution. Do not revive it after compaction.

Confirmed implementation conflicts, not new language requirements:

- The pushed code retains lifecycle completion type and execution separately. The current
  selected-invocation checkpoint replaces those fields with the existing BoundCallResult and uses its
  selection constructor for ordinary calls too. Source lowering, outgoing admission and
  static signatures consume it. Generated push_lifecycle_call still forces Immediate.
  Removing that override requires completing the shared continuation path, not merely
  changing the field type. Do not claim the representation batch completes this repair.
- The execution-kind checkpoint removes `MirUnitKind::GeneratedLifecycle`. Generated
  provenance remains in `MirSourceOrigin` and `MirUnitKey`, while bodies use the ordinary
  execution categories. Generated bodies still select synchronous execution until shared
  resumable cleanup is connected. This removal alone does not complete that connection.
- Native `TaskObservation` owns a separate Rust frame, cleanup sequence and failure
  collection. Generated task cleanup uses blocking `resolve_task`. Native `observe`
  consumes the outcome, so it is not a non-consuming readiness check. Reusing these
  interfaces unchanged cannot preserve all quiescence and retained-owner invariants.
- Existing Bray `activation.run` already owns composed execution and retirement.
  Complete the compiler/Bray connection and remove superseded policy. Do not introduce
  a replacement Rust observation frame or another cleanup scheduler.

The governing designs already state this boundary: `docs/design/composed-async-execution.md`
and `docs/design/cleanup-storage-and-reports.md`. The deficiency is implementation
alignment, not permission to invent another runtime architecture.

## Active agreement

Complete BRA-505, then BRA-619, with one independently reviewed stacked PR per issue.
BRA-561 is implemented and reviewed in PR #611 at `7a254388`.
Work only in `C:/Users/anton/.codex/worktrees/bra-553-host-bootstrap/bray`, on
`feature/bra-505-admitted-ordinary-finalization`, draft PR #612, based on
`feature/bra-561-compiler-described-activation`. No merge is authorized.

The old BRA-505 checkpoint `b3022d87` was transplanted as `eb91736c`.
BRA-619 admission and rollback remain at `6bbcb4a3` on
`feature/bra-619-complete-selected-scope-native-cleanup`. Preserve those branches.
Do not replay older stack work or modify the user's main checkout.

No new language feature, syntax, compatibility layer, ABI bump, second cleanup
walker, post-failure allocation or downstream authority proof. Reuse selected
lifecycle expansion, checked cleanup transitions, existing outcomes, BRA-561
same-run activation and the existing report chain. Structural traversal shares
retained enclosing progress where possible. An organizational helper does not
justify another reservation. Reconsider accumulating special cases or hidden
blocking execution. Measure production and test growth separately.

Delete this temporary record after BRA-505 acceptance and independent PR review.
Carry only unfinished BRA-619 inputs into its record. No handoff survives the stack.
Completed implementation detail and evidence belong in Git and PR history.

## Current transport replacement boundary

Checkpoint `6306d9f33` is pushed to PR #612. Its evidence is posted at
https://github.com/lejmer/bray/pull/612#issuecomment-6246187339.
BRA-505 remains incomplete. The PR is draft. Independent review 5560583230 found no
checkpoint findings before the active human architectural review. Follow the workflow's
human-review rule instead of launching repeated agent reviews.

Replace helper-local failure collection with the enclosing cleanup collection. Retain the
existing native outcome states and report representation. No Rust owner, TLS sink, source
feature, extra structural activation or outcome journal is needed. The concrete approach
under examination is borrowed state/report places for generated cleanup helpers, using
ordinary MIR parameter storage and explicit cleanup operands. Keep source-call failure
storage distinct from that collection so a nested source catch cannot consume its caller's
already-retained incidents. Native callbacks already own initialized outcome destinations.

The collection invariant is that completed state has no report obligation. Cancellation and
panic states own a report, possibly an empty incident chain for cancellation. Initialize a
report on an abnormal transition, not eagerly at every successful cleanup entry. A finalizer
Error can become a retained incident only during abnormal cleanup. Normal fallible scoped
exit returns its Result explicitly. Check the existing normal-exit legality coverage for
represented and inactive captures before relying on this invariant globally.

Cancellation transport preserves reports through ordinary Bray outcome destinations.
Fixed C native platform-service boundaries consume the report and terminate when no such
destination exists. Use shared routing for panic and cancellation. Completed native callback
reports are a separate already-existing boundary and must be retained regardless of the
callback control tag. Do not extend ordinary source completion with an incident channel
without a demonstrated source-language case requiring it.

Remaining proof before changing generated helper execution: resumable structural expansion
must share the enclosing continuation rather than acquire a frame per organizational helper.
The existing scalar-only MIR splice is not a cleanup splice. Do not broaden it by relaxing its
safety checks. Remove the forced Immediate invocation only when the actual continuation is
connected. Do not repeatedly reconsider TLS, a Rust frame, universal extra hidden arguments,
zeroing full reports on successful paths, or independent nested helper report grouping.

## Shared collection checkpoint

The checkpoint replaces helper-local collections with the enclosing collection.
Source cleanup, generated lifecycle actions, raw-buffer/generator traversal and native
cleanup callbacks must name the same collection owner. Source calls retain separate failure
storage so their catches cannot observe the caller's earlier incidents. An organizational
helper must borrow collection storage and return after updating it. Only the owning boundary
dispatches the final outcome. Remove repeated shields and helper-local suppression when that
connection is complete.

The ordering counterexample is an outer panic A, then typed incident E, then cleanup panic C.
Merging a helper-local headless E/C report into A can produce A/C/E. Appending into the enclosing
collection preserves A/E/C. No timestamps, event journal or runtime frame is needed.

Native callback outcomes can complete while owning a headless incident chain. Their collector
must distinguish that ownership from an uninitialized successful source collection. Reusing
only panic/cancellation booleans as the report-initialization test is insufficient for that
consumer. A single internal state with normal, incidents, cancelled and panicked alternatives
can replace the two booleans, without new source syntax or a runtime type. Normal source
cleanup still initializes only the state. The report is initialized on the first incident or
abnormal transition. Shared helpers need the state and report places, with one separate
incoming-call report. Resolve this representation together with its actual consumers before
publishing another partial collection implementation.

Concrete consumer inventory: source cleanup lifecycle/outcomes/frame, synthetic lifecycle
body/calls/aggregate/representation/storage/task, heap-construction failure, LLVM raw-buffer
cleanup, static cleanup callbacks and retained-incident destruction. StaticFinalize remains
an ordinary selected invocation returning its result, not a structural collector. The native
boundary must keep its borrowed collection disjoint from the source-call failure destination.
The current scalar MIR splicer remains ineligible for cleanup and must not be relaxed.

Checkpoint 6306d9f33 uses `MirCleanupCollection` state/report operands on
structural cleanup, raw-buffer release and generator traversal. Structural helpers borrow
those two slots, have no hidden source-failure outcome, and neither enter a shield nor
publish the enclosing outcome. Native callbacks use their initialized destination as the
owning collection. Source calls keep separate incoming failure storage. This scratch slot
is now created only on demand and shared by the builder views, so structural-only helpers
allocate no report temporary even before optimization. The shared cell is private to one
sequential MIR construction, never part of published compiler data or generated runtime.

Typed incident transfer now moves the admitted payload into its concrete backing and passes
the owned enclosing report by value to Bray `report.incident.retain`. The returned report
replaces that same collection. This removes the raw destination-slot witness loss and its
read-and-zero operation. Existing callback authority requirements remain. The exact production
report declarations and 49 realization checks pass in `bra-505-shared-collection-tests.log`.
The completed library batch passes 93 LLVM, 44 MIR, 39 lowering, 191 package-interface and
21 runtime-ABI tests. The follow-up emission batch exposed one stale generated-helper
signature assertion, now corrected to the explicit borrowed collection parameters. Four
focused production/authority and scratch-storage regressions pass. Logs are
`bra-505-shared-collection-remaining.log`, `bra-505-shared-collection-emission.log` and
`bra-505-collection-regressions.log` in the shared target directory.

The first native smoke attempt stopped during standard-library code generation. Static
initializer signatures omitted the synchronous Bray outcome destination that their MIR
requires. Their owning signature producer now uses the existing invocation-signature rule,
as runtime-default bodies also do. All 27 static and adjacent lifecycle emission checks pass
in `bra-505-collection-boundary-verified.log`. Nine lifecycle emission tests pass after moving
shared continuation routing into the existing continuation module, keeping CleanupOutcome
below the production-module size threshold. See `bra-505-collection-lifecycle-final.log`.
The relocation changes no runtime behavior or representation.

The native ordinary-finalization fixture passes in development and release against the actual
standard library and trusted runtime. It covers asynchronous replacement, later-input failure,
discarded and unused Future captures, and a retained 2 KiB-aligned typed error whose finalizer
must be skipped. The catch result ends ownership in handle_failure before main checks disposal.
A discard binding retains its unnamed owner until scope exit, so an earlier assertion in the
same scope was incorrect. Native debugger traces confirmed owner destruction, incident transfer,
report disposal and the intact payload. Logs: bra-505-ordinary-{development,release}-{build,run}.log.
This does not cover a retained error containing child work or generated asynchronous traversal.

Fixed C platform-service boundaries now consume and report retained incidents, then terminate
on uncaught panic or cancellation. This is the user's authorized internally consistent policy.
Ordinary Bray calls forward their existing outcome. Ordinary foreign callback trampolines retain
their separate declared contract. The obsolete report-less cancellation export and demand are
removed. Both platform-boundary regressions and all 11 native-entry checks pass in
`bra-505-terminal-entries.log`. All 28 runtime-interface tests pass after updating the closed
role inventory. No new language feature, runtime frame or ABI version was introduced.

The runtime ABI now distinguishes owned PanicReport values from PanicReportPointer slots.
CleanupIncidentTransfer consumes a value. Slot consumers retain their existing contract.
The source signature projection, native adapter audit and C fixtures agree with that distinction.
The all-target LLVM matrix, production report declaration test, five ABI classification tests,
six adapter contract tests and 11 native-entry tests pass. Impossible classifier states now
panic with context rather than returning a recoverable compiler-bug diagnostic.

Native execution exposed a Win64 caller defect: indirect report argument storage had 8-byte
alignment, but the callee legitimately used a 16-byte aligned load. The debugger located the
fault in bray_runtime_cleanup_incident_transfer. The private ABI emitter now aligns indirect
value arguments to 16 bytes and shares aligned temporary allocation with ordinary source calls.
The existing all-role/all-target matrix checks the allocation contract. The native fixture
passes after that repair. No extra runtime allocation or ownership mechanism was added.
Logs: bra-505-win64-abi-tests.log, bra-505-native-crash.log, bra-505-native-reports.log.

The runtime smoke workflow built all archives and passed bootstrap denied-allocation/lifetime
checks and activation depths through 2048 with constant measured native stack usage. Ordinary
finalization passed in both profiles. The next task case exposed an old generated callback caller
that omitted the shared collection arguments. The callback now uses the same begin/publish
operations as static and retained-incident callbacks. Its existing emission regression passes,
including the three-argument invocation and balanced shield. Inspecting compiler block structure
uses unoptimized IR because LLVM may merge those blocks. The full rerun passes all cases, including ordinary finalization and started tasks in development
and release. The native phase took 117.7 seconds. Logs: bra-505-task-callback.log and
bra-505-boundary-checkpoint-native.log. This establishes existing task callback compatibility,
not the later BRA-560 task migration.

The shared propagation check exposed an unchecked string-deallocation call in generated
cleanup. Its owning lifecycle lowering now emits the existing checked outcome transition.
LLVM string release establishes the destination before its ownership/refcount branches,
so even the no-deallocation path reaches the same initialized outcome. A dedicated MIR
regression and all nine lifecycle emission cases pass. The 93 LLVM unit tests pass.
Logs are `bra-505-string-collection.log`, `bra-505-propagation-lifecycle.log` and
`bra-505-propagation-llvm.log`. Both isolated native-provider regressions now pass. The boundary and report-ABI batch passed rustfmt, mutating cargo xtask style,
cargo xtask format and git diff --check. The task callback correction also passes the same
formatting commands in bra-505-boundary-checkpoint-format.log. Its audit traces all generated
native lifecycle callback callers and verifies that runtime callers initialize their outcomes.
The boundary/ABI checkpoint removes about 37 production lines net and adds about 140 test lines.
It introduces no runtime owner, scheduler, allocation, compatibility version or new source feature.

Checkpoint maintainability audit: collection construction, source failure collection,
structural forwarding, native callback publication, typed payload transfer, parameter
signatures, outgoing demands, MIR operand/storage/type walks, remapping, package codecs
and inspection were traced through their existing owning modules. The audit found and
fixed string-release forwarding and an unnecessary visibility expansion. No second
runtime frame, report chain, cleanup scheduler or compatibility version was introduced.
The batch is about 1,270 net production lines and 350 net test/fixture lines, excluding
this record. Most production growth makes shared ownership explicit through the existing
MIR and LLVM paths. Native performance and denied-allocation evidence remain pending,
so passing emission tests does not justify claiming the cost constraints are satisfied.

Backing trace: outgoing activation acquires an admitted record and payload lease. Retirement
releases the temporary payload lease while preserving the owner's credit. Typed incident
transfer claims an already-registered free lease from that allowance. Reservation ensures
credits plus one available lease independently of in-use leases. Owner discharge decrements
credits without releasing transferred leases. The retained payload therefore survives owner
discharge without fresh allocation. Native execution under denial remains required evidence.

Audit scope: static destruction callers initialize `NativeRunOutcome::new(COMPLETED, 0)` in
`bray-runtime/src/product/cleanup.rs` before invoking the generated callback. Retained incident
destroy callbacks initialize both their own outputs. Ordinary source calls can publish only
completed, cancelled or panicked outcomes, so their collector does not interpret runtime
admission failures or pending frame states. Source normal ownership end cannot retain a
possibly fallible implicit finalizer. Headless completed reports arise at native boundaries,
where publication retains report ownership independently of the control state.

The static finalizer error callback still selects full lifecycle cleanup rather than
quiescence plus abandoned destruction. This is the existing host consumer owned by open
[BRA-507](https://linear.app/bray-lang/issue/BRA-507), not proof of BRA-505 ordinary retained
incident acceptance. General generator production remains open
[BRA-484](https://linear.app/bray-lang/issue/BRA-484), explicitly excluded by BRA-506. Its existing
MIR callback contract now forwards the shared collection, but no executable general-generator
implementation is claimed. BRA-505 still must finish ordinary shared continuation, inactive
Future integration and retained-payload native acceptance. Do not count synchronous MIR tests
as proof of those paths.

## Remaining implementation and acceptance

| Obligation | Owner and required work |
| --- | --- |
| Ordinary async cleanup | Shared lifecycle expansion and protected source continuations. Replace generated immediate-call assumptions with admitted same-run completion under the cleanup shield. Retain parent progress, pending outcomes and dependencies. |
| Inactive Future captures | Existing frame descriptor and abandonment entry. Never enter an inactive body. Resolve accepted captures before retaining a terminal representation. Future selection currently rejects quiescence explicitly. |
| Retained error quiescence | Shared selected expansion. Resolve child runs while retaining payload and terminal handles. General generator work remains owned by BRA-484/BRA-506. Buffer prefix selection is covered. |
| Abandoned destruction | Generated destruction and source destructor receiver cleanup must not replay implicit graceful finalizers. Explicit operations keep ordinary checked semantics. |
| Physical incident ownership | Verify the implemented transfer of pre-admitted payload backing, provider and attachment dependencies into the existing report chain. Discharge cannot release transferred storage. Do not use immediate disposal or a separate activation. |
| Run ownership and disposal | Retain ordered typed entries across direct await, suspension and nested independent roots. Destroy and release exactly once, preserving reporting/destruction/release failures and cancellation. |
| Production acceptance | Source/imported contracts, normal/panic/cancel exits, temporary/partial/nested owners, inactive captures, large/aligned errors, denied allocation, exact release and link maps. BRA-561 driver tests are not proof of these consumers. |

Keep `scoped_selected_calls_emit_native_artifacts` required through BRA-505/BRA-619.
Incident transfer is now implemented, but full quiescence and abandoned destruction
acceptance must establish its contract before the PR becomes ready.

## Continuation proof still to implement

A source-owned Future is inactive: await and start consume it. Its native composition path
currently consumes and retires that token, and frame failure cleanup destroys captures.
Therefore calling await_frame with no completion destination cannot implement retained
error quiescence. That would destroy captured owners early and leave a stale token inside E.
See lowering/asynchronous.rs, continuation.rs and activation/run/step.bray and retirement.bray.
This is a concrete ownership mismatch, not a request for new syntax or a Rust frame.

Keep quiescence distinct from destruction using the existing activation and frame ownership.
A quiesced Future must retain its capture storage and descriptor until synchronous abandoned
destruction consumes it. Ordinary await/unused-Future cleanup must still retire exactly once.
Synchronous destructors cannot await or start a Future (asynchronous/check.rs owns the start
restriction), and they cannot let receiver-derived ownership escape. Do not use field privacy
or a fabricated valid token as a substitute for these contracts.

Source cleanup already emits array loops into its enclosing frame and materializes counters
in MIR storage. Concrete generic cleanup expansion must retain that same enclosing progress,
collection and checked lane/guard information before layout. Do not relax the scalar inliner
or allocate an activation for each organizational helper. Recursive or erased ownership must
use its already-admitted child allowance. This continuation work has not been implemented,
and the remaining table above remains the acceptance boundary.

## Resuming efficiently

Earlier detailed evidence and abandoned approaches are preserved in this file at
`6306d9f33` and its ancestors. They are historical records, not active instructions.
The tracked stack plan remains in runtime-stack-linear-execution.md and its semantic-proof,
rule-coverage and coverage-inventory companion records. Preserve BRA-619-remaining-work.md.

No new runtime frame or source feature is authorized. The generated lifecycle call still
forces Immediate twice in synthetic/lifecycle/calls.rs, and generated lifecycle/body.rs
still selects Synchronous. Checker lifecycle selection explicitly rejects Future quiescence
pending admitted capture-cleanup activation. Resolve these through existing shared selection,
continuation and Bray activation. Recursive/erased owners and arrays prevent assuming every
graph can simply be inlined. Do not relax the scalar MIR splicer's eligibility checks.

Terminal analysis distinguishes possible introduction or reactivation of graceful ownership
from scalar mutation. It covers aliases, dependencies and abnormal outcomes, and imported
consumers reuse the established result. Resource operations require complete-carrier authority,
not reconstructed fields. Native resource acquisition ordering remains part of BRA-505.
The broader scalar-resource authority audit belongs to open BRA-508. BRA-625 owns the separate
general compiler-defect diagnostic mechanism. Do not reopen closed issues or duplicate those
subsystems. Current lifecycle invariant failures remain BRA-505 work.

Reuse C:/Users/anton/source/repos/bray/target for CARGO_TARGET_DIR, with its cargo subdirectory
as CARGO_BUILD_BUILD_DIR. BRAY_LLVM_PREFIX and LLVM_SYS_221_PREFIX both point to
C:/Users/anton/source/repos/bray/target/toolchains/llvm/active. Never clean shared artifacts.
Worktree writes and native workflows require escalation outside the sandbox.

Native smoke uses the fresh target/release/xtask.exe runtime-artifact smoke-test with process-local
RUSTUP_TOOLCHAIN=1.98.1-x86_64-pc-windows-msvc. Rust 1.99 emits incompatible LLVM 23 bitcode while
this checkout uses LLVM 22.1.8. Do not change the global toolchain or rebuild the compiler just
to select the provider producer. Native TMP/TEMP use target/native/bra-505. Keep logs and temporary
products inside the repository target. No full native rerun until its known blocker changes.

Check the active task-owned process before launching another build. Use focused test filters,
not the broad unit:: filter. Avoid duplicate release xtask rebuilds. Find files with rg and read bounded
relevant sections without truncation. PowerShell does not expand directory wildcards for rg.
Preserve exact leaf failures instead of repeatedly running expensive workflows.
