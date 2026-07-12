# Semantic checker design

This document defines the goal-state architecture for Bray's focused semantic checker services and analysis domains.

The language documents define Bray semantics. This document defines how the compiler checks those semantics without moving rule
policy into binding, publishing checker-private analysis state, or creating circular semantic passes.

`docs/design/compiler-architecture.md` defines the compiler-wide phase, query, and publication model.

`docs/design/symbols.md` defines canonical semantic identities, types, constants, contracts, and symbol-owned facts.

`docs/design/binder.md` defines binding orchestration, bound semantic units, storage terminology, and the shared control-flow graph
boundary.

This document is authoritative for checker domain ownership, dependencies, inputs, outputs, convergence, recovery, and durable
conclusions.

---

## Goals

The checker architecture should:

- implement each language rule in one focused domain,
- expose typed service contracts rather than untyped rule names or generic fact maps,
- support local checks during binding and whole-unit checks over committed bound structure,
- make dependencies between checker domains explicit and acyclic,
- use one control-flow graph for every flow-sensitive domain in a semantic unit,
- combine mutually dependent storage rules into one coherent flow domain,
- retain only durable semantic conclusions in the checked bound representation,
- produce structured source-correlated diagnostics without user-facing English in checker logic,
- recover conservatively from malformed bound input without panics or nontermination,
- support deterministic cancellation, parallelism, and future incremental reuse,
- leave lowering with no unresolved source-semantic decisions.

---

## Non-Goals

The checker does not:

- parse syntax, discover declarations, or resolve lexical names,
- own compilation queries, caches, scheduling, or publication,
- mutate symbols or published bound nodes,
- construct a second durable semantic tree,
- publish its control-flow graph, work lists, lattice states, or solver traces,
- lower checked semantics into cleanup operations, normalized control flow, or lower-level IR,
- erase unrelated rule outcomes into one universal fact record,
- use runtime assertion failures for ordinary invalid source,
- infer language policy from implementation convenience.

---

## Terminology

### Rule Check

A rule check validates one typed semantic operation whose relevant inputs are already available. Examples include checking a
conversion, selecting an implementation witness, checking pattern compatibility, or validating a call contract.

Rule checks can run while the binder constructs a candidate. They do not require a whole-unit control-flow graph unless their
contract explicitly says otherwise.

"Local" describes the service input, not necessarily when it runs. A flow domain can invoke a point-local contract, conversion, or
target check with the exact facts available at that program point.

### Analysis Domain

An analysis domain owns one coherent semantic problem, including its typed state, transfer rules, merge rules, diagnostics, recovery
behavior, and durable conclusions.

A domain is not merely a module name. Its state must have a precise semantic meaning and a stated convergence contract.

### Flow Domain

A flow domain evaluates state over the shared checker-internal control-flow graph. It declares a forward or backward direction, an
entry or exit boundary state, a deterministic merge operation, and a finite-height or otherwise provably convergent state space.

### Durable Conclusion

A durable conclusion is semantic information required after checking, such as a selected conversion, an expression type, a checked
move, a borrow capability, a control-completion category, an instantiated dependency contract, or a callable body effect summary.

Durable conclusion types belong in `bray-bound-tree`, `bray-symbols`, or another lower representation-owning crate. Checker-private
analysis IDs and intermediate states are never durable conclusions.

### Recovery State

A recovery state is a typed conservative result used after malformed or already erroneous input. It preserves category and known
relationships without pretending that a valid semantic proof exists.

Recovery unknown is distinct from disproven. A failed proof, an unavailable fact, malformed source, and compiler cancellation must
not collapse into the same state.

---

## Ownership And Boundaries

`bray-checker` owns semantic rule algorithms, its checker-private control-flow graph and analysis state, transfer functions,
convergence engines, and the construction of structured checker diagnostics.

`bray-bound-tree` owns the source-shaped checked HIR, durable node conclusions, unit-local storage and access identities, borrow
capabilities, instantiated dependency contracts, and immutable checked-unit result types.

`bray-symbols` owns canonical semantic types, constant values and terms, generic substitutions, implementation selections,
declaration contract summaries, portable dependency-contract templates, and symbol-facing lazy fact contracts.

`bray-binder` owns when checks run, which expected context applies, candidate transactions, placement of conclusions on task-local
bound builders, deterministic diagnostic merging, and atomic publication of a complete checked unit.

`bray-compilation` owns lazy query keys, caches, dependency scheduling, cancellation sources, cross-unit parallelism, and immutable
fact publication.

A checker service receives typed immutable inputs. It must not reach into binder builders, compilation caches, syntax internals, or
global mutable state.

### Method Naming

Functions and methods whose responsibility is enforcing a semantic rule use the `check_*` prefix. Examples include
`check_conversion`, `check_pattern`, `check_call_contract`, and `check_unit`.

Operations that compute a value without deciding semantic validity use a name for that computation, such as `evaluate_constant`,
`select_implementation`, `merge_states`, or `infer_dependency_contract`. A helper should not use `check_*` merely because a caller
eventually reports a diagnostic.

This mirrors `scan_*` for lexical scanning, `parse_*` for parsing, and `bind_*` for binding while keeping rule enforcement
discoverable.

---

## Service Shape

Focused checker APIs use exact typed request and result records. A request contains only the semantic inputs needed by that rule,
plus narrow read-only fact access and cancellation when the operation can request facts or perform substantial work.

Conceptually:

```rust
pub struct ConversionCheckRequest {
    source: TypeId,
    target: TypeId,
    context: ConversionContext,
    origin: BoundNodeOrigin,
}

pub struct ConversionCheckConclusion {
    conversion: CheckedConversion,
    result_type: TypeId,
}
```

The exact public boundary can use borrowed inputs where the caller already owns canonical records. It must not use strings, loosely
typed maps, or boolean parameter combinations to describe semantic categories.

`UnitChecker` is the whole-unit orchestration facade used by the binder. It does not imply one universal checker algorithm. Its
implementation builds the shared control-flow graph, runs the required domains in dependency order, and returns a category-specific
checked-unit conclusion.

`UnitCheckConclusions` is a closed category-specific transfer shape rather than a record of unrelated optional fields.
Conceptually:

```rust
pub enum UnitCheckConclusions {
    CallableBody(BodyCheckConclusions),
    AnonymousCallable(AnonymousCallableCheckConclusions),
    RuntimeDefault(RuntimeDefaultCheckConclusions),
    ConstantTemplate(ConstantCheckConclusions),
    PredicateDefinition(PredicateCheckConclusions),
    Constraint(ConstraintCheckConclusions),
    ContractClause(ContractClauseCheckConclusions),
}
```

Each variant retains the exact `BoundUnitId`. A conclusion for one unit or unit category cannot be applied to another.
The transfer wrapper belongs to `bray-checker`. Durable semantic values carried by its payloads belong to their lower representation
owners.

### Unit Check Schedules

Every independently published `BoundUnitKind` uses the shared control-flow graph. A simple expression produces a trivial graph. The
uniform boundary prevents declaration-owned units from bypassing ordinary control, storage, ownership, lifecycle, effect, and
recovery rules merely because their current syntax is small.

The checker derives a closed typed schedule from the unit kind and exact bound-unit key. Callers do not assemble domain lists or
toggle analyses with booleans.

| Bound unit kind       | Entry context                                                                                                         | Required checks and domains                                                                                                    | Required completion                                                                                                   |
|-----------------------|-----------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| `CallableBody`        | Receiver, parameters, generic constraints, callable requirements, declared capabilities, and lifecycle context        | Target availability, type and selection checks, control-flow graph, every whole-unit flow domain, then callable-body finalization        | Every reachable exit is valid and the body summary fits the declaration surface                                       |
| `AnonymousCallable`   | Anonymous parameters, generic and expected callable context, and the capture-free local boundary                      | The callable-body schedule plus anonymous-callable signature and boundary checks                                               | Every reachable exit is valid and the inferred callable summary fits its checked callable type                        |
| `RuntimeDefault`      | Permitted receiver, earlier parameters, generic values, selected implementations, and declaration context             | Target availability, type and selection checks, control-flow graph, every whole-unit flow domain, then runtime-default finalization      | Normal completion produces the required value and its complete provider requirements are summarized                   |
| `ConstantTemplate`    | Declared expected type, symbolic generic and trait context, and selected target facts                                 | Target availability, type and selection checks, control-flow graph, every whole-unit flow domain, then constant-template finalization    | Every reachable normal result is constant-valid, control terminates, and closed instances can be evaluated separately |
| `PredicateDefinition` | Predicate parameters, symbolic generic context, and declared trusted relation context                                 | Target availability, type and selection checks, control-flow graph, every whole-unit flow domain, then predicate-definition finalization | Normal completion produces `bool` and a reusable semantic predicate summary                                           |
| `Constraint`          | Generic parameters and facts available before the constraint being defined                                            | The predicate-definition schedule with static-constraint restrictions                                                          | Normal completion produces a total, deterministic, effect-free `bool` constraint summary                              |
| `ContractClause`      | Callable parameters and clause-specific facts, with `result` present only for a value-producing `ensures(...)` clause | The predicate-definition schedule with clause-specific fact, trust, and visibility rules                                       | Normal completion produces a total contract fact valid for its exact clause category                                  |

"Every whole-unit flow domain" means reachability, refinement, liveness, composite storage flow, dependency-contract propagation,
and effect, capability, contract, and trust validation. Category finalization consumes those conclusions and cannot rerun a private
replacement analysis.

In the schedule table, target availability means the preselection layer. Every selected target-dependent type or operation completes
the post-selection target-validity layer before control-flow graph construction.

Runtime-default conclusions record requirements without imposing them on calls or constructions that supply an explicit value.
Constant templates are validated symbolically. Only a closed constant instance is evaluated, keyed by its exact substitution,
selected implementations, and target profile.

For contract-clause entry contexts, `requires(...)` does not assume itself. `ensures(...)` can reference the declared normal result,
but body checking must prove the ensured fact independently on every reachable normal completion. Trusted facts retain their
provenance in every category.

### Outcomes And Cancellation

Every substantial checker operation observes the caller-provided cancellation source. `CheckerOutcome::Cancelled` carries no
diagnostics and no partial conclusions.

Cancellation is not a semantic result and must not be represented as an error type, recovery node, unknown proof, or user
diagnostic. Binder and compilation orchestration discard all task-local checker state after cancellation.

### Diagnostics

Each checker operation owns the diagnostics for rules it has enough information to explain. It returns them with its typed outcome.

Checker diagnostics use structured diagnostic kinds, typed arguments, and source anchors. User-facing English is rendered through
`bray-messages`.

Domains suppress only diagnostics that restate a known root cause. An error type or recovery state does not automatically suppress
an independent ownership, contract, or target error when that error remains meaningful and source-correlated.

Whole-unit diagnostics are merged in stable source-semantic order with a deterministic domain tie-break order. Worklist order,
parallel completion order, hash iteration, and candidate exploration order cannot affect published diagnostic order.

---

## Domain Dependency Graph

Checker dependencies form an explicit directed acyclic graph. The ordinary body-checking order is:

```text
selected target profile and target facts
    |
    +--> target gates and declaration-availability conclusions
                  |
bound structure, symbol facts, and target conclusions
                  |
    +--> type, compatibility, and candidate selection
                  |
    +--> target-dependent layout and ABI validity where required
                  |
    +--> checked operations and pattern conclusions
                  |
          shared control-flow graph
                  |
       reachability and control completion
             /                 \
    fact refinement          liveness
             \                 /
          composite storage flow
                  |
       dependency-contract propagation
                  |
  effect, capability, contract, and trust validation
                  |
    +--> callable or anonymous body conclusions
    |
    +--> runtime-default conclusions
    |
    +--> constant-template conclusions or requested instance evaluation
    |
    +--> predicate, constraint, or contract-clause conclusions
```

The diagram describes semantic dependencies, not a requirement for one monolithic execution. Target gate and declaration-
availability conclusions are available before any check that can select or reject a target-conditional declaration. Layout and ABI
validity consumes canonical selected types and operations, so it follows selection and cannot feed overload choice. Type and
candidate checks normally run during binding as their operands become available. Whole-unit domains consume committed checked
operations.

Constant, predicate, constraint, and contract-clause finalization consumes the same checked type, selection, target, control,
storage, dependency, effect, and contract conclusions as ordinary units. It does not resolve names, select operations, or build a
private control-flow model again.

Fact refinement and liveness can run independently after reachability when neither requests the other's conclusions. The composite
storage domain waits for both because refinement can prove disjointness and valid variants while liveness supports borrow shortening
and lifecycle decisions.

No domain can request a later domain in this graph. A rule that appears to require a cycle must be moved into one composite domain or
reformulated around an explicit finite fixed point.

---

## Local Rule Domains

### Type And Compatibility Checking

The type domain owns:

- expression and pattern result types,
- expected-type compatibility,
- literal adaptation,
- generic argument and substitution validity,
- callable parameter and result compatibility,
- branch and aggregate type merging,
- nullable, union, product, tuple, array, borrow, callable, and type-form rules,
- `unit`, `never`, and error-type behavior,
- type requirements for assignment, return, propagation, construction, and access.

Inputs use canonical `TypeId`, substitutions, selected semantic entities, typed operation categories, and source origins. Outputs use
canonical types and category-specific compatibility or adaptation conclusions.

The canonical error type supports recovery but never proves compatibility by itself. Checks that consume an error type return a
typed recovered conclusion and avoid diagnostics that merely repeat the originating type failure.

Generic definition checks operate over canonical symbolic parameters and constraints. Concrete instantiation requests check only
the substitution-dependent type, selection, constant, and target facts required by that instance. They do not eagerly enumerate
possible substitutions.

Type checking follows the language rules in `docs/language/types.md`, `docs/language/expressions.md`, and
`docs/language/patterns.md`.

### Candidate, Trait, And Conversion Selection

The selection domain owns:

- trait satisfaction and implementation witness selection,
- overload applicability,
- callable and method candidate applicability,
- operator implementation selection,
- conversion selection,
- generic constraint satisfaction,
- ambiguity, wrong-category, inaccessible, and unavailable-candidate outcomes.

Candidate enumeration comes from typed symbol lookup and implementation indexes. The checker evaluates candidates in canonical
order. It must not rank candidates when the language says that exactly one applicable arm is required.

Callable overload applicability uses explicit argument mapping, parameter type compatibility, receiver type and receiver mode for
methods, explicit generic substitution, static generic constraints, and target availability. It does not use expected result type,
argument ownership availability, borrow availability, mutation authority, dependency contracts, effects, capabilities, trusted
obligations, `requires(...)` facts, or postconditions.

After exactly one arm is selected, ordinary call checking validates every ownership, borrowing, mutation, dependency, effect,
capability, trust, and contract requirement. Failure rejects that selected call. It does not make resolution fall back to another
arm.

Speculative candidate checks use binder-owned candidate transactions. An abandoned candidate publishes no bound nodes, local
symbols, diagnostics, or selected target. Facts that influenced rejection, ordering, ambiguity, or the committed answer remain query
dependencies.

Selection follows `docs/language/callables/function-overloading.md`, `docs/language/callables/function-calls.md`,
`docs/language/types/traits.md`, and `docs/language/expressions/conversion-expressions.md`.

### Pattern Checking

The pattern domain owns:

- compatibility with the subject type,
- declaration, assignment, and match operation modes,
- binding types and storage projections,
- refutability and irrefutability,
- exhaustiveness and coverage across a complete arm set,
- coherent bindings across alternative patterns,
- move, copy, borrow, and observation requirements introduced by the pattern,
- active-variant, nullable, literal, shape, and predicate refinement seeds.

The local pattern conclusion describes required operations and possible refinements. Whole-unit storage and refinement domains decide
whether those operations are valid at the exact program point.

Recovered pattern syntax produces an error-aware pattern conclusion with conservative storage use and no unproven refinement.

Pattern checking follows `docs/language/patterns.md` and its operation-mode, refutability, partial-move, and fact-refinement rules.

### Constant Checking And Evaluation

The constant domain owns:

- constant-expression context validity,
- constant-call eligibility,
- exact integer contract arithmetic,
- selected runtime-format floating-point semantics,
- representability in the declared type,
- deterministic evaluation resource limits,
- constant dependency and cycle detection,
- admissible constant control flow and termination,
- target-fact dependencies,
- canonical constant value and term production.

Checking validity and evaluating a closed constant are separate typed operations. A definition template can be valid before every
concrete generic or target-dependent instance is evaluated.

The evaluator operates on checked semantic operations, not syntax. It cannot call non-const behavior, read runtime storage, allocate
runtime storage, perform I/O, spawn, await, use runtime dynamic dispatch, or execute another forbidden operation indirectly.

Evaluating a call to a const callable requests that callable's complete checked-body fact as a cross-unit dependency. It does not
invoke binding or a later domain of the constant instance currently being evaluated.

Evaluation failure returns an error-aware constant fact with structured diagnostics. Deterministic resource exhaustion is a
compile-time rejection. Cancellation remains a non-semantic `CheckerOutcome::Cancelled`.

Constant checking follows `docs/language/declarations/constant-declarations.md`,
`docs/language/contracts-and-trust/contract-arithmetic.md`, and the constant-expression rules of each expression category.

### Predicate, Constraint, And Contract Rules

This domain owns typed predicate-expression validity, static generic constraints, callable requirements and guarantees, trusted
facts, and proof queries over the current fact context.

A proof outcome uses a closed typed category such as:

```rust
pub enum ProofOutcome {
    Proven,
    Disproven,
    Unknown,
    Recovered,
}
```

`Unknown` means the available facts do not prove the proposition. It is not interchangeable with `Disproven`. `Recovered` means an
earlier malformed or error-aware input prevents a sound proof and cannot satisfy an obligation.

Facts retain typed subjects and dependencies on values, storage identities, storage versions, borrow capabilities, scoped
capabilities, target facts, and implementation witnesses. Trusted provenance is part of the fact and cannot be reconstructed from a
boolean result.

`requires(...)` obligations are checked at the call boundary. `ensures(...)` facts enter only normal-completion successors.
`with(...)` constraints are checked in the generic semantic context. Trusted obligations must be proved, visibly acknowledged, or
propagated through the declaration contract.

These rules follow `docs/language/contracts-and-trust.md` and
`docs/language/declarations/generic-declarations-and-constraints.md`.

### Target, Layout, And ABI Validity

The target domain owns two typed request layers.

The preselection availability layer owns:

- target-fact evaluation and dependency recording,
- target-gated contribution validity,
- target-conditional declaration availability,
- target dependencies that determine candidate participation.

Availability is a typed semantic result, not name lookup failure. An unavailable declaration can remain a retained candidate so the
checker can report the actual target constraint.

Target checks receive the immutable selected target profile and available compiler-known surface. They do not read process-global
host properties or infer the target from the machine running the compiler.

Product constraints and module-contribution gates use the same target rule service before ordinary body checking. Their earlier
request point does not make target policy part of package loading or declaration discovery.

Preselection target expressions use the closed target-selection context defined by the language. That context can reference only
the selected profile, compiler-known target facts and values, literals, and the permitted built-in operations. It uses the canonical
built-in scalar checks but cannot request source declaration lookup, user callable selection, or a source-owned constant fact. This
closed bootstrap surface prevents a dependency cycle from target availability back into ordinary source selection.

Public target-dependent facts record the exact target-fact dependencies required by compiled package interfaces and incremental
queries.

The post-selection validity layer consumes canonical selected types, substitutions, declarations, and operations. It owns:

- target-dependent generic and constant validity beyond declaration participation,
- layout and ABI requirements whose answer depends on the selected target,
- target validity of raw-memory and compiler-known operations.

Post-selection target validity can reject the selected operation. It does not cause overload or implementation selection to fall
back to a different candidate.

These rules follow `docs/language/targets-layout-abi-and-raw-memory.md` and
`docs/language/compiler-known-and-standard-library/target-profiles-and-target-facts.md`.

---

## Whole-Unit Flow Domains

### Reachability And Control Completion

Reachability is the first whole-unit flow domain.

Its state distinguishes reachable, unreachable, and conservative recovery control. Its forward merge is deterministic reachability
union. It recognizes normal continuation, `never`, return, break, continue, yield, propagation, panic, cancellation, suspension,
resumption, and other typed exits represented by the shared control-flow graph.

Durable conclusions include the unit's normal and non-normal completion categories and source-correlated unreachable-operation
facts needed by diagnostics or lowering. The complete reachable block and edge masks remain checker-private unless another domain
consumes them during the same check.

Recovery control remains reachable when dropping the path could hide meaningful errors. It cannot prove normal completion or satisfy
an exhaustiveness requirement by itself.

### Fact Refinement

The refinement domain is a forward must-analysis over facts known to hold at each program point.

Edge transfers add facts established by conditions, successful patterns, guards, assertions, nullable branches, union-variant
selection, predicate checks, trust boundaries, and normal-completion guarantees.

Operation transfers invalidate facts whose storage, value, version, initialization, ownership, borrow, capability, witness, or
target dependencies may have changed.

The ordinary merge keeps only facts proven on every reachable predecessor. Facts with different subjects, versions, capability
requirements, or trusted provenance do not merge merely because their rendered predicates look alike.

Durable conclusions include only facts required by checked operations, branch results, dependency guards, diagnostics, or lowering.
Full per-program-point fact sets remain private.

Recovery removes any fact whose truth is uncertain. It never invents a positive refinement to keep checking moving.

Refinement follows `docs/language/contracts-and-trust/fact-context.md`,
`docs/language/patterns/fact-context-refinement.md`, and
`docs/language/ownership-and-borrowing/fact-and-borrow-invalidation.md`.

### Liveness

Liveness is a backward may-analysis over the exact storage, access, borrow-capability, scoped-capability, and lifecycle-obligation
subjects needed after each program point.

Its merge is deterministic union. Transfers remove subjects defined or ended by an operation and add subjects read, observed,
borrowed, mutated, transferred, finalized, destroyed, joined, cancelled, or otherwise required by that operation.

Liveness supports borrow shortening, scope-exit planning, obligation diagnostics, and decisions about when a capability is no longer
required. It does not itself decide whether an access is initialized, whether two accesses overlap, or whether ending an obligation
is legal.

Only liveness conclusions required by durable borrow or lifecycle decisions are retained. Full live-in and live-out sets are
checker-private.

### Composite Storage Flow

Initialization, ownership, movement, borrowing, alias compatibility, mutation authority, partial-state tracking, and lifecycle
obligations form one composite forward domain.

These facts are mutually dependent. Splitting them into independent passes would create circular requests, duplicate storage state,
or allow one pass to validate an operation against stale conclusions from another.

The domain state is a typed product over storage identities and capabilities. It includes only meaningful relationships such as:

- initialization state and initialized subparts,
- current ownership and transfer state,
- moved, consumed, destroyed, and recovery state,
- active shared and mutable borrow capabilities,
- mutation authority and valid reborrow ancestry,
- active scoped, task, thread, and other flow-sensitive capabilities,
- known storage overlap or disjointness,
- active union variant and nullable presence when required for storage legality,
- attached destruction, finalization, joining, cancellation, and scoped-use obligations,
- already attached dependency requirements carried by the current value.

This composite state is the sole flow owner for lifecycle, scope, task, thread, joining, cancellation, and other run obligations.
Later domains consume its finalized conclusions and cannot maintain another independently merged obligation state.

The implementation must represent impossible combinations structurally where practical. It must not use one bag of optional fields
for every storage category.

Transfers cover construction, assignment, observation, copy, move, consumption, borrow, reborrow, mutation, projection, variant
replacement, nullable replacement, destruction, finalization, scope enter and exit, task or thread transfer, join, cancellation,
panic exit, and ordinary control exit.

Storage overlap uses a closed typed result:

```rust
pub enum StorageOverlap {
    Same,
    Disjoint,
    MayOverlap,
    Recovered,
}
```

`MayOverlap` is conservative and conflicts whenever the language requires proven disjointness. Distinct `StorageAccessId` values do
not imply disjoint storage.

Merges retain only operations valid for every reachable incoming state. Partial initialization and partial movement merge by exact
represented-part identity. Active borrow and lifecycle obligation sets merge conservatively. A path cannot silently discard an
obligation or restore mutation authority merely because another predecessor does not carry it.

The domain uses the reachability mask, fact refinements, and liveness conclusions. It produces durable per-operation storage
conclusions, borrow capabilities, checked accesses, scope-exit obligations, and recovery facts required by lowering.

Recovery state is conservative. Unknown overlap conflicts, unknown initialization cannot be read as initialized, and unknown
ownership cannot be moved or destroyed as though valid. The domain reports independent errors where useful but avoids repeating an
earlier malformed-source diagnostic for every affected operation.

Storage flow follows `docs/language/ownership-and-borrowing.md`, `docs/language/lifecycle.md`,
`docs/language/expressions/expression-ownership.md`, and the storage behavior specified by each expression form.

### Dependency-Contract Propagation

The dependency domain infers and normalizes the non-local requirements carried by values, accesses, borrows, callable values, trait
views, task handles, thread handles, and obligations.

It consumes checked type, refinement, storage, capability, witness, and lifecycle conclusions. It does not recompute those rules.
Storage flow can retain and validate an already attached contract as opaque typed input, but it does not infer derived contracts or
portable templates.

Moving a value moves its dependency contract. Copying creates the contract required by the copy result. Aggregate construction
combines child requirements. Nullable and union contracts retain guards for presence and active variants. Calls instantiate portable
symbol templates into unit-local subjects.

The merge is a deterministic normalized union of requirements with typed guards. Requirements are discharged only by a checked
operation that proves their resolution. Recovery preserves conservative requirements rather than dropping them.

Durable results are unit-local `BoundDependencyContractId` values and inferred portable templates required by symbol facts or
compiled interfaces. Full propagation states remain checker-private.

These rules follow `docs/language/ownership-and-borrowing/dependency-contracts.md` and the async task and thread obligation rules in
`docs/language/async-and-concurrency.md`.

### Effects, Capabilities, Contracts, And Trust

This domain computes the body effect summary and validates effect, capability use, contract, trust, and boundary requirements.

It observes selected calls and lifecycle operations, mutation, allocation, deallocation, I/O, async creation, suspension, spawn,
join, cancellation, panic behavior, trusted operations, and dependency transfers.

It consumes composite storage conclusions for capability availability and for lifecycle, scope, task, thread, joining, cancellation,
and other run obligations. It does not transfer or merge those states again.

Its state retains:

- accumulated body effects,
- capability uses and the declaration envelopes against which they are validated,
- ordinary and trusted contract obligations still requiring proof or propagation,
- normal-completion facts promised by selected operations.

Effect accumulation is deterministic set union over typed effect identities. Each capability use is checked against the exact
availability conclusion produced by composite storage flow or against declaration-scoped trusted authority that is not flow-varying.
Contract-obligation merge retains any ordinary or trusted requirement unresolved on a reachable predecessor.

At unit exits, this domain validates the storage domain's finalized run-obligation conclusions against the declaration contract. It
does not create a second lifecycle or run-obligation result.

The callable body summary must fit the declaration's caller-visible surface. A trusted callable's `uses(...)` clause must exactly
cover trusted implementation capabilities used by the body. Trusted caller obligations used by wrappers must be discharged or
exposed. `ensures(...)` facts are checked on every reachable normal completion, not on panic, propagation, divergence, or
cancellation exits unless the language contract explicitly says otherwise.

Durable conclusions include the normalized body effect summary, checked capability uses, discharged or propagated contract
obligations, and source-correlated exit conclusions needed by lowering and symbol facts.

These rules follow `docs/language/callables/effects-and-capabilities.md`, `docs/language/contracts-and-trust.md`,
`docs/language/lifecycle.md`, and `docs/language/async-and-concurrency.md`.

---

## Fixed-Point Contract

The shared fixed-point engine owns scheduling mechanics only. A concrete flow domain owns:

- its typed state,
- traversal direction,
- entry or exit boundary state,
- transfer functions,
- edge refinement handling,
- merge operation,
- equality or change detection,
- convergence argument,
- diagnostic emission points,
- durable projection.

The engine must not require domains to implement an untyped value interface or store unrelated states in one enum.

Every merge is deterministic, monotone, and independent of worklist order. Hash-based collections cannot define semantic iteration
or diagnostic order.

Finite domains converge by finite height. Domains with normalized terms, ranges, or aggregate state define widening or another
explicit bound before implementation. An iteration limit cannot silently turn a valid program into recovery. Exceeding a proven
domain bound is a compiler invariant failure, while deterministic constant-evaluation resource exhaustion remains an ordinary
compile-time diagnostic under the constant rules.

Cancellation is checked during control-flow graph construction, between substantial transfer batches, and during long solver
operations. A cancelled fixed point publishes nothing.

---

## Durable Publication

Checker output is a task-local transfer value until the binder applies it to the exact bound unit builder.

Durable checked data can include:

- expression, pattern, and block result categories,
- canonical result types and selected semantic operations,
- checked conversions, calls, implementations, and witnesses,
- checked storage accesses and operation modes,
- borrow capabilities and mutation-authority conclusions,
- control-completion and scope-exit conclusions,
- instantiated dependency contracts,
- normalized body effects and capability uses,
- checked contract facts and obligation outcomes,
- constant values, predicate summaries, and target dependencies,
- recovery facts required to keep later phases panic-free.

Checker output does not include:

- analysis block, edge, operation, or program-point IDs,
- complete block-entry or block-exit states,
- work lists, predecessor counts, or iteration counters,
- solver traces or temporary candidate sets,
- temporary alias or overlap caches,
- checker-owned diagnostic suppression state.

The binder validates unit identity and category, applies every required conclusion, merges diagnostics, and publishes the checked
unit atomically. A missing required conclusion is a construction error, not an invitation for lowering to re-run checker logic.

---

## Parallelism And Determinism

Independent semantic units can be checked in parallel through compilation queries.

Within one unit, independent domains can run in parallel only after all declared input conclusions are complete. Reachability runs
before the flow domains that consume its mask. Refinement and liveness can run in parallel where their inputs are independent.
Composite storage flow waits for both. Dependency-contract propagation waits for storage conclusions. Effect, capability, and
obligation validation waits for the dependency conclusions it consumes.

Small units should remain serial when parallel coordination costs more than the work.

Parallelism cannot change:

- candidate or implementation selection,
- canonical type, value, contract, or witness identity,
- block, edge, or operation order,
- fixed-point results,
- diagnostic content or order,
- durable bound-node or local-symbol IDs.

---

## Recovery Policy

Ordinary malformed source must never cause checker panics, unchecked indexing, nontermination, or invalid cross-unit access.

Each domain defines a conservative recovery element. Recovery elements preserve enough category information to continue checking but
cannot establish positive proofs, availability, disjointness, initialization, ownership, capability, or obligation discharge.

Recovery rules include:

- error types retain type category without satisfying unrelated constraints,
- unresolved references retain lookup failure category and viable candidates,
- recovered control remains conservatively reachable,
- recovered facts are not treated as proven,
- recovered storage overlap is treated as potentially conflicting,
- recovered initialization is not readable as a valid value,
- recovered ownership does not erase lifecycle obligations,
- recovered dependency contracts retain possible requirements,
- recovered effects conservatively retain effects relevant to context validity.

Every checker lookup validates typed unit ownership before indexing task-local storage. Invalid internal IDs return a typed
construction or checking error. They do not panic because source recovery happened to expose an incomplete relationship.

---

## Test Contract

Each local rule domain requires tests for valid, invalid, ambiguous, inaccessible, unavailable, recovered, and cancelled inputs that
are meaningful for that domain.

Each flow domain requires tests for:

- straight-line transfer,
- branch merge,
- loops and convergence,
- unreachable control,
- normal and non-normal exits,
- malformed recovery operations,
- deterministic results under different valid worklist orders,
- cancellation without partial conclusions.

Cross-domain tests cover:

- refinement invalidation after mutation, movement, destruction, and reinitialization,
- liveness-driven borrow shortening,
- partial initialization and partial movement across branches,
- overlapping and proven-disjoint borrows,
- lifecycle obligations across return, panic, propagation, and cancellation,
- dependency contracts moving with values and surviving guarded aggregates,
- body effects and trusted obligations matching declaration surfaces,
- target-unavailable candidates retaining target-specific diagnostics,
- constant and predicate checks sharing ordinary type and selection semantics,
- stable diagnostics and conclusions under serial and parallel unit checking.

Tests use language-spec examples and terminology. They must not invent type names, formatting conventions, overload ranking, effect
rules, or recovery behavior absent from the language specification.

---

## Implementation Slices

Implementation issues should be split along these dependency boundaries:

1. shared checker-internal control-flow graph and typed fixed-point mechanics,
2. type, compatibility, candidate, trait, conversion, and pattern rule services,
3. constant, predicate, constraint, contract, and target rule services,
4. reachability and control-completion analysis,
5. fact-refinement and liveness analyses,
6. composite storage-flow analysis,
7. dependency-contract propagation,
8. effect, capability-use, contract, and trust validation,
9. category-specific durable conclusion types and binder publication,
10. deterministic diagnostics, cancellation, recovery, convergence, and parallelism coverage.

An issue can combine adjacent slices when the implementation remains focused. It must not bypass an earlier dependency with a
temporary public API or duplicate semantic state that the owning domain will later replace.
