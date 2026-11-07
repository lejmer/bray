# BRA-567 implementation handoff

## Scope and checkpoint

Continue [BRA-567](https://linear.app/bray-lang/issue/BRA-567/implement-async-root-and-main-lane-orchestration-in-bray)
on draft [PR #710](https://github.com/lejmer/bray/pull/710), branch
`feature/bra-567-async-root-orchestration`. The branch head is the implementation checkpoint. The comparison baseline is
`e477f82ce8cdbae354a81c23b6744fb8d68475d6`, which includes the parent implementation and develop integration.
Inspect current remote branches and issue relations before changing the stack. Parent
[PR #709](https://github.com/lejmer/bray/pull/709) supplies demanded worker lifetime through
[BRA-566](https://linear.app/bray-lang/issue/BRA-566).

The user has authorized the complete migration plan below and implementation within PR #710. Preserve useful work,
replace unsuitable mechanisms, and keep coherent checkpoints pushed. The architecture is derived from the specification.
Implementation follows the ordered ownership protocols below, rather than selecting the next edit from a failing test.
Representation names can change when the same invariant is clearer with existing types. No new language feature or
weaker authority is authorized.

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

## Refactor plan

Progress: step 1 is implemented and undergoing whole-protocol self-review. Its explicit observation variant has owner,
contents and owned/reference/scalar role fields. Old observation variants and nested reference-data role tags are gone.
No build or behavioral test has run on the refactor. Temporary production trace branches have been removed.
The observation model, arena migration and initial startup/claim changes are pushed in `48de937`, `68ad79a` and
`aaf3caaf`. The integrated protocol checkpoint is `41cf664435530e6db41509d73e4217674b18f19d`.
Its mandatory style and formatting passed. The complete Source run found 69 failing bodies/certificates in 3,254 checks.
Checker units passed 415. Lowering passed 37 and failed one stale instruction-position assertion. The combined Cargo run
stopped there, leaving interface/ABI suites unrun. Native/imported acceptance and comparative costs remain unrun.

A related contract/representation batch is pushed through `e82986f7d5d86c321cc7fc1afbac62d7c1adf1b6`,
with final self-review refinements and no additional test run. It adds the missing arena component
contract to the execution-domain borrow, uses address words for observational wait links, transports source claims in a
noncopy native admission record, and moves prepared startup rollback to consuming scope entry/exit so graceful closure
precedes destruction. Final cell recycling now moves the complete slot into a retirement packet, replacing individual
field-detachment helpers. It preserves actual arena/execution retention through disposal. Namespace/borrow/nullable
wiring errors from the first batch are addressed together. Worker formation now uses one decision enum including Grow,
rather than nesting an allocation-capacity continuation in Result. Existing complete publication/disposal is preserved. The lowering test now checks Main startup through
outcome/static/shutdown order, and the scalar-context matrix includes data equal to its storage identity.

The follow-on batch passed mutating style and explicit formatting and is undergoing a second integrated validation.
Follow-on checker/lowering/interface/ABI suites passed 415/38/35/29 respectively. The full runtime Source run is still
in progress. Its whole-protocol review covers
prepared-start scope contracts, whole-slot materialization, owned nullable extraction, physical effect framing across
runtime calls, nested entry view restoration and worker formation's active conditional guarantee. Do not turn failure counts into a sequence of local edit/test loops. Keep checkpoints pushed.

### Keep, adjust, replace and remove

| Area and files | Decision | Result |
| --- | --- | --- |
| `bray-lowering/src/host.rs`, generated frame cleanup and `activation/run/*` | Keep | Compiler-owned lexical broadcast/resolution precede terminal publication, with direct await inside one run |
| `host/product/{state,progress,cleanup,operations}.bray`, provider and attachment domains | Keep and adjust integration | One resident teardown state, fixed teardown set, dependency order, callouts outside host locks and incident drain |
| `execution/{data,context,owner,binding,lifetime}.bray` | Keep distinct execution owners, pins and validated binding | Root outcome and product cleanup retain actual services and genuine execution authority |
| `worker/context*`, `task/runtime_retirement.bray`, observer scratch output, per-provider operations | Keep complete publication/disposal units | Preserve worker rollback, task pin lifetime, report ownership and absent-service selection |
| `host/owner.bray` task fields and their cleanup | Move to execution-domain storage | Common host no longer owns concrete task storage or its lifecycle dependencies |
| Resident external-event name directory | Keep | Unattached native notification can route process-wide names without adding another registry |
| `task/{native,reference,retirement,runtime_retirement,waiting/*}.bray`, `event/{directory,registration}.bray` | Adjust all directory access together | Each operation reaches the arena through a retained execution domain, rather than an ambient host acquisition |
| `execution/{start,ownership,task_entry}.bray` | Replace staged loose startup values with a prepared-start owner | Reserved admission, slot, registration, binding and cleanup backing commit together; one rollback protocol |
| `task/{data,control,ownership,frame_retirement,execution}.bray` | Adjust tagged state | Terminal phase owns its actual outcome; retirement has one tagged status; genuine dispatch and source-claim state remain distinct |
| `execution_guarantees/condition/*`, `snapshot.rs`, guarantee flow/completion/ownership | Replace implicit observation-role encoding | One explicit observation record separates identity, contents and owned/reference/scalar role |
| Existing storage targets, regions, borrow capabilities, witnesses, deferred entries and checked dependency contracts | Keep and align consumers | Reuse established semantic owners and one operation transition; no second witness/alias store |
| Typed transfer footprint, physical variant paths, captured input identities, provenance audits and native fixtures | Keep | Preserve justified fixes and accepted/rejected semantic distinctions |
| Temporary trace branches and diagnostic runner | Remove traces before implementation; remove local runner before finalization | No diagnostic mechanism becomes production architecture |

### 1. Explicit compiler observations

Introduce a small immutable observation record in `execution_guarantees/condition/` with separate identity, current
contents and role fields. The closed role is owned value, reference observation or scalar storage observation.
Replace `CurrentValue` and `StorageValue` encodings with one observation variant. `Borrowed` continues to denote a live
predicate/access view; it must not double as a hidden role tag inside another observation's contents. A reference's
value identity and access capability remain retained independently of its non-owning reached data.

Update construction, projection, predicate normalization, const evaluation, substitution, equality rewriting,
observation indexing, ownership traversal, retirement, joins and portable-term rejection together. Reuse existing
`ExecutionState.current`, witness carriers and physical storage targets. Do not add a parallel context store or infer
freshness. Keep immutable copied/captured entry values separate from current storage versions. Keep existing correlated
nested-case ownership and finite block/place join identities. Extend the existing observation/join regression matrix
only where needed to protect these semantic distinctions.

### 2. Move task arenas behind the execution owner

Move `HostOwner.tasks` into an explicitly owned task arena retained by the execution domain. Reuse `task.Entries`,
geometric slab growth, generational handles and cell locks. The low-level native task-control protocol also forms cells
without an executor, so the arena has its own backing owner and lifetime. Each admitted slot retains the arena until
its final storage reference ends. The domain retains an arena owner and supplies its separate execution pin before
publishing an executable task. The existing resident `HostLease` remains the synchronization boundary for arena/list
mutation. `ArenaAnchor` borrows the actual arena reference and resident lease and provides scoped access to entries. This preserves synchronization without another lock allocation or acquisition. Common host
shutdown no longer references concrete task storage or its lifecycle.

Keep the resident external-event name directory. `EventSignal` explicitly supports notification without a native-thread
attachment, and event waits can retain tasks from different execution domains. Moving that directory into a domain and
resolving an ambient execution would break its contract. The retained directory contains event-name/reference records,
not an executor owner. Do not introduce another global execution registry.

Replace task-handle queue links with a small copyable `WaitLink` containing an observational stable-slot address word. `WaitRegistration`,
`Delivery` and event/task queue heads retain the existing complete list ownership protocols. A link is observational
and grants no storage authority. Scoped link access is introduced from an actual retained task, owned registration or
owned detached delivery together with the resident lease. All slot recycling and final arena extraction use that same
lease, preserving backing through the access interval. This supports cross-domain event delivery without global task
lookup, extra node allocation or another storage reference per traversal.

Rewrite wait admission to duplicate the actual retained child/parent references instead of reducing them to numerical
coordinates and looking them up again. `duplicate_reference` counts the existing live cell under its cell lock and
copies its actual coordinates/pointer. Delete superseded forwarding/coordinate helpers. Rewrite wait publication,
unlink, delivery and event registration as one intrusive-list protocol using the existing preadmitted membership slot.
Callbacks and registration disposal remain outside the resident acquisition.

Source task admission/retention helpers receive the arena reference explicitly. Native `TaskControlCreation` writes an
owned arena reference and returns its observational runtime identity. `TaskControlAdmission` and `TaskControlRetention`
borrow that actual arena through a raw input slot, and a consuming `TaskControlArenaRelease` ends the caller's arena
reference. Update catalog signatures, Source entry contracts and all native fixtures together. This replaces numerical
runtime-only lookup without a global registry or ambient execution assumption. Existing task reference C layout remains
unchanged. Reference-based operations use their actual cell. Domain retirement walks its arena. Source execution pins
and native context installation remain attached before an executable task is published.

Last storage release produces one retirement packet. Under the resident acquisition, move the complete final slot
into owned retirement storage and initialize its vacant row. Materialize the actual control, execution pin and arena
reference, then recycle the row through that arena and release synchronization. Dispose cell, metadata and reports before releasing execution and final arena retention. Runtime retirement reuses that same operation.
This preserves the useful existing complete retirement unit while moving its storage dependency to the right owner.

Create the arena during domain formation and retain its actual owner in the domain. On domain destruction release that
owner after all domain work resolves. The final arena reference extracts entries under the resident lease, destroys
backing outside synchronization and finally releases its allocation. This also supports retained low-level native cells
without a live executor.
Delete task swapping/destruction from `shutdown_host`. Keep private ABI names/signatures where their role contracts
suffice, changing owning declarations, metadata and callers together if a concrete boundary requires more information.

### 3. Complete admission and tagged task state

Keep the existing admitted/active/executing/terminal/resolved distinction where it represents real run state. Attach the
owned `ReportOutcome` to the terminal alternative instead of maintaining an always-present empty outcome sentinel.
Replace the `retiring`/`retired` pair with one retirement enum. Keep dispatch reservation and the sole source-resolution
claim independent because they have different lifetimes. Preserve the C inspection snapshot through explicit projection
from the new state, without exporting its internal representation.

Add `PreparedStart` beside execution startup. It owns the admitted task reservation, scheduler registration, selected
lane/initial requirements, actual execution binding and secured rollback resources until activation publication.
Preparation reserves admission under the domain's closure protocol and completes fallible provider/native installation.
Enter consumes the prepared owner into one scoped capability. Commit borrows that capability and begins the actual inactive frame, installs complete cell state and publishes ready
work. Scope exit withdraws an uncommitted exact registration and resolves the service acquisition once. It runs for every
body exit, including panic. Preparation forms the owner before fallible registration or worker demand. Final destruction
follows that graceful scope exit. Retire task/context/report ownership through their existing actual owners. Existing frame
ownership remains with the actual native input until activation accepts it. Root and ordinary task-start entries use
this same protocol. Preserve existing handle-based native source claims with validated introduction contracts.

### 4. Entry, outcome and host lifecycle integration

Separate source resolution claims from observational arena coordinates. Keep each cell's generation/index handle for
scheduler lookup and native storage inspection, always paired with its actual arena owner. A protected source task/root
handle is an opaque claim to the stable cell, introduced by the actual resolution owner. Its existing private word ABI
can encode the stable cell address because the claim's storage reference and arena retention prevent recycling. Add the
cell's immutable local index so a valid claim can reconstruct its inspected coordinate. Never use the claim address as
permission without its complete input contract. This removes ambient-domain lookup and cross-arena coordinate ambiguity
without a global registry or per-claim allocation.

Update root/task allocation, frame start, resolution, cancellation, observation and wake entries, native task-reference
handle projection, generated host assumptions and C fixtures together. Scheduler keys remain coordinates. Source claim
operations use the actual cell's retained execution binding; the current entry supplies physical execution authority and
restoration rather than selecting which task a numeric coordinate refers to. Borrowed wake access and the sole source
resolution right remain separate contracts.

Use the `Current` already owned by `EntryScope` throughout root start, observation and completion. Short helpers borrow
that view and its domain; they do not reacquire counted views. Keep the complete displaced execution owner and its counted view in the scope
until restoration. When a nested root installs a new context, retain its distinct operation view in the scope and drop
that view before retiring the nested context. Do not select the displaced parent view as the nested execution view. Keep native-provider and exact-thread attachment ownership through callback and incident disposal.
Change root APIs, resolution, driver suspension/resumption and worker-context call sites together where needed.

Retain the established host product cleanup engine. Verify the generated host order against frame completion,
outcome ownership, entry closure, domain cleanup and final service release. Make progress operations take an owned
reservation before leaving synchronization, and consume its outcome before committing progress. Keep blocked explicit
shutdown unresolved. Keep provider retention attached to callbacks/reports rather than relying on a running task flag.
BRA-507 retains the same admitted binding for suspending fallback and BRA-559 retains test-host policy.

### 5. Align the shared semantic transition and portable contract

Use the existing storage-flow operation collector and completion records as the one producer of physical effects and
reference/lifetime relationships. Preserve effect target, access type and scope through the shared survival query.
Pass its resolved observations to guarantee invalidation, borrowing and cleanup. Remove any superseded interpretation
introduced during observation migration. Preserve complete-carrier evidence through actual moves and retire it through
raw transfers that reach the matching owner. Storage retention does not prove an active conditional clause.

Export the same ownership transfers, reference regions, retained dependencies, normal guarantees and abnormal incident
dependencies used by Source consumers. Keep physical reference targets distinct from lifetime anchors. Unknown results
remain conservative without fabricating independence from absent inputs. Reuse checked contracts, existing imported
interface schemas and role validation. Extend persisted representation only for a concrete required semantic field,
with owning serialization/validation consumers changed together.

### 6. Remove superseded mechanisms and assess cost

Remove the old observation variants/role normalization, common-host task arena lifecycle, loose startup rollback paths,
empty terminal outcome storage, duplicate retirement flags and temporary diagnostics. Keep existing Source provider
bindings rather than introducing a host-wide dispatch table. Keep common orchestration concrete and specialize only
provider adapters. Check sync/minimal-async dependency selection from actual retained units. Measure production and test
code separately against the recorded baseline, with movement and formatting excluded from simplification claims.

### 7. Validate the completed migration and finish this PR

No build/test run occurs for each edit or in response to each failing expression. Finish the related ownership and
semantic migrations and self-review this checklist before mandatory `cargo xtask style` and `cargo xtask format`.
Use the existing workspace production feature graph and target. Run checker/affected lowering-interface checks, all
runtime Source bodies and certificates, and representative positive/negative capture, witness, union and raw-transfer
controls as a completed batch. A failure is assessed against this model and its operation class before choosing edits.

Then run the established runtime-artifact smoke workflow for Source/imported/native startup rollback, blocked-root
progress, genuine Main identity, compatible children, direct await, success/panic/cancellation, nested entry restoration,
cleanup-before-publication, retained cleanup access, reports and dependency-ordered shutdown. Audit authenticated native
units/archive members and link maps. Measure direct, synchronous and minimal async build/startup/size/retained-resource
costs using normal products and separate instrumentation. No new test runner is committed.

Push coherent checkpoints throughout. After implementation and required verification are complete, use the required final
reviewer on PR #710, address real findings as batches, then mark the PR ready and Linear In Review. Never mark Done.
The branch is one PR throughout the migration. No follow-up issue or permission checkpoint is needed for this plan.

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
   specification before relying on an assumption or running tests. Select work from this architecture and protocol plan.
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

Checkpoint `41cf664` passed mandatory mutating `cargo xtask style` and explicit `cargo xtask format`.
`cargo run --release --bin bra567 -- runtime` completed with 3,254 checks and 69 failing bodies/certificates.
`cargo test --release -p bray-checker -p bray-lowering -p bray-runtime-interface -p bray-runtime-abi --lib`
passed 415 checker units and 37 lowering units, then failed the async host's obsolete fixed-position assertion.
Later crates were not run in that first command. The follow-on complete unit command added `--no-fail-fast` and passed
415 checker, 38 lowering, 35 runtime-interface and 29 ABI tests. Follow-on style and explicit formatting passed.
The follow-on full runtime Source run is in progress. All native/imported acceptance and cost measurements remain
pending. Do not start final review before required verification is complete.

Pre-refactor checkpoint `7c7a70ba7c0ff5d153c554bfc49bd926efc5d665` passed 415 checker units and all 16
representative Source controls. Full runtime Source checking had 3,160 checks and 22 failures. That evidence establishes
neither current refactor correctness nor end-to-end acceptance. Native/imported acceptance and comparative costs are
unrun. Preserve the positive/negative controls for real/fabricated owners, correlated union carriers, immutable captures,
conditional promises, scalar/pointer raw transfers and mutable exposed markers.

The explicit observation model reuses the existing physical storage-flow transitions, checked owned-result transfers,
reference regions, typed effect scope, active/stored loan survival and portable dependency contracts. Internal observation
snapshots are not persisted. No new interface field, witness store, alias store or freshness rule is introduced.
Compiler/lowering/interface checks and Source/imported runtime builds must verify this agreement.

The current whole-protocol self-review separates mutable intrusive membership from retained wait resources. An actual
`WaitResources` carrier owns endpoints/callbacks. Link address words grant no physical access. Their scoped producers
reconstruct a pointer only with the actual retained collection or detached chain and current lease. Neighbor access also requires a current membership witness under the
actual host lease. Popped and withdrawn registrations have cleared links. `Delivery` owns its remaining detached chain.
Each slot retains its own arena. Final cell retirement moves the complete slot into one owned packet before recycling,
then disposes metadata/reports/control before execution and arena release outside synchronization. The domain's arena
retention supplies its resident host lifetime transitively, avoiding a duplicate host-domain count.

Root startup and completion use the same prepared-start and entry-scope protocols as ordinary tasks. A prepared start
retains actual execution admission through commit or rollback. The entry scope keeps a displaced parent view for
restoration and, only when a nested root installs another context, a distinct operation view. It releases that operation
view before retiring its context. Product detachment transfers the actual execution owner into pending progress before
calling providers, then returns it under the host lease. These are inspection conclusions awaiting execution evidence.

Temporary production traces are removed. The untracked local runner under `xtask/src/bin/` uses the workspace production
feature graph and existing target, and is excluded from checkpoints. Remove it before finalization. Optional resource
instrumentation runs separately from normal product builds. Existing runtime-artifact smoke and provenance audit tooling
must produce Source/imported/native, minimal-demand and build/startup/size receipts.

Connected GitHub account `lejmer` has admin/push access to the public `lejmer/bray` repository. Shell GitHub
authentication is invalid. Use connected Git object APIs for checkpoints on existing PR #710. Parent #709 remains
open at the unchanged stack base. Push coherent checkpoints regularly, as requested by the user.

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
