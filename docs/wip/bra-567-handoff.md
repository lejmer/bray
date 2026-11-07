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

The checker distinguishes non-owning scalar `StorageValue` context, retained-owner `CurrentValue` contents, and
structural `Constructed` components. `OwnedTarget` represents real backing or a borrowed referent, rather than also
representing an owned value's current contents. The current batch removes that overloaded representation from
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
| Checker library unit suite | 415 passed before the carrier and current-value batches |
| Runtime Source bodies and execution certificates | Latest complete valid sweep: 3,160 checks, 17 failures before current-value batch |
| Same-site inactive numeric deferred frame | Accepted, active numeric and Boolean partial extraction rejected |
| Shared/stored owner and nested union forwarding | Real owner accepted, fabricated marker and payload authority rejected |
| Typed raw transfer controls | Unrelated nominal moves accepted, whole/nested owner moves and writes rejected for scalar/address owners |
| Mutable union completion | Promised owner accepted, unpromised owner rejected |
| Exposed mutable marker | Demonstrated old rejection of marker-free write, corrected behavior pending verification |
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

The current-value batch is implemented and undergoing finalization. It keeps the complete carrier until ownership
changes, and moving it reanchors its contents at the destination. Borrowed values retain their existing loan identity
and never acquire those owned components. A bare owner and a refreshed current value join their current observations.
The existing join regression now covers both branch shapes, owned and borrowed contexts, and whole/component carriers.
Current-value owners and captured-frame owners use identity-only substitutions. Current component witnesses follow
joined values. This prevents a contents substitution from recursively renaming its own owner.

Next: inspect required style/format results, run the checker library suite and full Source consumer sweep, then verify
the capture, raw-transfer, fabrication and exposed-marker controls as a combined model. Push the coherent checkpoint.
Imported/native acceptance, comparative costs and final review remain pending.

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
