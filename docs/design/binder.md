# Binder and bound tree design

This document defines the goal-state architecture for name binding, semantic-analysis orchestration, checked semantic units, and the
immutable bound representation.

The language documents define Bray semantics.

`docs/design/compiler-architecture.md` defines the compiler-wide phase and query model.

`docs/design/symbols.md` defines semantic identities, symbol facts, completion, and local symbol snapshots.

This document defines how syntax and symbols become complete source-correlated semantic facts without moving checker policy into the
binder or making lowering reinterpret source.

---

## Goals

The binder and bound representation should:

- resolve every semantic reference to a typed identity,
- publish complete immutable results for clearly defined semantic units,
- preserve enough source correlation for precise diagnostics and tooling,
- orchestrate checker services without owning their semantic rules,
- represent erroneous source explicitly and continue without panics,
- support lazy queries, independent parallel work, cancellation, and future incremental reuse,
- give lowering every semantic decision it needs without requiring source reinterpretation,
- keep declaration-surface symbols separate from region-scoped local symbols,
- avoid cloning syntax, symbol records, or bound subtrees merely to cross phase boundaries.

---

## Non-Goals

The binder does not:

- parse source or repair syntax,
- discover declarations already owned by `bray-declarations`,
- construct the compilation-wide symbol identity skeleton,
- define type, ownership, borrowing, effect, capability, contract, or target policy,
- lower the checked source-shaped bound HIR into normalized lowered-bound nodes or lower-level IR,
- execute constant expressions or runtime defaults except through the owning evaluator service,
- own compiler command workflows or eagerly bind the whole program,
- expose mutable bound nodes or partially checked public results.

The bound tree is not syntax with renamed node kinds. It contains resolved semantic references and checked facts that syntax alone
cannot represent.

The checked bound tree is Bray's source-shaped high-level intermediate representation. It is not the normalized lowered-bound form
or the lower-level `bray-ir` representation. It remains closely correlated with source and diagnostics until lowering makes implicit
execution behavior explicit.

---

## Terminology

### Binding

Binding resolves syntax names, paths, members, patterns, declarations, and callable references against lexical scopes and symbol
facts.

Binding also constructs the source-shaped bound representation and asks checker services for the semantic decisions required to
complete that representation.

### Checking

Checking applies language rules after or during reference resolution. It includes type compatibility, overload selection,
conversions, implementation selection, ownership, borrowing, initialization, lifecycle, effects, capabilities, contracts, constant
validity, and target availability.

The binder orchestrates checking. `bray-checker` owns the rule implementations.

### Semantic Unit

A semantic unit is the smallest independently requested, checked, cached, and published source-shaped bound representation.

Not every binder query produces a semantic unit. A binding-dependent symbol fact can return an exact semantic value such as a
resolved type, trait application, or overload target without publishing a bound tree.

Initial semantic unit categories are:

- a declared callable or lifecycle body,
- an anonymous callable together with its signature, contracts, and body,
- a declaration-owned expression such as a runtime default, constant template, predicate definition, constraint, or contract clause.

Patterns, blocks, match arms, and ordinary nested expressions belong to their containing unit. They are not independently published
merely because they have their own lexical scopes.

### Bound Tree

A bound tree is one immutable source-shaped high-level IR arena for one semantic unit.

It owns typed bound nodes and the semantic facts stored on those nodes. It references symbols, types, local symbols, source anchors,
places, and other semantic identities through typed IDs.

### Checked Unit

A checked unit is the public immutable semantic value for one semantic unit. It contains the exact typed root, bound tree, local
symbol snapshot, and nested-unit references required by its contract. The published fact result pairs that value with its unit-owned
diagnostics.

---

## Phase And Crate Boundary

The intended dependency direction is:

```text
Arrows point from a dependency to its consumer.

bray-syntax ---------> bray-binder
bray-declarations ---> bray-symbols
bray-declarations ---> bray-bound-tree
bray-symbols --------> bray-bound-tree
bray-symbols --------> bray-binder
bray-bound-tree -----> bray-checker
bray-symbols --------> bray-checker
bray-bound-tree -----> bray-binder
bray-checker --------> bray-binder
bray-binder ---------> bray-compilation
bray-bound-tree -----> bray-lowering
bray-ir -------------> bray-lowering
```

The exact Cargo edges can be narrower, but these ownership rules are mandatory:

- `bray-symbols` must not depend on `bray-binder`, `bray-bound-tree`, or `bray-checker`,
- `bray-bound-tree` owns published bound node types, typed node IDs, immutable arenas, and bound walkers,
- `bray-checker` owns focused semantic rule services and checker-specific analysis state,
- `bray-binder` owns name resolution, task-local construction state, checker orchestration, and final unit assembly,
- `bray-compilation` owns query keys, caches, dependency tracking, cancellation, and publication,
- `bray-lowering` consumes checked units and must not call back into binding to make semantic decisions.

`bray-binder` does not depend on `bray-compilation`. It defines or consumes a narrow read-only binder fact context that
`bray-compilation` implements when invoking a binding query. That context exposes typed fact requests and cancellation observation,
not compilation cache internals or workflow methods.

Durable semantic fact value types required on bound nodes belong in `bray-bound-tree` or another lower owning representation crate.
`bray-checker` owns the algorithms that produce those values and must not force `bray-bound-tree` to depend back on checker services.

Binding-dependent symbol facts are exposed through symbol-facing APIs but computed through compilation queries implemented by the
binder and checker services. This does not create a reverse crate dependency from `bray-symbols`.

---

## Binding-Dependent Symbol Facts

Symbol construction creates identities and immediate declaration relationships without binding syntax. Some declaration-surface
facts later require the binder or checker but remain symbol-facing results.

Examples include:

- declared parameter, field, result, and constant types,
- implementation subjects and implemented trait applications,
- generic constraint meanings,
- callable contract meanings,
- overload arm targets,
- type-valued member bindings,
- checked constant templates and values,
- checked predicate definitions,
- checked runtime-default summaries.

Each fact has a typed owner-specific query and returns a `DiagnosticResult<T>` with the exact semantic value or category-specific
error value. Callers do not invoke an untyped declaration-binding workflow.

Conceptually:

```rust
impl CallableParameterSymbolView<'_> {
    pub fn declared_type(&self) -> Arc<DiagnosticResult<TypeId>>;
}

impl ImplementationSymbolView<'_> {
    pub fn subject(&self) -> Arc<DiagnosticResult<ImplementationSubject>>;
    pub fn implemented_trait(
        &self,
    ) -> Arc<DiagnosticResult<Option<TraitApplicationId>>>;
}
```

These conceptual signatures omit the query layer's outer cancellation transport. Cancellation is never represented as an error
type, `None`, or an error-aware semantic value.

Exact category APIs and completion rules remain owned by `docs/design/symbols.md`.

A fact that only resolves a type, target, or relationship need not allocate a `BoundTree`. Its result retains the source anchors and
typed error state required by diagnostics and tooling.

A fact whose meaning includes a checked expression can be backed by a category-specific checked semantic unit. The symbol-facing
result exposes the stable semantic summary required by symbol consumers, while the binder-owned checked unit retains the full
source-shaped representation. Symbol records and summaries do not store bound node IDs.

An expression-backed symbol fact and its full checked-unit view have one canonical diagnosing query. The full unit can be cached as
part of that computation or exposed as a projection, but it must not publish a second copy of the same diagnostic bag. Alternate
summary, tooling, and lowering views preserve the original diagnostic identity and ownership.

Binding-dependent symbol facts use the same injected fact context, dependency recording, cycle detection, cancellation, diagnostic
ownership, and immutable publication rules as checked units. Requesting one fact must not force unrelated symbol facts or executable
bodies.

---

## Semantic Unit Identity

### Bound Unit IDs

`BoundUnitId` identifies one exact published bound unit in one compilation snapshot.

It is distinct from:

- a declaration or symbol ID,
- `LocalSymbolRegionId`,
- a bound node ID,
- a callable instance ID,
- an IR unit ID.

One bound unit and its local symbol snapshot use corresponding deterministic keys, but their ID types remain distinct so callers
cannot accidentally use a local region where a bound arena is required.

### Bound Unit Keys

Every query uses a typed `BoundUnitKey`. Its exact fields depend on the unit category and include the explicit semantic context that
selects the answer:

- exact declared or synthesized owner,
- unit category,
- syntax anchor and source version,
- generic checking environment or substitution where applicable,
- selected implementation context where applicable,
- target profile.

Declaration-surface facts, imported facts, nested units, and checker facts requested while computing the unit are query dependencies.
The query engine records those dependency edges. Their complete values are not copied into the unit key.

Declared bodies, anonymous callables, and declaration-owned expressions use distinct key types or a closed validated family. String
keys and unvalidated tuples are not acceptable public APIs.

Keys must not include memory addresses, worker IDs, query request order, cache insertion order, or mutable builder identities.

Numeric unit IDs are compilation-local handles. Persisted caches and tooling use stable keys and dependency fingerprints rather than
raw numeric IDs.

`BoundUnitKey` is the lower domain key owned with the bound-unit contract so checked units can retain nested references without
depending on `bray-compilation`. The compilation query layer wraps it in the exact checked-unit fact key for the current compilation
snapshot and owns interning, caching, scheduling, dependency edges, cancellation, and publication.

---

## Checked Unit API

Public results remain category-specific.

Conceptually:

```rust
pub struct CheckedCallableBody {
    data: CheckedUnitData,
    root: BoundCallableBodyId,
}

pub struct CheckedAnonymousCallable {
    data: CheckedUnitData,
    callable: AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
}

pub struct CheckedPredicateDefinitionUnit {
    data: CheckedUnitData,
    root: BoundExpressionId,
}
```

`CheckedUnitData` is private shared composition. It contains:

- `BoundUnitId`,
- immutable `BoundTree`,
- immutable `LocalSymbolSnapshot`,
- ordered nested semantic-unit keys.

The category-specific wrappers expose only valid roots and relationships. The public API must not use a universal result with
optional body, expression, callable, contract, or constant fields.

Checked-unit construction validates these invariants before publication:

- the root ID belongs to the contained `BoundTree`,
- every bound node ID stored by the tree belongs to that tree's unit,
- the local snapshot region corresponds to the same semantic unit key,
- every local and scope reference resolves through that snapshot,
- nested unit keys are complete, unique where identity requires it, and in canonical source order.

Conceptually:

```rust
impl CheckedCallableBody {
    pub const fn unit(&self) -> BoundUnitId;
    pub const fn root(&self) -> BoundCallableBodyId;
    pub const fn tree(&self) -> &BoundTree;
    pub const fn local_symbols(&self) -> &LocalSymbolSnapshot;
    pub fn nested_units(&self) -> &[BoundUnitKey];
}
```

Anonymous callable, runtime-default, constant-template, predicate-definition, constraint, and contract-clause results expose
corresponding typed wrappers and accessors. Shared private composition must not collapse their distinct completion contracts into one
public `CheckedDeclarationExpression` type.

The exact implementation can use `Arc` around the result or its shared data. Repeated requests must return the same immutable
semantic value and diagnostics for one compilation fact key.

### Lazy Entry Points

Callers request checked units through typed symbol views or `Compilation` fact APIs. They do not construct a binder or call a
phase-execution method.

Body presence is cheap identity-level information and does not force body binding:

```rust
pub enum ExecutableBodyPresence {
    Absent,
    Present,
    Recovered,
}
```

Body-bearing symbol views expose category-specific lazy accessors conceptually shaped like:

```rust
impl FunctionSymbolView<'_> {
    pub fn body_presence(&self) -> ExecutableBodyPresence;

    pub fn checked_body(
        &self,
    ) -> Option<Arc<DiagnosticResult<CheckedCallableBody>>>;
}
```

`checked_body()` returns `None` exactly when body presence is `Absent`. Present and recovered bodies return an error-aware checked
fact. Methods, constructors, lifecycle members, defaulted trait callables, and other body-bearing categories expose their exact
counterparts rather than requiring callers to erase them to one callable kind.

The conceptual signature omits the query layer's outer cancellation transport. Cancellation must not be represented as `None` or as
an error-aware checked body.

Anonymous callable views resolve through their owning local snapshot and expose `CheckedAnonymousCallable`. Declaration-owned
expression facts remain available through their owner-specific symbol APIs. The shared query implementation can use closed internal
unit-key adapters, but public APIs must not expose a generic `bind_syntax` or `bind_unit` workflow.

### Fact Results And Cancellation

Every checked-unit query returns or exposes an immutable fact result containing the value and the diagnostics produced while
computing that unit.

The shared `DiagnosticResult<T>` contract is defined in
[Compiler diagnostics](compiler-diagnostics.md#diagnostic-results). The binder must not introduce a second incompatible
value-plus-diagnostics wrapper.

Cancellation is not a source diagnostic and does not produce an error bound tree. A canceled query returns the query layer's typed
cancellation outcome and publishes no fact value, local snapshot, dependency set, or diagnostics.

Ordinary invalid source publishes an error-aware checked unit and structured diagnostics. It is not returned as an infrastructure
failure.

---

## Bound Tree Storage

### Immutable Per-Unit Arena

Each `BoundTree` owns immutable dense storage for one unit.

The implementation can use mutable per-category arena builders during binding. Publication freezes them into immutable arrays or
equivalent compact storage.

Committed slots use canonical source-semantic construction order. Speculative candidate order, checker worker completion, and query
request order cannot affect published node IDs.

A bound node belongs to exactly one bound unit. Cross-unit relationships use typed unit, symbol, or stable semantic references rather
than direct arena slots from another tree.

The bound tree must be safe for concurrent read-only use after publication.

### Typed Node IDs

Each substantial bound category uses a distinct ID type containing its unit and category-specific slot.

Conceptually:

```rust
pub struct BoundExpressionId {
    unit: BoundUnitId,
    slot: BoundExpressionSlot,
}

pub struct BoundPatternId {
    unit: BoundUnitId,
    slot: BoundPatternSlot,
}

pub struct BoundBlockId {
    unit: BoundUnitId,
    slot: BoundBlockSlot,
}

pub struct BoundCallableBodyId {
    unit: BoundUnitId,
    slot: BoundCallableBodySlot,
}
```

Fields and constructors remain private to bound-tree construction infrastructure. Checked accessors reject an ID from another unit.
Unchecked indexing is crate-private and reserved for established compiler invariants.

A closed `AnyBoundNodeId` can support diagnostics, visitors, debugging, and tooling. Core APIs use exact IDs or narrow family IDs.

`BoundNodeKind` answers which category a node has. It is not node identity.

### Typed Storage API

Conceptually:

```rust
impl BoundTree {
    pub const fn unit(&self) -> BoundUnitId;
    pub fn expression(&self, id: BoundExpressionId) -> Option<&BoundExpression>;
    pub fn pattern(&self, id: BoundPatternId) -> Option<&BoundPattern>;
    pub fn block(&self, id: BoundBlockId) -> Option<&BoundBlock>;
    pub fn callable_body(&self, id: BoundCallableBodyId) -> Option<&BoundCallableBody>;
}
```

The exact storage can be generated mechanically. Public access remains typed and does not expose raw arena indexes, mutable vectors,
or one canonical heterogeneous child list.

### Node Shape

Bound expression, pattern, declaration, block, and callable-body records use category-specific enums or records.

Common private composition can store genuinely shared data such as:

- exact typed node ID,
- source `SyntaxAnchor`,
- recovery or error state.

It must not grow into one public bound node structure containing unrelated optional fields.

Variant-specific records retain their meaningful relationships. For example:

- a bound name expression stores its exact resolved reference,
- a bound call stores the selected callable target, argument mapping, conversions, and used default providers,
- a bound member access stores the selected member and receiver facts,
- a bound local declaration stores its bound initializer and introduced local symbol IDs,
- a bound pattern stores exact introduced bindings, projections, and pattern-checking facts,
- a bound control-flow expression stores its source-shaped branches and checked result merge,
- a bound error expression stores error type and recovery information without pretending resolution succeeded.

### Source Correlation

Every source-originating bound node retains a stable source anchor. Synthesized bound nodes retain an origin describing the source
construct and semantic rule that required them.

Bound storage does not copy source text, trivia, token streams, or green-tree internals. Tooling can use the source anchor and syntax
snapshot to recover syntax detail.

Multiple bound nodes can refer to one syntax anchor when one source construct requires several semantic operations. Synthesized roles
or ordinals distinguish their stable keys when identity is required.

### Parent And Traversal Data

Canonical bound storage is child-directed. Parent pointers are not required on every node.

When parent or path lookup is needed by diagnostics or tooling, `bray-bound-tree` can provide an immutable derived parent index scoped
to one unit. The parent index must not become the canonical ownership model.

Bound walkers and visitors belong to `bray-bound-tree`. Default traversal follows deterministic source-semantic order and supports
explicit subtree skipping and early termination.

---

## Semantic Facts On Bound Nodes

A published checked unit contains every semantic fact promised by its category.

Facts stored on or indexed by bound nodes include, where meaningful:

- expression and pattern types,
- exact resolved symbol or local symbol references,
- selected overload arms and callable instances,
- selected implementations and trait applications,
- implicit and explicit conversion decisions,
- value, place, access, and storage categories,
- ownership, move, borrow, mutation, and initialization outcomes,
- effect, capability, trust, and lifecycle outcomes,
- constant or predicate context validity,
- control completion such as normal, `never`, return, break, continue, yield, propagation, or cancellation behavior,
- source-correlated error facts used for recovery.

Not every fact belongs as a field on every node. Category-specific records and typed side tables should represent only meaningful
states.

Checker-internal traces, solver work queues, temporary constraint graphs, and full per-program-point data-flow states are not durable
bound facts unless a later phase or tooling contract requires them. Publish the semantic conclusions, not every intermediate step.

Bound nodes are immutable after publication. Later analysis must not mutate nodes to add a type, selected target, conversion, or
ownership state that the checked-unit contract already promised.

---

## Symbol And Reference Model

### Surface And Local Symbols

Bound references preserve the distinction between compilation-wide symbols and region-scoped local symbols.

A language operation that accepts both uses a closed typed family, for example:

```rust
pub enum ResolvedValueSymbolId {
    Surface(SurfaceValueSymbolId),
    Local(LocalValueSymbolId),
}
```

Callable, constant, type, predicate, field, and implementation references use their own exact or narrow family types. The bound tree
must not reduce every reference to `AnySymbolId`.

Bound nodes store IDs and checked semantic facts. They do not clone symbol records or cache rendered symbol names.

### Name Lookup

Unqualified lookup combines the current immutable or task-local lexical scope with the applicable declaration-surface lookup facts.

The binder enforces the language's one ordinary namespace and no-shadowing rules. It preserves typed outcomes for:

- one valid target,
- no target,
- ambiguous targets,
- inaccessible targets,
- wrong semantic category,
- malformed or recovered targets.

Local bindings become visible at the language-defined source position. Pattern-arm bindings, guards, block locals, parameters, and
contextual contract symbols use their exact scope rules.

Qualified lookup proceeds through typed module, type, trait, implementation, or value-member APIs. The binder must not rebuild member
sets by walking declaration syntax.

### Anonymous Callables

An anonymous callable uses its deterministic lambda region and local snapshot from `docs/design/symbols.md`.

The enclosing bound lambda expression references the exact `AnonymousCallableSymbolId` and nested unit key. Completing the enclosing
unit requests every nested callable fact needed to satisfy its own type and semantic contract, but nested callable diagnostics remain
owned by the nested unit.

Sibling anonymous callable units can be checked in parallel once their keys and required enclosing declaration context are known.

A lambda body begins a new callable lookup boundary. It does not inherit enclosing locals or an enclosing receiver because Bray
lambdas are capture-free.

---

## Local Symbols And Scopes

The local identity, snapshot, recovery, and lifetime contracts are defined in `docs/design/symbols.md`.

The binder uses a task-local mutable builder to create:

- deterministic local symbol slots,
- lexical scope records,
- visibility start points,
- ordinary-name indexes,
- error-aware duplicate and recovered local records.

Before semantic binding, the binder can perform an identity-only scan over the unit's typed syntax. That scan assigns deterministic
keys and slots in canonical syntax order without resolving names or making type decisions.

Pre-allocating identity does not make a local visible before its declaration. Name-index activation follows the language-defined
scope and source-order rules during binding.

The checked unit publishes the frozen `LocalSymbolSnapshot` with its bound tree. No compilation-wide mutable local registry exists.

Bound scope references use `LocalScopeId`. Scopes are not symbols, storage places, or control-flow blocks.

---

## Binder Inputs And Task-Local State

### Immutable Inputs

A binder request receives typed immutable inputs, including those applicable to the unit:

- read-only binder fact context supplied by the compilation query layer,
- typed syntax root and source snapshot,
- owning symbol or declaration-owned fact key,
- declaration-surface symbol context,
- generic parameters, substitutions, and selected implementation context,
- expected type or expected semantic category,
- target profile,
- callable, predicate, constant, trust, effect, and capability context.

Inputs should be grouped into category-specific request types. The binder must not accept long lists of unrelated optional parameters
or boolean mode combinations.

### Mutable Construction State

One binding task can own mutable state such as:

- bound arena builders,
- local symbol and lexical scope builders,
- a structured diagnostic builder,
- dependency recording,
- checker service state,
- expected-context stacks,
- control-target stacks,
- fact-context and data-flow work state,
- speculative checkpoints and rollback trails.

This state is task-local and never exposed through public APIs. It is consumed when the immutable checked unit is finalized.

The binder must not hold compilation cache locks while requesting another fact. Shared compilation state is accessed through the
query API rather than mutable references embedded in binder state.

### Binding Contexts

Context categories should be explicit types or enums. Examples include:

- ordinary expression context,
- type-expression context,
- constant-expression context,
- predicate-expression context,
- pattern context with operation mode,
- callable body context,
- contract clause context,
- trusted boundary context.

Context types carry only facts relevant to that category. They should prevent invalid requests such as binding an assignment in a
predicate context without relying on scattered boolean checks.

---

## Binding And Checking Order

One checked-unit computation proceeds conceptually as follows:

1. Validate the typed query key, syntax root, and owner relationship.
2. Establish the bound unit and corresponding local region identities.
3. Scan local identity and lexical-scope shape in canonical syntax order where the unit can introduce locals.
4. Bind names, paths, declarations, patterns, and nested unit references in source-semantic order.
5. Request symbol facts and checker decisions through typed APIs as their inputs become available.
6. Record checked facts and error-aware recovery on task-local bound builders.
7. Run required whole-unit checker analyses over the task-local representation.
8. Freeze the bound arenas and local snapshot, finalize diagnostics, and return recorded dependency edges to the query layer.
9. Publish the complete immutable fact atomically through the compilation query.

Note: This is a dependency order, not a requirement for one monolithic function. Focused binders and checker services should own the
individual grammar and semantic categories.

The binder must not create a valid-looking partial public tree after step 4 and mutate it later. Intermediate representations remain
private task-local construction state.

---

## Checker Service Integration

Checker services own semantic rules. The binder owns when to ask for them and where to place their immutable conclusions.

Focused checker APIs should accept typed semantic inputs and return typed outcomes with structured diagnostics. Examples include:

- checking a conversion,
- resolving and validating an overload candidate set,
- selecting an implementation witness,
- checking a pattern against a subject type and operation mode,
- checking ownership transfer or borrowing,
- merging branch types and states,
- validating effects, capabilities, and trusted obligations,
- validating constant or predicate context,
- finalizing whole-unit control-flow, initialization, and lifecycle facts.

Checker services must not append user diagnostics to a process-global bag, mutate published symbols, or publish bound nodes
independently of the binder's unit transaction.

A checker can use a private analysis representation over task-local bound data. It returns conclusions for the binder to store before
publication. If an analysis requires a control-flow graph, that graph is checker-owned unless the durable bound or lowering contract
specifically requires it.

When a checker needs whole-unit structure, it receives a read-only draft or analysis view whose type is owned by `bray-bound-tree`.
It must not depend on binder-private builders. The binder freezes task-local structural nodes into that view, receives typed checker
conclusions and side tables, and then finalizes the published tree without cloning the complete unit.

Binding and checking can be mutually dependent at a fine grain. For example, overload selection can require argument types while
argument binding can use parameter expectations. Such cooperation uses explicit typed candidate and expected-context APIs, not phase
ownership shortcuts or mutable partially published nodes.

---

## Speculative Binding

Speculation is allowed for ambiguous semantic candidates, contextual typing, overload exploration, and implementation selection.

Speculation uses checkpoints over task-local state. A checkpoint records enough information to restore:

- bound arena lengths and side-table trails,
- local symbol and scope builder state,
- diagnostics produced by the candidate,
- expected and control-context stacks,
- checker constraints and temporary decisions,
- a candidate-local dependency log for relevance classification.

Abandoning a candidate restores its semantic construction state. It does not leave bound nodes, locals, scopes, selected targets, or
diagnostics in the enclosing unit. Its dependency log is classified separately according to whether each observation influenced the
committed answer.

Committing a candidate transfers its results through the ordinary builder path. Committed binding never bypasses normal recovery,
diagnostics, or semantic validation.

Facts requested from compilation while exploring a candidate remain immutable cached facts owned by their own keys. Their
diagnostics are not merged into the current unit merely because an abandoned candidate inspected them.

Abandoning a candidate does not automatically discard its query dependencies. A fact that influenced candidate rejection, candidate
ordering, ambiguity, or the final selected result remains a dependency of the enclosing unit because changing that fact can change
the answer. Only observations proven irrelevant to the committed semantic result can be omitted from the published dependency set.

Checkpoint implementations should use arena lengths and rollback trails where practical. They should not clone an entire binder,
scope graph, bound tree, or diagnostic bag for every candidate.

Speculation must be deterministic. Candidate order comes from typed lookup and overload APIs, never hash iteration or worker
completion order.

---

## Recovery And Error Representation

Malformed user source must produce error-aware checked units rather than compiler panics.

The binder consumes parser recovery explicitly:

- missing syntax remains distinguishable from present syntax,
- skipped syntax remains available through the syntax tree,
- recovered declarations and symbols retain their recovery state,
- missing names do not become empty-string lookup entries.

The bound tree provides category-specific error forms such as error expressions, error patterns, unresolved references, error calls,
and error conversions. An error form preserves the known source shape and semantic category without pretending a valid target or
type was found.

When recovery can determine a useful type, symbol category, scope, or control effect, the error node retains it. Otherwise, it uses the
corresponding error type or typed unknown result.

Recovery should suppress diagnostics that merely restate an earlier root cause while preserving independent errors. Binder and
checker diagnostics remain owned by the operation with enough information to report them accurately.

Ordinary malformed source must never cause unchecked indexing, `unwrap()`, `expect()`, or an impossible-kind panic. Panics are
reserved for compiler invariants that validated internal APIs make unreachable from user input.

---

## Diagnostics

The binder owns diagnostics for:

- unresolved and ambiguous names or paths,
- invalid lexical scope and visibility timing,
- illegal shadowing and duplicate local lookup names,
- wrong semantic category at a reference site,
- invalid reference forms that do not require checker policy,
- malformed binding relationships not already diagnosed by syntax or symbol construction.

Checker services own diagnostics for the semantic rules they implement, including type compatibility, overload validity,
implementation selection, ownership, borrowing, initialization, lifecycle, effects, capabilities, contracts, constant validity, and
target availability.

Diagnostics use `bray-diagnostics` kinds, typed arguments, labels, notes, suggestions, and source spans. Binder and checker logic must
not construct user-facing English. Rendering goes through `bray-messages`.

Every semantic unit has one canonical diagnosing fact. Executable and anonymous callable units own their unit diagnostics directly.
A declaration-owned expression backed by a symbol fact uses that symbol fact as the canonical diagnostic owner. Nested semantic
units own their own diagnostics even when an enclosing unit depends on them. Compilation diagnostics traverse required canonical
facts and merge their bags deterministically without duplicating dependency diagnostics into each caller or projection.

Diagnostic order within one unit follows stable source-semantic traversal and typed diagnostic ordering. Parallel worker completion
order is never observable.

---

## Nested Units

Nested anonymous callables are separate semantic units because they own callable surfaces, parameters, contracts, execution scopes,
local symbols, and bodies.

An enclosing checked unit records nested unit keys in deterministic source order. It does not embed or clone nested bound trees.

The enclosing unit requests the nested facts required to establish its own checked result. A package-check or emission query follows
the complete reachable nested-unit graph according to its completion contract.

Nested diagnostics are collected once from the nested fact. An outer unit can contain an error-aware lambda reference when the nested
unit is invalid without copying every nested diagnostic into the outer bag.

This model allows sibling nested units and unrelated declared bodies to run in parallel while preserving deterministic traversal and
diagnostic ordering.

---

## Concurrency, Cycles, And Reentrancy

Published checked units, bound trees, local snapshots, and fact diagnostics are immutable and safe for concurrent read-only use.

Independent units can bind and check in parallel when their query dependencies are available.

The compilation query engine owns evaluation state, dependency stacks, waiting relationships, and cycle detection. Binder code must
not implement ad hoc global locks or recursive one-time cells.

Cycle policy is fact-specific:

- ordinary recursive callable references resolve through callable surfaces without recursively requiring bodies,
- a body that calls itself does not create a body-query cycle,
- constant and predicate cycles follow their evaluator and checker rules,
- nested unit dependencies must not form ownership cycles,
- an impossible internal query cycle is a compiler invariant failure, not a fabricated user diagnostic.

A worker must not wait for another fact while holding a mutable fact publication lock. Cancellation unwinds task-local state and
publishes nothing.

Observable IDs, node order, lookup results, nested-unit order, and diagnostics must be identical under serial and parallel execution.

---

## Incrementality And Reuse

The first binder implementation does not require a complete incremental engine, but its contracts must allow one.

Requirements:

- query keys include every semantic input that can affect the result,
- syntax, symbol facts, target facts, and selected implementation dependencies are recorded explicitly,
- published units and trees are immutable,
- unit and local IDs are interpreted only within their compilation snapshots,
- persisted references use stable keys and source anchors rather than raw arena slots,
- unchanged units can be reused only when their exact dependency fingerprints remain valid,
- replacing one body does not rebuild the compilation-wide symbol graph or unrelated local snapshots.

Structural sharing is permitted when ownership and source-correlation contracts remain clear. The binder should not clone a complete
unchanged tree merely to attach one changed side fact.

---

## Lowering Contract

Lowering consumes only complete checked units.

The bound representation must provide lowering with:

- exact resolved callable, member, implementation, and symbol targets,
- argument-to-parameter mapping and runtime default-provider choices,
- explicit conversion decisions,
- checked place, access, ownership, borrow, and movement behavior,
- control-target and completion behavior,
- effect, capability, trust, lifecycle, destruction, and finalization conclusions,
- nested callable unit references,
- source anchors required for downstream diagnostics.

The first lowering stage transforms the checked source-shaped HIR into a normalized lowered-bound representation. Lowered-bound nodes
can introduce temporaries, explicit control-flow blocks, merge values, cleanup paths, and other execution machinery that do not need
source-level symbol identities.

The second stage translates that normalized bound representation into the lower-level backend-independent `bray-ir` representation.
The split lets source-oriented semantic lowering finish before lower-level IR construction without pretending the checked bound tree
was not already an intermediate representation.

Lowering must not perform name lookup, overload resolution, implementation selection, type inference, borrow checking, or contract
proof. If lowering cannot proceed without one of those decisions, the checked-unit contract is incomplete and must be fixed in the
binder or checker layer.

---

## Public API Direction

Public cross-crate binder and bound-tree APIs should favor:

- category-specific checked-unit types,
- typed unit, node, scope, symbol, type, place, and target IDs,
- immutable borrowed access or shared immutable ownership,
- exact root accessors,
- typed node and relationship accessors,
- structured error and recovery variants,
- explicit compilation fact queries,
- deterministic walkers and visitors.

They should avoid:

- mutable public bound nodes,
- universal nodes with unrelated optional fields,
- raw arena indexes,
- strings as semantic identities or query keys,
- caller-managed phase workflow methods,
- global diagnostic bags,
- global local-symbol registries,
- trait-object inheritance as canonical node storage,
- syntax or symbol cloning for convenience,
- APIs that return a partially checked tree under a complete result name.

Crate-private traits and macros can remove mechanical repetition when they preserve concrete public node and result types. They must
not hide semantic policy or recreate an inheritance hierarchy.

---

## Testing

Binder tests should validate semantic contracts rather than builder implementation details.

Required unit coverage includes:

- exact typed node and unit ID separation,
- rejecting cross-unit node and scope IDs through checked accessors,
- deterministic arena and local slot assignment,
- source anchors and synthesized origins,
- lexical visibility start points and no-shadowing behavior,
- qualified and unqualified lookup outcomes,
- surface-versus-local resolved reference families,
- pattern binding identity, including coherent alternative patterns,
- capture-free anonymous callable boundaries,
- complete category-specific checked-unit roots,
- category-specific error nodes and recovery facts,
- checker facts being present before publication,
- speculative checkpoint rollback and commit,
- abandoned candidates publishing no nodes, locals, or diagnostics,
- cancellation publishing no partial unit,
- nested units retaining independent diagnostics,
- deterministic diagnostics and nested-unit order under serial and parallel execution,
- recursive surface references not forcing recursive body binding,
- malformed source producing checked error units without panics,
- lowering obtaining required semantic decisions without rebinding.

Bound-tree tests should validate immutable typed storage, concrete node relationships, walkers, parent indexes where provided, and
source-order traversal.

Checker tests should validate each semantic rule service and its structured outcomes independently where practical.

Integration tests should request checked units through `Compilation` rather than manually executing phase workflows. They should
cover valid programs, invalid programs, lazy demand, repeated requests, nested callables, parallel scheduling, and deterministic
diagnostic collection.

Fuzzing should cover binder entry points with valid and malformed syntax trees. Invalid source must produce error-aware results or
structured diagnostics, never memory unsafety or user-triggered panics.

---

## Decisions Requiring Follow-Up

### Type And Constant Representation

The exact `TypeId`, generic substitution, constant value, and type interning contracts must be finalized before concrete expression
and signature node APIs are implemented. Their ownership must not introduce a dependency cycle through `bray-symbols`.

### Place And Dependency Representation

The bound representation needs a typed model for values, places, projections, storage identity, borrows, and dependency contracts.
The exact split between durable bound facts and checker-owned data-flow state should be designed with ownership and borrowing.

### Control-Flow Analysis Representation

Checker services will require control-flow and data-flow representations for reachability, initialization, ownership, borrowing,
lifecycle, and fact propagation. Whether those analyses share one checker-internal graph or use focused graphs remains to be settled.
The source-shaped checked bound tree remains the canonical published HIR either way. Lowering separately publishes or consumes the
normalized lowered-bound representation required before `bray-ir` construction.

---

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Define the injected binder fact context and binding-dependent symbol-fact provider contracts.
2. Define bound unit kinds, typed IDs, stable keys, origins, and checked accessor behavior.
3. Define immutable per-unit bound storage and category-specific error nodes.
4. Define category-specific checked-unit results and `DiagnosticResult<T>` publication integration.
5. Implement local snapshot and lexical-scope builders against the contracts in `docs/design/symbols.md`.
6. Define binder request contexts, task-local builders, and deterministic dependency recording.
7. Implement typed name and path resolution over surface and local symbol APIs.
8. Implement patterns, locals, blocks, and anonymous callable unit boundaries.
9. Add expression, call, member, conversion, and control-flow bound nodes incrementally by grammar category.
10. Integrate focused checker services and whole-unit analysis finalization.
11. Add deterministic diagnostic aggregation, cancellation, speculation, and parallel-query tests.
12. Establish the checked-HIR-to-lowered-bound and lowered-bound-to-`bray-ir` boundaries before implementing production lowering.

Each step must publish only complete immutable facts and must not add temporary eager workflow APIs.
