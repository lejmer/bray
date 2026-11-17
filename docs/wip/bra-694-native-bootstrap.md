# Compiler boundary refactor

## Required outcome and ordering

Start from05d2dc1b6 on feature/bra-694-native-bootstrap-contracts. No initial baseline tests.
Keep CFG, composite flow, CurrentValues, proof, SCC and demand-driven computation. Replace independent interpretations
with captured values, typed storage transitions and connected guarded outcomes. Function summaries abstract those same
transitions. Possible regions never grant value equality, layout, allocation lifetime, initialization or authority.
Unknown never becomes empty. Compiler value identity never implies one unique runtime allocation.

The user explicitly requires completing every agreed refactor boundary before repairing dispatch_ready or start_frame.
Correct regressions introduced by a slice, then continue even while those native failures remain. Do not retry or repair
them now. Native acceptance follows the completed refactor. Keep PR717 draft and BRA-694 In Progress.

Remaining work includes the complete portable captured-value graph, opaque generic transport, one phase-aware
instantiator, actual effect addresses separate from footprints, typed read/follow/backing routes, connected guarded
result bindings, unified observation survival and dependency-index invalidation. Remove replaced mechanisms completely
within each migrated path. Do not infer permissions from alias bounds, pointer equality, nominal exemptions or the
owned_allocation predicate. Lowering consumes checked storage decisions and does not repeat ownership reasoning.

Final joint gate is actual Linux/Windows bootstrap checking AND lowering on x86_64/AArch64 plus the fixed comparison
on the same candidate. Complete the agreed graph first, then address any remaining native source/checker failures.
BRA-695 execution and BRA-696 shutdown remain separate. No new issues or subagents have been created.

## Pushed checkpoints and current evidence

The route checkpoint8be430cea is pushed. The validated observation-classification checkpoint follows it.
Earlier checkpoint551d12bb1 replaces References/StoredValue with AddressValue/ValueBinding,
separates actual addresses from carried bounds, preserves nullable empty alternatives and distinct NoPayload, removes
staged clone/merge/restore reconciliation and transports inactive slots through source/import/lifecycle/deferred code.
Package interface revision2 and specialization hashing distinguish inactive, null and independent slots.
Production growth is about300 lines plus tests. This is the internal binding boundary, not the full portable graph.

Checkpoint551d12bb1 passes564 checker,222 symbols,217 interface and57 returned-dependency tests plus the new slot-shape
identity control. All1714 frozen comparison identities pass in303.88s with zero failures and zero missing identities.
Mutating cargo xtask style and cargo xtask format pass. Source/build/binary fingerprints accompany the comparison.

Checkpoint b27260893 separates private production from completion. Transfer::take_outputs moves output fields only.
State::produce_transition installs private current values while retaining entry captures and abnormal/deferred metadata.
Normal complete_transition produces anything still staged, then ends the capture. Cleanup uses produce_transition.
Repeated production and normal completion cannot replay pre-cleanup values. This adds20 production lines and29 test lines.
It passes564 checker and362 deferred, returned-value and cleanup compositions in9.88s, including source/import and MIR
lowering. Mutating cargo xtask style passes. The maintained Bray sources are unchanged since the explicit format pass.
The1714 comparison has not been rerun after this boundary. No native retry or native repair has occurred.

Checkpoint d087dde8f already replaces independently supplied normal/deferred-storage booleans with immutable
DependencyEffectOutcomes. Each effect binds once with shared possible/normal identities. Admission captures possible
failure effects against incoming memory. Normal paths apply normal effects. Deferred formation keeps body effects latent,
and inactive cancellation cleans only captures. Ordinary failure can retain incoming support justified by effects.
Observing an already-running task has no fresh body-entry snapshot and cannot use normal-result transport on failure.
The old normal_storage_preserved/deferred_storage_preserved/has_normal_effects promises are removed throughout the path.
Its Windows native18 failed only dispatch_ready and start_frame. That is old-code evidence, not a current native run.

## Active whole-value contract gate

Whole-value transport must retain actual data, dependencies, ownership and duties even when T is opaque. A captured
value must be the same value CurrentValues publishes. Physical and logical return preparation must stop independently
reconstructing fields from the substituted type. Bounds remain bounds. Completeness alone never proves value equality.
A move removes source authority and relocates value support while source-storage support does not follow it.

The active experiment extends raw_buffer_reachability_preserves_reference_bearing_payload_dependencies in
compilation/export/build/tests/standard_library.rs. A witnessed Owner goes through generic store<T>, a buffer move,
restore<T>/pop and normal cleanup, then inspect. Source and imported callers must retain the witness and lower.
The fabricated-owner counterpart must remain unproved. The existing reference-bearing escape cases stay unchanged.
This is a shared composition oracle, not a new fixture for a helper. Test code and this record are uncommitted.

Probe b27260893 plus that test builds in2m48s and fails in8.47s only at inspect for the witnessed source and imported
callers. Provider bodies and both fabricated-owner controls pass. Positive lowering is unverified because checking
rejects the callers. capture-probe-source.json records the exact source fingerprint. No production tracing is present.

The source summary evidence is decisive:
- store has no owned_result transfer. Its complete stored dependency at Result.OwnedTarget.AnyElement names Parameter0.
  Its output pointer has an allocation anchor. The root backing dependency is explicitly empty and complete.
- restore has no owned_result transfer. Its complete result dependency names Parameter0.OwnedTarget.AnyElement.
- round_trip has empty ownership, stored dependencies and borrows. Its concrete Owner has no pointer fields.

The rule cannot be repaired by treating complete stored dependencies as ownership or by adding a RawBuffer exemption.
The documented raw_buffer_push/pop operations transfer one actual T and its initialized-prefix duty. Their graph must
carry that actual value separately from possible generic dependency bounds. The witness must survive because the same
owner moves through storage, never because a predicate is reasserted after invalidation.

Authoritative producers and consumers traced so far:
- initialize_entry_references seeds demanded opaque entry paths and buffer AnyElement lineage. seed_whole_source currently
  describes possible dependencies. Do not reinterpret all those sources as exact copies.
- memory.stage_buffer_push/stage_buffer_elements capture full physical binding subtrees. stage_whole_sources demotes them
  to dependency bounds for portable summaries. stage_buffer_payload and stored_transfers skip reference-free concrete T,
  which cannot express a witness-bearing owner with no pointer fields.
- prepare_returned_values/stage_preserved_references enumerate BoundReturnedValue.observations. Logical returned-value
  reconstruction in guarantee/trusted/borrows uses the same type inventory through ValuePreservation::returned. Remove
  both reconstruction paths within the migrated scope, not just one consumer.
- dependency/call/references::returned_values constructs that inventory using the substituted source type.
- Physical captures are Transfer.inputs. Logical data, ownership, proof and duties are ExecutionCallEvidence.arguments,
  owned_inputs, extracted_values and witness_inputs. CurrentValues is authoritative for current contents.
- CFG begin_invocation has Input, then a possible precondition-failure edge, then Attempt. All argument evaluation and
  conversions precede Input. Logical call_entry captures at Input. Physical preparation/capture occurs before admission
  consumption at Attempt. Requirements must see incoming evidence before the operation changes it.
- Memory effects stage buffer output before physical invalidation. Logical guarantee flow has no RawBufferPush/Pop
  value transfer. owned_values/witness_values traverse actual CurrentValues components and deferred captures only.
- retire_consumed_inputs and expire_trusted_scope retain evidence only through those actual value contexts. A full
  captured graph must connect stored T to its real owning value without retaining every possible alias as an owner.
- StorageFlowState.merge joins logical values once with the existing program-point identity, then physical bindings.
  New graph identities must remain stable and finite through this join and recursive summary normalization.
- Deferred formation captures by-value data and pointer descriptors. Execution-time pointee reads use current memory,
  not formation-time pointee data. Cancellation before execution cleans captures only. Partial execution handles acquired
  duties. An observer's normal RunResult.Failed is different from an ordinary call's abnormal outcome.
- Cleanup instantiates against its current receiver and ordered scheduler. Private produced output is current while
  captures remain retained. Normal completion uses its surviving value. Panic/cancellation cannot install normal output.
- Checked RawBufferAllocate in memory/operations creates the physical allocation through memory.fresh and retains its
  descriptor. Push/pop have physical staging but no logical value route. Allocation identity must originate from checked
  allocation/entry transitions, never allocation_owners, which indexes witness conditions by their actual carrier.
- ExecutionCondition::with_storage_component and project already support explicit Constructed storage components.
  owned_components visits those actual children. ConstantProjection and ExecutionPlace still describe inline values.
  Do not extend ordinary constant projections to pretend a pointer region is owning backing. A generic initialized
  prefix needs actual value transfers, not promotion of its AnyElement may-dependencies into equality or ownership.

The complete value graph contract remains open. The typed storage route prerequisite is now complete.
Do not add a buffer-only value registry or infer owning edges from bounds. Capture data and its carried graph together.

## Completed typed route boundary

Type preparation owns each route's pointer-read kind, reached type and owning-backing descriptor. Ordinary projections
only select components. A pointer read follows the actual descriptor in the selected memory version. A backing step
follows its prepared pointer field or the established structural backing of an owning indirection. Neither grants
ownership, a fresh allocation or a layout from possible dependency bounds. Backing selection itself does not load an
element or require an already established element layout. Typed pointer dereferences keep their layout check.
Unknown and displaced descriptors cannot
become exact typed locations. Empty nullable alternatives remain distinct from unknown.

Storage::reference_path produces these routes. Entry seeding, access resolution, source/imported calls, output staging,
deferred captures and cleanup consume the same memory route interpreter. Captured inline descriptors use the chosen
capture. Reads outside that captured inline root use current memory. No route interpreter scans candidate nominal
buffer types or appends an owning projection in place of reading a buffer descriptor.

The representation retains structural projections for liveness and entry identity, plus ordered typed read/backing
steps. Prefixes retain those prepared steps rather than reconstructing their meaning. Remove inferred address-load
classification, the separate stored_locations segment interpreter, and its raw-buffer-type scan. Reuse actual-address
lookup and typed_targets. Preserve incoming state and existing publication/cleanup phases. This batch adds no portable
ownership promise and does not resolve the red generic witness oracle by itself.

Positive controls are existing nested pointer, moved-buffer, source/imported backing, cleanup and deferred transport
compositions. Negative controls cover bounds without actual descriptors, wrong layout, displaced descriptors, overwritten
captured slots and changed pointees. Run checker controls and the affected returned/deferred compositions after the
coherent batch. Keep the frozen1714 identities unchanged. Reconsider if this requires another state store, caller-side
type reconstruction, or more than200 net production lines beyond the removed route implementations.

Checkpoint8be430cea implements this boundary and removes about51 net production lines while adding78 net test lines.
Mutating cargo xtask style passes. All564 checker controls and368 source/imported return, cleanup, deferred and buffer
compositions pass. The existing escape cases pass. The generic experiment fails only at inspect for the witnessed source
and imported callers, with fabricated owners still rejected. The round-trip summaries remain unchanged from the earlier
probe. No native retry occurred. The1714 comparison remains551d12 evidence and has not been rerun for this checkpoint.
route-build-source.json records parent b27260893 plus diff84f9394019af07801852cd340ce7378a7a5120ea0eed7282f1217318df01b05d.
route-build-current.log covers the final checker-test fixture correction, which took45.36s after the3m34s production build.
route-checker-current.log, route-controls-current.log and route-capture-gap.log contain the corresponding results.

## Observation-transition boundary

Move operation change classification to the composite producer. Existing guarantee
invalidation separately derives transfers and content_only from call tables, assignment plans and intrinsic kinds.
Replace those booleans with a typed classification attached to the resolved StorageInvalidation. Transfer means source
value consumption without an independent pointee mutation. Contents means backing lifetimes survive while contents
can change. Unknown grants neither preservation. This records the existing distinctions, not a new permission promise.

StorageFlowCollector::operation_observations receives incoming resolved effects, assignment targets and the checked
operation. It produces scope, classified surviving observations and this change kind once, for normal and possible
outcomes. Cleanup produces Unknown. A merge retains a kind only when both inputs agree, otherwise Unknown. Planned
Paths retain no resolved kind and read as Unknown. All invalidation already follows the conservative global path.

ExecutionFlowDomain::invalidate_call_effects and invalidate_observations consume this result. Remove their independent
changes_contents, transfers and content_only classification and remove boolean arguments from cleanup/replay callers.
Initial composite flow uses planned refinements but passes resolved observations at operation application. Final
execution replay reads body.refinements().invalidations(). Failure captures retain the same resolved classification.
Do not change witness reconstruction, effects or portable ownership as part of this bounded removal.

Use existing physical/logical descriptor, raw contents, allocation lifetime, moved-source, cleanup and deferred controls
as positive and negative oracles. The generic witness gap remains open. Reuse route validation for unchanged code.
Expected production growth is below100 net lines. Reconsider rather than adding a parallel state store or a second
classifier if a path cannot receive the common result. This prerequisite does not complete the full value graph.

The classification is stored only in resolved Observed frames. All and planned Paths always report Unknown, so no
meaningless promise is attached to an opaque frame. Atomic restriction happens inside resolved_observations before the
operation classification is attached. Restriction preserves an existing classification. Valid selected memory calls
receive dependency entries from dependency/check, including empty contracts. Contents takes precedence for intrinsic
content changes. Actual source consumption still occurs at its existing checked move boundary.

The completed batch passes mutating cargo xtask style,136 bound-tree tests and564 checker tests. All368 affected return,
cleanup, deferred and buffer compositions pass in11.31s. Another36 raw-memory and owner-transport controls pass in18.79s,
with3 existing tests ignored. The joint lib-test build took3m36s. observation-build-source.json records parent8be430cea
plus diff153ae60022ec21ccc8d1aa37fe10e2045a3b4fab0d8135babf2f4da744be718a. Logs use observation-*-current.log.
The generic graph oracle was not rerun for this classification-only batch. Native failures remain untouched.

## Validation, storage and repository operation

Use the shared Cargo target from .cargo/config.toml, target/cargo. Never clean shared outputs. Before builds,
target/cargo/tooling measured232.83GiB and the drive had99GiB free. Keep one bounded current log per purpose in target/audit.
No task-owned builds, tests, native workflows or agents are running. The next active boundary is the complete captured
value graph, whose contract remains open above. The generic witness experiment remains uncommitted and red.

Current unit executables are bray_checker-05848d70c4b314f7.exe and bray_compilation-4af58abd509cab18.exe under
 target/cargo/release/deps. Build checker and compilation together to retain feature reuse. Current bootstrap-build.log
belongs to the test-only generic round-trip probe. Do not mistake it for a completed graph implementation.

Run mutating cargo xtask style at completed Rust-batch finalization. Run cargo xtask format before the final handoff.
Native LLVM/linker/executable workflows run outside the sandbox. The generic round-trip probe is a justified targeted
experiment deciding the next design. Broader tests are deferred until its implementation batch is complete.

The frozen comparison-baseline.json contains1718 identities. Four removed provider identities remain OPEN native-product
replacements, never passes. comparison-run.py runs the remaining1714 with bounded subprocesses and source/binary hashes.
It keeps8 workers,3 libtest threads, isolates known slow cases, then runs the attachment/admission tail separately.
Do not change the frozen identities or counts. Do not use comparison-pending.json as current status.

Issue https://linear.app/bray-lang/issue/BRA-694. Draft PR https://github.com/lejmer/bray/pull/717.
Branch feature/bra-694-native-bootstrap-contracts. origin push is git@github.com:lejmer/bray.git. Verified public repo,
owner lejmer, authenticated account lejmer with admin/push permission. SSH authenticated as lejmer. Push non-forced to
that exact branch, already authorized by the user. Reuse trust evidence unless destination/auth changes.
The retained review through4f767fbb2 has no findings at PR review5922465574. Current refactor is not ready for handoff.

Before finishing the full task, re-read AGENTS.md, mechanically audit every changed behavior and equivalent repo paths,
check architecture/visibility/DRY/size/contracts/errors/clones, run necessary tests and formatting, finish actual native
products and the frozen comparison on the same candidate, update the PR description and complete required review.
