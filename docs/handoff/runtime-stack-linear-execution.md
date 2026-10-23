# Runtime stack execution from Linear

## Current work

The authority and cleanup foundation landed in [#607](https://github.com/lejmer/bray/pull/607).
Physical backing, entry authority and incident consumption landed in
[#608](https://github.com/lejmer/bray/pull/608), [#609](https://github.com/lejmer/bray/pull/609)
and [#610](https://github.com/lejmer/bray/pull/610).
BRA-561 is implemented and independently reviewed in [#611](https://github.com/lejmer/bray/pull/611).
BRA-626 is implemented and independently reviewed in [#613](https://github.com/lejmer/bray/pull/613),
stacked on BRA-505. Its conversion, authority, import and native cleanup acceptance passes.
Finish BRA-505's remaining native boundary audit in [#612](https://github.com/lejmer/bray/pull/612),
then BRA-619, with one reviewed stacked PR per issue. Do not merge without user approval.

Use [the active BRA-505 record](BRA-505-retained-incidents.md) for the branch,
requirements, evidence and next work. [BRA-619 acceptance](BRA-619-remaining-work.md)
remains required after the shared incident prerequisite and the BRA-626 construction-conversion repair.
BRA-626 blocks BRA-619 in Linear. Its repo-wide workaround audit is recorded in #613, with the
ordinary-finalization preparation locals and two reference fixture detours removed. Its independent
review is 5565548084. Preserve these direct inputs when completing BRA-619. Deliberate state,
ownership and witness boundaries remain. No identified construction workaround remains.
Its earlier selected-scope
implementation is already included. Do not replay the old stack.

## Contracts and issue ownership

- [Rule coverage and replacement plan](runtime-stack-rule-coverage.md) defines the architectural work.
- [Semantic proof](runtime-stack-semantic-proof.md) derives the agreed rules from the language.
- [Closed inventory](runtime-stack-coverage-inventory.md) maps operations to the rule owners.

The language specification and design documents remain authoritative. Linear issue descriptions
contain each owner's work and acceptance. Follow their current blocking relations. No new syntax,
field privacy assumption, library-name authority, runtime witness registry or downstream semantic
revalidation is authorized. Discuss a language gap before adding a feature.

## Handoff lifetime

Keep only documents used by unfinished work. Completed issue records and superseded proposals
are removed. Their evidence remains in Git and PR history. Delete the BRA-505 record after its
acceptance and review are complete, carrying only the remaining BRA-626 and BRA-619 inputs into their active records.
Delete the BRA-619 record when that issue's implementation, review and fixes are pushed.
Before finishing the stack, remove its remaining handoffs. Preserve any still-needed architectural
contract in its owning design documentation and any broader initiative work in its open Linear
issues, then remove this temporary execution guide, proof, coverage plan and inventory.

## Later initiative work

Beyond the current BRA-505/BRA-626/BRA-619 stack, follow the existing issue dependencies, one PR per issue.
The later sequence includes synchronous teardown/TLS BRA-556/557, synchronous checkpoint BRA-608,
providers BRA-555, represented owners BRA-506/508/484, task/wait/lane/event/deadline work
BRA-560/562/563/564/565, workers BRA-566, roots BRA-567, fallback BRA-507 and child test hosts BRA-559.
BRA-568 packages completed consumers. BRA-552 owns catalog completeness. BRA-192 and
BRA-540/541/542 own final native and cost acceptance. These are not additional work silently added
to the current stack. Consult current Linear status rather than treating the original inventory
below as a list of open issues.

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
