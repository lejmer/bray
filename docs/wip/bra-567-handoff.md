# BRA-567 implementation handoff

## Scope and checkpoint

Continue [BRA-567](https://linear.app/bray-lang/issue/BRA-567/implement-async-root-and-main-lane-orchestration-in-bray)
on draft [PR #710](https://github.com/lejmer/bray/pull/710), branch
`feature/bra-567-async-root-orchestration`. The branch head is the implementation checkpoint. The comparison baseline is
`e477f82ce8cdbae354a81c23b6744fb8d68475d6`, which includes the parent implementation and develop integration.
Inspect current remote branches and issue relations before changing the stack. Parent
[PR #709](https://github.com/lejmer/bray/pull/709) supplies demanded worker lifetime through
[BRA-566](https://linear.app/bray-lang/issue/BRA-566).

Implementation and testing are paused while the architecture is derived from the specification. The current
implementation is evidence about an attempted design, not the starting assumption or a constraint on its replacement.
The user authorizes rewriting unsuitable mechanisms. Representation choices below remain proposals until the complete
ownership and transition model has been assessed. No diagnostic or test run is the next action.

### Requirements that govern the design

The executable host owns the root resolution obligation. Async root execution uses genuine Main authority supplied by
validated entry roles. Direct await stays within the current run and requires no additional task control block.
The generated frame owns lexical cancellation broadcast and lifecycle resolution. It publishes a terminal outcome only
after lexical cleanup. Host observation never resumes that frame or repeats lexical cleanup.

Outcome payloads and incidents remain owned through disposal. Required execution, exact-thread attachment, services and
provider code remain available until disposal and selected static cleanup finish. Caller shutdown ownership and resident
host ownership share one terminal teardown state. Closing admission preserves existing cleanup authority. Blocked
shutdown returns an unresolved owner rather than waiting for a dependency held by its caller.

Ownership admission secures actual cleanup storage before publication. Rollback retains initialized inputs and resolves
every accepted resource exactly once. Native boundaries contain panic/cancellation and transfer actual incident owners.
Internal visibility, marker construction, raw addresses and observational thread IDs do not establish authority.
No language change, C borrow ABI, Rust root frame, second shutdown engine or host-wide runtime operation table is needed.

Compiler semantics distinguish immutable copied data, current storage contents, complete witness ownership, physical
reference targets and lifetime/access dependencies. Legal moves transport evidence, while ordinary field copies do not.
Mutation invalidates affected observations. Branches preserve every possible dependency and only universally supported
guarantees. Loop summaries are bounded. Imported contracts preserve the same semantics as Source checking.

Runtime selection follows all reachable obligations, including cleanup. Synchronous products require no async executor.
Absent deadline or worker demand omits the corresponding implementation units, with authenticated provenance evidence.
Maintainability, compile cost, generated execution, product size and retained resources are acceptance criteria.

### Architecture proposal under assessment

Use ordinary Bray owners for distinct responsibilities: the host root resolution claim, stable task/frame storage,
retained domain access, exact-thread entry restoration, admitted startup resources, outcome/report custody and product
teardown. Short operations borrow those owners. A retained storage lease does not acquire a task's sole resolution claim.
Types represent distinct ownership or lifetime invariants. Enums represent mutually exclusive phases. Traits represent
selected behavior, while validated runtime roles establish execution authority.

Task admission is preparation followed by publication. Preparation owns the frame, a reserved admission entitlement,
stable task storage, binding and actual cleanup backing. Native installation and other fallible work occur before
publication. The admission entitlement accounts for races with entry closure. Commit transfers the complete prepared
ownership; rollback uses the same owned fields and initialization state.

The task arena stores ordinary task cells, including the host-owned root. Generated frames retain their own body,
direct-await and cleanup state. Scheduling state does not duplicate the frame's lifecycle state. Only a completed
generated frame can publish terminal storage, which is taken once by the root observer. The retained host execution
binding survives observation and payload disposal without resuming a terminal root or inventing Blocking authority.

The common resident host owns one teardown state, the cleanup domain graph, attachment state and report sink.
It has no concrete executor field or static reference to async implementation units. An async product separately forms
an execution owner and retains it through payload/static cleanup. Admitted continued cleanup can retain its selected
execution binding through the existing host state. Graph edges
record source owners and provider dependencies. External-root status is relative to the fixed teardown set. Internal
consumer edges order cleanup rather than block it. Extract owned cleanup work before releasing host locks and invoking
callbacks. Drain incidents while their providers remain retained. Exact-thread cleanup uses the existing attachment
owner and selected execution binding. BRA-507 integrates suspending fallback with this same state, while BRA-559 owns
test-host policy.

Keep common orchestration concrete. Use statically selected provider behavior and small per-owner bindings where an ABI
requires erasure. Separate worker and deadline capabilities so absent demand introduces no concrete provider references.
Generated frame descriptors remain protected compiler metadata, not an ordinary source-implementable Future trait.
Ordinary Source callbacks preserve checked incident dependencies. Native bindings use owned values or raw pointers,
anchor borrows within their entry and consume boundary incidents before returning.

The proposed compiler model uses explicit data terms, storage places/versions, whole-value carriers, loan capabilities,
physical target regions and dependency-bearing facts. These are semantic responsibilities, not a commitment to new
stores or a compiler-wide rewrite. A checked operation has one ownership/storage transition consumed by facts, borrowing
and cleanup. Call summaries carry entry requirements, effects, ownership/reference transfers, normal guarantees and
abnormal incident dependencies. Deferred frames retain relevant entry data and dependencies rather than mutable caller
state. Joins retain correlated case/carrier alternatives, with canonical block summaries rather than execution history.

The proposed ownership records have these concrete responsibilities. Names and layouts remain provisional.

| Record | Owned fields or retained dependencies | Exclusive responsibility |
| --- | --- | --- |
| Common host record | Attachment registry, report sink, teardown state, admitted cleanup binding and provider retention | Product infrastructure and the shared teardown state |
| Async execution owner | Execution domain, validated Main binding and host root resolution claim | Retain demanded execution services through root observation and selected product cleanup |
| Root/task owner | Domain retention, generation-checked task key and introduced resolution authority | Take and resolve one terminal outcome |
| Task storage lease | Domain retention and task key, without resolution authority | Keep cell/backing available to observers or registrations |
| Prepared start | Admission entitlement, reserved slot, inactive frame, binding and physical cleanup backing | Commit complete publication or resolve rollback |
| Task cell | Scheduling state, cancellation request, stable frame, wait linkage and terminal storage | Execute one run and publish its terminal record once |
| Entry scope | Exact attachment, prior binding, installed binding and admission permit | Restore the displaced execution context on every exit |
| Outcome owner | Payload/report, incidents, backing still in use and execution/provider dependencies | Dispose or transfer the complete observed result |
| Teardown state | Fixed domain set, phase, graph progress and in-progress cleanup reservations | Advance one shared product cleanup operation |

A type name, task key or matching fields cannot introduce any of these authorities. Introduction and movement follow
checked complete-carrier contracts. Only explicit retention creates another storage lease; it never duplicates the
resolution claim. Runtime phases use tagged state rather than unrelated flags. Scheduling may continue while generated
cleanup suspends, so scheduling and frame lifecycle state are separate responsibilities.

| Operation | Required semantic transition |
| --- | --- |
| Prepare | Reserve admission and backing while retaining the actual inactive frame |
| Commit | Publish the fully installed frame and transfer the sole resolution claim |
| Roll back | Resolve initialized resources and frame ownership exactly once |
| Resume | Run under the validated binding, preserving the frame through suspension |
| Publish terminal | Commit outcome after generated lexical cleanup, with completion visibility |
| Observe | Acquire completion visibility, take the outcome once and prevent further resume |
| Dispose outcome | Preserve execution/providers and release backing only after its last user |
| Begin teardown | Fix the teardown set, close new admission and retain existing cleanup authority |
| Blocked attempt | Return control with the unresolved owner and secured resident backing |
| Clean domain | Own its reservation, run callbacks without host locks, drain incidents and release consumer edges |
| Finish teardown | Release infrastructure only after all work that can use it has completed |

For compiler semantics, a storage place identifies a location, a data term identifies an immutable observation, a
carrier identifies a complete witness introduction, and a loan identifies access permission plus its dependencies.
A reference's physical target set and its authority/lifetime roots are separate fields. Facts record the observations
and complete carriers they require. One operation transition updates occupancy, versions, capabilities, carriers and
obligations consistently. Raw publication affects reachability; raw transfer also applies its typed ownership effect.
Unknown reachability is conservative and does not invent either freshness or authority.

A scalar copy retains data without coupling it to later source writes. A legal witness move retains its carrier and
revokes its old owner path. Borrowing retains access without transferring target ownership. Deferred captures retain
copied data, moved carriers or loan dependencies according to their actual capture mode. Joins keep possible dependencies
and correlated guarded ownership alternatives, and require valid evidence on every relevant path. A finite block/place
summary bounds loop state. Proof-budget exhaustion can lose a proof but cannot drop an obligation or dependency.

Small compile-time provider traits express replaceable behavior. Closed semantic alternatives and concurrent runtime
phases use enums. Authority is supplied only by validated roles or established live contracts. Common orchestration
remains concrete; specialize provider adapters and erase only at actual ABI boundaries. Borrowed local views incur no
retention operation. Frame and control storage can be co-allocated. Common synchronous host ownership contains no optional concrete
executor whose lifecycle would statically retain async units. Facts, paths and summaries use shared immutable
representations, cached per-type classification and dependency-directed invalidation rather than repeated whole-state
traversal. Broad effects still require conservative broad handling.

Assess this proposal through complete success, panic, cancellation, startup failure, nested entry/restoration, blocked
shutdown and imported-consumer scenarios before selecting concrete layouts or changing code. The ownership/transition tables above are the starting engineering artifact. Complete the requirement-to-component
mapping and assess unresolved synchronization, admission and cleanup contracts before choosing representations. Compare the existing implementation
against that model only after it is coherent. Identify mechanisms to remove and justify every additional concept.

This document is the single current handoff. Update it in place. Keep current verified evidence, architectural decisions,
open design questions and the next engineering action. Do not promote an experiment into a design commitment merely
because it improves a test. Do not accumulate parallel progress records.

## Mandatory working instructions

1. Read `AGENTS.md` and the current repository-local `work-on-bray-issues` skill before working.
   Read the coding conventions and crate responsibilities before code changes.
2. Use `use-bray-language` before designing Bray behavior or writing Bray code. Read the actual relevant language
   specification before relying on an assumption or running tests. Use `debug-bray-end-to-end` for this investigation.
   Existing fixtures and compiler behavior do not override the specification.
3. Investigate the mechanism end to end: creation, forwarding, mutation, capture, branch and loop merging,
   cleanup, and final consumption. Explain both accepted and rejected programs through the same rules.
   Do not investigate and repair one evidence boundary at a time.
4. Keep facts with the existing owner or context whose identity and lifetime justify them. Pass or borrow that context
   through ordinary forwarding. Introduce a separate type only for a distinct owner, lifetime or invariant.
   Actual ownership changes must transfer or retire evidence.
5. Preserve the distinctions between call-entry facts and current facts, and between ownership and permission to access
   storage. Distinguish immutable copied values, current storage contents, retained loans and their current referents.
6. Evaluate the whole mechanism, including what can be deleted. Reusing a type does not help if different consumers
   interpret it inconsistently. Remove superseded machinery rather than adding compatibility layers.
7. Consider compilation of large programs. Extend existing traversals and reuse established contexts and cached
   classifications. Any additional traversal, repeated reachability calculation or cloning needs a concrete justification.
   Loop analysis must converge without accumulating unbounded histories.
8. Finish related changes as one coherent batch before rebuilding or testing. Validate assumptions against the spec first.
   A narrowly focused diagnostic run is justified only when it resolves a specific design uncertainty.
   Do not use build/test iterations to discover the design one local patch at a time.
9. Exclude `bray-compilation` test binaries from probes unless code in that crate changes. Its ordinary production library
   may be an unavoidable transitive dependency. Reuse matching artifacts and one Cargo target directory.
10. At batch finalization, run the mandatory mutating `cargo xtask style` after Rust changes and `cargo xtask format`
    for applicable Bray changes. Do not substitute `cargo fmt --all` for repository style tooling.
    Documentation-only work does not require builds or tests. Run native toolchain workflows outside the Codex sandbox.
11. Keep the draft PR available for user inspection. **Do not request independent review until implementation and required
    verification are complete and you believe the issue is done.** Review is a final check for missed problems.
    Do not repeat the previous practice of reviewing intermediate fixes while Source acceptance still fails.
    Existing review conclusions are not proof that the current implementation is complete.
12. Commit and push coherent checkpoints regularly. Do not leave useful implementation or essential handoff evidence
    solely in ignored files on a temporary VM. Do not commit build products or huge trace archives.
13. Keep necessary fixes within the active PR. Follow-up issues are exceptional and require independent material work.
    Do not launch implementation agents. Follow the current skill for a final reviewer, reusing the existing reviewer
    if available. A reviewer from another thread might not be available.
14. After implementation, verification and final review, mark the PR ready and Linear In Review. Never mark Linear Done.

The user permits deleting task-owned build output inside `target/` on the isolated VM. This overrides the skill's
raw-deletion restriction for that task-owned output. Keep useful current caches rather than triggering avoidable rebuilds.

PRs use a lowercase Conventional Commits title without a parenthesized scope. Include Problem, Change Summary,
Related Issues with ID-only link text, and LLM Contributions. Use imperative summary bullets. Omit test steps unless
asked. Issues, PRs and ordinary documentation must stand alone and avoid temporary filesystem or conversation context.
Attribute agent PR comments according to the current workflow skill and verified authenticated account.
Bray declarations are public by default. Do not add a `public` modifier. In long C lists, put each parameter or condition
on its own line, leave the opening line empty, and put the closing bracket on its own line.

## Runtime implementation to preserve

The Source `Domain` owns configuration, queues, tasks, counts, pins, completion, services, failures and reports.
`Host.ExecutionOwner` carries the actual context, `Current` is a counted view, and `EntryScope` retains the displaced
host context. A task slot's execution owner stays fixed until recycling. Claim ownership before native factory execution,
and dispose the outcome before physical completion.

The change replaces Rust root, Main and shutdown policy. Retained runtime ownership uses the existing Source owner.
Worker and deadline services retain independent domain references. Do not add another shutdown engine, an all-runtime
function table, fabricated lane authority or a C borrow ABI. No language change is authorized.

[BRA-507](https://linear.app/bray-lang/issue/BRA-507) owns suspending host/static fallback integration.
[BRA-559](https://linear.app/bray-lang/issue/BRA-559) owns test-host policy. Preserve their existing integration contracts
without implementing those separate outcomes here.

## Current verification boundary

Implementation and end-to-end verification remain in progress. Do not request final review yet.

The checker separates a retained value's identity from its current data. A reference value still owns its access
capability. Its reached data is `Borrowed` within `CurrentValue`, so it does not own the target storage. Entry inputs
retain their original identities. Checked type and borrowed-input metadata determine reference role during contents
refresh, without changing entry values. `StorageValue` remains a non-owning scalar physical observation, while
`Constructed` contains structural components. `OwnedTarget` represents real backing. Synthetic backing projections
for current contents are removed. Substitution preserves the non-owning role independently of ordinary borrow
normalization. No additional witness or alias store is introduced.

Predicates discard the access loan identity while retaining the owned referent's physical context. Aliases of one
referent agree, while equal data at distinct storage does not identify predicate subjects. Const calls compute from
data independently of those contexts. Joins distinguish complete value identity from current component observations.
The existing join matrix covers owned values, explicit loans and formal reference values, with whole/component
carriers and mixed initial/refreshed predecessors.

The cached inline footprint classifies complete nominal carriers as well as empty markers. Reached raw moves revoke
matching scalar-only or pointer-containing owner authority. Unrelated typed moves preserve another nominal owner.
Raw writes remain conservative for contents. Active loans and stored views share one typed survival query, with
active loan regions resolved once. Mutable union reconstruction retains checked physical variant paths. Nested
conditional ownership carries every enclosing active-case guard in one paired predecessor traversal.

The latest implementation checkpoint is `7c7a70ba7c0ff5d153c554bfc49bd926efc5d665`. The non-owning reached-data
representation replaces the entry-seeding experiment. Combined checks pass checker units and representative controls,
but full Source acceptance still fails. Do not request final review.

| Check | Result and limit |
| --- | --- |
| Checker library units | Current batch: 415 passed |
| Runtime Source bodies and certificates | Current batch: 3,160 checks, 22 failures. Earlier canonical-loan sweep: 15 failures. Entry-seeding experiment: 106 failures |
| Representative Source controls | All 16 behave as expected on the current batch, including genuine/fabricated owners, nested unions, captures, promises, raw transfers and exposed mutable markers |
| Required style and formatting | Current mutating style workflow passed |
| Native/imported acceptance and comparative costs | Unrun |

Previously failing destructor terminal-effect checks pass. Composition, direct invocation, event registration,
worker context construction, task retirement, service retirement and observer rollback pass their Source bodies and
certificates in earlier sweeps. Remaining body families are execution dispatch, resume binding, root entry cleanup, task-allocation cleanup,
worker formation and task-result transfer. The earlier regressions in activation abandonment, entry execution
restoration and task dispatch are absent in the current sweep.

Runtime ownership follows complete publication and disposal units:

- Root admission reuses the actual entry scope's `Current` view. The native frame claim remains in its original input
  slot until admission commits.
- Worker construction accepts the complete binding before publication. Failed native installation retires the actual
  context and pin.
- One task-slot operation owns detachment, metadata cleanup, report disposal and final pin release. Its detach phase
  releases the resident host acquisition before callbacks, retaining the pin through disposal.
- Deadline backing retirement detaches its pin before closing and retains it locally through callbacks.
- Observers validate execution before allocation. Native record output uses scratch before committing to preparation.
- Execution observation explicitly promises valid native report callbacks on its returned outcome.

Optional services bind native operations per retained domain. Deadline driving returns one C-layout value. Final
provider destruction reuses its existing native entry. This introduces no C borrow ABI, extra runtime owner or
host-wide dispatch table. Native acceptance shares authenticated runtime provenance auditing across sync/async
products. It includes minimal async demand omission and preparation/build/startup/size receipts, still unrun.

Focused diagnosis found task allocation's last `dispose_reference` call does not physically frame the local Current
root/acquisition despite framing the original factory temporary. Its admission and disposal calls export Unknown
among other effects. Admission's result has no exported pointer relation, while its deeper `admit_locked` producer
has a pointer relation from Entries backing through slabs. Simple nullable pointer, tuple and destructor controls
export references correctly, including a retry loop with nested Result/nullable extraction and guard cleanup
(14 checks, no failures), so those control-flow shapes alone do not explain this loss. Inspect actual producers and
exits, without inferring freshness from allocation ownership or absent input dependencies.

Worker formation carries a row witness but lacks an active trusted clause for Existing at the final call. Ordinary
branch facts establish Existing/Ok. Availability or ownership of a witness alone must not activate mutable conditions.
The physical-framing experiment was removed. No speculative relaxation of Unknown effects is implemented.

Temporary traces in `trusted/check.rs`, `cleanup/transition.rs`, `terminal.rs` and `transfers.rs`, plus the local harness
under `xtask/src/bin/`, must be excluded from checkpoints and removed before finalization. The harness uses ordinary
production tooling artifacts and supports selected Source body, certificate and dependency-summary diagnosis.
No temporary artifact is required to resume. The optional Linux retained-resource probe runs separately from normal
products, so instrumentation does not affect product-size receipts.

Connected GitHub account `lejmer` has admin/push access to the public `lejmer/bray` repository. Shell GitHub
authentication is invalid. Use connected Git object APIs for checkpoints on existing PR #710. Parent #709 remains
open at the unchanged stack base. Push coherent checkpoints regularly, as requested by the user.

The user has directed a specification-first engineering design, with the current implementation set aside during
architecture derivation. The proposal above is provisional. Continue the ownership and transition model before code or
tests. Current representation details in this section describe the checkpoint only and do not constrain the design.
Native/imported/cost acceptance and final review remain pending.

## Final acceptance

Complete Source, imported and relevant native coverage for startup rollback, blocked-root progress, genuine Main
identity, compatible children, direct await without another task control block, success/panic/cancellation,
nested foreign entry and restoration, lexical cleanup before terminal publication, retained cleanup access and
shutdown without waiting on dependencies held by the initiating caller.

Preserve complete incidents and keep services live through outcome disposal and selected static cleanup.
Prove synchronous products retain no async executor or Rust root policy, and absent deadline/blocking demand omits
those optional components. Use link maps and retained archive members, not only runtime laziness.

Measure direct, synchronous and minimal async whole-build time, startup, executable size and retained resources.
Separate production-code growth from tests and do not count formatting or movement as architectural simplification.
Use existing Cargo/xtask automation, including runtime-artifact smoke coverage and the committed native fixtures.
Do not introduce a second task runner or commit temporary diagnostic infrastructure merely to preserve a prior workflow.
