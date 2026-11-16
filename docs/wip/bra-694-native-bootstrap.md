# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Draft PR: https://github.com/lejmer/bray/pull/717.
Branch: `feature/bra-694-native-bootstrap-contracts`, rebased onto develop 5de46243b.
Pushed checkpoint: e9c174764. PR #710 and its prerequisites are merged.
Reviewer: `/root/bra694_review`, clean published review through e9c174764:
https://github.com/lejmer/bray/pull/717#pullrequestreview-5914521927.

## Contract

Make supported Linux and Windows bootstrap products check and lower for x86_64 and AArch64.
Preserve shared fields, disjoint owners, source and imported contracts, and exact negative rejection locations.
Reject fabricated owners, replacement, previous epochs and actual raw mutation or retirement. Compiler defects panic.
Execution and shutdown remain BRA-695 and BRA-696. Performance is separate work.

Trace actual producers and consumers before semantic changes. No historical reconstruction, freshness from
owned_allocation, exact identity from an inexact region, layout projection onto alias bounds, predicate reassertion,
or blanket shared-loan exemption. Keep branch masks and authoritative empty ownership records.
Stop on unexplained fixed-suite regressions. Keep good coherent design changes even when another defect remains.
Commit and push checkpoints, then publish independent review on the PR. Defer style until finalization.

## Evidence and open gates

- 563 checker and 136 bound-tree tests pass on core 1440b142a. Protected raw-read, Uninit and nested-buffer composition,
  conditional future active/inactive/drop, allocator separation, nullable masks and recursive lazy cleanup controls pass.
- Frozen 1,718 on 1ceb765eb had three regressions and seven panic timeouts. The three pass focused checks after 1440.
  No atomic-frame panic has recurred in focused runs. Corrected broad comparison remains unrun.
- c606 removes four redundant whole-provider test identities. Actual native product gates replace them. Record those
  replacements explicitly when comparing frozen identities, never count deleted tests as passes.
- All 27 named production probes pass in 16.92 seconds. One immutable source compilation shares dependencies and a
  CPU-sized worker pool. First probe spends 10.25 seconds on cold dependencies, the other 26 total about two seconds.
  These check and lower small source functions, without native execution. Remaining cold cost is performance work.
- Five native artifact reuse controls pass. Thirteen boundary, totality and cache controls pass in 19.1 seconds.
- 5fac observer controls pass in 120 ms, all 62 catalog tests pass, and four adjacent allocator/raw-read/opaque-carrier
  controls pass in 4.72 seconds. Source and imported preservation and impurity rejection are covered.
- Current eight-file source ordering batch: both native focused controls still fail at dispatch_ready/context(view_owned)
  and start_frame/commit_start/scoped exit authority, in 40.51 seconds. No new diagnostic category appeared.
- Extended protected nested fixture passes in 4.67 seconds: nullable record buffers, incoming raw read through a function
  return, bounded retirement summaries, source/imported positives and exact negative sites. No return-transport defect
  was established. Its negative generator now changes only the intended function and uses unambiguous call text.

Still mandatory: four-target actual product checks/lowering, event layout, fixed comparison, owned/deferred matrices,
projection/cleanup gates, fixed-five/loop regression controls, maintainability audit, mutating style and Bray formatting.
Native toolchain operations run outside the sandbox. Do not rerun broad suites before the current contract is resolved.

## Setup and artifact ownership

Readiness, composition, native std tests, smoke and default performance preparation share the existing canonical
per-target standard-library bundle and release runtime destination. Prepare once and assemble from those bundles.
Explicit outputs and pinned inputs stay exact. Carry the builder's Package inventory through readiness to smoke.
Publication locks reject concurrent writers but do not pin readers or promise single-flight waiting. Avoid competing
configurations while consuming a pair. Use shared Cargo and LLVM storage, never clean shared storage wholesale.

Production probes share one immutable discovered program with a bijective named-case inventory. Source edits invalidate
compiler-local semantic identities, including unchanged-source certificates. Do not bypass that existing contract.

## Current design commitments

The platform aligned allocator publishes origin separation only, without access or lifetime authority. Event.Reference
stores its actual typed Domain pointer. Physical inline contents retain UninitPayload and stop at OwnedTarget. Existing
backing records keep their actual physical coordinates through moves. Real retirement removes current contents and aliases.
Normal return publication retains whole-value source information and the actual current pointer graph.

Guarded cleanup expansion supplies effect paths, finite pointer demands and source callee discovery before component
solving. Entry seeding follows prerequisite pointers. Cleanup, calls and stored replay use the same segmented resolver
and preserve AnyElement bounds. Nested element disposal and backing retirement are distinct effects. Static and imported
paths use declared types and the existing publication machinery. Never recover layout from an inexact Returned bound.

The ordinary static-reference exit policy is separate from lifetime requirements. host_storage can return the local
static OR an admitted external View.host pointer. BorrowFrom's BINDING lifetime anchor is not that external storage's
identity. Preserving only the local static arm cannot bound all host writes.

5fac recognizes only the two authorized bodyless OS thread-identity getters as storage observers. Imported summaries
and real source-body inference take precedence. Their execution remains impure, foreign-call capability stays exact,
and every ordinary same-symbol extern retains its conservative effects. Exports serialize the producer-owned summary.
Final native summaries confirm host_access has only Atomic effects and domain getters stay bounded.

Reservation is inlined in start_frame's existing scoped PreparedStart owner. Store registration before demand_lane.
Failed admission withdraws it once before releasing services. Commit transfers it under the task lock, clears the field
before wake and leaves scope exit only the preparation's independent services. Never move the scoped capability away.

## Source lifetime repair in progress

`_ = value` creates anonymous owned storage whose cleanup occurs at scope exit. It does not destroy the value immediately.
The correlated native trace shows the arena's inner pointer is seeded and survives host_access and take_retired_arena.
release_execution_domain's legitimate ambient host-field update then removes it before delayed entries cleanup.
The normal-return hypothesis was rejected by the current-pointer trace and extended small fixture.

The coherent ordering batch changes only explicit lifetimes:

- task/arena: put retired and entries in an inner lexical block. Reverse cleanup destroys entries before the remaining
  allocation. Reuse ArenaAdmission around that block to release the domain after cleanup on normal exit or unwinding.
- task/retirement: end retired control before releasing its execution pin and arena owner.
- execution/entry_scope: destroy optional operation Current before retiring its context.
- execution/lifetime: end emptied Once cells and locks before scheduler and arena release. Keep the original allocation
  binding until function exit instead of creating a newer anonymous allocation owner.
- deadline/storage and worker/storage: destroy state before scheduler release and keep allocation to function exit.
- scheduler/storage: destroy the moved Domain value before explicitly freeing its allocation.
- execution/initialization: destroy a rejected allocation before finishing its native provider.

CFG resolves nested scopes before enclosing scopes on normal, panic and cancellation paths. with.exit runs before locals
in its own body, so ArenaAdmission requires an explicit inner block and no direct body-owned resources or trailing release.
The existing guard avoids a new framework. Production growth is 15 source lines, reconsidered from the zero-growth
hypothesis because the guard is needed on abnormal exits. Scalar ended acknowledgements and inert Uninit transfers remain.
Explicit native-release calls skipped after a source-call panic are existing separate execution/shutdown work.

Oracles are both native owner/registration controls, finite actual retirement effects, the small composition fixture and
fixed raw-mutation negatives. Reconsider on skipped/duplicate cleanup, widened authority or release before cleanup.
The source ordering batch has not resolved view_owned and is not claimed to complete native composition.

## Active diagnosis and next action

Production tracing is removed. A temporary test-only BRAY_AUDIT_CALLABLES branch in contract_tests/bootstrap.rs
queries fully qualified source callable summaries and checks/lowers selected bodies. BRAY_AUDIT_WRITES inspects only
the existing dependency-membership graph for untyped unbounded writers, without reconstructing another graph. These
runs are diagnostic probes, never native gate passes. Remove the entire branch before commit. Resolve test executables from Cargo's emitted path, never a retained
hard-coded filename. The final producer trace finished in 41.83 seconds. An earlier stale-executable run supplied no
evidence and is excluded above.

Trace dispatch_ready's actual wait_execution effect and the corresponding stable owner observation. It transfers no
owned Current, so input movement cannot explain selected's loss. Dependency contracts, refinements and flow project the
same BodySemantics publication. Normalized and actual table effects now agree. No publication/equality defect is established. Do not call the public view
shallow. The first untyped unbounded Write is published by host.insert_attachment (owner.bray), on a normal path, at an
actual entry.identity assignment. It propagates through attachment acquisition into wait_execution. The small indexed
assignment has the same missing physical target. The no-write indexed borrow does not publish a mutation.

The owning defect is memory::typed_target requiring exact-cell identity to use a known current typed view. A dynamic
selector supplies a location but remains inexact. Accept exactly one nonprevious target with matching recorded type and
no unknown, displaced or region-only flags. Return its unchanged target. Formation and entry seeding still declare the
actual view type, segmented projection follows that view, actual writes stay weak, and effect publication stays bounded.
Keep References::is_exact, value_targets and borrow_targets strict. Region bounds cannot supply layout, multi-target
choices cannot become one identity, previous epochs remain invalid and no initialization or authority is introduced.

Oracles extend the existing slice memory test and imported call-effects fixture: dynamic typed projection is available,
exact-cell queries remain unavailable, region-only projection stays rejected, an indexed loan/read has no write, and a
real indexed replacement stays bounded. Reuse all checker controls and native focused gates. Production growth is five
lines in the existing helper with no new representation. Reconsider on strong updates, alias-layout recovery, loop
instability or any unexplained fixed-suite regression. The fallback for genuinely unresolved writes remains conservative.
Current typed-view evidence: all 563 checker tests pass in 220 ms and the expanded imported indexed test passes in 80 ms.
Both native controls still fail at the same authority sites. The selective query confirms insert_attachment no longer
publishes an untyped unbounded Write, but wait_execution still does. Its other writer producers remain to be inventoried.
Do not migrate the entire effect publisher as a workaround. That independent change would need explicit move/private
admission/borrow-descriptor/call-completion coverage.

A second verified producer defect affects four more OS leaves on the attachment-load path: Linux pthread_getspecific,
Windows FlsGetValue, GetLastError and SetLastError. Current Windows summary probes confirm generic Ambient/Escape.
Their implementations access OS-private local state without dereferencing or retaining Bray payloads. Extend the same
authorized bodyless policy after imported/source summaries. Keep all calls impure, pointer results independent/unknown,
and the source wrapper's actual output-pointer write. Keep pointer-retaining setters, key creation, FLS destruction,
callbacks, allocators and deallocators conservative or precisely mutating. No purity/access/freshness is introduced.
The existing platform fixture now covers all six leaves using shared parameterized source in both import modes, fake
ordinary names and purity negatives. Catalog has 16 recognized scopes and 162 declarations. New digest is
460af27514d774f00024c7668824d92416c2616ea4fe71a6cbfcd3a48bc97ce1. Generation and fresh test/CLI builds passed. All 62 catalog tests, 563 checker tests and the expanded source/import platform controls pass. Both normal native gates still fail in 41.49 seconds. The diagnostic-only Windows run takes 17.25 seconds and confirms empty effects for the three new Windows leaves. Three cached callable summaries still contain the untyped unbounded Write. The bounded producer trace identifies publish_event_wait state.head and state.tail assignments, propagated through register_execution_wait to wait_execution. Before head, state has two current exact Entry targets for the Event and ExecutionEvent alternatives. Both have the expected State type. The one-target resolver rejects that finite choice, publishes a global Write and globally invalidates memory, losing state before tail. Descriptor-retention and historical-layout hypotheses do not explain this first divergence.
Primary contracts: glibc nptl/pthread_getspecific.c and Microsoft FlsGetValue/GetLastError/SetLastError documentation.
Reconsider if any leaf dereferences payloads, retains arguments, runs callbacks, masks a real source body or gains authority.

Next repair contract: preserve finite typed alternatives across calls, direct joins and stored paths without creating a synthetic exact cell. Reuse existing target sets, Vec effects and weak memory writes. Actual assignment effects, observations, terminal checks and physical writes must agree. Single-target proof/strong-update queries stay strict. Reference output staging cannot initialize or install the new value definitely at every candidate. Resolve that staging boundary before production edits. Required controls include bounded summaries through source/import, branch and call selection, overwritten candidates, disjoint owners, real raw mutation and unchanged strict exact/unknown/previous checks. Do not normalize plural aliases into a Returned cell to make a one-target API succeed. The retained reviewer independently confirmed that would introduce exact-cell paths and lose exact alias export.

Remove temporary tracing, validate and push the next checkpoint after completing this contract. Obtain an official incremental
review on the PR. Run the required gates on corrected code, then final maintainability/DRY audit and mutating style/format.
Update PR description and mark ready/BRA-694 In Review only after acceptance, required validation and published review.
