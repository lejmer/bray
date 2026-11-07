# BRA-567 implementation handoff

## Scope and checkpoint

Continue [BRA-567](https://linear.app/bray-lang/issue/BRA-567/implement-async-root-and-main-lane-orchestration-in-bray)
on draft [PR #710](https://github.com/lejmer/bray/pull/710), branch
`feature/bra-567-async-root-orchestration`. The implementation checkpoint is
`e40e37ed0b3bb6784fd2796cd0cfad9ae8e1f9ca`. The comparison baseline is
`e477f82ce8cdbae354a81c23b6744fb8d68475d6`, which includes the parent implementation and develop integration.
Inspect current remote branches and issue relations before changing the stack. Parent
[PR #709](https://github.com/lejmer/bray/pull/709) supplies demanded worker lifetime through
[BRA-566](https://linear.app/bray-lang/issue/BRA-566).

The current batch completes the evidence model described below before validating the runtime end to end.
The user authorizes rewriting unsuitable mechanisms when that produces a simpler, more maintainable result.

### Design commitments

`ExecutionPlace` identifies a checked storage path. `ExecutionState.current` maps that path to its current value,
including independently versioned contents. A `Borrowed` owner identifies a retained loan. Refreshing its referent
or joining current contents must not replace the loan identity. A scalar observation reached through a stored view
uses that loan's established storage context, not a current contents version as a new storage identity.

`ExecutionCallEvidence` owns an invocation's entry snapshot. An inactive frame retains that context independently
of later caller mutation. Each copied capture uses the existing frame-relative `CapturedInput` identity, including
constant inputs. Ordinary facts describe its value. Copy origins only correlate still-unchanged caller observations,
and expire without deleting captured inputs. Owned captures transfer their complete carriers. Borrowed inputs retain
access dependencies without acquiring ownership. Normal completion can use entry proof, while abnormal cleanup
receives no normal-completion guarantees.

Joins intersect proven facts and merge possible dependencies. They must not conflate reduced value knowledge with
input existence or ownership. Captured identities are bounded by frame owner, source invocation and input ordinal.
Invocation entries and current values use the same snapshot join, including retained loans with changed referents.
Existing block joins summarize successive dynamic frames from one source site, without accumulating an incarnation
history. Losing a copy origin cannot erase an input or preserve a stale caller selector.

Storage preservation uses the resolved `Effect` together with its target, raw access type and scope until the validity
decision. The cached `Footprint` classifies which marker types a transfer can carry. It does not decide reachability.
One transfer query serves active loans and stored shared views. A raw transfer revokes a matching observation only
where it may reach its storage. Ambient effects use established exposure, and a target bound uses the existing memory
relationship. Ordinary operations respect retained shared loans. Marker-free data transfers cannot retire markers. Whole-carrier transfer, replacement and retirement remain distinct
from contents mutation. Scalar-only carriers must also lose authority after a reached raw move.

Remove the global raw-transfer marker set used as a substitute for reached storage. Preserve the existing owner,
witness and invalidation representations, rather than adding another evidence store or shutdown policy.
Remove deferred refinement's replacement of captured arguments with proven constants. Constant knowledge belongs in
the entry facts, while the frame retains its input identities through refinement, source mutation and joins.
Reconsider this design if it requires per-consumer exceptions, repeated reachability traversals, unbounded loop
history, or weakening rejected fabrication and raw-transfer cases.

Optional execution services use their own existing retained domains and operation bindings. Common root driving and
cleanup must not directly reference concrete deadline work or worker storage destruction. Deadline domains bind poll,
wait, backing retirement and final destruction when formed. Worker domains bind their final destruction alongside their
existing demand, stop and detach operations. The operation signatures preserve existing retained-owner and retirement
requirements. Native entries take raw pointers or complete owned values, with anchored borrows inside their implementations.
They consume their own incidents before returning. An unconstrained ordinary Bray callback can retain its input borrows
in incident reports, so it cannot provide this service boundary. This adds no host-wide runtime table, C borrow ABI or
alternate shutdown owner.

The affected-contract checklist covers capture and forwarding, source mutation, branch and loop joins, raw and ordinary
transfers, normal and abnormal cleanup, result consumption, Source/imported contracts, runtime native boundaries,
genuine Main authority, optional service selection and synchronous no-executor controls.

This document is the single current handoff. Update it in place as decisions and evidence change.
Remove superseded statements. Do not accumulate historical task notes or parallel progress records.
The repository and the facts recorded here are sufficient to resume. No temporary VM artifacts are required.

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

The current redesign separates scalar `StorageValue` observations from aggregate `Constructed` contents. The old
single-`OwnedTarget` shape test misclassified refreshed aggregate owners as scalar storage contexts. Scalar data and
storage identity now have an explicit representation. Ordinary equalities transform data without renaming its storage
context. The nested conditional ownership collector now traverses both predecessors together and carries enclosing
active-case guards into each payload, replacing flat inner-tag tests. Both changes are implemented. The checker unit suite reports 414 passing and one failing existing test,
`known_value_writes_revoke_only_borrowed_authority_on_the_changed_storage`. The new nested guard and scalar/aggregate
refresh regressions pass. Investigate the existing test before the next batch. The redesign is not yet covered by the
Source results below. The queued intermediate checker run was
cancelled while waiting for the Cargo lock so only this completed redesign is checked.

Implementation and end-to-end verification remain in progress. Do not request final review yet.

Stable capture identities accept the same-site inactive numeric guard and reject numeric and Boolean active guards.
Shared and stored shared-view controls accept the original owner and reject fabricated markers and payload authority.
Reached raw moves of scalar-only and address-only owners both reject subsequent authority use.

The canonical refresh batch reduced the extended runtime Source sweep from 53 to 30 failures in 3,160 body and certificate
checks, including anonymous bodies. Invocation composition and direct invocation now certify. All previously failing
destructor terminal-effect checks now pass after distinguishing ordinary loans from actual raw publication of owned inline
storage. Remaining families include mutable cursor forwarding, event registration, dispatch, task result transfer,
root-frame scalar authority, worker context startup, task retirement, observer rollback and worker formation. Several
certificate errors are downstream from those bodies. The deadline callback's local name collision is corrected.

The current next batch consolidates borrowed markers under physical loan observations, rather than also treating their
logical reference slots as the marker storage. Mutable loans use the same typed transfer query, while shared protection
remains restricted to shared loans. This change was made after compiling the canonical refresh sweep and is not covered by it.
It is building in the temporary public compilation diagnostic binary. Checker units for the combined batch are also queued.
The previous checker run was 411 passing with one incorrect new assertion, now corrected to assert loss of known value.

Optional services now use per-domain native callbacks. Deadline driving returns one C-layout value. Final destruction
reuses each provider's existing native destruction entry, which consumes the actual reference slot. Deadline backing
retirement uses the last execution service owner's pointer protocol, rather than borrowing the pin that it removes.
No C borrow ABI, extra runtime owner or host-wide function table is introduced.

Temporary traces use `BRA567_TRACE` in `trusted/check.rs` and `terminal.rs`, and `BRA567_TRANSFER_TRACE` in `transfers.rs`.
They remain only in the working tree and are excluded from checkpoints. Remove them before finalization. The diagnostic harness moved to `xtask/src/bin/bra567.rs` to avoid example dev-dependency
feature changes. It must not be committed. Use `cargo run --release --bin bra567 -- runtime`, then reuse its binary for
focused cases. The source files are snapshotted at compilation load. Logs for current batches are under ignored
`target/bra-567`, with `source-physical.log` and `checker-canonical-unit.log` currently active.

Native acceptance shares authenticated runtime provenance auditing between async and synchronous products, once per
smoke group. It adds a minimal async fixture, optional-service omission checks and preparation/build/startup/size metrics.
Those checks remain unrun. Required style/format checks passed on the earlier shared-join batch and must repeat after the
completed current batch. No final review agent has been requested.

Shell GitHub authentication (`GH_TOKEN`) is invalid. The connected GitHub account is verified as `lejmer`, with admin/push
access to the public `lejmer/bray` repository. Use its Git object APIs to publish a coherent checkpoint on existing PR #710.
This checkpoint continues from `73f55623bb25b21bc1f9b87afc97253c87e14c5e`. It preserves the implementation and current
redesign while validation remains incomplete. Parent #709 remains open with the unchanged stack base. Push coherent
checkpoints regularly, as explicitly requested by the user, and update this record each time.

Use the public compilation API for Source diagnosis without building a `bray-compilation` test binary. Keep essential
final evidence in this record, rather than only in ignored logs. The native smoke, imported acceptance, comparative
costs, final review, PR readiness and Linear In Review handoff all remain required.

At the implementation checkpoint:

| Check | Result |
| --- | --- |
| Runtime Source bodies and execution certificates | 1,118 checks, 24 failures |
| Binding, syntax and unverified checks in that sweep | No failures |
| Focused ownership cases | 37 of 38 pass |
| Focused deferred and scalar cases | 22 of 23 pass, including all 13 scalar controls |
| Checker, symbols, package interface and invalidation unit checks | Pass |
| Selected standard-library memory bodies and certificates | Pass |
| Required style and formatting | Pass |
| Native/imported runtime acceptance and final comparative costs | Incomplete |

These results came from focused public compilation consumers and selected unit binaries. They are not a claim that
all Cargo or CI tests pass. Reconstruct needed checks from repository tools and current sources, rather than trying to
recover the former ad hoc harness collection. Temporary tracing hooks were removed. No uncommitted production code
remained when preparing this handoff.

The 24 Source failures are in these declarations. Some certificate failures are downstream consequences rather than
independent defects.

| Repository path under `runtime/bootstrap/src/` | Declarations |
| --- | --- |
| `activation/composition.bray` | `compose` |
| `activation/execution/invoke.bray` | `invoke`, `invoke_retaining` |
| `event/delivery.bray` | `notify`, `deliver` |
| `execution/native_owner.bray` | `owner_entry_scope`, `native_owner_scope_enter` |
| `execution/resolution.bray` | `task_resolution`, `task_resolution_action`, `transfer_task_result` |
| `execution/root_scope.bray` | `resume_binding` |
| `execution/service_borrows.bray` | `release_services` |
| `execution/start.bray` | `start_root` |
| `execution/task_entry.bray` | `task_allocation` |
| `execution/worker_context.bray` | `create_worker_context` |
| `task/dispatch.bray` | `step_dispatch` |
| `task/observer/creation.bray` | `create` |
| `task/runtime_retirement.bray` | `retire_execution` |
| `worker/entry.bray` | `run` |
| `worker/formation.bray` | `ensure` |
| `host/attachment/acquisition.bray` | Destructor |
| `host/product/cleanup.bray` | Destructor |
| `host/product/thread_domain.bray` | Destructor |
| `report/chain.bray` | Destructor |

## Confirmed evidence and remaining uncertainties

### Deferred frame loses a captured argument

The focused numeric case forms two futures at the same call site in a two-iteration loop. Each consumes a sealed
`Box<Marker>` and captures the current numeric index. The async callee returns the marker field only when the captured
code equals zero, otherwise it returns `none`. Its postcondition is `code == 0 || result matches none`.
The whole box carries an opaque trusted `sealed(owner = &result)` guarantee from its producer.

Save the first future when index equals zero and the second otherwise, incrementing the index after storage.
Awaiting the second future should be accepted because it does not partially extract the sealed owner.
Awaiting the first should be rejected for separating the whole-owner guarantee. The second case is currently rejected
with `CheckingTrustedWitnessTransferNotProven`.

The diagnostic showed that the second frame retains a condition saying its captured argument 1 is not zero, but its
`ExecutionCallEvidence.arguments` map no longer contains argument 1. A copy-origin entry can also remain for the missing
argument. The trace proves an inconsistent frame context. It does not establish the exact first operation that loses it.

Trace these existing owners together:

- `crates/bray-checker/src/analysis/guarantee/flow/deferred.rs`: capture and execution of the actual frame.
- `crates/bray-checker/src/analysis/guarantee/deferred.rs`: producer selection and merging.
- `crates/bray-checker/src/execution_guarantees/model.rs`: argument maps, copy origins, proof projection and intersection.
- `crates/bray-checker/src/execution_guarantees/witness.rs`: renaming of captured arguments and facts.
- `crates/bray-checker/src/analysis/guarantee/retirement.rs` and `state.rs`: current observation retirement.
- `crates/bray-checker/src/analysis/guarantee/trusted/completion.rs`: substitution and use of entry proof.

`ExecutionCallEvidence.intersect` retains an argument only if its representation matches the other argument exactly.
Investigate this together with renaming and retirement. Losing knowledge of a value must not accidentally erase a live
frame input or disconnect its surviving facts. Do not restore stale current branch selectors to make the case pass.
The paired Boolean case, capturing `index != 0`, must continue rejecting the active partial extraction.

### Shared owner loses marker preservation at startup

In `owner_entry_scope`, the no-argument call to `origin_driver_startup` precedes the failure to prove the requirement of
`domain(&owner.execution)`. At that call, the caller's shared entry borrow remains active but its owner has no stable
value observations after preservation. The resolved effects include Unknown, Ambient, Atomic and typed unbounded
transfers with both ambient and input scope. Restricting effects to explicit call arguments alone cannot repair this.

`preserve_storage_values` in `analysis/storage_flow/check/analysis/observations.rs` reduces transfers to marker-type sets
before applying shared-loan preservation. That discards target and scope distinctions. This is an architectural concern,
not a proven complete diagnosis of every Source failure. `record_target_effect` in the sibling `effects.rs` also publishes
an ambient alternative for returned origins before forwarding their known origins. Inspect the actual owner and alias
contracts before changing that behavior. Do not assume returned storage is fresh or remove raw-transfer rejection broadly.

The existing typed-data transfer matrix forms a raw pointer from the borrowed owner even in variants that later call a
no-argument helper. Those variants do not establish behavior for an owner that has never been exposed through a raw address.

### Stored shared scalar view across a loop

The committed test `shared_protocol_markers_survive_native_exposure_and_loop_merges` in
`crates/bray-compilation/src/compilation/trusted_contracts/sync.rs` is the remaining ownership failure.
It borrows an owner, publishes its address, iterates with an opaque payload operation, and observes both its marker and
scalar identity. Marker observation survives, but the scalar requirement fails through the stored shared view.

`borrowed_condition` in `execution_guarantees/normalize/values.rs` follows checked reference boundaries but selects
identity through `into_referent().owner_identity()`. Confusing current contents with the retained loan identity is a
hypothesis to investigate, not a verified fix.

## Required design deliverable

Before editing the compiler, describe the meaning and lifetime of each existing identity and context. Follow each
through admission, normal execution, abnormal execution, merging and cleanup. Explain which decisions become unified,
which remain distinct because they enforce different invariants, and which existing code will be removed.

Pay particular attention to joins: less precise value knowledge must remain separate from ownership, argument existence
and permission validity. Explain convergence and the treatment of multiple dynamic frames from one source call site.
For storage effects, preserve the authoritative operation and reached storage context through the validity decision.
A shared witness store alone does not unify independently implemented preservation policies.

Read the relevant rules in `docs/language/contracts-and-trust/`, `docs/language/ownership-and-borrowing/`,
`docs/language/targets-layout-abi-and-raw-memory/` and `docs/language/async-and-concurrency/` through the language skill.
Use `docs/design/compiler-architecture.md` for phase responsibilities. Read `docs/design/AGENTS.md` before modifying
anything under `docs/design/`.

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
