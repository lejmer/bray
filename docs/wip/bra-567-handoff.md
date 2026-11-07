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

## Architectural completion

The runtime protocol and ownership representation are being completed together. The latest validated checkpoint
is `e12e722f9867f8cc959b7e6a0d56cf3f4db63e1c`: 520 affected unit tests and all 18 Source controls passed, while
the full runtime Source run finished 3,264 checks with 69 failures. Native/imported acceptance remains unrun.
The reconstruction audit and integrated replacement below supersede local repairs. Its production code compiles,
style and format pass, and the affected unit batch passed 1,093 tests with two new interface fixture setup failures.
Those fixtures are corrected and awaiting verification. Full Source and native acceptance remain pending.
Complete the whole acceptance contract in this PR.

### Ordered implementation and removals

1. **Make execution closure a consuming transition.** An `Owner` always owns admission and a physical execution pin.
   `close_owner(Current, Owner)` consumes that admission and returns the remaining `Reference` pin. It does not leave
   an `Owner` whose meaning depends on a negative service predicate. Represent a binding's open-owner and cleanup-pin
   alternatives with a tagged union. Close callbacks consume an open owner and return a pin. Release callbacks consume
   a pin. Update driver closure, native owner destruction, prepared-start rollback and nested pin bindings together.
   Delete the optional-close/negative-predicate encoding and `take_binding_owner` conditional extraction.
2. **Transfer retired task resources at their physical ownership boundary.** Under the resident lease and final-reference
   authority, extract the actual control, optional execution pin and arena reference into `RetiredCell`, then publish the
   vacant row. The trusted extraction has one complete result contract for those independently owned fields. Do not first
   introduce an opaque witness for a whole `Slot` and then partially move it. Delete `retired_slot`, its separate
   initialization premise and `materialize_retired_slot`. Keep one retirement operation for native and execution-domain
   retirement. Dispose control and incidents before execution and arena release outside synchronization.
3. **Finish startup and entry scopes as ownership protocols.** `PreparedStart` owns service admission and the exact
   uncommitted scheduler registration. Its task reference and inactive frame are borrows from the native input owners,
   which remain live until activation accepts the frame. Commit transfers the registration only after complete cell
   publication. Every other exit withdraws it. Consuming scope exit closes admission and releases the returned pin.
   Use consuming `EntryRestoration` scope entry/exit around `EntryScope` so root execution, observation, completion and
   result transfer restore their displaced context on abnormal exits too. This scope owns the entry and borrows its
   completion-status destination so cleanup can preserve the existing status result. It allocates no storage.
   Native RAII storage retains explicit `finish_entry_scope` because its
   owner is outside Source. Preserve the distinct parent and nested-operation views and release the latter first.
4. **Make wait mutation use a stable access interval.** `WaitResources` owns retained endpoints and callbacks. Linked
   membership under the resident lease authorizes neighbor access. Derive the necessary current/previous/next accesses
   before changing membership, then update links and transfer or clear the registration as one operation. Delivery owns
   its detached remainder. Do not use the registration's former links as evidence after unlinking or mutate the resource
   owner to encode membership. Keep observational addresses, actual retained collections and callouts outside the lease.
5. **Complete general compiler transport, without runtime exceptions.** Keep the checked storage-flow collector as the
   authority for physical targets, effect kinds, access types and scope. Keep current storage observations, immutable data,
   whole carriers and lifetime roots distinct through its consumers. Review and correct the complete pipeline together:
   effect inference/export and instantiation, physical survival, guarantee invalidation, consuming moves, conditional
   carrier transport, joins, implicit cleanup and portable result contracts. An effect on a pointee must not become an
   effect on its lifetime anchor. A contents update must not silently retire an independent complete owner. A real raw
   ownership transfer must retire every overlapping matching carrier. A branch's carrier availability cannot establish
   an unproved active clause. Use the existing finite place/block summaries and shared immutable metadata. Replace any
   duplicate interpretation found here rather than adding another alias or witness store. No absence-of-input freshness,
   harmless-unknown rule, trusted-body exemption or runtime-name recognition is permitted.
6. **Validate and finish the whole composition.** After the migration and self-review, run mandatory style and format,
   the affected unit suites, all runtime Source bodies/certificates and the representative positive/negative Source
   controls as a batch. Classify any failure against a violated transition before changing its owner, rather than making
   each expression a separate work item. Then execute the existing Source/imported/native acceptance and provenance
   workflows, measure costs, remove diagnostic infrastructure, push and perform final PR review. The finish line is all
   required evidence and a maintainable complete protocol, not a count of remaining diagnostics.

### Complete lifecycle obligations

| Scenario | Ownership and completion obligation |
| --- | --- |
| Successful root and direct await | Native frame owner transfers once at commit. Main authority comes from the validated driver. Direct awaits remain in that frame. Generated lexical cleanup precedes terminal publication. |
| Partial startup failure | Native input still owns the inactive frame. Prepared scope withdraws its exact registration and closes its service owner. Source admission resolves the task claim and storage. No fallible work occurs between taking a resource and securing its rollback owner. |
| Panic or cancellation | Generated frame performs shielded lexical cleanup and owns all incidents. Source entry/preparation scopes resolve on abnormal exits. Native boundaries contain the resulting incident owners. |
| Terminal observation and payload disposal | Observer takes the outcome once. It never resumes a terminal frame. Execution, exact-thread attachment and providers survive outcome/report disposal and selected static cleanup. |
| Nested foreign entry | Parent owner and counted view survive isolation. Nested work uses the installed operation view. That view ends before nested context retirement, then the previous owner and quota are restored. |
| Wait publication, withdrawal and delivery | Endpoint ownership survives list mutation. Neighbor permission is valid under the actual resident lease. Detached delivery retains the remaining chain and invokes callbacks after unlocking. |
| Final task storage release | Final-reference authority and the lease protect extraction. The vacant row is published only after its old resources have one retirement owner. Recycling cannot release arena backing before the retired control and reports finish. |
| Closed admission and blocked shutdown | Existing pins and the resident host retain cleanup access. A blocked attempt returns unresolved ownership. The shared teardown state drains consumers and incidents before provider/service release. |
| Imported consumer and minimal demand | Exported contracts preserve the same effects, transfers, guards and dependencies. Ordinary reachable cleanup selects implementation units. Sync products do not acquire an executor and absent worker/deadline demand does not retain those units. |

### Architecture acceptance checklist

- [ ] Before every build, test or executable probe, check changed declaration and binding names against Bray keywords,
      enclosing bindings and visible module declarations. Fields and variant parameters use their own declared scopes.
      Do not use compilation to discover predictable naming conflicts. Check this again after a rewrite or rename.
- [ ] Open service ownership and closed cleanup retention are different binding alternatives with consuming transitions.
- [ ] Startup rollback, root entry restoration and native entry storage each have one identified owner on every exit.
- [ ] Task retirement returns independent resource owners directly, with no invalid whole-witness partition.
- [ ] Queue resources and live membership have separate contracts and one lease-bounded mutation protocol.
- [ ] Compiler effects retain their physical target and type through inference, use and export. Moves, mutation, guarded
      joins and cleanup agree on the carrier and dependency that survive.
- [ ] Source positive and negative controls, complete runtime checking, imported/native execution, provenance and costs
      establish the issue acceptance contract.
- [ ] Superseded mechanisms and diagnostic infrastructure are removed, checkpoints are pushed and final review is complete.

Execution closure now consumes open `Owner` and returns its actual `Reference` pin. Tagged binding ownership,
`PreparedStart`, `EntryRestoration`, wait resource/membership separation and `RetiredCell` are the intended runtime
protocol shapes. Their implementation is still subject to complete Source, imported/native and resource validation.

The checker distinguishes identity, current data, scalar copies, owned components, reference contexts and physical
lifetime targets. Current storage now has one retained root data tree and per-component availability. Joins can retain
a possible value over initialized predecessors without treating unknown data as absence. Finite internal initialization
guards identify their block or returned-expression join and value incarnation. They create no authority, cannot become
portable evidence, and expire with that incarnation. Conditional cleanup selects already-proved clauses under the
receiver's initialization guard. Availability never proves an active predicate. Ordinary storage legality remains
owned by the composite physical storage flow.

## Reconstruction audit and replacement plan

The current task includes tracing all ownership reconstruction, including paths that pass. No further behavioral
runs are authorized by habit or a local edit. Finish the architecture below as a coherent batch before validation.
The scope is the representation of values, owners, physical targets and evidence through checking, runtime
boundaries, lowering and imported contracts. Source-text reconstruction and rebuilding a product after its inputs
change are different operations and do not recover an ownership object.

The trace follows producers and consumers, including construction factories and missing-value fallbacks that do not
use the word reconstruction. The local search indexes are diagnostic aids, not a replacement for this assessment.
The governing rule is to retain an actual symbolic value once produced. Reads select it or project it. A missing
record cannot silently recreate the object's original identity, shape, lineage or authority from syntax or a marker.

### Inventory

| Mechanism and owning code | Actual authority | Decision and reason |
| --- | --- | --- |
| Explicit constructors and intrinsic results in `execution_guarantees/normalize/values.rs`, `flow/inputs.rs`, `flow/numeric.rs` | Checked operation, evaluated inputs, selected conversion and intrinsic contract | Necessary first production of a symbolic value. Perform it at the defining transition and retain its result. Later reads must not run the producer again. |
| Expression and storage reads in `flow/value.rs`, `normalize/expression.rs`, `normalize/calls.rs` and `ExecutionPlace::value_in` | Evaluated result or current storage record | Remove recovery from old syntax or `Input(place)` after execution. Distinguish declaration/contract normalization from reading an evaluated object. Retain ordinary projection of the actual object. |
| Implicit invocation results in `storage/expression/access.rs`, `flow/operation.rs`, `flow/inputs.rs`, `flow/value.rs`, and physical reference completion | The planned invocation result slot and completed result | Unify iteration, next and scoped entry. Scoped entry discarded its completed value and reconstructed a capability identity. A partial migration now binds its actual result slot, stores completion and reads that slot. Complete this with the whole inventory. |
| `flow/value.rs::aggregate_snapshot` | Independently retained field entries after the aggregate was forgotten | Remove. Reassembling an object on read is compensation for lost state. Partial objects must already have one retained description. |
| Whole and targeted mutation in `flow/invalidation.rs` | Checked storage invalidation, physical survival and normal completion observations | Replace clear/reset and restore of roots, nested descriptors and fields with an update of the existing value. Unknown changed contents are necessary. Recovering supposedly surviving owners from witnesses or marker prefixes is avoidable. Preserve typed physical paths and the same survival query. |
| Parent/component propagation in `state.rs::update_parent_observations`, `set_current`, `ExecutionValues` | The same actual aggregate and field update | Remove independently writable parent and child data copies. Centralize storage writes in one authoritative value tree. Component initialization metadata may remain separate from contents in that same domain, but must not be another data or initialization solver. |
| Reference refresh in `flow/invalidation.rs::observe_references` and `trusted/observations.rs` | Physical target and reference layer published by `StorageCompletion` | A current view of changed reached storage is necessary. Multiple consumers rediscovering the target, referent and containing shape are avoidable. Resolve from the authoritative target once and capture an entry or completion view explicitly. The reference's own identity and lifetime remain distinct from reached contents. |
| Returned ownership in `trusted/borrows.rs::complete_owned_result` and `construct` | Checked owned-result identity, checked contents preservation and captured actual input values | Necessary call-boundary instantiation. Identity alone cannot preserve mutable inline data. Share only actual input subtrees selected by the checked contents summary, while changed contents become unknown. Apply paths with the shared typed update operation. No missing-node recovery. |
| Returned borrows in `trusted/borrows.rs::complete_returned_references` | Checked reference-preserving relations, actual input reference and actual result | Necessary boundary substitution. Preserve the same reference identity and target. Do not manufacture a reference from a lifetime anchor or an unknown pointer. Use the shared projection operation. |
| Returned-carrier matching in `trusted/carriers.rs::returned_owners` | The actual returned value's owned nodes | Remove manufactured nullable aliases such as `owner_identity().present()` and recovery projections used to guess another representation of the same owner. Traverse canonical retained nodes and their declared projection views. |
| Branch/result joins in `flow/merge.rs`, `flow/joins.rs`, `flow/witnesses.rs`, and `snapshot.rs::common_components` | Both actual predecessor states and proofs | Necessary abstract merge, because paths can contain different objects or different data. This creates new joint knowledge rather than recovering a discarded object. Join authoritative nodes once, retain every possible dependency and only proved guarded clauses. Publish the joined record once. |
| Deferred captures in `flow/deferred.rs`, `snapshot.rs::owned_snapshot`, and `deferred.rs` | Actual captured entry values and actual future owner | Necessary immutable capture and incarnation binding. The frame must survive later changes to its producer's local slots. Share immutable contents and rebind ownership deliberately at capture. Do not rerun initializer syntax or recover an entry after its capture was discarded. |
| Cleanup entry and completion in `cleanup/transition.rs` and `trusted/completion.rs::complete_trusted_cleanup` | The selected initialized receiver, its actual value and lifecycle contract | Necessary capture and formal-argument rebasing. Stop assembling a receiver independently from a whole value plus unrelated field entries. Use the same invocation snapshot and completion protocol as other calls. |
| Scope expiry in `trusted/completion.rs::expire_trusted_scope` | Actual terminal input values, surviving borrows and ended storage | Remove save/clear/restore of input observations. Retain the terminal description where postconditions need it, mark its storage unavailable and retire access authority once. A snapshot needed after storage retirement does not resurrect storage. |
| Repeated evaluation and invalidation in `state.rs::retire_evaluation`, `retirement.rs`, and deferred selection retirement | The checked new incarnation and actual retained frame/value nodes | Necessary expiration of old observations. Use the same structural update mechanism and existing finite identities. Keep immutable captures distinct from mutable current observations. Never recover a retired incarnation on read. |
| Ownership and dependency summary inference in `ownership.rs`, `trusted/owned.rs`, `dependency/projection.rs`, `dependency/result/state.rs`, and storage reference ownership | Actual checked transfers, selected source operations and input/output paths | Necessary summary production and projection. These describe interprocedural transport. They are not permission to regenerate a lost caller object. Reuse the summary rather than independently guessing ownership in consumers. |
| Physical regions in `storage_flow/memory/references/transition.rs::owned_references`, `owned_region`, and reference slots | Existing declared input region or actual physical producer/transfer | Mixed. An abstract declared entry region is necessary. Materializing it as a fallback during a later read can confuse initial formation with lost state. Bind region identity at its owning boundary and make subsequent reads use that record. Preserve conservative aliasing and the distinction between ownership and freshness. |
| Formal terms, predicate normalization, equality substitution and portable contracts in `execution_guarantees/normalize/clauses.rs`, `condition/substitution.rs`, `portable.rs` and package interface decoding | Validated declarations/contracts and explicit actual arguments or results | Necessary translation or substitution across a real boundary. Keep one projection and representation policy. Never encode local incarnation/availability state as portable authority. |
| Native callbacks, entry storage, task references and arena views in the runtime | Actual retained native/Source owner, exact ABI storage and admitted pointer | Necessary physical boundary access to the actual object. A checked raw read or borrow is not a symbolic recovery fallback. No authority from addresses, marker construction, TLS observations or reconstructed IDs. |
| Lowering captures, generated helpers, MIR imports and specialization | Checked ownership/storage plan or authenticated portable executable metadata | Necessary lowering or decoding into the next representation. Consume established records and mappings. Do not repeat the checker's value/owner inference to fill missing metadata. |
| Query memoization and artifact reuse | Complete inputs and validated query/artifact identity | Necessary evaluation and cache policy. This does not justify discarding part of a value and recovering it with a second semantic policy. |

### Implementation order

1. Finish one result-slot protocol for every implicit invocation. The existing local changes include the planner's
   `InvocationResult(ScopedEnter)` binding, builder validation, shared physical lookup, completion storage and scoped
   value reads. They are intentionally unvalidated. Remove the remaining bare-capability reads at scope exit.
2. Make current data authoritative at its storage root. Retain one immutable value tree per root and component
   availability metadata driven by checked transitions. Do not keep independently writable copies of parent and
   child values. Reads project retained nodes. First partial initialization constructs the partial value at that
   transition, rather than an `aggregate_snapshot` fallback later. Keep immutable entry/result/frame snapshots by
   shared ownership, separate from the current place whose contents can change.
3. Route assignment, checked movement, physical transfer, mutation and retirement through that representation's
   update operation. Apply the existing physical invalidation/survival result directly to retained nodes. Preserve
   surviving identities and replace only changed knowledge. Delete whole-map reset/restore, witness-filtered nested
   reconstruction and the aggregate recovery helper. Do not replace them with another recovery pass.
4. Make current reference views consume the physical target already published by storage completion. Capture actual
   entry values once. Call completion and lifecycle completion use the same stored result and receiver protocol.
   Retire storage/access separately from retaining a terminal value needed for postconditions.
5. Consolidate returned-path application, nullable/union projection and updates into one operation. Extend each checked
   owned-result transfer with its actual inline contents preservation. The old summary records identity only, and ordinary
   effects intentionally omit writes to consumed inline inputs. Reusing their frozen data is therefore unsound. Infer a
   compact subtree mask from actual terminal data against immutable entry observations, intersect it across normal exits,
   and preserve it through substitution, import validation, encoding and structural identities. The mask has an inherited
   preservation default and typed child overrides. It contains no object data or authority. Changed contents remain unknown
   at the caller. Reference handles and reached contents remain separate. Delete synthesized matching aliases and
   missing-value fallbacks. Keep phi joins and captured-frame rebinding as operations over actual input snapshots.
6. Move abstract physical-region initialization to its genuine entry/result producer. Reads cannot create a new
   region after losing the old record. Review pruning against retained owners and lifetime roots so it drops only
   data that no consumer still requires. Preserve the existing raw transfer, epoch, fabricated-owner and alias rules.
7. Trace the final mechanism through Source, imported contracts, lowering and native consumers. Finish the runtime
   transitions under those contracts. Current known runtime corrections include extracting initialized retirement
   fields from their actual storage and fixing a missing mutable-borrow argument. Do not make every remaining
   diagnostic a new helper, exceptional predicate rule or trusted guarantee.
8. Self-review the full inventory and naming before validation. Run mutating style, explicit format, affected unit
   suites (including bound-tree after its planner binding change), all runtime Source bodies/certificates, positive
   and negative ownership controls, then native/imported/provenance/resource acceptance and costs. Assess structural
   improvement separately from correctness, remove diagnostic infrastructure, push and complete PR review.

The replacement must remove more synchronization obligations than it adds. In particular, a new CurrentValues facade
that still exposes independently mutable duplicated data is insufficient. Keep one physical reference/alias model,
one witness ledger and the existing composite storage flow. No generic object graph, historical event log, new
initialization solver, runtime-name exceptions or implicit allocation freshness is authorized.

## Current validation and remaining acceptance

### Phase, role and conditional-value completion

The first integrated reconstruction batch exposed three architectural contract gaps. They are not individual runtime
call-site failures. Keep the strict actual-value readers and canonical root data representation, then finish these
contracts together before another behavioral run:

1. **Separate pre-transition storage reads from evaluated results.** `check_witness_transfers` executes before the
   occurrence's own producer. It cannot read that occurrence from `expressions`. Resolve its actual source using the
   collector's existing live physical target, then read the retained current object. Keep post-move reads on evaluated
   snapshots. Apply the same phase rule to Bound-form FutureStart, using its captured receiver. Retention must use
   selected runtime input metadata, including unstaged method receivers across argument-call edges. Do not retain
   arbitrary syntax subtrees or restore normalization on cache misses.
2. **Keep data, loan descriptors and complete owners distinct from first production.** Borrow production must create
   a loan immediately. Scalar copies and their projections retain data without the source's ownership. Centralize
   role-aware lineage, witness matching and retirement selection. General observation identity remains separate from
   permission to own a carrier. Actual projections and tag/nullable decorations must preserve these roles. Stable
   identities cannot contain changing object contents.
3. **Infer conditional preservation before joining values.** Attach preservation to actual ownership layouts when
   predecessor snapshots are captured. Entry/checked input moves establish their actual unchanged input once.
   Structural updates change the affected subtree summary, opaque updates keep only checked survivors, and calls
   compose input preservation with callee preservation. Joins retain guarded layouts and intersect only guarantees
   that hold on all relevant alternatives. Terminal publication translates those established summaries instead of
   comparing an unconditional merged value with an original input after branch data was discarded. Tag refinement
   over actual contents preserves their equality. A constructor or mutation has its own new data basis.
4. **Remove semantic-shape stand-ins for storage ownership.** Whole versus partial carrier checks must use the actual
   checked storage boundary and role, not infer current containment from an immutable input term's old field path.
   Eliminate empty `Constructed` wrappers used to suppress partial-witness checks. Keep actual object identity,
   evaluated data and the checked complete-result transfer. Review all repeated transfer/capture paths for finite
   identities and canonical value updates. Prove convergence with a representative repeated transfer boundary,
   without increasing the stack or making unknown data grant authority.
5. **Complete and review this whole batch.** Revisit the reconstruction inventory's producers, selectors, movement,
   mutation, joins, retirement, captures and imported contracts. Add meaningful regressions for the confirmed phase,
   receiver retention, role and repeated-transfer contracts. Agents use GPT-6.1 low effort and run no validation.
   Root runs required style/format, affected units, complete Source controls/runtime checking and the established
   imported/native acceptance after the completed implementation. No per-edit test loop.

The first replacement's evidence is 1,095 affected unit tests passed after test-only fixture corrections. The owned
Source/imported integration suite passed 47, failed 3 and ignored 6 tiered tests. Its failures include improperly
accepted copied-owner negatives, lost conditional valid transfers, and fresh array-factory guarantees. Input-free
factories do not use owned-input masks and must not receive speculative mask fixes. The explicit Source controls
matched 16/18. `frame-boolean` was improperly accepted, and the expanded scoped-owner composition loses its guarantee
after a callback. The complete runtime Source runner aborted with stack overflow before a total, after reporting a
deadline storage obligation. This is an unresolved termination defect, not a new failure count. Logs are
`reconstruction-units.log`, `reconstruction-interface-units.log`, `reconstruction-owned-integration.log`,
`reconstruction-controls.log` and `reconstruction-source.log` in the ignored task directory.

The latest validated implementation checkpoint is `e12e722f9867f8cc959b7e6a0d56cf3f4db63e1c`.
Mutating style and explicit format passed. Affected units passed 418 checker, 38 lowering, 35 runtime-interface and
29 ABI tests, total 520. All 18 Source controls matched, including the corrected fabricated-owner negative and the
ownership-transition composition. The complete runtime Source run finished 3,264 checks with 69 failures. These
results do not establish end-to-end acceptance. Logs use the `availability-` prefix in the ignored task directory.
The reconstruction checkpoint has partial validation below. Production Rust compilation completed after
correcting the shared owned-value iterator's lifetime capture. Mutating `cargo xtask style` and explicit
`cargo xtask format` passed. Owned-result graph validation is a focused helper in the general interface validator.
The affected unit command adds symbols, bound-tree and package-interface to the previous suites:
`cargo test --release -p bray-symbols -p bray-bound-tree -p bray-checker -p bray-lowering -p bray-package-interface -p bray-runtime-interface -p bray-runtime-abi --lib --no-fail-fast`.
It passed 132 bound-tree, 424 checker, 38 lowering, 214 package-interface, 35 runtime-interface, 29 ABI and 221 symbols
tests, total 1,093 passed. Two new package-interface tests initially stopped in fixture setup because `identity_surface` lacked
valid struct-field and generic-parameter relationships. The helper now uses the actual symbol relationship kinds,
with fixture assertions unchanged. The affected interface suite then passed all 216 tests, bringing the combined
affected unit result to 1,095 passing tests.
One new checker fixture initially used a private ID constructor. It now obtains IDs from the existing bound-tree
builder, and all 424 checker tests passed. Logs use `reconstruction-` in the ignored task directory.
`cargo test --release -p bray-compilation --lib owned --no-fail-fast` completed with 47 passed, 3 failed and 6 ignored.
The paired unchanged/changed Boolean control passed. The 18 explicit Source controls matched 16 expectations.
The complete runtime Source run aborted with stack overflow before a total. These results cover pushed production
checkpoint `0e9e56034e09d6b206c012a50361d36d6d4f7556`, with the described test fixture corrections.
The phase, role and conditional-value completion batch above supersedes that implementation and is not yet validated.
Native acceptance remains unrun.
`CurrentValues` now keeps one root data tree plus component availability, with no independently stored field data.
Writes update that tree, reads project it, and terminal retirement retains data while live-subtree views exclude
retired ownership. `aggregate_snapshot`, opaque whole-map reset/restore, witness-filtered nested reconstruction,
unchanged-field repopulation and repeated-evaluation parent repair are deleted. A temporary `ValuePreservation` trie
contains only the checked physical-survival mask, applied atomically to actual old nodes. It holds no object data or
proof and does not recover anything from syntax, witnesses or newly constructed markers. Mutation still necessarily
creates fresh unknown contents. Stable nodes are selected from the actual retained object.

The implicit invocation result-slot protocol now stores completed values once and reads them without recreating
ScopedCapability/PostState identities. Scope exit uses its captured actual input. Cleanup captures one actual receiver
and retains terminal descriptions without save/clear/restore. Call retention uses live subtrees rather than terminal
field data. Returned-owner lookup no longer manufactures nullable aliases, and returned writes share the typed
`with_storage_component` operation, including OwnedTarget/UninitPayload selectors. Formal declaration normalization
may introduce Input terms, while execution-state reads cannot recover missing formal objects.

Runtime expression reads now select evaluated or pending results only. Syntax normalization is restricted to actual
defining transitions and formal declaration boundaries. Logical short circuit, negation and condition wrappers publish
their actual Boolean result after the selected operand path completes. Previously their CFG definition preceded their
operands and recovery from syntax concealed the missing producer. Reference refresh updates the retained expression or
pending result in its existing channel and does not recreate dead temporary parents.

Actual planned temporary and expression-result storage now participates in `CurrentValues` alongside local and
invocation slots. Previously those physical value slots existed only in the evaluated-expression cache, so a retained
factory result could lose ownership across another call. Definition publishes the same actual object to current
storage and its evaluated snapshot. A move vacates current storage, cache pruning does not destroy it, and scope expiry
ends its ownership. Call attempts keep prospective results pending until successful completion. Allocation identities
remain physical regions rather than inline value slots. Opaque invalidation applies the checked survival mask first,
then retains authority through actual live snapshots, preserving ownership roles and replaced-child boundaries.

The call-boundary review found a missing owned-contents contract. A typed preservation mask on each owned-result
transfer separates unchanged data from owner identity. Its inference and consumption are integrated with portable
encoding, validation, substitution and specialization identity. This is an internal semantic contract change, with no
source-language or native ABI change. It replaces unconditional reuse of frozen argument contents.

Owned-result identity now means the transferred node's own identity. Actual current children have their own mappings,
and retained contents select unchanged subtrees. A recorded top identity cannot republish descendant mappings from an
older aggregate. Retirement compares actual retained ownership and data separately, rather than treating a surviving
owner's spatial prefix as proof that all its old children survive. Old owned children are visited even when their parent
identity survives. Spatial write invalidation keeps its existing physical semantics. These are temporary transition
views over actual snapshots, not a second persistent ownership or alias ledger.

Physical buffer regions now bind at declared entry, actual allocation or actual result formation. Queries read those
records, and transfers preserve them. They do not recreate missing regions or add ownership/freshness. Existing
physical unit fixtures now set up genuine producers and keep the previous behavioral assertions. The runtime
retirement initialization boundary takes actual field borrows under the final reference and lease, and no longer
uses a panic-capable const projector in its total certificate.

The truth/availability review also removed `invalidate_cleanup`'s promotion of possible witness clauses merely
because the carrier was available. It retains only clauses already proved in the current truth environment. Added
regressions protect this distinction, canonical component writes, retired-field retention and repeated effect masks.
The existing native composition now includes consuming scoped entry with opaque callbacks and early-return exit.

The user explicitly authorized implementation subagents at GPT-6.1 low effort, with no test runs. Agents
`/root/invocation_results`, `/root/retirement_packet` and `/root/cleanup_snapshots` own expression production and phase
retention, ownership-role identity and retirement, and owned-return inference/completion respectively. They do not run builds, tests,
style, format or probes. Root integrates their changes and owns all validation.
Before validating the completed integration batch, repeat static self-review and naming/keyword preflight across its
new declarations and producers. No changed Bray binding conflicts with a keyword or visible module declaration. Do not
claim the prior 520 unit/18 control evidence covers these new changes. Native/imported acceptance remains unrun.

The remaining failures include scoped entry/exit and prepared start, whole binding ownership transfer, current
owner/view survival through callbacks and loops, optional result/borrow extraction, arena admission, task retirement,
wait access and worker formation. The audit above supersedes a per-call-site repair strategy. The runtime's `Binding`
closure, `EntryRestoration`, `PreparedStart`, wait resources/membership split and `RetiredCell` remain useful protocol
shapes, but do not assume they establish complete contracts until the full representation migration is validated.

Required end-to-end evidence remains:

- Native root Main identity, direct await, independent child scheduling, startup rejection/rollback, panic and
  cancellation, lexical cleanup before terminal observation, retained incidents and actual frame destruction.
- Nested entry restoration on normal and abnormal exits, exact-thread/provider lifetime and one resident shutdown
  state. A blocked shutdown returns unresolved ownership instead of waiting for a caller-held dependency.
- Source and imported development/release products with representative ordinary, scoped and operation cleanup.
- Provenance and link maps prove selected runtime units are Bray. Synchronous execution requires no async executor.
  Minimal async execution omits worker/deadline startup and implementation units when not demanded.
- Direct, hosted synchronous and minimal async whole-build time, startup time, size and retained resources, with
  production/test code growth measured separately. Moving code and formatting do not count as simplification.
- Remove `xtask/src/bin/bra567.rs`, retain task-owned logs and caches, push the final work, publish the final reviewer
  artifacts on the draft PR, resolve findings, mark PR ready and move BRA-567 to In Review. Never mark Done.

## Working instructions and environment

Read root `AGENTS.md`, coding conventions and crates documentation. Skills already read and applied are
`.agents/skills/work-on-bray-issues/SKILL.md`, `.agents/skills/use-bray-language/SKILL.md`, and
`.agents/skills/debug-bray-end-to-end/SKILL.md`. The user explicitly authorizes implementation subagents using GPT-6.1
at low effort, with no test/build/format/style runs by those agents. Root owns canonical current values, invalidation
and integration. Dispatch the required fresh PR reviewer
only after implementation and required validation finish, with direct attributed publication on PR #710.

Before every build, test or executable probe, check new names against keywords, enclosing bindings and visible
module declarations. The preflight caught a task-module `admission` conflict, renamed to `arena_admission`.
Earlier `pin` and nested `scope` conflicts are fixed. Rust names must also be checked for reserved and conflicting
names. Do not use compiler errors to discover predictable naming conflicts.

Use the existing workspace and Cargo/LLVM caches. Source `/workspace/.bray-tools/env.sh` for Cargo and the configured
sysroot. Cargo storage is `target/cargo` (about 9.8 GiB), LLVM is `target/toolchains/llvm/active` (about 12 GiB), and
produced executables are in `target/release`. Do not clean shared storage. The diagnostic runner is untracked and
must not enter the PR. Native LLVM, archiver, linker and produced-executable workflows run outside the sandbox.
The intended native command is `cargo xtask runtime-artifact smoke-test --output target/bra-567/native --profile release`.

The shell's GitHub token is invalid. The connected GitHub Git-object API has verified the existing public
`lejmer/bray` destination and authenticated `lejmer` account with push permission. Use create_tree with the current
base tree, create_commit and update_ref with expected_sha and force=false. Fetch that branch without the invalid
credential helper outside the sandbox, then `git reset --mixed` the published commit to align history while preserving
worktree edits. Keep pushing recovery checkpoints. The issue branch head saves the canonical-value migration,
expression production, owned-contents contract and retirement integration. Use `git rev-parse HEAD` and the remote
issue branch to identify the saved code state. Behavioral validation after the last validated checkpoint remains pending. Verify mutable
remote PR/base state before final handoff. Parent PR #709
remains open and ready, base commit `4ac3ad99259cc5bc02e968f830adaa4c521bed6a`.

All validation processes for that checkpoint finished. No native/imported acceptance or final reviewer has run.
Do not stop at this record, a push or a passing local suite. Continue the inventory's coherent replacement and finish
BRA-567's required end-to-end outcome in the same PR.
