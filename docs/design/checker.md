# Semantic checker design

This document defines the goal-state architecture for Bray's focused semantic checker services and analysis domains.

The language documents define Bray semantics. This document defines how the compiler checks those semantics without
moving rule policy into binding, publishing checker-private analysis state, or creating circular semantic passes.

`docs/design/compiler-architecture.md` defines the compiler-wide phase, query, and publication model.

`docs/design/symbols.md` defines stable semantic identities, types, constants, contracts, and symbol-owned semantics.

`docs/design/binder.md` defines binding orchestration, bound semantic units, storage terminology, and the shared
control-flow graph boundary.

`docs/design/lowering.md` defines the durable checker output consumed by lowering.

This document is authoritative for checker domain ownership, dependencies, inputs, outputs, convergence, recovery, and
durable semantics.

---

## Goals

The checker architecture should:

- implement each language rule in one focused domain,
- expose typed service contracts rather than untyped rule names or generic result maps,
- support point-local rule checks and independently demandable unit-scoped analyses over committed bound structure,
- make dependencies between checker domains explicit and acyclic,
- use one control-flow graph for every flow-sensitive domain in a semantic unit,
- combine mutually dependent storage rules into one coherent flow domain,
- retain only durable semantics after checking,
- produce structured source-correlated diagnostics without user-facing English in checker logic,
- recover conservatively from malformed bound input without panics or nontermination,
- support deterministic cancellation, parallelism, and incremental reuse,
- leave lowering with no unresolved source-semantic decisions.

---

## Non-Goals

The checker does not:

- parse syntax, discover declarations, or resolve lexical names,
- own compilation queries, caches, scheduling, or publication,
- mutate symbols or published bound nodes,
- construct a second durable semantic tree,
- publish its control-flow graph, work lists, lattice states, or solver traces,
- lower checked semantics into cleanup operations, normalized control flow, or backend-independent MIR,
- erase unrelated rule outcomes into one universal checker record,
- use runtime assertion failures for ordinary invalid source,
- infer language policy from implementation convenience.

---

## Terminology

### Rule Check

A rule check validates one typed semantic operation whose relevant inputs are already available. Examples include
checking a conversion, selecting an implementation witness, checking pattern compatibility, or validating a call
contract.

Rule checks can run while the binder constructs a candidate. They do not require a unit-scoped control-flow graph unless
their contract explicitly says otherwise.

"Local" describes the service input, not necessarily when it runs. A flow domain can invoke a point-local contract,
conversion, or target check with the exact semantic inputs available at that program point.

### Analysis Domain

An analysis domain owns one coherent semantic problem, including its typed state, transfer rules, merge rules,
diagnostics, recovery behavior, and durable outputs.

A domain is not merely a module name. Its state must have a precise semantic meaning and a stated convergence contract.

### Flow Domain

A flow domain evaluates state over the shared checker-internal control-flow graph. It declares a forward or backward
direction, an entry or exit boundary state, a deterministic merge operation, and a finite-height or otherwise provably
convergent state space.

### Durable Checker Output

A durable checker output is semantic information required after checking, such as a selected conversion, an expression
type, a checked move, a borrow capability, a control-completion category, an instantiated dependency contract, or a
callable body effect summary.

Durable output types belong in `bray-bound-tree`, `bray-symbols`, or another lower representation-owning crate.
Checker-private analysis IDs and intermediate states are never durable outputs.

### Recovery State

A recovery state is a typed conservative result used after malformed or already erroneous input. It preserves category
and known relationships without pretending that a valid semantic proof exists.

Recovery unknown is distinct from disproven. A failed proof, an unavailable prerequisite, malformed source, and compiler
cancellation must not collapse into the same state.

---

## Ownership And Boundaries

`bray-checker` owns semantic rule algorithms, its checker-private control-flow graph and analysis state, transfer
functions, convergence engines, and the construction of structured checker diagnostics.

`bray-bound-tree` owns the source-shaped bound HIR, durable node and side-analysis types, unit-local storage and access
identities, borrow capabilities, and instantiated dependency contracts.

`bray-symbols` owns interned semantic types, constant values and terms, generic substitutions, implementation
selections, declaration contract summaries, portable dependency-contract templates, and symbol-facing semantic query
contracts.

`bray-binder` owns expected contexts, candidate transactions, point-local checker cooperation during binding,
deterministic diagnostic ownership, and atomic publication of the published bound unit.

`bray-compilation` owns typed lazy query accessors, caches, private dependency evaluation, cancellation sources,
cross-unit parallelism, and immutable result publication.

A checker service receives typed immutable inputs. It must not reach into binder builders, compilation caches, syntax
internals, or global mutable state.

### Method Naming

Functions and methods whose responsibility is enforcing a semantic rule use the `check_*` prefix. Examples include
`check_conversion`, `check_pattern`, `check_call_contract`, and `check_control_flow`.

Operations that compute a value without deciding semantic validity use a name for that computation, such as
`evaluate_constant`, `select_implementation`, `merge_states`, or `infer_dependency_contract`. A helper should not use
`check_*` merely because a caller eventually reports a diagnostic.

This mirrors `scan_*` for lexical scanning, `parse_*` for parsing, and `bind_*` for binding while keeping rule
enforcement discoverable.

---

## Service Shape

Focused checker APIs use exact typed request and result records. A request contains only the semantic inputs needed by
that rule, plus narrow read-only semantic access and cancellation when the operation can request other semantics or
perform substantial work.

Unit-scoped domains receive a validated `CheckerUnitView` that pairs the committed bound-unit view and exact root with a
closed `SemanticUnitContext`. The semantic context identifies the declaration or local callable boundary that supplies
parameters, generic context, requirements, and category-specific contextual bindings. A view whose root, unit key, unit
category, and semantic context do not agree is an infrastructure failure and cannot enter semantic analysis. The view is
shared immutable input for focused services. It is not a request to run every checker domain.

The shared request context resolves source text and spans only through `BoundSourceAnchor`. It can return the anchored
text and exact span but cannot expose a source snapshot, syntax tree, token stream, or arbitrary syntax traversal to
checker rules. Symbol semantics are requested through their typed `SymbolQueryContract` without selecting a source,
compiler-known, or imported implementation API at the checker boundary.

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

The exact public boundary can use borrowed inputs where the caller already owns defined records. It must not use
strings, loosely typed maps, or boolean parameter combinations to describe semantic categories.

Requests that can encounter compiler-known representations, implementations, or special values must borrow the
compilation-local target-available compiler-known symbol view. Checker rules must classify resolved symbols through that
view rather than declaration names or catalog keys. The view supplies available identity and role association only.
Checker-owned semantic rules remain in focused checker services.

### Typed Semantic Queries

The public semantic model is the set of typed lazy accessors exposed by `Compilation` and symbol views. A caller requests
the semantics it needs and receives a stable immutable value view and diagnostics. Examples include expression types,
selected calls, selected operations, control-completion analyses, storage-access plans, effect summaries, and constant
values.

Correlated semantics can share one immutable owning stage when they require the same traversal, fixed point, or
intermediate representation. Focused accessors return borrowed views backed by that stage and do not republish cloned
tables or diagnostics. The owning stage key is the identity used for caching and dependency evaluation. Dependency
recording, single-flight evaluation, scheduling, waiting, and cancellation remain private mechanics.

Each stage declares only the prerequisites needed to establish its own contract. A focused view can require its owning
stage, but no ordinary request implies a closed checker schedule or completion of later stages. Control-flow analysis for
an enclosing callable does not require control-flow analysis for nested callable units. Call selection does not require
storage or behavior analysis merely because those outputs may later be needed by lowering.

The semantic unit category selects contextual inputs, not a list of analyses:

| Bound unit kind       | Semantic context                                                                                                             |
|-----------------------|------------------------------------------------------------------------------------------------------------------------------|
| `CallableBody`        | Receiver, parameters, generic constraints, callable requirements, declared capabilities, and lifecycle context               |
| `AnonymousCallable`   | Anonymous parameters, generic and expected callable context, and the capture-free local boundary                             |
| `RuntimeDefault`      | Permitted receiver, earlier parameters, generic values, selected implementations, and declaration context                    |
| `ConstantTemplate`    | Declared expected type, symbolic generic and trait context, and selected target properties                                   |
| `PredicateDefinition` | Predicate parameters, symbolic generic context, and declared trusted relation context                                        |
| `Constraint`          | Generic parameters and propositions available before the constraint being defined                                            |
| `ContractClause`      | Callable parameters and clause-specific propositions, with `result` present only for a value-producing `ensures(...)` clause |

Runtime defaults record requirements without imposing them on calls or constructions that supply an explicit value.
Constant templates are validated symbolically. Only a closed constant instance is evaluated, keyed by its exact
substitution, selected implementations, and target profile.

For contract-clause semantic contexts, `requires(...)` does not assume itself. `ensures(...)` can reference the declared
normal result, but body checking must prove the postcondition independently on every reachable normal completion.
Trusted propositions retain their provenance in every category.

Package diagnostics is an intentionally broad consumer. It requests every applicable diagnostic-owning stage reachable
from the package and merges their diagnostics deterministically. Lowering requests the exact owning stages containing
the inputs named by `LoweringInput`. Neither consumer establishes unrelated semantic units or later product stages.

### Outcomes And Cancellation

Every substantial checker operation observes the caller-provided cancellation source. `CheckerOutcome::Cancelled`
carries no diagnostics and no partial results.

Cancellation is not a semantic result and must not be represented as an error type, recovery node, unknown proof, or
user diagnostic. Compilation query evaluation discards all task-local checker state after cancellation.

Failures to resolve a bound source anchor, obtain required semantics, or satisfy a checker request invariant are typed
infrastructure failures. They are distinct from cancellation and from source diagnostics, and they publish neither a
recovered semantic result nor a user-facing diagnostic.

### Diagnostics

Each checker operation owns the diagnostics for rules it has enough information to explain. It returns them with its
typed outcome.

Checker diagnostics use structured diagnostic kinds, typed arguments, and source anchors. User-facing English is
rendered through `bray-messages`.

Domains suppress only diagnostics that restate a known root cause. An error type or recovery state does not
automatically suppress an independent ownership, contract, or target error when that error remains meaningful and
source-correlated.

Whole-unit diagnostics are merged in stable source-semantic order with a deterministic domain tie-break order. Worklist
order, parallel completion order, hash iteration, and candidate exploration order cannot affect published diagnostic
order.

---

## Domain Dependency Graph

Checker dependencies form an explicit directed acyclic graph. The ordinary body-checking order is:

```text
selected target profile and target properties
    |
    +--> target gates and declaration-availability results
                  |
bound structure, symbol semantics, and target results
                  |
    +--> type, compatibility, and candidate selection
                  |
    +--> target-dependent layout and ABI validity where required
                  |
    +--> checked operations and pattern results
                  |
          shared control-flow graph
                  |
       reachability and control completion
             /                 \
        refinement          liveness
             \                 /
          composite storage flow
                  |
       dependency-contract propagation
                  |
  effect, capability, contract, and trust validation
                  |
    +--> callable or anonymous body results
    |
    +--> runtime-default results
    |
    +--> constant-template results or requested instance evaluation
    |
    +--> predicate, constraint, or contract-clause results
```

The diagram describes semantic dependencies, not a requirement for one monolithic execution. Target gate and
declaration- availability results are available before any check that can select or reject a target-conditional
declaration. Layout and ABI validity consumes stable selected types and operations, so it follows selection and cannot
feed overload choice. Type and candidate checks normally run during binding as their operands become available.
Whole-unit domains consume committed checked operations.

Constant, predicate, constraint, and contract-clause finalization consumes the same checked type, selection, target,
control, storage, dependency, effect, and contract results as ordinary units. It does not resolve names, select
operations, or build a private control-flow model again.

Refinement and liveness can run independently after reachability when neither requests the other's results. The
composite storage domain waits for both because refinement can prove disjointness and valid variants while liveness
supports borrow shortening and lifecycle decisions.

No domain can request a later domain in this graph. A rule that appears to require a cycle must be moved into one
composite domain or reformulated around an explicit finite fixed point.

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

Inputs use stable `TypeId`, substitutions, selected semantic entities, typed operation categories, and source origins.
Outputs use stable types and category-specific compatibility or adaptation results.

Whole-unit expression typing publishes one immutable result for every reachable `BoundExpressionId`. The result stores
the checked or recovery `TypeId` and whether recovery affected that occurrence. It is a focused side table over the
stable bound tree, not another semantic tree. Literal adaptation and semantic selection contribute typed evidence to the
same inference context before publication.

General generator results use a structural generator type parameterized by the yielded element type. The generator
iteration expression itself has type `unit`. Fixed-array generator results use the ordinary fixed-array type and require
a proof that every continuing iteration contributes exactly one element and that the source cardinality equals the
result length. Failure to establish that proof is a source diagnostic rather than an inferred fallback length.

Type inference and semantic selection cooperate through one task-local fixed point. Selection can inspect currently
resolved operand types, contribute a selected result type or an additional expected type, and request propagation again.
Intermediate rounds do not publish diagnostics. Cannot-infer and incompatibility diagnostics are finalized only after
the cooperating domains reach a stable state.

Iteration source selection first consumes the source expression type, then contributes the selected element type to
iteration pattern checking and final expression typing. The final expression semantics must depend on those
occurrence-specific selections without introducing a dependency cycle. Internal partial inference may supply the source
types required for selection, but it must not be exposed as complete checked semantics or publish final diagnostics.

The same fixed point resolves source type-expression templates when their embedded constant expressions depend on
expression typing, callable selection, implementation selection, or a target-sized representation. Each occurrence is
keyed by its declaration owner and exact syntax anchor. Its expected type comes from either an already stable type or
the declared type of the exact generic const parameter. Resolving one occurrence must not force unrelated declaration
types, bodies, or constant instances.

Successful occurrence checking publishes a stable open `ConstantTermId`. Resolving a containing template then constructs
the stable type, trait application, implementation subject, or callable signature. No partially resolved template or
intermediate fixed-point state is published.

Expected types are directional constraints. They propagate into language-defined child contexts such as tuple and array
elements, but an expected type alone does not establish an expression's actual type or select an overload, member,
operator, conversion, or other operation. A completed inference variable must have independent type evidence or resolve
to the shared error type with an owned diagnostic.

The shared error type supports recovery but never proves compatibility by itself. Checks that consume an error type
return a typed recovered result and avoid diagnostics that merely repeat the originating type failure.

Generic definition checks operate over stable symbolic parameters and constraints. Concrete instantiation requests check
only the substitution-dependent type, selection, constant, and target properties required by that instance. They do not
eagerly enumerate possible substitutions.

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

Candidate enumeration must come from typed symbol lookup and implementation indexes. Exact implementation lookup must
expose an immutable candidate set keyed by the checked subject type and trait application. Candidate records must be
origin-neutral and must retain stable implementation identity, inferred substitution, generic constraint templates,
target dependencies, and coherence evidence without asserting applicability. The checker must evaluate candidates in
stable order. It must not rank candidates when the language says that exactly one applicable arm is required.

The checker must evaluate the retained generic constraints, target availability, and coherence requirements before
requesting an `ImplementationSelectionQuery`. Candidate lookup must not publish a selected implementation witness. A
selection must represent the checked commitment reached after all applicability predicates that can affect candidate
participation are stable.

The binder must supply source-associated callable candidate records with stable semantic keys, target availability,
effective accessibility, static-constraint state, and the declared operand or parameter surface. Trait-backed operator,
index, and conversion candidates must also carry typed evidence for the exact compiler-known trait application, callable
member, checked signature, and implementation selection used by the candidate. The checker must consume those records
together with stable expression types.

The checker request context must resolve each closed compiler-known operation role, including exact operator and
indexing forms, to the trait and callable declarations assigned by the compiler-known catalog. Candidate evidence cannot
assign its own operation role. The checker must reject evidence whose trait application, callable instance, signature,
or source operation disagrees with that trusted role binding.

The checker must produce category-specific selections for exact callable targets and ABIs, normalized explicit and
defaulted argument mappings, members, operators, indexing contracts, construction behavior, conversions, and
implementation witnesses. The durable selected values and the immutable expression-keyed selection table must belong to
`bray-bound-tree`. Checker-owned request, candidate, failure, and algorithm types must remain in `bray-checker`. The
table supplements the stable bound tree and does not create another semantic tree.

Construction selections must retain explicit field, payload, or parameter mappings in source order followed by omitted
runtime defaults in declaration order. Composite conversions must retain the exact recursively selected conversion for
every converted component, including participating `ConvertTo<Target>` implementations. Publication must validate every
entry against the owning bound unit, source expression category, selected result type, and source-order operand mapping.

Selection failures are typed as unavailable, ambiguous, inaccessible, incompatible, or recovered. An inaccessible
candidate is reported only when it would otherwise be applicable. Ambiguities retain candidate keys in stable order.
Structured diagnostics identify the selection category without embedding rendered language text in checker code.

Callable overload applicability uses explicit argument mapping, parameter type compatibility, receiver type and receiver
mode for methods, explicit generic substitution, static generic constraints, and target availability. It does not use
expected result type, argument ownership availability, borrow availability, mutation authority, dependency contracts,
effects, capabilities, trusted obligations, `requires(...)` preconditions, or postconditions.

Each callable candidate carries the complete substitution produced by binding the source-ordered explicit generic inputs
against that candidate's type and const parameters. Selection validates that substitution and its static constraints
against the candidate evidence. Generic inputs remain separate from evaluated call arguments and never become expression
operands.

Operator and indexing applicability likewise must not use a previously inferred expression result type to choose a
candidate. A uniquely selected callable or operation contributes its result type to the cooperating type and selection
fixed point. Result-type agreement is validated only when publishing the final checked types and selections.

After exactly one arm is selected, ordinary call checking validates every ownership, borrowing, mutation, dependency,
effect, capability, trust, and contract requirement. Failure rejects that selected call. It does not make resolution
fall back to another arm.

Speculative candidate checks use binder-owned candidate transactions. An abandoned candidate publishes no bound nodes,
local symbols, diagnostics, or selected target. Semantic inputs that influenced rejection, ordering, ambiguity, or the
committed answer remain query dependencies.

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
- active-variant, nullable, literal, constant, shape, and predicate refinement seeds.

Literal and named-constant patterns use the shared checked constant term and value representation. Closed values provide
exact duplicate, subsumption, and coverage identity. Open terms remain valid pattern predicates but cannot prove finite
coverage until a concrete request supplies a value.

Coverage uses closed constant guard results when they are available. A proven `true` guard contributes its pattern
region, a proven `false` guard makes the arm unreachable, and every other guard remains statically unknown without
executing ordinary user code.

The local pattern result describes required operations and possible refinements. Whole-unit storage and refinement
domains decide whether those operations are valid at the exact program point.

Recovered pattern syntax produces an error-aware pattern result with conservative storage use and no unproven
refinement.

Pattern checking follows `docs/language/patterns.md` and its operation-mode, refutability, partial-move, and refinement
rules.

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
- target dependencies,
- interned constant value and term production.

Source type-expression templates are inputs to this domain, not checked constant terms. The domain validates their
embedded occurrences through ordinary expression typing and selection before producing open terms. A declaration-surface
binder must not duplicate those rules or reject grammar-valid expressions merely because their semantic support is not
available yet.

Checking validity and evaluating a closed constant are separate typed operations. A definition template can be valid
before every concrete generic or target-dependent instance is evaluated.

Literal type selection remains part of expression type checking. Contextual expectations select a compatible scalar
representation before language defaults are applied. Integer literals default to `i32`, real literals default to `r64`,
and complex literals default to `c128`. Complex literal components use the real representation associated with the
selected complex type. Defaulting is a finalization step and must not run while callers may still add expected-type
evidence.

Closed evaluation receives the complete checked expression types, exact semantic selections, and the results of every
referenced constant dependency. The compilation query layer owns dependency scheduling, caching, and cycle detection. It
supplies either the referenced `ConstantValueId` or a cycle result for each constant-reference occurrence. The checker
owns the source-correlated diagnostic and error-value recovery for a reported cycle.

The evaluator reads literal spellings through their exact token ranges, excluding trivia, and converts them directly
into the selected language representation. Integer values use arbitrary-width stable magnitude storage. Floating-point
and complex components use the selected IEEE binary format without host floating-point conversion. Equivalent typed
values are interned in the semantic value store and published as `ConstantValueId` results.

Every evaluation request has deterministic limits for evaluated operations, aggregate elements, decoded literal bytes,
and compile-time expansion. Limit exhaustion emits a structured diagnostic and publishes a typed error constant rather
than partial aggregate state. Cancellation publishes neither a value nor diagnostics.

The evaluator operates on checked semantic operations, not syntax. It cannot call non-const behavior, read runtime
storage, allocate runtime storage, perform I/O, start tasks, await, use runtime dynamic dispatch, or execute another
forbidden operation indirectly.

Evaluating a call to a const callable requests the callable semantics required by constant evaluation, including its
bound body, expression types, selected operations, constant-validity checks, and referenced constant values. It does not
request unrelated storage analyses or tooling queries and does not invoke binding or another checker service directly.

Evaluation failure returns an error-aware constant value with structured diagnostics. Deterministic resource exhaustion
is a compile-time rejection. Cancellation remains a non-semantic `CheckerOutcome::Cancelled`.

Constant checking follows `docs/language/declarations/constant-declarations.md`,
`docs/language/contracts-and-trust/contract-arithmetic.md`, and the constant-expression rules of each expression
category.

### Declared Type Representation Contracts

Each product or union declaration must expose one independently demandable, immutable representation contract keyed by
its named type identity. The checker derives this representation from fields and payloads, lifecycle declarations,
generic parameters, and the declaration's `@layout`, `@copy`, and variant `@tag` directives.

The contract records source-level layout mode and options, the selected union tag type and values when source layout
fixes them, derived copy behavior, the exact generic type parameters on which copying depends, plain-storage
eligibility, finite outer size, and recovery state. Recursive representation checking must memoize completed named types
and detect active inline cycles. Indirection can terminate an outer-size cycle, but it does not make the representation
plain storage.

Representation checking must also honor the request's deterministic active-recursion limit. Exhaustion produces one
structured diagnostic and a recovered representation contract. It must not publish a partially checked representation as
valid.

This representation validates source semantics only. Target-specific offsets, padding, aggregate size, ABI alignment
support, and physical layout calculation remain separate target-dependent queries. Public contracts must be serializable
through compiled package interfaces so importing compilations consume the same checked representation without rechecking
dependency source.

### Predicate, Constraint, And Contract Rules

This domain owns typed predicate-expression validity, static generic constraints, callable requirements and guarantees,
trusted propositions, and proof queries over the current refinement context.

A proof outcome uses a closed typed category such as:

```rust
pub enum ProofOutcome {
    Proven,
    Disproven,
    Unknown,
    Recovered,
}
```

`Unknown` means the available refinements do not prove the proposition. It is not interchangeable with `Disproven`.
`Recovered` means an earlier malformed or error-aware input prevents a sound proof and cannot satisfy an obligation.

Refinements retain typed subjects and dependencies on values, storage identities, storage versions, borrow capabilities,
scoped capabilities, target properties, and implementation witnesses. Trusted provenance is part of the refinement and
cannot be reconstructed from a boolean result.

Synchronous `requires(...)` obligations are checked at the call boundary and `ensures(...)` refinements enter only
normal-completion successors. For async calls, the checker splits invocation requirements from the deferred execution
contract and publishes `ensures(...)` refinements only after normal direct-await completion or in a
`RunResult.Completed` refinement. `with(...)` constraints are checked in the generic semantic context. Trusted
obligations must be proved, visibly acknowledged, or propagated through the declaration contract.

These rules follow `docs/language/contracts-and-trust.md` and
`docs/language/declarations/generic-declarations-and-constraints.md`.

### Target, Layout, And ABI Validity

The target domain owns two typed request layers.

The preselection availability layer owns:

- target-property evaluation and dependency recording,
- target-gated contribution validity,
- target-conditional declaration availability,
- target dependencies that determine candidate participation.

Availability is a typed semantic result, not name lookup failure. An unavailable declaration can remain a retained
candidate so the checker can report the actual target constraint.

Target checks receive the immutable selected target profile and available compiler-known surface. They do not read
process-global host properties or infer the target from the machine running the compiler.

`CheckerRequestContext` supplies the stable `bray_target::TargetProfile` used by the request. Target-sized literal and
constant checks take their width from that profile and do not accept an optional caller-supplied width. The same request
context supplies the target-filtered compiler-known declaration view. Post-selection layout and ABI checks must consume
these inputs rather than build a parallel target model.

Post-selection target validity uses a typed request that pairs the selected representation, callable ABI, or alignment
requirement with its exact source anchor. Its immutable result distinguishes valid and invalid requirements, and an
invalid result owns a source-correlated structured diagnostic. The check does not reopen candidate selection or
substitute a different operation.

Each finalized semantic result that establishes a target-dependent representation, callable ABI, or layout requests
validity by the exact requirement key. The semantic result retains the returned diagnostics in its own immutable result,
so ordinary semantic diagnostic projection includes them without scanning caches or depending on which unrelated queries
happened to run first.

Product constraints and module-contribution gates use the same target rule service before ordinary body checking. Their
earlier request point does not make target policy part of package loading or declaration discovery.

Preselection target expressions use the closed target-selection context defined by the language. That context can
reference only the selected profile, compiler-known target properties and values, literals, and the permitted built-in
operations. It uses the stable built-in scalar checks but cannot request source declaration lookup, user callable
selection, or a source-owned constant query. This closed foundational surface prevents a dependency cycle from target
availability back into ordinary source selection.

Public target-dependent semantic results record the exact target-property dependencies required by compiled package
interfaces and incremental queries.

The post-selection validity layer consumes stable selected types, substitutions, declarations, and operations. It owns:

- target-dependent generic and constant validity beyond declaration participation,
- layout and ABI requirements whose answer depends on the selected target,
- target validity of raw-memory and compiler-known operations.

Post-selection target validity can reject the selected operation. It does not cause overload or implementation selection
to fall back to a different candidate.

These rules follow `docs/language/targets-layout-abi-and-raw-memory.md` and
`docs/language/compiler-known-and-standard-library/target-profiles-and-properties.md`.

---

## Unit-Scoped Flow Domains

### Reachability And Control Completion

Reachability publishes the control-flow result consumed by flow domains that need a reachable-operation mask.

Its state distinguishes reachable, unreachable, and conservative recovery control. Its forward merge is deterministic
reachability union. It recognizes normal continuation, `never`, return, break, continue, yield, propagation, panic,
cancellation, suspension, resumption, and other typed exits represented by the shared control-flow graph.

Durable control results include the unit's normal and non-normal completion categories and source-correlated
unreachable-operation records needed by diagnostics or lowering. The complete reachable block and edge masks remain
checker-private unless another domain consumes them during the same check.

Recovery control remains reachable when dropping the path could hide meaningful errors. It cannot prove normal
completion or satisfy an exhaustiveness requirement by itself.

### Refinement

The refinement domain is a forward must-analysis over propositions known to hold at each program point.

Edge transfers add refinements established by conditions, successful patterns, guards, assertions, nullable branches,
union-variant selection, predicate checks, trust boundaries, and normal-completion guarantees.

Operation transfers invalidate refinements whose storage, value, version, initialization, ownership, borrow, capability,
witness, or target dependencies may have changed.

The ordinary merge keeps only refinements proven on every reachable predecessor. Refinements with different subjects,
versions, capability requirements, or trusted provenance do not merge merely because their rendered predicates look
alike.

Durable refinements include only propositions required by checked operations, branch results, dependency guards,
diagnostics, or lowering. Full per-program-point refinement sets remain private.

Recovery removes any refinement whose truth is uncertain. It never invents a positive refinement to keep checking
moving.

Refinement follows `docs/language/contracts-and-trust/contract-reasoning.md`,
`docs/language/patterns/pattern-refinement.md`, and
`docs/language/ownership-and-borrowing/contract-and-borrow-validity.md`.

### Liveness

Liveness is a backward may-analysis over the exact storage, access, borrow-capability, scoped-capability, and
lifecycle-obligation subjects needed after each program point.

Its merge is deterministic union. Transfers remove subjects defined or ended by an operation and add subjects read,
observed, borrowed, mutated, transferred, finalized, destroyed, joined, cancelled, or otherwise required by that
operation.

Liveness supports borrow shortening, scope-exit planning, obligation diagnostics, and decisions about when a capability
is no longer required. It does not itself decide whether an access is initialized, whether two accesses overlap, or
whether ending an obligation is legal.

Only liveness decisions required by durable borrow or lifecycle semantics are retained. Full live-in and live-out sets
are checker-private.

### Composite Storage Flow

Initialization, ownership, movement, borrowing, alias compatibility, mutation authority, partial-state tracking, and
lifecycle obligations form one composite forward domain.

These storage subdomains are mutually dependent. Splitting them into independent passes would create circular requests,
duplicate storage state, or allow one pass to validate an operation against stale results from another.

The domain state is a typed product over storage identities and capabilities. It includes only meaningful relationships
such as:

- initialization state and initialized subparts,
- current ownership and transfer state,
- moved, consumed, destroyed, and recovery state,
- active shared and mutable borrow capabilities,
- mutation authority and valid reborrow ancestry,
- active scoped, async-computation, task, and other flow-sensitive capabilities,
- known storage overlap or disjointness,
- active union variant and nullable presence when required for storage legality,
- attached destruction, finalization, joining, cancellation, and scoped-use obligations,
- already attached dependency requirements carried by the current value.

This composite state is the sole flow owner for lifecycle, scope, async-computation, task, standard-library child-run,
joining, cancellation, and other run obligations. Later domains consume its finalized results and cannot maintain
another independently merged obligation state.

The implementation must represent impossible combinations structurally where practical. It must not use one bag of
optional fields for every storage category.

Transfers cover construction, assignment, observation, copy, move, consumption, borrow, reborrow, mutation, projection,
variant replacement, nullable replacement, destruction, finalization, scope enter and exit, async-computation or
child-run-owner transfer, task start, join, cancellation, run-result forwarding, panic exit, and ordinary control exit.

Storage overlap uses a closed typed result:

```rust
pub enum StorageOverlap {
    Same,
    Disjoint,
    MayOverlap,
    Recovered,
}
```

`MayOverlap` is conservative and conflicts whenever the language requires proven disjointness. Distinct
`StorageAccessId` values do not imply disjoint storage.

Merges retain only operations valid for every reachable incoming state. Partial initialization and partial movement
merge by exact represented-part identity. Active borrow and lifecycle obligation sets merge conservatively. A path
cannot silently discard an obligation or restore mutation authority merely because another predecessor does not carry
it.

The domain uses the reachability mask, refinements, and liveness results. It produces durable per-operation storage
decisions, borrow capabilities, checked accesses, scope-exit obligations, and recovery state required by lowering.

Recovery state is conservative. Unknown overlap conflicts, unknown initialization cannot be read as initialized, and
unknown ownership cannot be moved or destroyed as though valid. The domain reports independent errors where useful but
avoids repeating an earlier malformed-source diagnostic for every affected operation.

Storage flow follows `docs/language/ownership-and-borrowing.md`, `docs/language/lifecycle.md`,
`docs/language/expressions/expression-ownership.md`, and the storage behavior specified by each expression form.

### Per-Unit Storage Planning

Each bound semantic unit has one independently demandable immutable storage plan. The plan assigns unit-local identities
to parameter, receiver, local, result, temporary, allocation, compiler-created, and recovery storage and retains every
evaluated storage-access occurrence in source evaluation order.

An access plan records the exact expression occurrence, its checked access root and ordered projections, its reached
type, and the operation performed through it. Operations include reads, writes, moves, unresolved value transfers,
borrows, assignments, member access, indexing, slicing, and pattern projections. A single expression can produce several
projected accesses with the same operation, as with product or sequence destructuring.

Planning requests only the exact unit's bound tree, final expression types, checked pattern analysis, and semantic
selections, including selected iteration sources. Nested semantic units and unrelated declarations retain independent
query identities and are not scanned or materialized by the enclosing request.

The planner does not decide flow legality, storage overlap, borrow duration, initialization state, or ownership
validity. It preserves checked selections and projections when available. Missing or recovered semantic providers
produce conservative source-correlated storage and access plans without guessing a target.

### Dependency-Contract Propagation

The dependency domain infers and normalizes the non-local requirements carried by values, accesses, borrows, callable
values, trait views, async computations, task handles, and obligations.

It consumes checked type, refinement, storage, capability, witness, and lifecycle results. It does not recompute those
rules. Storage flow can retain and validate an already attached contract as opaque typed input, but it does not infer
derived contracts or portable templates.

Moving a value moves its dependency contract. Copying creates the contract required by the copy result. Aggregate
construction combines child requirements. Nullable and union contracts retain guards for presence and active variants.
Calls instantiate portable symbol templates into unit-local subjects.

Generic bodies use open dependency subjects. An operation that publishes such a subject to a synchronized shared owner,
an independent in-process run, or an encoded child-process protocol adds an open transfer term naming the destination
class. The term remains in the portable template and is validated against concrete value dependencies at instantiation.
Process transfer terms add encoding and process-isolation requirements rather than pretending that an
address-space-local borrow can move. This is the same analysis for user declarations, standard-library declarations, and
private trusted ABI wrappers. No package or textual declaration name is special.

The merge is a deterministic normalized union of requirements with typed guards. Requirements are discharged only by a
checked operation that proves their resolution. Recovery preserves conservative requirements rather than dropping them.

Durable results are unit-local `BoundDependencyContractId` values and inferred portable templates required by symbol
semantics or compiled interfaces. Full propagation states remain checker-private.

These rules follow `docs/language/ownership-and-borrowing/dependency-contracts.md` and the async computation and task
rules in `docs/language/async-and-concurrency.md`.

### Effects, Capabilities, Contracts, And Trust

This domain computes the body effect summary and validates effect, capability use, contract, trust, and boundary
requirements.

It observes selected calls and lifecycle operations, mutation, allocation, deallocation, I/O, async creation,
suspension, task start, join, cancellation, panic behavior, trusted operations, and dependency transfers.

It consumes composite storage results for capability availability and for lifecycle, scope, async-computation, task,
joining, cancellation, and other run obligations. It does not transfer or merge those states again.

Its state retains:

- accumulated body effects,
- capability uses and the declaration envelopes against which they are validated,
- ordinary and trusted contract obligations still requiring proof or propagation,
- normal-completion guarantees promised by selected operations.

Effect accumulation is deterministic set union over typed effect identities. Each capability use is checked against the
exact availability result produced by composite storage flow or against declaration-scoped trusted authority that is not
flow-varying. Contract-obligation merge retains any ordinary or trusted requirement unresolved on a reachable
predecessor.

At unit exits, this domain validates the storage domain's finalized run-obligation results against the declaration
contract. It does not create a second lifecycle or run-obligation result.

The callable body summary must fit the declaration's caller-visible surface. A trusted callable's `uses(...)` clause
must exactly cover trusted implementation capabilities used by the body. Trusted caller obligations used by wrappers
must be discharged or exposed. `ensures(...)` postconditions are checked on every reachable normal completion, not on
panic, propagation, divergence, or cancellation exits unless the language contract explicitly says otherwise.

Durable outputs include the normalized body effect summary, checked capability uses, discharged or propagated contract
obligations, and source-correlated exit decisions needed by lowering and symbol semantics.

These rules follow `docs/language/callables/effects-and-capabilities.md`, `docs/language/contracts-and-trust.md`,
`docs/language/lifecycle.md`, and `docs/language/async-and-concurrency.md`.

### Async Frame And Task Plans

Async checking publishes typed frame, suspension, deferred-execution-contract, affinity, and structured cleanup results.
It does not assign machine frame layout.

For each async invocation, the checker separates argument evaluation, transfer, value preconditions, generic
constraints, and frame construction from body effects, capabilities, execution predicates, lifecycle behavior, and
normal-completion postconditions deferred into `Future<T>`. Direct await validates the deferred contract against the
current execution context and publishes postconditions only on normal completion. Task start records the contract for
runtime lane selection and publishes postconditions only in a `RunResult.Completed` refinement.

`try` on `RunResult<T>` produces one normal value edge and two current-run abnormal-propagation edges. The checker does
not search for a callable returning `RunResult<R>`. It verifies that the panicked edge transfers the existing
`PanicReport`, the cancelled edge enters current-run cancellation, and both edges execute every intervening lifecycle
and structured-cleanup obligation. An enclosing catch captures only the panicked edge.

The cancelled edge contributes `may_cancel_current_run` to the body-effect summary, as do run checkpoints and
cancellation-aware operations. This is panic-like implicit abnormal-control metadata rather than a source callable
modifier or an overload/assignment discriminator. Constant, predicate, and other effect-free contexts reject it.
Exported checked declaration metadata preserves it for diagnostics, lowering, and inspection.

For each async lexical scope exit, composite storage flow publishes the exact initialized roots, moved access paths, and
active borrows. Async checking combines that state with checked type representations to emit one two-phase cleanup plan
over exact storage access paths: all owned unresolved tasks receive cancellation before any task is awaited, then normal
reverse lifecycle resolution proceeds with dependency ordering. Partial moves remain explicit masks rather than being
collapsed into whole-root state. Lowering consumes this plan without repeating flow or type analysis. Ordinary
standard-library `Thread<T>` and `Process<T>` lifecycle obligations participate through their checked declaration
contracts rather than compiler name recognition. The plan names separate descriptor broadcast visitors and
lifecycle-resolution operations for concrete and erased state. It rejects implicit thread cleanup when a possible
completion payload cannot be resolved synchronously and infallibly. Because the ordinary process finalizer returns
`Result<unit, ProcessError>`, it always rejects an unresolved `Process<T>` on normal exit and requires explicit
consuming observation. Abnormal cleanup can record its failure as an incident.

The complete implementation contract is defined in `docs/design/async-runtime.md`.

---

## Deterministic Semantic Work Limits

Result-affecting semantic work limits belong to the immutable compilation request. The checker reads them through
existing domain contexts rather than a separate budget service.

Package-level implementation-coherence and callable-overload validation each consume the configured comparison limit
within their requested package query. Candidate families and pairs remain ordered by stable identity. Reaching the limit
emits one structured diagnostic at the next deterministic source participant and rejects the incomplete package-level
answer instead of silently accepting unchecked pairs.

Changing a semantic work limit invalidates only queries whose result depends on that limit and their ordinary
dependents. Worker count, query priority, cancellation, and cache retention remain scheduling inputs and do not affect
semantic identity.

The pre-lowering bounded-work policy is divided by domain:

- source loading and compiled package decoding enforce their own input counts, byte sizes, section sizes, and allocation
  limits,
- parser recursion is bounded while green-tree traversal and teardown remain iterative,
- declaration, symbol, bound-node, analysis-node, and semantic-value arenas use checked compact identities and reject
  capacity overflow before publication,
- generic and type work remains demand-driven per requested occurrence, detects dependency cycles, and observes syntax
  or semantic recursion limits instead of expanding an eager transitive closure,
- constant evaluation owns operation, aggregate, literal, expansion, exact-integer, and call-depth limits,
- flow-sensitive analysis owns explicit refinement and storage-cell capacities plus finite convergence arguments,
- implementation and overload families own package-level pair-comparison limits,
- query caches own finite retention independently of semantic answers,
- diagnostic accumulation is bounded by the source occurrences, semantic entities, and explicitly limited work items
  that can produce diagnostics.

No limit may be replaced by a machine-time deadline or available-memory probe. Capacity conversion and arithmetic must
be checked, and source-triggered exhaustion must become structured recovery or rejection rather than a compiler panic.

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

Every merge is deterministic, monotone, and independent of worklist order. Hash-based collections cannot define semantic
iteration or diagnostic order.

Finite domains converge by finite height. Domains with normalized terms, ranges, or aggregate state define widening or
another explicit bound before implementation. An iteration limit cannot silently turn a valid program into recovery.
Exceeding a proven domain bound is a compiler invariant failure, while deterministic constant-evaluation resource
exhaustion remains an ordinary compile-time diagnostic under the constant rules.

Cancellation is checked during control-flow graph construction, between substantial transfer batches, and during long
solver operations. A cancelled fixed point publishes nothing.

---

## Durable Publication

Checker output remains task-local until the compilation validates its unit identity and publishes it through the
matching typed query accessor. Publication freezes that output and its diagnostics without mutating or wrapping the
published bound unit.

Durable checked data can include:

- expression, pattern, and block result categories,
- defined result types and selected semantic operations,
- checked conversions, calls, implementations, and witnesses,
- checked storage accesses and operation modes,
- borrow capabilities and mutation authority,
- control-completion and scope-exit decisions,
- instantiated dependency contracts,
- normalized body effects and capability uses,
- checked contracts and obligation outcomes,
- constant values, predicate summaries, and target dependencies,
- recovery state required to keep later phases panic-free.

Checker output does not include:

- analysis block, edge, operation, or program-point IDs,
- complete block-entry or block-exit states,
- work lists, predecessor counts, or iteration counters,
- solver traces or temporary candidate sets,
- temporary alias or overlap caches,
- checker-owned diagnostic suppression state.

Each published output validates the unit identity and category required by its contract. A missing prerequisite prevents
that output from being published and is not an invitation for a consumer to rerun checker logic. Package diagnostics
merges diagnostics from the diagnostic-owning queries it requests. Lowering validates its exact typed inputs before
constructing MIR.

---

## Parallelism And Determinism

Independent semantic analyses can be evaluated in parallel when their declared prerequisites are available.

Within one unit, domains whose prerequisites are satisfied can run in parallel. Reachability precedes only domains that
consume its mask. Refinement and liveness can run concurrently where neither depends on the other. Storage, dependency,
effect, capability, and obligation domains declare their actual inputs rather than inheriting a universal schedule.

Small units should remain serial when parallel coordination costs more than the work.

Parallelism cannot change:

- candidate or implementation selection,
- stable type, value, contract, or witness identity,
- block, edge, or operation order,
- fixed-point results,
- diagnostic content or order,
- durable bound-node or local-symbol IDs.

---

## Recovery Policy

Ordinary malformed source must never cause checker panics, unchecked indexing, nontermination, or invalid cross-unit
access.

Each domain defines a conservative recovery element. Recovery elements preserve enough category information to continue
checking but cannot establish positive proofs, availability, disjointness, initialization, ownership, capability, or
obligation discharge.

Recovery rules include:

- error types retain type category without satisfying unrelated constraints,
- unresolved references retain lookup failure category and viable candidates,
- recovered control remains conservatively reachable,
- recovered state is not treated as proven,
- recovered storage overlap is treated as potentially conflicting,
- recovered initialization is not readable as a valid value,
- recovered ownership does not erase lifecycle obligations,
- recovered dependency contracts retain possible requirements,
- recovered effects conservatively retain effects relevant to context validity.

Every checker lookup validates typed unit ownership before indexing task-local storage. Invalid internal IDs return a
typed construction or checking error. They do not panic because source recovery happened to expose an incomplete
relationship.

---

## Test Contract

Each local rule domain requires tests for valid, invalid, ambiguous, inaccessible, unavailable, recovered, and cancelled
inputs that are meaningful for that domain.

Each flow domain requires tests for:

- straight-line transfer,
- branch merge,
- loops and convergence,
- unreachable control,
- normal and non-normal exits,
- malformed recovery operations,
- deterministic results under different valid worklist orders,
- cancellation without partial results.

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
- stable diagnostics and results under serial and parallel unit checking.

Tests use language-spec examples and terminology. They must not invent type names, formatting conventions, overload
ranking, effect rules, or recovery behavior absent from the language specification.

---

## Implementation Slices

Implementation issues should be split along these dependency boundaries:

1. shared checker-internal control-flow graph and typed fixed-point mechanics,
2. type, compatibility, candidate, trait, conversion, and pattern rule services,
3. constant, predicate, constraint, contract, and target rule services,
4. reachability and control-completion analysis,
5. refinement and liveness analyses,
6. composite storage-flow analysis,
7. dependency-contract propagation,
8. effect, capability-use, contract, and trust validation,
9. category-specific durable output types and binder publication,
10. deterministic diagnostics, cancellation, recovery, convergence, and parallelism coverage.

An issue can combine adjacent slices when the implementation remains focused. It must not bypass an earlier dependency
with a temporary public API or duplicate semantic state that the owning domain will later replace.
