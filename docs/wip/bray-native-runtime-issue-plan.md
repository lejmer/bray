# Bray native runtime issue plan

## Status and purpose

This is the WIP plan for writing the implementation issues for the [native runtime design](bray-native-runtime-design.md). It records the issue format, authoring checks, proposed reuse of existing Linear issues, labels and dependency rules. It does not implement the runtime or make the proposed issue boundaries final.

The target is a complete v0.1.0 architecture. Each implementation issue delivers the intended design for its stated consumers. Known missing behavior, placeholder ownership, unsupported required targets and a planned replacement of the delivered design are incomplete work. Bugs and later language features may require changes after delivery. Profiling may still justify tuning after the architecture is complete.

An issue must be executable by an agent that has never read this conversation. Its description contains the requirements that decide completion and explicit links to every required source. The agent must not have to discover the strategy, infer a contract from Rust, or follow a chain of parent issues to learn an essential rule.

This plan precedes the full issue rewrites. Proposed dependencies below are authoring constraints, not changes already made to Linear. Write and check the complete issue bodies before replacing their existing descriptions and relations.

The [worked BRA-555 draft](bray-native-runtime-issue-drafts.md) shows the intended issue body and metadata. It is a review example, not a ready implementation issue.

The metadata audit covers 31 existing roadmap issues. All are now assigned to the requesting user in Linear. BRA-568 and BRA-192 now use `Improvement`, matching their packaging and verification outcomes. Existing dependency relations remain unchanged. No new issue or scope rewrite has been published yet.

## Sources available to a fresh agent

The agreed design currently lives on `docs/bray-native-runtime-design-research`, outside the implementation baseline. Its selected architecture is recorded at commit `c3a8765e`:

- [Runtime design and temporary language appendix](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-design.md).
- [Consumer inventory](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-consumer-inventory.md).
- [Cost analysis](https://github.com/lejmer/bray/blob/c3a8765e/docs/wip/bray-native-runtime-cost-analysis.md).

Before implementation issues become ready, make these documents available on the implementation branch through an ordinary reviewed integration. Until then, issue drafts use the immutable links above. Do not ask an implementing agent to assume an absent local file exists or to start implementation from the research branch.

Each published issue gives its checked source revision and relevant document sections. It includes repository paths as well as immutable links. A changed implementation baseline requires rechecking the consumer inventory. A changed approved design requires updating affected open issues. A stale pinned link must not silently override an approved correction.

The language specification defines current source semantics. The WIP identifies intended amendments. The documentation-alignment issue names those amendments explicitly. Later issues depend on that work where they require the amended semantics. An implementing agent must report a remaining contradiction rather than choose whichever document makes its implementation easier.

## Requirements repeated in every implementation issue

Include this compact contract in each issue body, adjusted only to name its consumers:

> Implement the selected behavior in Bray using Bray ownership, dependency, lifecycle, storage and capability contracts. Use Rust as an inventory of callers, failures and code to remove. Rust representations are evidence, not the replacement design. Project-owned implementation linked into produced programs must be Bray, with the pinned temporal provider as the exception and documented system ABI dependencies permitted. Compiler and external coordination tools may remain Rust.
>
> Deliver the final contract for the consumers listed in this issue. Move their live callers and delete their replaced production implementation together. Do not complete the issue with a wrapper around Rust, a success-only path, a placeholder service, fixed capacity that cannot cover the required obligations, or a second policy owner awaiting later consolidation.
>
> Preserve the execution-speed, binary-size and compilation-speed goals. Explain storage, synchronization, dependency and compiler costs before adding a mechanism. Measure comparable consumers when meaningful. A necessary temporary regression needs its cause, the enabling dependency and the issue that will reevaluate it. Correctness alone does not establish completion.
>
> Read the required sources before proposing the implementation. Persist the issue contract, design hypothesis and unresolved blockers in a short repository task record that survives a VM reset. Re-read it after compaction. Do not rewrite the contract to fit the code.

Links supplement this contract. They do not replace it. Keep issue-specific semantics in the body so a missed document cannot turn a required behavior into an optional improvement.

## Format for each issue

Use the following sections. Tracking parents use the same goal and source rules but list child outcomes instead of pretending to be implementation tasks.

| Section | Required content |
| --- | --- |
| Outcome | Name the real consumer and its complete resulting behavior. State what disappears from the linked implementation. |
| Baseline and required reading | Give the checked revision, exact paths, immutable links and relevant headings. Explain any intended change to existing semantics. Include applicable `AGENTS.md` and skill instructions directly. |
| Behavior and ownership | Define inputs, owners, storage, publication, state transitions, outputs, rejection, rollback, finalization, destruction and final release. State who retains authority after a blocked or failed operation. |
| Bray language choices | Identify relevant features, authoritative chapters, existing examples and the obligations each feature supplies. Distinguish verified support from compiler gaps. |
| Compiler responsibilities | Name the owner of each needed fact, its consumers, representation and reuse requirements. State explicitly when no compiler change is needed. |
| Consumer and removal inventory | List production entry points, generated callers, platform variants and replaced Rust policy or storage. Name any remaining bridge consumer and its removal issue. |
| Acceptance evidence | Pair each requirement with a discriminating observable result. Include failure, authority, lifetime and race cases relevant to the consumer. Define retained-code evidence and target coverage. |
| Costs and performance | Give the expected allocation, storage, synchronization, code/data and compiler effects. Name comparable probes and any genuinely unavailable comparison. |
| Boundaries and dependencies | Give exact included and excluded consumers. For each prerequisite, name the contract it provides. Identify later consumers without deferring required behavior to them. |
| Completion and deviation rules | Require the final stated contract, one owner per responsibility, removals and evidence. Describe how a real blocker or architectural contradiction is handled. |

A published issue has no authoring placeholders or unresolved externally observable choices. Long issues should split at complete consumer boundaries. Do not omit requirements merely to keep an issue short.

## Bray language preparation

Every issue that writes, reviews or designs Bray includes `.agents/skills/use-bray-language/SKILL.md` in its required reading. Every issue includes `.agents/skills/unslop/SKILL.md` for repository prose. Compiler work also names `docs/contributing/coding-conventions.md` and `docs/contributing/crates.md`.

The author supplies a focused feature table. The agent verifies it against the specification and searches for competing existing idioms before adding types or wrappers. The table is a starting point, not an exhaustive list of features the agent is allowed to use.

| Work | Language features to evaluate | Sources |
| --- | --- | --- |
| Host, leases and authority | Ordinary move and borrow ownership, inferred dependencies, callable and lifecycle requirements, witness invalidation, explicit callable state | `docs/language/ownership-and-borrowing/`, `docs/language/contracts-and-trust/`, `docs/language/callables/` |
| Bootstrap and attachment | Static materialization, explicit initialization, `Once`, exact-thread authority, compiler-validated roles, explicit target ABI | `docs/language/declarations/static-storage-declarations.md`, `docs/language/async-and-concurrency/low-level-runtime.md`, `standard-library/std/src/sync/once.bray`, `standard-library/std/src/thread/` |
| Storage and cleanup | `Uninit<T>`, `RawAllocation`, `RawBuffer`, initialized prefixes, anchored views, storage policies, local cleanup allowance and selected lifecycle operations | `docs/language/targets-layout-abi-and-raw-memory/uninitialized-storage-and-anchored-borrows.md`, `raw-allocation-and-buffers.md` in the same directory, `docs/language/types/type-forms.md`, `docs/language/lifecycle/finalization.md` |
| Frames and tasks | Protected `Future<T>` and `Task<T>`, direct-await composition, independent start, separate cancellation broadcast and resolution, completion guarantees | `docs/language/async-and-concurrency/async-representation-and-storage.md`, `starting-tasks.md`, `task-handles-and-obligations.md`, `structured-task-scope-exit.md` in the same directory, `docs/language/contracts-and-trust/execution-guarantees.md` |
| Shared state and waits | Typed atomics, synchronization guards, explicit callbacks, non-copy registration ownership, cancellation shielding and affinity | `docs/language/async-and-concurrency/atomic-operation-contracts.md`, `standard-library-concurrency.md`, `cancellation.md`, `cross-run-memory-model.md` in the same directory |
| Erasure and native callbacks | Trait views with concrete lifecycle, provider-retaining symbols, native layout, explicit ABI, callback drain before release | `docs/language/types/type-forms.md`, `docs/language/targets-layout-abi-and-raw-memory/foreign-data-and-symbols.md`, `extern-declarations-and-ffi.md`, `callable-abi.md` in the same directory |

Replace directory-level entries with the exact relevant chapters and verified source examples when writing each issue. Do not publish this broad table as the only reading list. Useful existing report and admission examples include `runtime/bootstrap/src/outgoing.bray`, `records.bray` and `record-pool.bray` in that directory.

Bray has no private visibility. Declarations and fields are public by default. `internal` and explicit internal-use acknowledgement do not establish ownership, validity, synchronization or compiler-role authority. Each affected issue names the safe-source mutation or copying case its guarantees must resist and the actual contract or owner that prevents misuse. `uses(...)` alone is not a caller requirement.

Do not introduce Rust-style lifetime parameters, marker traits, reference-counted containers, mutex ownership, hidden closures, pinning APIs or downcasts by habit. If a mechanism is needed, name the concrete Bray invariant and consumer that requires it. Similarity to a Rust mechanism neither justifies nor disqualifies it.

## Compiler architecture requirements

Any compiler-changing issue explicitly requires `docs/design/compiler-architecture.md` and the owning design document. Use `checker.md`, `lowering.md`, `codegen.md`, `compiled-package-interfaces.md`, `emitter.md` or `linker.md` as appropriate.

The issue names where the compiler establishes each fact and where later phases consume it. For cleanup work, selected semantic actions and symbolic local storage needs come from one lifecycle description. Specialization supplies concrete layout. Validated MIR makes ownership and cleanup explicit. Backend code and runtime activation must not select lifecycle actions again.

Preserve typed demand-driven accessors, immutable publication, stable structural identities, exact dependency recording, cancellation and shared computation. Imported MIR and generic specialization use the same authoritative results. A narrow request must not force an unrelated whole-program pass. Serial and parallel execution use the same graph and produce deterministic results.

Target-dependent facts consume the selected target context. Native roles use validated metadata rather than names. Runtime units use normal package publication and demand selection. Optional service omission must hold through compiler planning, emitted metadata and final linking.

New diagnostics use structured IDs and typed arguments through `bray-messages`. Source rejection, external failures, cancelled work and compiler invariant failures remain distinct. A backend workaround for missing semantic facts is incomplete work even if its fixture passes.

An issue that changes a reusable fact states its invalidation inputs and relevant clean, incremental and imported-interface evidence. Do not add another query layer or cache unless it owns a meaningful result with a demonstrated reuse need.

## Precise behavior and evidence

Write transitions and examples, not instructions such as "handle shutdown correctly" or "support errors". Each applicable case identifies the actor, precondition, ownership commit point and observable result.

| Case | Required distinction |
| --- | --- |
| Rejected admission | Caller inputs remain owned. Acquired reservation state rolls back. No owner or callback becomes published. |
| Partial initialization | Dispose only initialized owners, exactly once. Preserve the original failure and continue required cleanup. |
| Retained provider | Code and product statics remain together while a dependent report, symbol, callback or borrowed value survives. Last callback return precedes release. |
| Blocked shutdown | The caller keeps its graceful owner. Shutdown does not wait for a root held by that caller. Resident fallback retains actual cleanup ownership and compatible continued execution. |
| Abnormal finalization | Shield required cleanup, retain ordered incidents, preserve fallback ownership and continue independent cleanup. A destructor cannot create a new task obligation. |
| Allocation denied after admission | Concrete mandatory activation, payload, report and release storage remains sufficient. Application allocations may fail through their ordinary contract. |
| Wake and cancellation | Separate notification commit, target retention, cancellation request and terminal completion. Callbacks run outside collection locks. |
| Optional service absent | Its code and data are absent from retained objects and link output. Lazy initialization alone is insufficient. |
| Direct await | Execute inside the same run without constructing an independent task control record. |
| Execution limit | Limit running work. Suspended work releases the execution slot while preserving its cleanup backing. |

Not every row belongs in every issue. The author must explain omissions relevant to its consumers. A test that only asserts a selected internal representation does not establish the behavior. Negative compiler cases must show that unsafe authority cannot arise through otherwise safe acknowledged internal access.

Inspect archive members, retained units and link maps to prove removals. Exported role names and successful native execution do not prove that Rust implementation disappeared. Distinguish project code, the temporal provider and documented system ABI dependencies.

Cover every supported target for changed ABI and layout contracts. Run native behavior on available target hosts. Cross-compilation alone is insufficient evidence of execution on an unavailable host. Track missing required native evidence as a blocker to the relevant acceptance issue.

## Costs without provisional design

Every issue includes cost reasoning even when measurement must wait. Ask whether the design adds allocation per owner, duplicate storage, atomics per operation, global contention, dynamic lookups, descriptor construction, generic code duplication or retained optional services. Compare the proposed mechanism with the existing consumer and the Bray features that can remove those costs.

Use matched behavior and build settings for meaningful before-and-after probes. Record source revision, toolchain, target and workload. Include small synchronous and constrained-resource consumers. Compiler costs include analysis, optimization, emission and linking, with cold and incremental cases where relevant.

Do not impose Rust/C++ parity on every prerequisite issue. Do not use that exception to omit cost evidence indefinitely. Assign any necessary temporary cost to a named dependency and review stage. Performance tuning can follow architecture completion. Known incomplete ownership, lifetime, target support or failure handling cannot.

No blanket production-line cap determines the architecture. Forecast production and test growth separately to detect duplicated policy and oversized cuts. Split work when a consumer can be completed independently, rather than preserving historical Rust module boundaries.

## Existing issue disposition

The audit found three active labels for team BRA, `Feature`, `Improvement` and `Bug`. Use the existing labels. Do not create another taxonomy for this work. `Bug` applies to a reproduced defect, not merely an old design. New implementation and architectural capabilities use `Feature`. Verification, cost work and documentation alignment use `Improvement`.

The following are proposed rewrites. Each row needs a complete issue body and consumer inventory before publication.

| Issue | Proposed outcome or disposition | Label |
| --- | --- | --- |
| BRA-409, BRA-281 | Retain as end-state tracking parents. State the Bray implementation boundary, whole-product proof and final design requirement. | Feature |
| BRA-547 | Track synchronous and loaded host delivery. Remove test-host completion as a prerequisite for unrelated execution-core work. | Feature |
| BRA-548 | Track complete async execution, waits, lanes and worker delivery. Depend on specific host contracts, not the entire host parent. | Feature |
| BRA-553 | Establish typed Bray host/service ownership, explicit bootstrap storage and exact-thread TLS entry. Resolve recursive bootstrap and admission before publication. | Feature |
| BRA-554 | Complete synchronous and foreign entry authority over that owner, including nesting, rejected entry and returned ownership. | Feature |
| BRA-502 | Derive and secure physical local cleanup backing in the owning compiler phases and Bray service. Preserve transfers, concrete layout, multiplicity and last-use lifetime. | Feature |
| BRA-505 | Complete ordinary-scope async/fallible finalization through the same admitted storage and selected cleanup. Remove donor-branch recovery instructions as a design mandate. | Feature |
| BRA-506 | Extend the completed cleanup contract to represented owners. Split only by independently complete owner families. | Feature |
| BRA-507 | Assign remaining host-boundary admission consumers explicitly. Remove policy already owned by BRA-554, BRA-555, BRA-556 or BRA-557. Close as superseded if no independent consumer remains. | Feature |
| BRA-555 | Complete independently loaded provider formation, whole-product retention, owner transfer, reload and final callback return. Do not mandate the Rust registry representation. | Feature |
| BRA-556 | Complete product-static eligibility, deterministic dependency cleanup, graceful shutdown ownership and admitted resident fallback. Separate synchronous and suspending consumers only if each cut has a final contract. | Feature |
| BRA-557 | Complete exact-thread attachment and dependency-ordered TLS cleanup, including explicit detach and native thread exit. | Feature |
| BRA-558 | Complete Bray report consumption, ordered incidents and output without an executor dependency. Reuse existing report storage. | Feature |
| BRA-561 | Complete compiler-described activation and direct-await execution without another task owner. Remove dynamic reconstruction and duplicated Rust activation graphs. | Feature |
| BRA-560 | Complete started-task admission, one task control record, one result transfer and final reclamation. Consume the activation contract instead of defining it again. | Feature |
| BRA-562 | Complete cooperative cancellation, separate broadcast/resolution and shielding over the same owners. | Feature |
| BRA-563 | Complete readiness, typed lanes, running-work limits, queued excess and remembered wakes. Remove the arbitrary requirement to preserve the old 64-transition constant. | Feature |
| BRA-564 | Complete event registration, notification commit, withdrawal and callback residency, including unattached native signaling. | Feature |
| BRA-565 | Complete monotonic deadline registration and expiry/cancel/shutdown races without another timer owner. | Feature |
| BRA-566 | Complete demanded worker lifecycle, partial startup rollback, exact-thread detach and non-self-joining shutdown. | Feature |
| BRA-567 | Restrict to async root/main-lane orchestration and terminal shutdown. Move synchronous startup/shutdown to the earlier completion issue below. | Feature |
| BRA-559 | Complete Bray child test-host policy and protocol, capture, command-wide serial exclusion, timeouts and session cleanup. External protocol readers remain tooling. | Feature |
| BRA-568 | Remove obsolete Rust artifact builds and partitions. Publish demanded Bray components through the normal native unit resolver and enforce retained-dependency rules. | Improvement |
| BRA-192 | Retain final cross-target native conformance and independence acceptance. Collect evidence throughout delivery. | Improvement |
| BRA-540, BRA-541, BRA-542 | Retain final execution, footprint and compilation-cost acceptance. Align probes with the agreed phased cost policy. | Improvement |

BRA-402, BRA-404 and BRA-406 remain the broader performance owners. Cross-link their exact responsibilities rather than duplicate them. Preserve delivered foundations and completed historical issues. BRA-501 is currently In Review in Linear, which does not prove its Git merge state. Verify its PR before using it as a delivered dependency. Preserve useful fixtures while replacing its code-only retention expectations.

## New issue candidates

Create only these identified gaps initially. A later compiler gap needs a concrete failing consumer before receiving a new issue.

| Working name | Complete outcome | Label |
| --- | --- | --- |
| Documentation alignment | Integrate the intended lifetime, shutdown, TLS cleanup-order and implementation-boundary amendments into their authoritative documents. Specify the fixture changes consumed by implementation issues. Preserve the WIP and its temporary appendix until implementation finishes. | Improvement |
| Synchronous product completion | Complete early synchronous formation, root observation, startup, rollback and shutdown in Bray. Integrate the migrated attachment, static and reporting owners. Remove the synchronous initialization/shutdown exports implemented in Rust and prove a complete synchronous executable retains zero project-owned Rust implementation before full async migration. | Feature |

The documentation issue does not silently change runtime behavior or delete failing tests. Changed behavior becomes an explicit prerequisite contract. The synchronous completion issue owns the final root orchestration and proof, not a second implementation of entry, attachment or static cleanup.

Before publishing, determine whether BRA-507 has a distinct returned-owner admission consumer. Likewise, determine whether suspending static cleanup is an independent complete consumer of BRA-556 or requires its own child issue. These are issue-boundary questions. They do not reopen the selected shutdown or admission semantics.

## Linear relationships and ordering

Use Linear's native `blockedBy` and `blocks` relationships for execution prerequisites. Text links explain the contract. Parent relationships group work, and `relatedTo` records a shared concern without blocking. None substitutes for a native blocking edge.

A blocking edge means the dependent implementation requires an accepted contract or delivered implementation from its prerequisite. Do not block an execution-core issue on all of BRA-547 merely because both share a parent. Do not block worker lifecycle on timers unless a required worker consumer actually uses them.

The ordering requirements are:

1. Documentation alignment precedes implementation of amended lifetime, shutdown and TLS ordering contracts. Relevant delivered compiler and standard-library foundations remain available.
2. Bootstrap and explicit startup storage establish service ownership without recursively admitting themselves. General admission must not require forming the same host that first requires admission.
3. Entry, synchronous static cleanup, exact-thread cleanup and reporting supply the contracts needed by synchronous product completion. That completion must not depend on async workers or the Bray child test host.
4. Loaded-provider formation consumes stable host, entry and admission contracts. Provider lifetime and fallback cleanup use the same terminal owner. They must not block each other through two competing owners.
5. Compiler-described activation and direct-await execution provide the frame contract for started tasks. Remove the historical BRA-560-before-BRA-561 edge if the final consumer cut follows this direction. If either issue still needs the other's complete implementation, redraw the cut before publishing a cycle.
6. Cancellation, dispatch, events, timers and workers depend on the specific task, activation, attachment and wait contracts they consume. State those contracts in each issue before choosing the edges.
7. Async root completion and suspending host cleanup consume the required lane and continued-execution contracts. Shutdown must retain those services through fallback completion.
8. The Bray child test host consumes the finished host and timeout contracts. Packaging consumes all migrated component outcomes. Final conformance and performance owners consume the applicable completed products and evidence.

These constraints are not a complete publishable graph. Build the exact graph from the authored consumer tables. A prose-only roadmap would conceal missing edges. The publication step below installs and verifies the native graph.

Do not create a blocked issue to defer a required part of an otherwise completed consumer. A genuine independent compiler prerequisite can be a separate issue, but the consumer remains blocked and incomplete until it lands. Ordinary implementation choices within the selected architecture do not require user approval.

## Authoring and publication procedure

1. Revalidate develop, BRA-501's PR, issue states and the documentation revision. Preserve user changes and completed history.
2. Write the documentation-alignment issue and synchronous product completion issue. Draft revised tracking parents and all reused implementation issues in this document's companion drafts, stored under `docs/wip/`.
3. For each issue, trace the current production callers and enumerate the final behavior. Resolve included consumers, exact reading links, compiler owners and removal responsibilities. Assign each required obligation and deletion exactly one issue owner.
4. Fill the feature and acceptance tables. Add a named failure or race example for each nontrivial ownership boundary. Identify the evidence that would distinguish a superficially successful Rust-backed wrapper from the actual Bray result.
5. Review each issue as a fresh agent. Check whether it can choose an implementation, know what it must preserve, reject unsafe shortcuts and prove completion using only its body and explicitly required sources.
6. Review the issue set together. Check coverage of all six WIP implementation stages, target contracts, compiler gaps, existing performance owners and integration into permanent documentation. Check dependency cycles, duplicate ownership and unowned obligations.
7. Publish serially. Create new issues, retain their returned IDs, rewrite reused descriptions and titles, and assign the table's labels. Assign every issue to the requesting user in Linear. Resolve the account through Linear when publishing. This includes tracking parents and verification issues, so they appear in the user's mobile view. Keep personal names, email addresses and account identifiers from connected services out of repository files. Set projects, parents and appropriate existing team states. Close superseded issues only after their complete scope has active replacement owners.
8. Install the exact native blocking edges. Remove obsolete edges explicitly without deleting unrelated prerequisites. Add nonblocking related links where appropriate. Record the contract supplied by each new dependency in the issue body.
9. Read every changed issue and its relations back from Linear. Verify assignee, labels, parent/project placement, closure reasons, reciprocal blocking relationships, acyclicity and complete coverage. Replace working names in the repository drafts with actual issue IDs.
10. Update the durable Linear delivery document, project descriptions and misleading milestones to match the published graph. Commit and push the repository plan and drafts so they remain accessible after a VM reset.

Do not publish an issue as ready while a missing reading source, a required semantic decision or an unresolved ownership boundary remains. An authoring draft may be explicit about that gap. An implementing agent must not be asked to settle it accidentally while claiming the issue complete.

## Implementation and review completion

Before coding, the implementing agent records the owning Bray values, state transitions, relevant feature choices, compiler facts, removals and expected costs. Type and method names, queue representations, coallocation and pooling remain implementation choices where the contract permits them. The issue must not freeze a Rust-inspired representation merely to avoid ambiguity.

After implementation, review correctness and architecture separately. Correctness review checks behavior, authority, rejection, rollback, lifetime, cancellation and target evidence. Architecture review checks one owner per responsibility, phase boundaries, unused service omission, replaced code deletion and costs. Passing fixtures cannot excuse a new duplicated runtime policy engine.

A language or compiler limitation needs a minimal failing Bray example, the authoritative required behavior and the phase that must implement it. Implement a bounded prerequisite in the appropriate owner or create an independent dependency and block the consumer. Do not substitute Rust policy because its implementation is familiar.

A contradiction with the selected architecture or a demonstrated inability to meet the destination costs requires an explicit design amendment. Record affected issues and obtain a decision for changed semantics. Routine implementation details and necessary bounded fixes proceed within the authorized scope.

An issue is complete only when its full listed consumer contract works, replaced production paths are gone and required evidence exists. A PR may be an intermediate review checkpoint without completing the issue. Preserve useful existing failure coverage and replace representation-specific tests with equivalent behavior checks.

PR titles and descriptions follow the repository user's conventions. Use a lower-case Conventional Commit title without a scope. Include `Change Summary` and `Related Issues`, with issue IDs as link text. Do not list verification steps in the PR description unless requested. Verification evidence belongs in the issue handoff and repository task record.

When all consumers implement the design in Bray, integrate remaining permanent architecture and language amendments, remove Appendix A from the WIP design, and retire migration-only issue drafts and inventory material deliberately. The temporary language helper must not become a second specification.
