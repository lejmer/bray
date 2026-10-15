# Runtime stack execution from Linear

## Agreed semantics and active implementation, 2026-10-13

The user accepted the closed observation dependencies and explicit trusted establishment/preservation boundary in the
[semantic proof](runtime-stack-semantic-proof.md), then authorized completing steps 1 and 2. The normative contract is
in [contract reasoning](../language/contracts-and-trust/contract-reasoning.md),
[trusted callers](../language/contracts-and-trust/trusted-caller-obligations.md) and
[witnesses](../language/contracts-and-trust/trusted-witness-values.md).

Continue on `feature/trusted-storage-integration`, the published combined branch. The implementation baseline is `e3f56424`, with proof checkpoint `7f300ef2`.
Steps 1 and 2 are complete. Shared authority analysis and its required removals passed published review at
`5144e732`. Step 3 production migration is complete at `a3de1083` with clean published review.
Step 4 dependency retirement and cleanup composition is complete at `246c4db3` with [clean published review](https://github.com/lejmer/bray/pull/607#pullrequestreview-5526458884).
The [Step 4 record](BRA-620-step4.md) contains its mapping, findings and acceptance evidence.
Dated progress entries below are historical evidence. The current verification checkpoint supersedes their status.

Updated 2026-10-12. The user accepted the architectural groundwork and requested executable Linear issues.
This checkpoint prepares that work. No new compiler, runtime or test behavior was implemented during issue preparation.
Start with [BRA-620](https://linear.app/bray-lang/issue/BRA-620). Follow actual blocking relations and each issue's
current implementation contract. No chat transcript is needed.

## Current implementation checkpoint

Steps 1-4 are complete on `feature/trusted-storage-integration` in [PR #607](https://github.com/lejmer/bray/pull/607).
Step 3 is published at `a3de1083` with [clean review](https://github.com/lejmer/bray/pull/607#pullrequestreview-5526203497).
Its native acceptance passes all 160 API cases, seven API plans and 25 outcome cases. All five original PR histories
are included and their branches are preserved. Follow [BRA-620 remaining work](BRA-620-remaining-work.md) and the
[Step 4 map](BRA-620-step4.md) for current work and evidence.

## Delivery decision, 2026-10-15

The user authorized readying and squash-merging #607 into develop at the completed Steps 1-4 boundary.
BRA-619's earlier implementation is included, but its native `CleanupIncidentTransfer` gap remains open and its
regression stays enabled. The merge does not complete that issue or the broader runtime initiative.

Continue with one PR per issue from updated develop after prerequisites land: BRA-502, BRA-554, BRA-558, then
BRA-619's remaining native acceptance. Continue Step 7 in the existing dependency order, also one PR per issue.
Keep the old branches and review history. Do not replay the squashed integration commits. Every issue retains its
own acceptance and independent review cycle. BRA-619's pending-outcome, typed-payload and provider-lifetime contracts
continue to constrain prerequisite design. This decision supersedes earlier instructions to keep #607 draft or finish
Steps 5-6 inside it.

## Original recovery checkpoint, retained as history

Repository: https://github.com/lejmer/bray
Original repair branch: `feature/bra-620-repair-synchronization-authority`.
Implementation recovery commit: `c744f48bc0cb5afee30351d2b91bdfafdcea0583`. This preserves the inherited incomplete repair.
The documentation commit that first adds this guide is its direct successor and is the preferred starting checkpoint.

For recovery of that historical checkpoint, fetch the original repair branch and inspect it. Reuse the existing repair worktree if it is
available and matches the published checkpoint. Otherwise create a clean checkout from the checkpoint, preserving any
other local work. Never reset an occupied worktree to follow this guide.

The original `feature/bra-620-complete-standard-library-trusted-storage-and-foreign` branch is historical stack ancestry.
It is not the restart branch. Its head `22473ff6384db081ad3e131e0726e16e7ff9c682` is already included.
The combined parent is `cca4bd4429d0648cae0493db72045b7d4a7a3e73`. All five live heads below were verified ancestors
before the checkpoint. Do not replay them or backmerge this combined descendant into the old BRA-620 PR.

| Issue | Existing PR | Included head |
| --- | --- | --- |
| BRA-609 | [#601](https://github.com/lejmer/bray/pull/601) | `f244c10c2dda07ca6fc9bb69a5483215e5feb724` |
| BRA-619 | [#602](https://github.com/lejmer/bray/pull/602) | `e3dbc6c5baca462ba185d902649534cb86a8cc4f` |
| BRA-620 | [#604](https://github.com/lejmer/bray/pull/604) | `22473ff6384db081ad3e131e0726e16e7ff9c682` |
| BRA-621 | [#605](https://github.com/lejmer/bray/pull/605) | `fb69346e31b225c39105d095e9f900c9cebad97c` |
| BRA-553 | [#606](https://github.com/lejmer/bray/pull/606) | `cca4bd4429d0648cae0493db72045b7d4a7a3e73` |

The recovery commit is deliberately incomplete and has no new acceptance claim. Earlier focused origin, authority,
style and native results apply only to their recorded checkpoints. Known remaining areas include the complete guard
carrier and mutation rules, Once protocol authority, RwLock overflow, report-pool construction, removal of repeated
analysis, and full source/import/native acceptance. Keep useful implementations and regressions. The removal ledger
identifies the superseded mechanisms. Do not treat all inherited code as either finished or disposable.

## Documents and authority

- [Rule coverage and replacement plan](runtime-stack-rule-coverage.md) is the current architectural implementation contract.
- [Closed inventory](runtime-stack-coverage-inventory.md) maps 461 entries to its 32 rules.
- [Architectural assessment](stack-architecture-assessment.md) retains source findings and growth attribution.
- [Earlier proposal](runtime-stack-implementation-plan.md) is historical and superseded.
- [BRA-620 evidence](BRA-620-remaining-work.md) and [stack evidence](stack-completion.md) retain dated results.

The plan, inventory and assessment are also tracked as documents attached to BRA-620 in Linear.
Repository documents remain the versioned source. Linear issue descriptions contain the actual work, removals and
acceptance needed for each owner, with immutable links to this checkpoint. Update the repository plan and affected
issues together when a material design decision changes. Do not create a competing plan in comments.

Language/specification and design ownership remain authoritative. No new syntax, protected initialized-storage type,
field privacy assumption, library-name authority, runtime witness registry or downstream semantic revalidation is part
of this plan. If the model cannot express a required rule, discuss that specific rule and costs before adding a mechanism.

## Execution and publication

1. Land the reviewed Steps 1-4 integration through the user-authorized squash merge of #607 into develop.
2. Keep BRA-619 open and retain its failing native regression. Its included partial implementation is the starting point.
3. Deliver BRA-502 physical backing, BRA-554 entry and BRA-558 incident consumption in dependency order, one PR per issue.
4. Finish BRA-619 in its own PR using those shared contracts, including sync/async native scope outcomes.
5. Continue the remaining protocol issues from updated develop, one issue and one PR at a time. Each consumer migration
   switches real callers and removes replaced Rust policy in that issue.
6. Complete packaging, native conformance and numerical cost gates through their existing issue owners.

The later sequence is synchronous teardown/TLS BRA-556/557 -> synchronous checkpoint BRA-608 -> providers BRA-555.
Activation BRA-561 can proceed after its own BRA-502/553 prerequisites. Continue BRA-505 -> BRA-506 -> BRA-508 and
BRA-484. Task/wait/lane/event/deadline work is BRA-560 -> BRA-562 -> BRA-563 -> BRA-564 -> BRA-565.
Workers BRA-566 require providers, TLS and events, then BRA-567 root and BRA-507 fallback. BRA-559 child test host
requires deadlines, fallback and incidents. BRA-568 packages the completed consumers. BRA-552 catalog completeness
can proceed early. BRA-192 and BRA-540/541/542 provide final native and cost acceptance. Existing parent and optimization
issues roll up their actual remaining requirements. See real Linear relations for independent branches and cross-dependencies.

## Issue ownership

All 45 listed issues were open when prepared. Closed issues are historical context only and receive no new work.
Original issue requirements remain in force. No issue was marked complete by this planning operation.

| Issue | Owned work | Rule references |
| --- | --- | --- |
| [BRA-192](https://linear.app/bray-lang/issue/BRA-192) | Prove native Bray platform and runtime independence across targets | R27-R32 |
| [BRA-281](https://linear.app/bray-lang/issue/BRA-281) | Deliver the native runtime entirely in Bray | R01-R32 |
| [BRA-402](https://linear.app/bray-lang/issue/BRA-402) | Make direct-await execution competitive without transition overhead | R22-R25, R31-R32 |
| [BRA-404](https://linear.app/bray-lang/issue/BRA-404) | Minimize nontrivial product footprint without shared runtime floors | R24, R28, R31-R32 |
| [BRA-406](https://linear.app/bray-lang/issue/BRA-406) | Make complete compilation and library reuse competitive | R29-R31 |
| [BRA-409](https://linear.app/bray-lang/issue/BRA-409) | Ship Bray products without project-owned non-Bray implementation | R01-R32 |
| [BRA-428](https://linear.app/bray-lang/issue/BRA-428) | Close the remaining native performance and executable size gaps | R24, R28-R32 |
| [BRA-429](https://linear.app/bray-lang/issue/BRA-429) | Finish direct bulk transfer performance for static I/O adapters | R08-R10, R27, R31 |
| [BRA-456](https://linear.app/bray-lang/issue/BRA-456) | Define performance regression gating and time-limited waivers | R31 |
| [BRA-482](https://linear.app/bray-lang/issue/BRA-482) | Deliver conditional execution guarantees and completion-aware finalization | R17-R25, R29-R32 |
| [BRA-484](https://linear.app/bray-lang/issue/BRA-484) | Complete executable native array and general generator construction | R01-R06, R12-R18, R22-R25, R32 |
| [BRA-502](https://linear.app/bray-lang/issue/BRA-502) | Secure physical local cleanup backing before ownership becomes live | R01, R17-R18, R20-R25, R28, R32 |
| [BRA-505](https://linear.app/bray-lang/issue/BRA-505) | Complete admitted ordinary-scope async and fallible finalization | R17-R25, R28, R32 |
| [BRA-506](https://linear.app/bray-lang/issue/BRA-506) | Complete represented-owner cleanup through the shared admitted lifecycle | R01-R06, R16-R25, R28, R32 |
| [BRA-507](https://linear.app/bray-lang/issue/BRA-507) | Complete suspending host cleanup and retained fallback execution in Bray | R17-R28, R32 |
| [BRA-508](https://linear.app/bray-lang/issue/BRA-508) | Align resource APIs with completion semantics | R17-R25, R28, R32 |
| [BRA-540](https://linear.app/bray-lang/issue/BRA-540) | Verify direct-await execution and allocation costs with the Bray runtime | R22-R25, R31-R32 |
| [BRA-541](https://linear.app/bray-lang/issue/BRA-541) | Verify linked footprint and retained resource costs with the Bray runtime | R24, R28, R31-R32 |
| [BRA-542](https://linear.app/bray-lang/issue/BRA-542) | Verify complete compilation and reuse costs with the Bray runtime | R29-R31 |
| [BRA-547](https://linear.app/bray-lang/issue/BRA-547) | Deliver Bray host, attachment and provider lifetime | R16-R21, R24, R26-R28, R32 |
| [BRA-548](https://linear.app/bray-lang/issue/BRA-548) | Deliver Bray asynchronous execution services | R16-R25, R28, R32 |
| [BRA-552](https://linear.app/bray-lang/issue/BRA-552) | Replace fixed standard-library test counts with catalog completeness checks | R31 |
| [BRA-553](https://linear.app/bray-lang/issue/BRA-553) | Establish Bray host ownership and bootstrap attachment | R01, R02, R05, R16, R24, R26-R28, R32 |
| [BRA-554](https://linear.app/bray-lang/issue/BRA-554) | Implement synchronous and foreign entry authority in Bray | R01, R12, R16, R24, R27-R28, R32 |
| [BRA-555](https://linear.app/bray-lang/issue/BRA-555) | Implement independently loaded provider ownership in Bray | R01, R16-R18, R24, R26-R28, R32 |
| [BRA-556](https://linear.app/bray-lang/issue/BRA-556) | Implement synchronous static teardown and retained shutdown ownership in Bray | R17-R21, R24, R26, R28, R32 |
| [BRA-557](https://linear.app/bray-lang/issue/BRA-557) | Implement exact-thread attachment and dependency-ordered TLS cleanup in Bray | R16-R21, R23-R24, R26, R28, R32 |
| [BRA-558](https://linear.app/bray-lang/issue/BRA-558) | Implement host incident consumption and report output in Bray | R17-R21, R24, R28, R32 |
| [BRA-559](https://linear.app/bray-lang/issue/BRA-559) | Implement the child test host and session protocol in Bray | R17-R28, R31-R32 |
| [BRA-560](https://linear.app/bray-lang/issue/BRA-560) | Implement started-task admission and terminal ownership in Bray | R05, R16-R25, R28, R32 |
| [BRA-561](https://linear.app/bray-lang/issue/BRA-561) | Implement compiler-described activation and direct-await execution in Bray | R01, R05, R16-R18, R20-R25, R28, R32 |
| [BRA-562](https://linear.app/bray-lang/issue/BRA-562) | Implement cooperative cancellation and retained wake registration in Bray | R16, R20-R25, R28, R32 |
| [BRA-563](https://linear.app/bray-lang/issue/BRA-563) | Implement lane dispatch and running-work limits in Bray | R16, R23-R24, R28, R32 |
| [BRA-564](https://linear.app/bray-lang/issue/BRA-564) | Implement event notification and committed delivery in Bray | R16, R23-R24, R28, R32 |
| [BRA-565](https://linear.app/bray-lang/issue/BRA-565) | Implement monotonic deadlines and timeout cancellation in Bray | R16, R23-R24, R28, R32 |
| [BRA-566](https://linear.app/bray-lang/issue/BRA-566) | Implement demanded worker lifetime and exact-thread shutdown in Bray | R16, R23-R24, R26-R28, R32 |
| [BRA-567](https://linear.app/bray-lang/issue/BRA-567) | Implement async root and main-lane orchestration in Bray | R17-R24, R26, R28, R32 |
| [BRA-568](https://linear.app/bray-lang/issue/BRA-568) | Publish only demanded Bray runtime implementation artifacts | R27-R31 |
| [BRA-574](https://linear.app/bray-lang/issue/BRA-574) | Optimize and select the required native program before LLVM | R29-R31 |
| [BRA-588](https://linear.app/bray-lang/issue/BRA-588) | Validate optimized demand and native granularity across the program corpus | R29-R31 |
| [BRA-608](https://linear.app/bray-lang/issue/BRA-608) | Complete synchronous Bray products without linked Rust implementation | R24, R26-R32 |
| [BRA-609](https://linear.app/bray-lang/issue/BRA-609) | Enforce trusted caller obligations and live witness authority | R02-R18, R25-R30 |
| [BRA-619](https://linear.app/bray-lang/issue/BRA-619) | Implement selected scope enter and exit invocation | R12, R16-R21, R23-R25, R28, R32 |
| [BRA-620](https://linear.app/bray-lang/issue/BRA-620) | Complete standard-library trusted storage and foreign contracts | R01-R23, R25-R31 |
| [BRA-621](https://linear.app/bray-lang/issue/BRA-621) | Repair trusted storage authority in the existing Bray bootstrap report transport | R02, R05, R08-R10, R24, R27-R29 |

## Completion discipline

Derive each mechanism's matrix from the rule inventory before implementation. Cover each distinct transfer and critical
composition, not the Cartesian product of every axis. Retain old regressions and explain any non-applicable variant.
A defect in an existing rule goes to its owning transfer and all equivalent paths. A missing model requirement requires
a model correction, not a consumer bypass. Remove superseded mechanisms when consumers switch.

Trust established semantic results downstream. Separate non-unwind, totality, completion, capacity, physical target,
lifetime anchor, carrier and observation facts. Preserve no-allocation mandatory cleanup and whole-provider retention.

Record production-code and test growth separately. Count prepared graphs, solved states, summary/body loads and narrow
demand reuse. Proof-only guard changes add no runtime fields/allocations/atomics/lookups. Same-run ready direct awaits
add no per-transition allocation/enqueue. Account for backing bytes/alignment and retained code/data/providers.

Apply BRA-456 regression policy at meaningful checkpoints. Final BRA-540 execution must match the faster Rust/C++ peer
outside variability. BRA-542 complete-process and reported-work rows and aggregate are at most 1.25 times the faster
peer, with complete-process also no slower than current rustc. BRA-541 and BRA-428 retain their original size/resource
gates and the stricter overlapping rule. Expose LLVM/link/setup/published-consumer and peak memory costs.

Before publishing implementation, complete the repo maintainability audit, required mutating style/format commands,
focused and broad relevant checks, and the Bray PR workflow review. Use the existing shared Cargo target and run native
toolchains outside the sandbox. Fail-fast skips and exclusions remain outstanding acceptance. Do not declare a protocol
complete merely because one consumer compiles or its PR is In Review.

This issue-preparation task did not run compiler experiments or add implementation. The checkpoint preserves existing
work and records known gaps rather than certifying it. No old branch was rewritten, no PR merged or closed, and no
closed Linear issue was changed.
