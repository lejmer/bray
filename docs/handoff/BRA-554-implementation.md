# BRA-554 implementation record

## Objective and baseline

Complete synchronous/source and foreign entry authority in Bray, then continue BRA-558.
Branch `feature/bra-554-implement-synchronous-and-foreign-entry-authority-in-bray` starts at
`f5e00436`, the reviewed BRA-502 tip in [PR #608](https://github.com/lejmer/bray/pull/608).
BRA-502 is In Review. Its final [published review](https://github.com/lejmer/bray/pull/608#pullrequestreview-5528576171)
has no remaining actionable findings. The complete native workflow and final-link storage checks passed.
BRA-554's PR must target the BRA-502 branch. BRA-558 follows this branch without waiting for merges.

## Behavioral requirements

Acquire entry and exact-thread ownership before source execution, serialize entry with closure,
and preserve inputs on rejection. Admit bounded outgoing records before arbitrary callbacks.
Restore the enclosing context on nested normal and abnormal exits. Keep locks out of callbacks
and disposal. Preserve checked cleanup and returned-owner dependencies. Return the native zero
representation on failure without native unwinding. Identity observation supplies no authority.
Do not create or retain a scheduler merely to inspect context.

## Existing implementation and concrete gaps

| Obligation | Existing owner and evidence | Required work |
| --- | --- | --- |
| Resident storage and exact-thread epoch | Bray host/attachment owners, production forgery tests and native bootstrap smoke | Reuse enter/leave and checked acquisition contracts |
| Concrete outgoing storage | BRA-502 pool, storage bundle and typed incident lease | Reuse actual records and backing, no second allocator or scalar admission |
| Entry/closure serialization | Product host registry control and its existing closure tests | Make generated entry consumers acquire/release the established product contract |
| Nested context | Rust native state and context scopes, restoration tests | Clear the complete native execution context, then restore it |
| Synchronous/foreign policy | Rust native callback boundary and adapters | Switch actual roles to Bray and remove replaced Rust entry policy |
| Foreign abnormal result | LLVM native trampoline and body callback | Preserve the body's checked panic outcome and zero its ABI result on failure |
| Native function/provider validity | Existing role binding, product formation and descriptor validation | Validate at the boundary, reuse results downstream |
| Returned synchronous owner | Existing checked result transfer, storage lease and inferred dependencies | Preserve actual ownership through native transfer, never replace it with an entry count |
| Remaining Rust execution contexts | Task execution and native test-host consumers | Keep any required context bridge restricted to those consumers |
| Typed incident consumption | Existing report consumer, followed by BRA-558 | Share the same incident owner and preserve failure ordering |

The foreign trampoline currently calls the boundary without acquiring a product entry. Its
callback helper also copies every body parameter from the native argument state and overwrites
the outcome with completion, which needs correction for the hidden panic context.
The independent-context helper clears only the task, leaving its runtime visible.
Product formation currently retains a runtime unconditionally, including synchronous products.

## Design hypothesis and removals

Bray owns admission, attachment lifetime and callback sequencing over the resident host.
Generated adapters supply the already checked callable and native result transfer.
Existing product control remains the formation/closure contract while BRA-555 owns its complete
provider migration. BRA-557 owns full attachment membership and static cleanup. BRA-507 owns
suspending disposal. These boundaries do not excuse a failing synchronous entry requirement.

Reuse the existing record service and typed native outcome. Any bridge for still-Rust task or
test-host context must use the admitted records and remain absent from ordinary synchronous
products. Remove the old synchronous and foreign Rust entry exports and their policy after switching
consumers. Preserve native-thread execution until its owning migration.

No new syntax, language feature, erased returned-owner protocol, witness registry, second host
registry or downstream semantic validation is planned. Keep changes in existing owning analyses
and runtime services. Reconsider the design if it needs duplicated admission, per-entry heap
allocation for ordinary synchronous calls, or another interpretation of checked lifetime facts.

## Acceptance matrix

Cover source and imported functions, direct and indirect native results, cold and nested entry,
entry rejection before source access, recursive entry, concurrent closure, body panic and cleanup
failure, returned synchronous owners, and lost/fabricated authority. Verify native zero results,
ordered incidents, exactly-once release, provider/backing validity, and restoration after failure.
Reuse the existing production declaration and native smoke facilities. Include real generated
foreign trampolines, not only runtime callbacks. Record TLS/atomic/bridge costs and link-map
retention for a tiny callback and ordinary synchronous entry.

Run batched focused checks after coherent edits, then required native checks, style and format.
Audit correctness and structural cost separately. Publish a stacked draft PR, obtain an independent
published review, fix findings, and ready it only after acceptance is complete.

## Outcome and context checkpoint

All native callback bodies now receive the explicit panic/cancellation destination. The generated
C invocation helper supplies that hidden argument separately from native arguments and publishes a
result only on completion. Native entry addresses still expose the original C signature.
The independent Rust execution scope now saves and clears both the runtime and task, while an
explicitly bound cleanup runtime keeps its deliberate binding. Both scopes restore their callers.

Evidence:

- The generated native scalar/aggregate test covers normal return, cancellation and panic. Both
  abnormal cases return zero. It fails immediately if native panic/cancellation propagation is used,
  and checks exactly two panic observations and balanced cleanup shielding. The isolated C provider
  tests ABI outcome transfer, not admission or production report ownership.
- The product code-generation group passed 116 checks. Its one failure is the retained BRA-619
  `MissingRole(CleanupIncidentTransfer)` finding, still enabled for Step 6. Log: `bra-554-codegen.log`.
- Complete outer-context restoration passed on normal return and native unwind, including runtime,
  task, lane, cancellation and output. Log: `bra-554-context.log`.
- `cargo xtask style` and `cargo xtask format` passed. Only existing module-size warnings remain.
  The changed Rust files also passed the installed Rust formatter and `git diff --check`.

The callback assertion in the shared mapping helper also covers existing native export/import and
cross-target checks. The two Windows native caller tests now share their compile/link/run helper.
No source entry migration or BRA-554 completion is claimed by this checkpoint.

## Entry migration constraints established by the audit

The native callback has already transferred its typed result when it returns to the entry service.
A later entry-cleanup failure cannot justify erasing that result without its selected cleanup.
Keep body completion and terminal host incidents distinct, as specified for static/thread terminal
cleanup. Body panic and cancellation must still produce ABI zero. Reporting a consumed root panic
must not return a second owning copy of its report.

Product formation currently retains a runtime for static cleanup and worker attachment coordination.
An optional-runtime edit was investigated and removed: removing retention alone leaves synchronous
cleanup without the attachment/context contract previously supplied by that runtime. Replace those
operations together when switching entry consumers. Do not add a fallback that invokes cleanup
without the required attachment.

The remaining implementation must provide the Bray entry owner, product acquisition/closure,
pre-admitted outcomes, exact attachment lifetime and an optional bridge for callers with Rust task
or test-host context. That bridge must contain only Rust context management and native Rust unwind
conversion, with admission and attachment policy owned by Bray. It must be absent from ordinary
synchronous products. Production authority, returned-owner, closure, report-order and cost evidence
remain outstanding. BRA-558 follows after this issue and Step 5 remains active.

## Direct invocation review repair

Published review 5529042829 identified that C-ABI source bodies received an explicit outcome but
source callers still excluded them from abnormal cleanup edges. Invocation mechanism now belongs
to the bound declaration signature, independently of the callable value's native ABI. Selection,
MIR and exported semantic/executable records retain that result. Code generation consumes it.
Native runtime/platform bodies and foreign imports retain their native convention, even when their
semantic contract can fail. The shared native-entry role classification lives in runtime-interface.

Unnamed C bodies and C lambdas receive native-address adapters as well. An anonymous body must not
inherit the enclosing declaration's export or platform role. The native regression exercises source
and imported nested calls, native callable values, anonymous bodies, scalar and aggregate results,
panic/cancellation ABI zero, and exactly-once caller destruction. Its C providers isolate invocation
and cleanup transport. They do not establish production report ownership or entry completion.

The audit also found ordinary runtime requirements lack executable lowering. That independent
operation-emission work is [BRA-623](https://linear.app/bray-lang/issue/BRA-623), an explicit BRA-608
blocker. Do not equate semantic fallibility with a hidden native argument or remove fallibility to
conceal this missing implementation.

Before source entry migration, repair these production authority boundaries:

- Replace unrestricted identity-to-exit construction with an acquisition owner. Native attachment
  retirement needs its explicit opaque boundary contract. Reentrant code must not retire an outer
  entry merely by observing its thread identity.
- Protect report-chain ownership independently of the copyable C transfer header. Numeric handles
  and initialized header memory do not authorize chain consumption. Paired mutable headers need
  non-overlap, and source owners need checked whole-state witnesses.
- Retain the optional Rust bridge provider through all entry teardown and restore the previous
  execution context after nesting. Invoke outside host locks and avoid pointers into the growable
  attachment array across callbacks. The continuation record below settles installation lifetime.
- Populate Rust panic backing in the Bray-admitted records before arbitrary Rust callbacks. Transfer
  the chain exclusively instead of admitting a second chain. The bridge surrounds the complete Bray
  entry continuation, including admission and retirement, so teardown cannot observe the outer task.

These are BRA-554 implementation obligations, not completion claims or new language proposals.

### Invocation repair verification

The full product-codegen group passed 116 tests, with only the retained BRA-619 failure.
The checker, codegen, lowering and package-interface groups passed 321, 67, 39 and 190 tests.
Logs are `bra-554-invocation-codegen.log` and `bra-554-invocation-core.log` in the shared
`target/bra-620-authority` directory. The final source/imported native regression passed after
signature consolidation. Its current expectations include five panic observations and three
caller destructor executions, with cancellation and balanced shielding checked separately.

The native-reuse mapping regression passed and retains distinct body and callable addresses.
Its artifact bytes are a planning fixture, so it establishes symbol contracts and does not claim
native-reuse execution coverage. Imported native adapters now contribute native symbol roots.
Native name conversion is shared by demand, link and static-host selection.

The audit traced declaration classification through call selection, MIR, semantic/executable
serialization, named/anonymous realizations, native callable values, and imported native reuse.
No new language syntax or runtime state was introduced. The additional stored enum records the
invocation distinction that native ABI alone cannot express. Required style and format passed,
and the installed Rust formatter and `git diff --check` completed without errors.

## Source attachment ownership checkpoint

The source acquisition now returns a noncopyable `host.attachment.Owner`. Its whole-value
`acquired` witness is required by destruction. Inactive owners exist before attachment publication,
and only a local verified-total commit changes their status and exact identity. The native entry
adapter transfers that owner under its existing opaque native-boundary requirement. Its local
transfer operation cannot escape the boundary. Native leave and explicit exit-scope construction
now require native release authority. Observing an identity or acknowledging an internal module
cannot acquire those contracts.

The owning storage analysis was missing whole-value observations for reference-free aggregates.
It already preserved their individual scalar fields across an opaque call that could not reach
them, but lost an opaque predicate over the aggregate. The existing cached shape traversal now
records whether the complete value contains only direct data. Existing reachability analysis
selects unexposed complete values along with scalar fields. Downstream witness analysis consumes
that result unchanged. Borrowed, raw-pointer, protected and unknown components remain conservative.
No runtime metadata or new language feature is needed.

Anonymous callable contracts also omitted referenced declaration types and constant inputs.
Anonymous and catalog clause checking now reuse the existing declared-value binder for references.
Anonymous normalization consumes the established constant inputs, as named and catalog clauses do.
Production cleanup uses the named status constant again.

The test audit replaced source-position assumptions with declaration-derived source identities.
Two old fixtures expected unrelated opaque calls to revoke unexposed scalar evidence. Completion
coverage now accepts that case and rejects mutable exposure. Deferred execution coverage observes
an atomic payload, whose opaque mutation must still revoke its witness when driven.

Checkpoint review matrix:

- Complete-value stability: scalar and nested tuple owners survive unrelated calls. Mutable
  arguments and escaped addresses do not. Borrowed, raw-pointer, atomic and uninitialized payloads
  do not acquire this proof.
- Production authority: acquisition succeeds, forged owners and copied fields fail, identity
  mutation revokes authority, and source callers cannot invoke native enter/leave contracts.
- Native lifetime: retain recursive-entry and allocator-failure smoke coverage. Active source
  owner destruction, entry closure and optional Rust-context bridge acceptance remain outstanding.
- Contract binding: qualified constants in anonymous requirements check through the shared binder.
  Existing named, catalog, imported and trusted-contract coverage must continue to pass.
- Structure: cached type traversal supplies the additional observation. No second reachability
  pass or downstream witness validator was introduced. Runtime-role binding checks moved intact
  into their existing owning module to keep symbol assembly within the enforced function limit.

BRA-554 remains incomplete. Report-chain source ownership, entry consumer migration, the optional
Rust-only context bridge, product closure integration and their native acceptance follow this
checkpoint. BRA-558 has not started. Step 5 is not complete.

## Native ownership and protocol audit

The retained native smoke exposed an asynchronous root regression in the previous context
isolation checkpoint. Clearing and restoring the runtime around root creation discarded the
runtime owning the returned handle. Root creation now retains that runtime and uses the existing
task scope to restore the enclosing task. Synchronous and foreign entry still isolate both fields.
The existing suspend/resume regression passed. No additional context representation was added.

Report consumption also recursed through ordinary foreign-entry admission. With further admission
denied, consuming the admission failure recursively tried to admit another callback. The existing
ReportConsumer role now binds the actual consumer instead of a pointer getter. Native callers
retain that function directly. Its reporting selector is an explicit u32 across Bray, Rust and C.
The two retained message callbacks likewise have explicit native report roles. These operations
consume their existing protocol ownership without fresh entry admission. The 54 native runtime
tests passed after the consumer repair, including denied-admission cleanup. Final verification of
the subsequent protocol restrictions and message bindings is pending.

A production-declaration probe proved that safe internal callers could obtain the resident pool
lock and overwrite credits and free-list state. An explicit opaque protocol requirement now gates
the raw pool operation and its transitive record, storage and message consumers. This permission is
supplied by native protocol boundaries, not by header initialization, numeric handles or internal
acknowledgement. Source acquisition wrappers must expose actual owning values rather than publish
that permission. The negative probe now rejects the mutation. Header admission and fabrication
probes cover the same restriction. Existing HostLease tests already reject equivalent resident
host mutations and remain enabled.

The protocol propagation exposed another over-invalidation: established opaque predicates with no
mutable observations were erased by unknown execution. Contract reasoning explicitly disallows
hidden mutable ambient dependencies. The existing observation traversal now handles its empty set
correctly, and cleanup retains established observation-free facts. This never introduces a fact.
The regression covers both zero arguments and literal arguments, with corresponding unproved
calls rejected. Mutable arguments and protected payloads retain the negative coverage above.

The corrected trusted-contract suite passed all 224 tests. Storage admission tests declare their
native protocol prerequisite. Task-cleanup invalidation observes an atomic input and starts its task
inside an async callable. Both retain their original negative obligations. Step 5 remains active.

## Reentrant pool growth

Native bootstrap execution exposed a deadlock when the allocator reentered attachment acquisition.
The source attachment owner reserves cleanup records before publication. The old record admission
called the allocator while holding the pool lock, so recursive admission tried to acquire the same
lock. This is a concrete BRA-554 entry-composition defect.

Record slabs, their indexes, cleanup shapes, storage slab indexes, and typed storage regions now
prepare owned capacity outside the lock. The existing metadata-buffer operation installs prepared
capacity and leaves replaced backing with the caller. It no longer allocates or frees under the
lock. Each retry reacquires the pool and rechecks current capacity. A partial bundle rolls back its
credits before requesting more capacity. No pointer into a growable index crosses the unlock.
Unused or replaced buffers are destroyed after unlocking. Pool shutdown also detaches free backing
before destruction and notices capacity created by reentrant admission.

`pool.growth.Request` describes the five concrete allocation requests used by these existing
consumers. `Prepared` owns the six buffer kinds, including paired bytes and slot metadata. These are
internal admission state, not another allocator or a public admission framework. Geometric growth,
record ordinals, storage epochs, and the shared pool remain unchanged. The former allocating
`heap.reserve` and in-lock storage-release interpretation have been removed.

The native acceptance now distinguishes successful balanced allocator reentry from rejected key-
construction reentry. Key-creation failure must leave identity and key state empty, and shutdown
must reclaim all unused backing while allocation is denied. Registration rejection reserves the
available record capacity before denying allocation, so the check does not depend on a cold pool.
The deallocation reentry check exercises empty admission while backing is being released.

Evidence before the final deallocation probe:

- `bra-554-pool-growth-contracts.log`: 224 trusted-contract tests passed.
- `bra-554-protocol-core.log`: 321 checker tests, the cross-target consumer ABI test, and production
  pool mutation/header-forgery probes passed.
- `bra-554-pool-growth-native-acceptance.log`: the complete native workflow passed, including
  asynchronous, synchronous, bootstrap and observation smoke products and their link-map audits.
- The bootstrap startup sample used six allocations and 672 bytes. The two 65,537-byte cleanup
  payloads with 4,096-byte alignment used nine allocations and 148,280 bytes including metadata and
  alignment slack. These are smoke observations, not comparative final entry performance claims.

The 54 native runtime tests passed in `bra-554-native-protocol-final.log`. The final deallocation
reentry probe and full native workflow passed in `bra-554-pool-reentry-native.log`.

The maintainability audit also found the same prepared-buffer installation in host attachment
growth. Both consumers now use the existing heap module, including its verified-total installation
operation. Optional metadata buffers add only the first-allocation case. The complete native
workflow passed again in `bra-554-shared-growth-complete.log`. `cargo xtask style`, `cargo xtask
format`, the installed Rust formatter, and `git diff --check` passed. Existing module-size warnings
remain. Published review of this checkpoint is pending. The remaining entry/report ownership and consumer migration above
remain required. BRA-554 and Step 5 are not complete.

The checkpoint audit reread AGENTS.md and searched the compiler/runtime/fixture consumers for
whole-value observation, observation-free facts, declaration type binding, root-context restoration,
report consumer/message ABI mapping, and prepared capacity installation. Binding and observation
reuse the owning analyses. Native report callback identities remain in the closed role catalog.
Root creation reuses the existing task scope. Prepared-buffer publication now has one heap-module
implementation shared by attachment and pool growth. The split pool root contains only its module
declaration. No new language feature, downstream validator, allocator, or scheduler was added.

## Entry ownership continuation

Published review [5532070959](https://github.com/lejmer/bray/pull/609#pullrequestreview-5532070959)
found no actionable defects in `bfb66419`. Its checkpoint-only scope was verified on GitHub.

Attachment acquisition is being restricted to the native entry protocol. Its scalar identity is
not an inferred exact-thread capability. The trusted synchronous implementation must keep the
owner local, invoke the native continuation on that same thread, and retire before returning.
Only the explicit native enter/leave protocol transfers that obligation to a native caller.
General source code cannot acquire or escape this owner. No thread-local bootstrap marker or
new dependency representation is needed for this closed entry implementation.

Report-chain ownership is being represented by a noncopyable source owner with an unchanged
whole-value witness. Raw copied headers remain unauthorized. Admission secures the owner cleanup
obligation before publication and commits the actual returned chain without another fallible step.
Disposal reuses the existing report consumer loop. A borrowed owner cannot be passed through a
fallible mutating operation and then rely on normal-return postconditions for abnormal cleanup,
so this source interface exposes admission and destruction, not a fallible mutating finish API.
The owner implementation and production-declaration regressions are in progress.

The 35 production host-contract tests passed in `bra-554-entry-owner-contracts.log`, including
source report-owner admission/destruction and forged, copied, mutated and cross-paired headers.
Subsequent attachment retirement and bridge edits still need their final checks.

The process-lifetime bridge hypothesis was rejected during read-only design review. A stored
function pointer and Rust `Once` do not retain a loaded provider. The replacement uses an optional
callback on the existing exact-thread attachment. The native scope acquires an actual attachment
depth, exchanges the callback, restores the previous callback, then releases that depth. Observing
a current thread is insufficient. Different nonzero provider callbacks are rejected because their
independent Rust TLS contexts have no established composition contract. Registration is confined
to native provider scopes and copied code addresses never retain an attachment-array pointer.

Source integration exposed inconsistent discriminant invalidation. Storage-flow absence already
preserved an unexposed nullable/union descriptor across opaque writes, while positive pattern
refinements discarded that same proof. A shared discriminant rule now consumes the existing
prepared footprint and physical invalidation result. Arbitrary predicates over reached mutable
contents keep their existing invalidation. The original raw-write-then-projected-assignment shape
has a focused regression. This repair and the new entry integration remain under verification.

Entry ABI integration will use `(callback, context, caller-owned outcome) -> terminal status`.
The caller initializes the outcome to runtime failure, and the generated body callback publishes
completion or panic directly into that destination. The result payload already stores the result
address, so it cannot carry terminal status. Existing host status can account for terminal failure
after normal result resolution. Keep completed body results intact. Append hidden entry cleanup
panics after an existing body panic using the existing outcome merge operation; do not report the
cleanup panic first. The foreign caller must report terminal incidents even when it returns a
successful body result. Returned dependency survival still needs the acceptance audit.

Current integration work is uncommitted. The new source entry roles and generated callbacks use
caller-owned outcomes and return terminal status. The replaced synchronous/foreign Rust exports
have been removed. Rust-only unwind probes now call the actual source providers through a test
adapter, while native-thread execution remains in its existing owner. Scoped registration is wired
to native task resume and retained cleanup execution. Test-host registration/timeout/output,
product closure and synchronous cleanup without scheduler retention still require completion.
Do not interpret this checkpoint as passing production acceptance or a completed BRA-554.

## Integrated entry validation in progress

The complete trusted-contract batch passes 226 tests in
`bra-554-entry-contract-fixed.log`, including actual host/entry declarations and the
nullable-borrow discriminant regression. Shared codegen tests pass 68 tests. The LLVM
fixtures now declare the selected atomic wait/notify roles. The combined core batch
passes 68 codegen, 93 LLVM, 44 IR, 39 lowering, 190 package-interface, 18 ABI,
28 runtime-interface and 18 test-protocol tests in `bra-554-entry-core-batch.log`.

Foreign export planning now supplies the existing product descriptor even when there
are no statics. Generated entry acquires the product before calling source and releases
it after outcome handling. Product-control emission is shared with root shutdown.
The generated boundary implementation is split by foreign-call and closed runtime ABI
responsibility. Completed results retain ownership independently of terminal status.
A structured entry-failure report represents the otherwise unreturnable foreign terminal
status. Its native consumer is explicit, rather than an inert copied report header.

Cold synchronous product formation no longer creates or retains a scheduler. Async
finalizers and exact-thread cleanup on an existing scheduler retain their required
runtime. Attachment setup failure preserves pending cleanup owners for retry. Actual
thread retirement uses its established cleanup borrow and rejects nested source entry.
Full multi-runtime attachment membership remains BRA-557's existing ownership.
The product-control implementation still references scheduler code for async consumers.
Do not claim that this issue removes that machine code from every static product.

The test session owns its bridge until cleanup and result publication. Runtime bindings
distinguish the initialization owner from borrowed worker/cleanup bindings, so restoring
an outer context does not leak an inner initialized runtime or release an outer owner.
These runtime changes and generated native acceptance are still under validation.
No Step 5 completion is claimed. BRA-558 remains next after BRA-554 review.


## Product entry ordering

The executable root previously materialized statics and ran source without holding a
product entry. Concurrent closure could therefore start cleanup during root execution.
Lowering now emits `BeginProductEntry` after required runtime initialization and before
static materialization. The existing host sequence contract checks that order. Codegen
holds the established product admission through result resolution and releases it before
`FINISH_ROOT`. Products with no descriptor need no admission call. Shared product-control
emission is reused by root and foreign entry. All closed MIR inventories include the new
operation, with no additional semantic analysis pass.

The generated scalar/indirect callback regression passes for local and imported source in
`bra-554-foreign-native.log`. Root ordering passes in `bra-554-root-order.log`. Static
libraries deliberately leave final-image descriptor ownership to the final host. The C
acceptance driver supplies that descriptor, as a Bray executable host does for a Bray
final image. Runtime and complete native fixture validation remain pending.


## Terminal ownership audit

The callback returns unused admitted records. Their empty Rust backing slots are not
incidents, so they remain silently consumed. Rendering those records would invent
panics. Source callback contracts already require returning only this unused chain.

Entry adapters now distinguish a terminal panic already transferred or consumed from
an unrepresented failure. The foreign wrapper emits no duplicate summary for the former.
The optional Rust context bridge admits its retirement record before callback entry,
retains exact shutdown panic details, and uses the shared outcome append operation.
A completed result remains owned by its caller. Existing body panics precede appended
retirement panics. Tests cover both direct scalar and indirect aggregate results.

Existing product/TLS incident sinks still render during their own cleanup. Global output
ordering and migration of those consumers belong to BRA-558, including its typed-error,
formatter failure and denied-allocation acceptance. BRA-554's entry ownership evidence
must not be described as completion of that report-output contract.

The native bootstrap audit found five internal helpers incorrectly marked C ABI:
`consume_pointer`, `recycle_reservation`, `take_report`, `heap.allocate`, and
`heap.deallocate_bytes`. They have only direct Bray consumers. Their C annotations were
removed, eliminating unused foreign-entry wrappers and their product-host dependency.
This preserves actual native callback roles, including the report consumer and message
copy/release callbacks. No fabricated final descriptor was added to the runtime tests.

The generated C entry fixture warms 1,000 calls, then reports C clock seconds for
100,000 checked `bray_entry_value` calls. The sample includes C/Bray boundary work,
product admission/release, source attachment/report ownership, the tiny body and its
identity assertion, and C result checking. It is an absolute smoke measurement, not a
Rust comparison or an isolated instruction-cost benchmark. The workflow accepts only
that measurement line, rejects stderr, and forwards the raw count/time to its log.
The cold-product runtime test separately establishes that synchronous entry cleanup
creates no scheduler. The retained Rust `ProductHostControl` graph still includes async
cleanup support. This timing sample makes no scheduler-code-size elimination claim.

The synchronous fixture also times 100,000 warmed source root entries with an unchanged
callback and checks each outcome. Its Rust `Instant` sample is elapsed time. The foreign
sample uses C `clock()`, whose interpretation depends on the platform CRT. Record it as
`clock_seconds`, not a portable CPU-time claim. These are separate absolute observations,
not an equivalent-program comparison or a throughput gate.


## Final lifetime and retirement audit

The complete runtime batch exposed a TLS teardown ordering defect. Foreign attachment
storage now initializes the generic cancellation/task/output context, native context,
selected host callbacks and thread-static registry before publishing an attachment owner.
An owned runtime registers its retirement guard after that storage. Implicit thread exit
therefore releases the runtime owner before its attachment and context dependencies.
Repeated runtime initialization uses the same order. Existing task/frame admission backs
cleanup failures; this change adds no report allocation or fallback error conversion.

The first corrected runtime batch passed 163 tests and exposed two remaining cases.
Product closure returned immediately from `CLOSING`, preventing a retained cleanup owner
from being retried. Closure now resumes pending work in both open and closing states.
The fresh-thread regression now uses an actual registered thread-static finalizer to
inspect cleanup contexts, rather than attempting to replace the canonical exit callback.
The final combined batch is pending below.

The audit removed the obsolete optional-runtime cleanup path, its lazy initialization,
owned shutdown guard and forwarding helper. Every caller already retains its required
runtime. The remaining cleanup binding reports the actual bridge retirement status.
Equivalent native product emission and bundle directory construction share their existing
owners. Closed MIR, runtime-role, ABI, serialization and diagnostic inventories were
searched across the repository. No new downstream analysis or source syntax was added.

`bra-554-terminal-source.log` passes the generated EntryContinuation native regression.
A cleanup panic preserves a completed result, or follows an existing body panic through
the existing suppression adapter. Both cases return represented terminal status without
duplicate output. An entry admission failure already carried by the body outcome now
returns clean terminal status, as the Rust context bridge does.
