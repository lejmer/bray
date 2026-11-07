# BRA-567 implementation handoff

## Scope and checkpoint

Continue [BRA-567](https://linear.app/bray-lang/issue/BRA-567/implement-async-root-and-main-lane-orchestration-in-bray)
on draft [PR #710](https://github.com/lejmer/bray/pull/710), branch
`feature/bra-567-async-root-orchestration`. The branch head is the implementation checkpoint. The comparison baseline is
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
decision. The cached `Footprint` classifies the nominal carriers and markers a transfer can carry. It does not decide reachability.
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

Implementation and end-to-end verification remain in progress. Do not request final review yet.

The checker distinguishes non-owning scalar `StorageValue` context, retained-owner/loan `CurrentValue` contents, and
structural `Constructed` components. `CurrentValue` with a `Borrowed` owner observes but does not own its contents.
`OwnedTarget` represents real backing. Synthetic borrowed-referent and owned-current wrappers are removed. The current batch removes that overloaded representation from
invalidation and joins. Ordinary equalities change data without renaming storage. Join substitutions distinguish
complete root identity from current component observations. No additional witness or alias store is introduced.

The cached inline footprint classifies complete nominal carriers as well as empty markers. A reached raw move must
revoke scalar-only owner authority. Unrelated typed moves cannot retire another nominal owner merely because both
contain scalar data. Raw writes remain conservative for contents. Active loans and stored views use one typed survival
query and retain their physical referent type. Mutable markers use that query even after raw publication. Active loan
regions are resolved once per preservation operation.

Nested conditional ownership carries all enclosing active-case guards in one paired predecessor traversal. Mutable
union reconstruction retains checked physical paths, including variant identity, instead of reconstructing parents
from execution places that omit that identity.

| Check | Result and limit |
| --- | --- |
| Checker library unit suite | Current-value batch: 414 passed, one expanded join matrix failed in the bare-loan/unchanged branch. Canonical loan batch pending |
| Runtime Source bodies and execution certificates | Canonical-loan batch: 3,160 checks, 15 failures, including five body families and downstream certificates. Predicate-context correction pending |
| Same-site inactive numeric deferred frame | Accepted, active numeric and Boolean partial extraction rejected |
| Shared/stored owner and nested union forwarding | Real owner accepted, fabricated marker and payload authority rejected |
| Typed raw transfer controls | Unrelated nominal moves accepted, whole/nested owner moves and writes rejected for scalar/address owners |
| Mutable union completion | Promised owner accepted, unpromised owner rejected |
| Exposed mutable marker | Marker-free write now accepted after raw publication |
| Required style and formatting | Running for current-value batch |
| Native/imported runtime acceptance and comparative costs | Unrun |

All previously failing destructor terminal-effect checks pass. Composition, direct invocation, event registration,
worker context construction, task retirement and service retirement pass their Source bodies/certificates.
Remaining body families in the last valid sweep are execution dispatch, resume binding, root entry cleanup,
task-allocation cleanup, observer preparation rollback, worker entry and worker formation. Remaining certificate
failures include downstream callers. The temporary physical-framing experiment did not improve acceptance and was
removed. The full Source sweep and new mixed initial/refreshed component-carrier join coverage are pending on the
current-value representation. Do not request review or claim acceptance from an earlier check count.

Runtime ownership now follows complete publication and disposal units:

- Root admission reuses the `Current` view in its actual entry scope, removing an extra counted view and the single-use
  `start_root` helper. The native frame claim remains in its original input slot until admission commits.
- Worker context construction accepts the complete binding before publication. Failed native installation uses the
  existing context retirement path to release that pin. Worker context bodies and certificates pass in the later sweep.
- One task-slot operation owns detachment, metadata cleanup, report disposal and final pin release. The separate detach
  phase releases the resident host acquisition before callbacks. Its pin remains live through all disposal. These
  task-retirement bodies and certificates pass in the later sweep.
- Deadline backing retirement detaches its pin before closing, but retains that pin locally through callbacks.
- Observers validate execution before allocating preparation resources. Native record output now uses a separate scratch
  value before committing to the preparation owner, keeping its acquisition outside the native output slot.
- Execution observation explicitly promises valid native report callbacks on its returned outcome.

Optional services use per-domain native callbacks. Deadline driving returns one C-layout value. Final destruction
reuses each provider's existing native destruction entry, which consumes the actual reference slot. Deadline backing
retirement uses the last execution service owner's pointer protocol, rather than borrowing the pin it removes.
No C borrow ABI, extra runtime owner or host-wide table is introduced.

Native acceptance shares authenticated runtime provenance auditing between async and synchronous products, once per
smoke group. It adds a minimal async fixture, optional-service omission checks and preparation/build/startup/size metrics.
Those checks remain unrun. No final review agent has been requested.

Temporary traces in `trusted/check.rs`, `cleanup/transition.rs`, `terminal.rs` and `transfers.rs`, and the diagnostic binary under
`xtask/src/bin/`, are local only and must be removed before finalization. They are excluded from checkpoints.
Use normal tooling production artifacts rather than examples or compilation test binaries for diagnosis.
Do not require temporary artifacts to resume this task.

The connected GitHub account is verified as `lejmer`, with admin/push access to the public `lejmer/bray` repository.
Shell GitHub authentication is invalid. Use connected Git object APIs to publish checkpoints on existing PR #710.
Parent #709 remains open with the unchanged stack base. Push coherent checkpoints regularly as requested by the user.

The current-value and canonical-loan implementation is pushed through `6522a56a294774b58ea949f229ab4c97cf4624f1`.
The latest canonical-loan style and format checks passed. Its checker suite passed the expanded join matrix, with
414 tests passing and one retirement failure exposing lost physical predicate identity. The next uncommitted correction preserves an owned current context inside its borrowed view. A loan's lifetime identity
and the owned referent's physical context are different roles. Whole-object predicates discard the loan identity and keep
the owned current referent, so aliases of one referent agree while equal contents at different storage stay distinct.
Const calls discard both contexts and compute from data. `with_referent` retains an existing owned/scalar storage context
when refreshing its data. An unchanged current loan collapses to its bare form. Combined verification is pending. Observer preparation rollback now
passes. The capture controls retain numeric inactive acceptance, active numeric/Boolean rejection, real shared owner
acceptance, fabrication rejection, and promised/unpromised union distinction. The exposed mutable-marker control passes.

The canonical-loan batch replaces synthetic `Constructed(Borrowed(owner), OwnedTarget = contents)` with
`CurrentValue(Borrowed(owner), contents)`. This removes borrowed contents' backing interpretation and makes the same
current-value context handle owned and borrowed snapshots. `into_referent` exposes reached data separately from the retained loan. Predicate arguments keep the owned referent context after discarding the enclosing loan identity, so equal data at
different storage stays distinct while aliases agree. Const calls compute from data. Owned-components traversal excludes borrowed current contexts. Bare loans join and substitute
through their referents before predecessor proof. The ordinary structural-snapshot early return remains in place, while
current contexts join their changed contents. Broadly removing that early return regressed unrelated Source consumers.

A focused final-storage trace confirms task allocation's last `dispose_reference` call does not frame the local Current
root or its acquisition, despite retaining the original factory-result temporary. Do not assume the join correction fixes
that separate physical-effect case. The fallback reference-result bounds may include the Current input region. Investigate
established reference-output and typed-effect contracts, without inventing freshness from ownership or absent inputs.
Worker formation has an available row witness but no active trusted clause for that row at the final call. Do not activate
all carried witnesses: availability/ownership alone does not prove mutable current conditions.

Temporary traces remain excluded from checkpoints. `transfers.rs` additionally prints `BRA567_STORAGE` at the end of
preservation for diagnosis. Update checkpoint filtering or remove it before publishing. The task-owned optional Linux
resource probe derives from the existing allocation probe and runs separately, so it will not affect product-size receipts.

The pushed owned-referent context batch `159cd8281451bcad6da95081991416b8a729483b` passes all 415 checker units and
all 16 representative Source controls (captures, genuine/fabricated owners, nested unions, promises, raw owner transfers,
and exposed mutable markers). The full runtime sweep is 3,160 checks with 21 failures. The extra body failures versus the
15-failure canonical batch include activation abandonment, entry execution restoration and task dispatch.

The next uncommitted producer correction uses existing checked type/borrowed-input metadata to seed reference values
as loans and preserve that role during contents invalidation. Formal borrow inputs were seeded as bare values, so their
refresh could create an owning current context and make equivalent aliases disagree. Owned values continue to retain
owned referent contexts. This is a representation correction, not a fresh-allocation or immutable-owner framing rule.
No speculative relaxation of unknown effects has been implemented.

The temporary harness supports `BRA567_SUMMARY=1` to print existing storage-flow body effects and selected call reference
relations. The focused task-allocation summary shows `current()` has Atomic/Ambient effects, while admission and
`dispose_reference` still export Unknown among their other effects. Inspect actual reference relations before changing
scope or reachability. Do not assume a lost type or independent allocation without evidence.

Next: finish the typed loan-introduction batch and self-review, finalize style, then run checker units, full runtime
Source consumers and affected controls. Resolve remaining physical-effect and guarded-carrier failures together before
native/imported/cost acceptance and final review.

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
