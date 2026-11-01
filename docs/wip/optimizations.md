Bray has exactly three optimization levels: **`None`, `Basic`, and `Full`**. Development builds use `Basic`; release builds use `Full`.

Below, **Level means the minimum level we recommend for future implementation**, not an existing setting for that optimization:

| Level   | Meaning                                                                                                                   |
|---------|---------------------------------------------------------------------------------------------------------------------------|
| `None`  | Canonical representation, required lowering, or runtime implementation used at every level. No optional MIR optimization. |
| `Basic` | Cheap, bounded transformations suitable for development builds. Also enabled under `Full`.                                |
| `Full`  | More involved analysis, specialization, or transformations that can increase compilation cost or complicate debugging.    |

**Status:** **Missing** means we have a concrete implementation gap; **Extend** means relevant machinery already exists; **Candidate** means a plausible optimization needing a focused legality and benefit investigation.

This is a comprehensive inventory of optimization families. It covers generated execution, memory use, and binary size; individual peepholes should belong under these families.

**Already implemented**

We should keep these off the “missing” list: generic specialization and static implementation selection; scalar constant/copy propagation; branch pruning; local pure scalar expression elimination; bounded direct MIR inlining; small union discriminants; zero-size argument omission; demand-based native selection; native library reuse; ThinLTO; and static I/O bulk lowering.

LLVM already handles ordinary instruction simplification, register allocation, vectorization, loop transformations, and eligible tail recursion. The opportunities below need semantic information, representation decisions, or dependency pruning that Bray must supply.

**Representation and layout**

LLVM generally cannot change a layout after Bray has committed to its offsets and ABI.

| # | Optimization                                                                                                                                                                                   | Level  | Status    |
|---|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|-----------|
| 1 | **Pointer and scalar niche filling.** Represent absence using proven invalid values: null for eligible borrows, spare boolean representations, invalid character values, or unused union tags. | `None` | Missing   |
| 2 | **Propagate and allocate niches through aggregates and nested nullable/union types.** Track remaining niches rather than adding a fresh tag at every layer.                                    | `None` | Missing   |
| 3 | **Remove redundant single-variant union tags.** Preserve the semantic variant without storing an unnecessary discriminant.                                                                     | `None` | Missing   |
| 4 | **Reorder default-layout product fields to reduce padding.** Preserve semantic declaration and destruction order; respect explicit layout contracts.                                           | `None` | Missing   |
| 5 | **Optimize default union payload field layout.** Reduce padding within variants and therefore the maximum payload size.                                                                        | `None` | Missing   |
| 6 | **Compact compiler-owned state.** Use appropriately sized discriminants and packed internal flags where independent addressing is unnecessary.                                                 | `None` | Candidate |

For these, `None` means selecting a good canonical layout at every level. Representation should agree across development and release builds. Ordinary source fields cannot simply become bitfields when borrowing or addressability should change.

The current [nullable and aggregate layout implementation](/workspace/bray/crates/bray-compilation/src/compilation/product/realization/aggregate.rs:296) provides direct evidence for the first five gaps.

**Storage, movement, allocation, and reference counting**

LLVM can remove some copies, but often lacks the ownership and lifetime facts needed to remove the operations that caused them.

| #  | Optimization                                                                                                                                                         | Level   | Status    |
|----|----------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|-----------|
| 7  | **Construct directly into the final destination.** Extend result-place passing through aggregate construction, returns, branches, and owned arguments.               | `Basic` | Extend    |
| 8  | **Coalesce owned temporary and move chains.** Transfer the destination and its obligations instead of materializing successive intermediate owners.                  | `Basic` | Candidate |
| 9  | **Scalar replacement of owned aggregates.** Keep components separate when whole-object storage is unnecessary, while preserving cleanup and dependency tracking.     | `Full`  | Candidate |
| 10 | **Promote eligible nonescaping owned allocations to stack or enclosing storage.** Requires allocation, identity, storage-policy, and failure semantics to permit it. | `Full`  | Candidate |
| 11 | **Coallocate compiler-managed backing.** Combine related frame, bookkeeping, and cleanup allocations when their lifetimes permit it.                                 | `Full`  | Extend    |
| 12 | **Reuse exclusively owned buffers.** Consume an input buffer as an operation’s destination when capacity and ownership permit.                                       | `Full`  | Candidate |
| 13 | **Cancel redundant local string retain/release pairs.** Use ownership facts to remove otherwise observable atomic operations.                                        | `Basic` | Missing   |
| 14 | **Optimize reference-count placement across branches and calls.** Avoid retaining merely to cross a boundary that can preserve borrowed or transferred ownership.    | `Full`  | Candidate |
| 15 | **Elide reference counting or atomics for proven unique or confined ownership.** Requires proof over every relevant alias and transfer.                              | `Full`  | Candidate |

Custom `Storage` implementations can execute observable code. “The box does not escape” is therefore insufficient justification for deleting its construction or release.

**Cleanup, finalization, and failure transport**

Bray owns the meaning of these operations. LLVM sees the resulting calls, state, and memory accesses.

| #  | Optimization                                                                                                                                                                 | Level   | Status    |
|----|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|-----------|
| 16 | **Prune cleanup scaffolding after concrete specialization.** Remove state and helpers that remain after a type’s cleanup becomes trivial.                                    | `Basic` | Extend    |
| 17 | **Eliminate unnecessary initialization and cleanup flags.** Replace runtime flags with proven initialization, active-variant, and partial-move state.                        | `Full`  | Candidate |
| 18 | **Use caller or enclosing-frame storage for cleanup capacity.** Avoid separately managed backing where existing storage satisfies the contract.                              | `Full`  | Extend    |
| 19 | **Reuse cleanup and incident storage across disjoint lifetimes.** Preserve admission guarantees and ownership of retained outcomes.                                          | `Full`  | Candidate |
| 20 | **Forward report and outcome destinations directly.** Avoid intermediate report storage, initialization, and movement.                                                       | `Basic` | Extend    |
| 21 | **Specialize call sequences using established execution guarantees.** Omit unnecessary panic/outcome machinery when the selected call and surrounding obligations permit it. | `Full`  | Candidate |
| 22 | **Remove unused calls proved pure and total before demand discovery.** Preserve argument evaluation, requirements, and cleanup that still have effects.                      | `Basic` | Missing   |
| 23 | **Collapse trivial aggregate cleanup traversal.** Omit per-element or per-field traversal when the concrete lifecycle proves it unnecessary.                                 | `Basic` | Extend    |

Completion-based finalizer discharge already exists as a semantic rule. That should not be recreated as an optional optimization. Likewise, proving that the current value is complete does **not** automatically permit releasing its cleanup allowance.

**Contracts, checks, patterns, and semantic facts**

Some of these enable LLVM optimizations; others must happen earlier so unnecessary helpers and dependencies never enter the native program.

| #  | Optimization                                                                                                                                                             | Level   | Status    |
|----|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|-----------|
| 24 | **Carry more established conditions into concrete MIR simplification.** Preserve useful postconditions and refinements through specialization and inlining.              | `Basic` | Extend    |
| 25 | **Use loop invariants to eliminate repeated bounds and requirement checks.** Relate induction variables, lengths, slices, and mutation validity.                         | `Full`  | Candidate |
| 26 | **Simplify exact contract arithmetic before runtime lowering.** Avoid unnecessarily expensive overflow-aware implementations when an equivalent cheaper check is proved. | `Basic` | Candidate |
| 27 | **Expose proven arithmetic ranges to LLVM.** Supply valid facts about overflow, division, shifts, conversions, and alignment.                                            | `Basic` | Extend    |
| 28 | **Expose scalar validity facts.** Communicate proven non-nullness, valid discriminants, and restricted value ranges.                                                     | `None`  | Extend    |
| 29 | **Expose borrowing and memory-effect facts.** Derive valid alias, capture, dereferenceability, and memory-access attributes from established contracts.                  | `None`  | Extend    |
| 30 | **Propagate aggregate contents and active variants.** Extend current scalar/nullable propagation to fields, union payloads, and projections.                             | `Basic` | Extend    |
| 31 | **Share pattern tests and projections across match arms.** Build better decision structures while preserving guard order, borrowing, and consumption.                    | `Full`  | Candidate |
| 32 | **Partially evaluate eligible runtime calls with constant inputs.** Reuse constant-evaluation machinery under bounded work and the correct execution contract.           | `Full`  | Extend    |

For rows 28–29, emitting already established facts can happen at every level. Expensive new analysis to discover additional facts should need its own `Basic` or `Full` policy.

Bray already emits some ABI attributes. The opportunity is broader, evidence-backed coverage—not blanket `noalias` on mutable-looking pointers.

**Dispatch and specialization**

LLVM can sometimes recover these after inlining. Bray can exploit them earlier, including when determining which implementations must be compiled.

| #  | Optimization                                                                                                                                                                                  | Level   | Status    |
|----|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|-----------|
| 33 | **Devirtualize trait-view calls with known concrete witnesses.** Resolve the call and remove unnecessary local dispatch metadata where possible.                                              | `Full`  | Extend    |
| 34 | **Resolve known callable values to direct calls.** Follow callable identities through assignments and control-flow merges before dependency discovery.                                        | `Basic` | Candidate |
| 35 | **Specialize ordinary calls on profitable constant arguments.** Create bounded variants for fixed modes, flags, or selectors without requiring source generics.                               | `Full`  | Missing   |
| 36 | **Broaden ownership-aware MIR inlining.** Extend current eligibility to additional aggregate, lifecycle, and dispatch cases.                                                                  | `Full`  | Extend    |
| 37 | **Share generic implementation bodies when type parameters do not affect execution.** Reduce duplicate machine code while preserving callable identity, statics, and type-dependent behavior. | `Full`  | Candidate |

Bray lambdas do not capture enclosing locals, so conventional closure-environment shrinking is not a missing Bray optimization.

**Async and protected frames**

LLVM cannot reconstruct Bray’s suspension, cancellation, stable-address, and cleanup rules from ordinary runtime calls.

| #  | Optimization                                                                                                                                                    | Level  | Status    |
|----|-----------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|-----------|
| 38 | **Remove non-suspending async forwarding overhead.** Lower eligible wrappers to equivalent direct execution with all effects preserved.                         | `Full` | Extend    |
| 39 | **Construct known future frames directly in caller or enclosing-frame storage.** Avoid unnecessary allocation and relocation.                                   | `Full` | Extend    |
| 40 | **Shrink persistent frames using suspension liveness.** Keep temporary work outside the frame when it need not survive suspension or another required callback. | `Full` | Extend    |
| 41 | **Overlap frame slots with disjoint lifetimes.** Include resume, cancellation, cleanup, and address-stability requirements in the analysis.                     | `Full` | Missing   |
| 42 | **Eliminate representation erasure for closed future flows.** Keep concrete storage and dispatch through known producers, merges, parameters, and returns.      | `Full` | Extend    |
| 43 | **Fuse eligible direct-await transitions.** Remove redundant state changes, descriptor accesses, and callback boundaries within the same run.                   | `Full` | Extend    |
| 44 | **Optimize recursive async storage when depth is provably bounded.** Use bounded enclosing storage rather than a general dynamic representation.                | `Full` | Candidate |
| 45 | **Improve task/control/frame coallocation.** Use the runtime’s permitted allocation strategy without introducing separate allocations unnecessarily.            | `None` | Extend    |

Direct await already has its own execution mechanism. It is not missing as a feature. The outstanding work concerns its costs and coverage.

The current [frame layout](/workspace/bray/crates/bray-codegen-llvm/src/translation/frame/core.rs:130) assigns separate storage fields; lifetime-based overlap is a concrete gap.

**Generators, strings, formatting, and bulk operations**

These involve operation-level semantics or library algorithms that LLVM generally cannot invent.

| #  | Optimization                                                                                                                                                           | Level   | Status    |
|----|------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------|-----------|
| 46 | **Eliminate intermediate generator materialization when equivalence is proved.** Preserve eager production, failure, cleanup, and consumer ordering.                   | `Full`  | Candidate |
| 47 | **Preallocate generator storage from proven cardinality.** Avoid repeated capacity checks and growth when the produced count is known.                                 | `Basic` | Candidate |
| 48 | **Lower eligible typed operations to bulk memory operations.** Cover initialization, movement, copying, and comparison only where representation and lifecycle permit. | `Basic` | Extend    |
| 49 | **Generate specialized formatting plans for constant format structure.** Avoid repeated interpretation and unnecessary generic dispatch.                               | `Full`  | Candidate |
| 50 | **Fuse string construction into fewer allocations and copies.** Compute lengths and write directly into the final destination where evaluation order permits.          | `Full`  | Candidate |
| 51 | **Reuse established text-validity proofs.** Avoid repeated encoding validation while the relevant storage remains unchanged.                                           | `Basic` | Candidate |
| 52 | **Remove unnecessary native encoding conversions and copies.** Exploit matching representations, termination, ownership, and lifetime contracts.                       | `Full`  | Extend    |

Generator fusion has a particularly important restriction: Bray specifies that **production completes before consumption begins**, including for `all` and `any`. Converting an effectful generator into a lazy loop would change the language’s behavior.

Similarly, bulk comparison cannot generally compare aggregate padding or replace floating-point equality with byte equality.

**Runtime services, statics, and retained footprint**

These are primarily dependency and runtime implementation choices. LLVM cannot decide which language services a product is allowed to omit.

| #  | Optimization                                                                                                                                                                                         | Level  | Status    |
|----|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------|-----------|
| 53 | **Further narrow optional runtime service demand.** Remove unnecessary scheduler, cancellation, callback, timer, and reporting dependencies from products that do not need them.                     | `None` | Extend    |
| 54 | **Prune unused generated tables and metadata entries.** Retain only required dispatch, lifecycle, descriptor, and callback content where contracts permit.                                           | `None` | Extend    |
| 55 | **Precompute eligible static initialization and omit unnecessary registration.** Extend beyond existing constant initializers without reordering observable work.                                    | `Full` | Candidate |
| 56 | **Eliminate repeated static/TLS initialization guards within proven regions.** Respect reentrancy, thread identity, provider lifetime, and abnormal exits.                                           | `Full` | Candidate |
| 57 | **Reduce runtime bookkeeping through stronger ownership and confinement.** Remove unnecessary locking, reference operations, and directory lookups where the runtime contract proves them redundant. | `None` | Candidate |
| 58 | **Coalesce redundant wakeups, registrations, and queue operations.** Preserve committed delivery, cancellation, and execution-lane semantics.                                                        | `None` | Extend    |

Here, `None` means improving the ordinary runtime implementation or demand resolver for every build. It does not mean adding an always-on optional MIR optimizer.

**What we should prioritize**

Our initial order should be:

1. **Niche filling and default aggregate layout**—clear missing capabilities with broad memory and cache implications.
2. **Direct destination construction and retain/release elimination**—avoid work LLVM often cannot legally remove.
3. **Frame liveness and slot reuse**—reduce async storage and admission costs.
4. **Stronger semantic facts reaching LLVM**, followed by effect-based call specialization.
5. **Constant-argument specialization and broader devirtualization**, with strict compilation and code-growth budgets.
