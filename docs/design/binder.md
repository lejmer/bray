# Binder and bound tree design

This document defines the goal-state architecture for name binding, semantic-analysis orchestration, checked semantic units, and the
immutable bound representation.

The language documents define Bray semantics.

`docs/design/compiler-architecture.md` defines the compiler-wide phase and query model.

`docs/design/symbols.md` defines semantic identities, symbol facts, completion, and local symbol snapshots.

`docs/design/compiler-known-catalog.md` defines compiler-known declaration surfaces and typed compiler-provided behavior roles.

`docs/design/compiled-package-interfaces.md` defines source-independent imported surfaces and checked declaration-owned templates.

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

### Binding Method Naming

Functions and methods whose responsibility includes binding typed syntax into bound semantics use the `bind_*` prefix. This follows
the compiler-wide phase naming convention: the lexer scans with `scan_*`, the parser parses with `parse_*`, and the binder binds
with `bind_*`. Examples include `bind_expression`, `bind_pattern`, `bind_block`, `bind_type_expression`, and
`bind_callable_body`.

This rule applies at every implementation level, not only to top-level binder entry points. A `bind_*` method can call more focused
`bind_*` methods, and a method that orchestrates or delegates binding still uses the prefix when binding is its semantic
responsibility. This keeps every operation that performs binding discoverable through one consistent name search.

The prefix is reserved for operations that perform actual syntax binding. Helpers that only resolve lookup candidates, request
facts, run checker policy, construct already-decided bound values, or publish completed results use names that describe those
narrower responsibilities instead.

These names describe internal binder implementation operations. Public symbol and compilation APIs remain lazy fact accessors such
as `checked_body()` and must not expose caller-driven `bind_*` workflow methods.

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
storage identities, storage accesses, and other semantic identities through typed IDs.

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

- `bray-symbols` owns canonical semantic types, constant values, open constant terms, generic substitutions, and their typed IDs in
  addition to declaration symbols, and must not depend on `bray-binder`, `bray-bound-tree`, or `bray-checker`,
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

## Semantic Types, Constants, And Substitutions

### Ownership

`bray-symbols` owns the canonical semantic identity substrate that directly composes with symbols:

- `TypeId` and immutable semantic type records,
- `ConstantValueId` and immutable closed typed constant values,
- `ConstantTermId` and immutable open checked constant terms,
- `GenericSubstitutionId`,
- `ConcreteGenericSubstitutionId`,
- trait applications, callable instances, implementation instances, and related canonical identities,
- interner and read-only view APIs for those values.

These values are not declaration symbols. They live in `bray-symbols` because their identities directly reference typed symbol IDs,
symbol APIs return them, and placing them in a higher crate would create a dependency cycle. A separate type crate would require
moving all typed symbol IDs into another lower identity crate without currently creating a clearer ownership boundary.

Other responsibilities remain separate:

- `bray-binder` resolves type syntax, constant references, and generic arguments,
- `bray-bound-tree` owns checked constant-expression templates and source-shaped HIR,
- `bray-checker` owns inference variables, unification, type relations, constraint proofs, and constant evaluation,
- `bray-compilation` owns semantic-store instances, query caches, target-specific evaluation, cancellation, and publication,
- `bray-package-interface` maps canonical values to and from stable interface encodings,
- `bray-lowering` consumes finalized types and values without rerunning type checking or constant evaluation.

`bray-symbols` owns pure structural construction, interning, inspection, and substitution over already validated semantic values. It
does not own the semantic algorithms that determine whether a conversion, constraint, operation, or constant expression is valid.

### Semantic Store

One compilation or immutable symbol snapshot owns a `SemanticValueStore` associated with its symbol identity space.

Conceptually, the store contains append-only canonical tables for:

- types,
- closed constant values,
- open constant terms,
- generic substitutions,
- trait applications,
- callable instances,
- implementation instances.

Entries are immutable after insertion. Internal synchronized mutation is permitted only to intern a new immutable entry or publish a
completed lookup cache. Concurrent construction of the same structural key must return one canonical ID in that store.

The store is not process-global because its records contain compilation-local symbol IDs. `TypeId`, `ConstantValueId`,
`ConstantTermId`, and substitution IDs are valid only with the semantic store that issued them.

Context-bound views provide checked access. Cross-store use is a compiler API error and must not be caused by ordinary malformed user
source.

### Type Representation

`TypeId` identifies one canonical immutable `TypeData` record.

Conceptually, durable variants include:

```rust
pub enum TypeData {
    Error,
    Named {
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    },
    TypeParameter(GenericTypeParameterSymbolId),
    AssociatedTypeProjection {
        application: TraitApplicationId,
        member: TraitTypeMemberSymbolId,
    },
    Tuple(Arc<[TypeId]>),
    Array {
        element: TypeId,
        length: ConstantTermId,
    },
    Slice(TypeId),
    Nullable(TypeId),
    Borrow {
        kind: BorrowKind,
        target: TypeId,
    },
    TraitView(TraitApplicationId),
    OwnedIndirection {
        storage: TypeId,
        target: TypeId,
    },
    Callable(CallableTypeData),
}
```

The exact variants follow durable semantic type categories and language-defined type-form identity, not parser productions. Callable
type data includes every parameter, result, contract, effect, capability, and ABI component that participates in callable type
identity.

A named constructed type retains its exact definition symbol and ordered generic substitution. Structural type records retain every
subject type and compile-time argument that participates in identity.

Associated type projections remain explicit canonical types while their selected type is not globally fixed. A context that selects
an implementation can resolve the projection through an ordinary semantic fact without mutating the original `TypeId`.

One canonical error type supports recovery. The diagnostic belongs to the fact that produced the error type. Error types do not
embed diagnostic IDs or source text and are forbidden in successfully emitted package interfaces.

### Inference Types

Inference variables are checker-local work state, not canonical semantic types:

```rust
pub struct InferenceTypeId(u32);
```

Inference variables, unification parents, candidate sets, deferred constraints, and solver obligations remain in a checker-owned
inference context. They must not be interned as `TypeId`, stored on published checked bound nodes, serialized into package interfaces,
or exposed by completed symbol facts.

Before publication, every inference variable is resolved to a canonical `TypeId` or the canonical error type with diagnostics owned
by the checking fact.

### Closed Constant Values

`ConstantValueId` identifies one fully evaluated, typed, materializable constant value.

Conceptually:

```rust
pub struct ConstantValueData {
    ty: TypeId,
    kind: ConstantValueKind,
}
```

`ConstantValueKind` is a closed category-specific representation for language-permitted constant values, including:

- Boolean and character values,
- typed integer values,
- selected-runtime-format real and complex values,
- strings,
- `unit` and nullable absence,
- nullable presence,
- tuples and arrays,
- constant product values,
- constant union variant values,
- other aggregate forms explicitly permitted by constant materialization rules.

Aggregate records reference child `ConstantValueId` values and form an immutable canonical DAG. They do not represent runtime storage
identity.

Integer evaluation can use exact intermediate arithmetic in checker-owned state. A published integer constant is normalized to its
declared type after representability has been checked. Real and complex values retain the exact selected runtime-format bits. Strings
use canonical string content. Aggregate identity includes exact type, variant where applicable, and ordered child values.

A canonical error constant value supports recovery. Its diagnostics remain on the failed constant-instance fact. Error values are
never valid generic arguments for concrete instantiation and are forbidden in emitted interfaces.

Literal adaptation intermediates, unbounded evaluation integers, evaluation stacks, resource counters, and traces are checker-owned
work values rather than `ConstantValueKind` variants.

### Open Constant Terms

A const parameter or an expression such as `N + 1` is not a closed `ConstantValueId`. Open compile-time expressions used in generic
types, type forms, substitutions, and static facts use `ConstantTermId`.

Conceptually, durable term variants include:

```rust
pub enum ConstantTermData {
    Value(ConstantValueId),
    Parameter(GenericConstParameterSymbolId),
    TargetFact(TargetFactId),
    Unary {
        operation: ConstantUnaryOperation,
        operand: ConstantTermId,
    },
    Binary {
        operation: ConstantBinaryOperation,
        left: ConstantTermId,
        right: ConstantTermId,
    },
    DefinitionApplication {
        definition: AnyConstantDefinitionId,
        substitution: GenericSubstitutionId,
        selected_implementation: Option<ImplementationInstanceId>,
    },
    Call {
        callable: CallableInstanceId,
        arguments: Arc<[ConstantTermId]>,
    },
    Projection(ConstantProjection),
}
```

The exact variants cover checked constant forms required in semantic identity. They reference selected semantic operations and exact
symbols, not syntax tokens or unresolved names.

`DefinitionApplication` can remain open. `ConstantInstanceKey` is the separate concrete evaluation key and requires a
`ConcreteGenericSubstitutionId` plus the selected target profile.

This is a restricted canonical identity representation, not a second general bound tree. Full checked constant initializer,
predicate, contract, and runtime-default templates remain in `bray-bound-tree`.

Closed subterms are evaluated and interned as `Value`. Open terms preserve evaluation order and selected operations. Interning does
not perform arbitrary algebraic rewriting. For example, `N + 1` and `1 + N` normally have different open term identities even if a
particular checking context can prove them equal.

### Generic Substitutions

A substitution records its exact generic owner and ordered arguments:

```rust
pub struct GenericSubstitutionData {
    owner: GenericOwnerId,
    arguments: Arc<[GenericArgument]>,
}

pub enum GenericArgument {
    Type(TypeId),
    Constant(ConstantTermId),
}
```

The owner lets canonical construction validate parameter count, order, and argument category. An empty substitution is canonical for
its owner rather than an untyped globally reusable empty list.

`GenericSubstitutionId` can describe an open generic context. `ConcreteGenericSubstitutionId` is a validated typed wrapper whose type
arguments are concrete and whose constant terms have evaluated to valid closed values. Operations that require concrete
instantiation, including concrete constant-instance evaluation and code generation, require the concrete ID rather than repeatedly
checking an open substitution.

Applying a substitution is a pure structural operation over canonical semantic values. It creates or reuses canonical types, terms,
trait applications, and instances through the semantic store. It does not perform name lookup, overload resolution, constraint
proof, or constant evaluation.

### Equality And Contextual Proof

Within one semantic store, equal canonical IDs guarantee equal canonical representations.

Closed constructed types use exact definition identity and exact ordered type and constant values. Open types use canonical open
terms. Different open `TypeId` values are not globally merged because one local generic constraint proves their const arguments
equal.

The checker owns context-sensitive type relations. A constraint context can prove two different open terms or open types equivalent
for one operation without changing global interning. Consequently:

- equal `TypeId` values are definitively the same canonical type,
- unequal concrete `TypeId` values are different concrete types,
- unequal open `TypeId` values can still be proven equivalent in an exact constraint context,
- such a proof is a checker fact and does not mutate or alias either canonical ID.

This keeps interning deterministic and context-independent while allowing generic proofs to establish the relationships required by
the language.

### IDs, Determinism, And Persistence

Semantic value IDs are opaque store-local cache handles. Equivalent structural keys return the same ID within one store, but numeric
ID values are not serialized and need not remain equal across compilations, snapshots, or different lazy demand orders.

Compiler outputs, interface encoding, diagnostics, and deterministic ordering must use canonical structural keys or rendered
semantic values rather than numeric ID order. Numeric assignment must never become observable language behavior.

Incremental reuse across snapshots uses stable symbol keys, canonical type and constant keys, package-interface values, and explicit
remapping. It does not persist raw `TypeId`, `ConstantValueId`, `ConstantTermId`, or substitution integers.

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
- value, storage-access, and other expression-result categories,
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

## Storage Access And Dependency Contracts

### Terminology

The binder and bound representation use the language terminology defined in
`docs/language/ownership-and-borrowing/storage-and-access-paths.md`:

- storage is a runtime entity that can hold a value or part of a value,
- substorage is a potentially distinct part of storage,
- a storage access is one evaluated access-path occurrence that reaches storage or substorage,
- a projection is one component of an access path,
- a value is distinct from the storage that currently contains it.

The compiler-theory term "place" is not part of the Bray semantic model or public API. Using storage identity and storage access
separately makes the relevant distinction without introducing a second term for the same language concept.

### Typed Identities And Ownership

`bray-bound-tree` owns unit-scoped typed identities for storage and instantiated dependency facts. Conceptually:

```rust
pub struct StorageIdentityId {
    unit: BoundUnitId,
    slot: StorageIdentitySlot,
}

pub struct StorageAccessId {
    unit: BoundUnitId,
    slot: StorageAccessSlot,
}

pub struct BorrowCapabilityId {
    unit: BoundUnitId,
    slot: BorrowCapabilitySlot,
}

pub struct BoundDependencyContractId {
    unit: BoundUnitId,
    slot: BoundDependencyContractSlot,
}
```

Fields remain private. Task-local builders issue IDs, checked accessors reject IDs from another unit, and publication freezes the
records with the rest of the checked unit. Raw slots are never persisted or used as cross-compilation identity.

A `StorageIdentityId` represents one exact or symbolic storage origin in the unit. Origins include local owned storage,
parameter-provided storage, receiver storage, source-correlated temporaries, allocation results, compiler-created storage, and
error storage used for recovery. An origin record retains its introducing symbol or bound node where one exists, but that provenance
does not make the symbol or node itself the storage identity.

A local binding is a symbol, not storage. Its checked binding fact records whether the binding introduces owned storage, names an
existing storage access, or binds another non-storage result. Destructuring can therefore introduce new storage for some bindings
and derived accesses for others without conflating binding identity with storage identity.

A storage identity for an allocation or other storage owned through indirection remains associated with the movable owner value's
semantic ownership and dependency facts. Moving the owner changes the access through which the owned storage is reached. It does not
manufacture a second identity for that allocation. The destination storage that contains the moved owner value remains a separate
storage identity.

### Storage Accesses And Projections

A `StorageAccessId` identifies one evaluated access-path occurrence. Its immutable record contains an access root, ordered
projections, reached type, source anchor, and recovery state. Conceptually:

```rust
pub struct StorageAccess {
    root: StorageAccessRoot,
    projections: Box<[StorageProjection]>,
    reached_type: TypeId,
    source: SyntaxAnchor,
    is_recovered: bool,
}
```

Roots can reference a unit storage identity directly or derive access through an active borrow capability or an owning value that
carries indirection storage. Recovery roots preserve an error storage identity. Projections include fields, tuple elements, active
union payload fields, array or slice elements, slice ranges, nullable contents, owned-indirection contents, and compiler-known
type-form projections.

Projection records retain exact typed semantic operands. For example, a field projection references its field symbol, while a
dynamic index or range projection references the checked selector expressions needed by overlap analysis. They do not reduce
selectors to source text.

Storage accesses are occurrence identities and are not structurally interned. Two evaluations of `items[index]` can reach different
storage even when their syntax is identical because `index` can change between evaluations. Conversely, different access paths can
reach the same storage. ID equality therefore never substitutes for alias or overlap analysis.

Equal `StorageIdentityId` values name the same modeled origin. Unequal IDs for symbolic or externally supplied origins do not prove
that the runtime storage is disjoint. Parameter modes, borrow derivation, projection semantics, and current flow facts still
determine the relationship between accesses.

Checker APIs return a typed storage relationship such as identical, disjoint, potentially overlapping, or error. A stronger result
requires a proof from projection semantics and the current flow facts. Failure to prove disjointness produces potentially
overlapping storage rather than an optimistic assumption.

### Borrow Capabilities

A borrow operation creates a distinct `BorrowCapabilityId` whose durable record identifies the borrow kind, reached storage access,
source anchor, and dependency contract established by the operation. Storage reached through a reborrow records the capability from
which it was derived.

The ID names the semantic capability created by the operation. Whether that capability is active at a particular program point is
checker-owned flow state. Ending, moving, shortening, or invalidating a borrow changes that flow state and does not mutate the
published capability record.

### Portable And Instantiated Dependency Contracts

Dependency contracts have two representations with different identity domains.

`bray-symbols` owns `DependencyContractTemplateId`. A template is a normalized, source-independent declaration contract whose formal
subjects can reference the receiver, parameters by stable ordinal, result, projections from those subjects, scoped declaration
capabilities, and required implementation witnesses. Templates are suitable for symbol facts, generic substitution, compiled
package interfaces, and cross-compilation structural identity. Numeric template IDs remain semantic-store local and are not
serialized.

`bray-bound-tree` owns `BoundDependencyContractId`. A bound contract is the instantiated contract for a value, storage access,
borrow, callable value, trait view, task, thread, or other result inside one checked unit. It can reference exact
`StorageIdentityId`, `StorageAccessId`, `BorrowCapabilityId`, scoped capability, implementation witness, and lifecycle-obligation
identities valid in that unit.

Typed requirements cover at least:

- storage that must remain alive,
- storage or substorage that must remain initialized,
- borrow capability that must remain active,
- mutation authority that must remain exclusive,
- scoped capability that must remain live,
- lifecycle, destruction, finalization, cancellation, or joining obligations that must remain attached.

A dependency contract can also retain guarded requirements. Guards represent semantic conditions such as nullable presence, an active
union variant, or another checked state under which a nested dependency exists. Contracts must not flatten such requirements into an
unconditional set when doing so would reject valid programs or lose required invalidation behavior.

Contract records are immutable, normalized, deterministically ordered, and deduplicated. Equivalent contract structure can be
interned within its owning identity domain. Formal templates and unit-local instantiated contracts are never assigned the same ID
type or stored in one arena.

Every checked value or storage-access expression result carries a dependency contract. A value contract describes the non-local
requirements that must remain valid while the value is used. A storage-access contract describes the requirements for continuing to
reach and operate on that storage. Moving a value moves its carried contract with the value rather than leaving the contract attached
to the old access path.

Instantiating a declaration contract maps formal subjects to exact argument, receiver, result, capability, and implementation facts.
That operation is typed and checked. It does not substitute source strings, syntax nodes, or unvalidated numeric ordinals.

### Durable Facts And Flow State

The published checked unit retains durable semantic structure and conclusions:

- storage origins and provenance,
- evaluated storage accesses and ordered projections,
- borrow-capability origins and derivation,
- dependency contracts attached to checked results,
- checked operation categories and final ownership or borrowing outcomes,
- storage relationship proofs required by lowering or tooling.

Checker-owned task-local state retains facts that change by program point:

- initialization and partial-initialization state,
- moved and partially moved state,
- active variant and nullable-presence refinements,
- active borrows and capabilities,
- current mutation authority and exclusivity,
- temporary alias, overlap, and dependency-propagation facts,
- analysis work lists, transfer state, and merge state.

The checker returns the immutable conclusions required by the checked-unit contract before publication. Full per-program-point state
does not become fields on source-shaped nodes unless a later tooling or lowering contract specifically requires a durable projection.

### Semantic And Query Dependencies

A Bray dependency contract is a language-semantic fact. A compiler query dependency is an incremental-compilation edge between fact
requests. APIs and records always use the complete names `DependencyContract`, `FactDependency`, or `QueryDependency` as
appropriate. A generic `DependencyId` or `DependencySet` must not make the two concepts ambiguous.

Requesting a symbol contract, storage-related target fact, or implementation witness can record query dependencies while producing a
dependency contract. The query edges remain owned by `bray-compilation`; the semantic contract remains owned by `bray-symbols` or
the checked unit according to its identity domain.

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

Bound scope references use `LocalScopeId`. Scopes are not symbols, storage identities, storage accesses, or control-flow blocks.

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

Local checker services can use private analysis representations over task-local bound data. Whole-unit flow services use the shared
checker-internal topology defined below. Both return conclusions for the binder to store before publication and neither publishes
checker-owned intermediate state as bound or lowering data.

When a checker needs whole-unit structure, it receives a read-only bound unit view whose type is owned by `bray-bound-tree`.
It must not depend on binder-private builders. The binder freezes task-local structural nodes into that view, receives typed checker
conclusions and side tables, and then finalizes the published tree without cloning the complete unit.

Binding and checking can be mutually dependent at a fine grain. For example, overload selection can require argument types while
argument binding can use parameter expectations. Such cooperation uses explicit typed candidate and expected-context APIs, not phase
ownership shortcuts or mutable partially published nodes.

---

## Control-Flow And Data-Flow Analysis

### Shared Analysis Topology

`bray-checker` owns one checker-internal control-flow topology for each semantic unit that requires whole-unit flow analysis. The
topology is built from the committed read-only bound unit view after binding has fixed source-semantic evaluation order. It is
immutable after construction and is shared by the focused analyses for that unit.

The topology is not:

- the canonical source-shaped bound HIR,
- a second published bound tree,
- the normalized lowered-bound representation,
- backend-independent IR,
- a symbol or compiled package-interface fact.

It splits source-shaped control only as far as semantic analysis requires. It does not introduce lowering temporaries, explicit drop
operations, cleanup blocks, ABI operations, or backend-oriented instructions. Scope-exit and lifecycle checks use typed control
points and edge metadata without pretending those checks are already lowered execution.

Constructing separate control-flow graphs for reachability, initialization, ownership, borrowing, lifecycle, and fact propagation is
not permitted. Those analyses must agree on one evaluation order and one branch topology. Focused analyses share the graph while
retaining their own state and transfer rules.

An analysis can derive indexes and views keyed by the shared block and edge IDs, including strongly connected components, loop
forests, dominators, postdominators, reachable-block masks, and reverse traversal indexes. Such data does not define another
control-flow topology and must not assign competing operation order or edge semantics.

### Graph Shape And Typed IDs

The graph uses checker-private unit-scoped IDs for blocks, edges, operations, and program points. Conceptually:

```rust
pub(crate) struct AnalysisBlockId {
    unit: BoundUnitId,
    slot: AnalysisBlockSlot,
}

pub(crate) struct AnalysisEdgeId {
    unit: BoundUnitId,
    slot: AnalysisEdgeSlot,
}

pub(crate) struct ProgramPointId {
    unit: BoundUnitId,
    slot: ProgramPointSlot,
}

pub(crate) struct AnalysisControlFlowGraph {
    unit: BoundUnitId,
    entry: AnalysisBlockId,
    exits: AnalysisExitSet,
    blocks: Box<[AnalysisBlock]>,
    edges: Box<[AnalysisEdge]>,
}
```

Fields and constructors remain checker-private. Numeric IDs are task-local implementation identities and are never persisted,
serialized, placed on symbols, or used for deterministic external ordering.

Blocks contain operations in exact semantic evaluation order. Operations reference exact typed bound nodes, storage accesses,
borrow capabilities, scopes, and control targets from the read-only bound unit view rather than copying their records. Program
points identify the meaningful positions before and after operations so forward and backward analyses use the same topology.

The graph records predecessor and successor relationships directly or through compact derived indexes. It supports deterministic
forward and backward traversal without requiring each analysis to reconstruct reverse edges.

### Edges And Refinements

Edges use a closed typed kind rather than labels or callbacks. Required categories include:

- ordinary sequential control,
- conditional true and false control,
- match-arm selection and no-match control,
- loop entry, back edge, break, and continue,
- normal return and divergent completion,
- result and nullable propagation branches,
- catch, panic, and other language-defined exceptional control,
- async suspension, resumption, cancellation, and task completion where applicable,
- recovery control for malformed but structurally bindable input.

An edge can carry typed refinements established by taking it, including nullable presence, active union variant, pattern success,
predicate facts, and other checked conditions. Refinements reference semantic IDs and checked facts, not source strings or arbitrary
closures.

Control exits are category-specific. A graph distinguishes normal fallthrough, return, propagation, divergence, cancellation, and
other outcomes required by the unit contract instead of collapsing every terminal block into one untyped exit.

### Focused Analysis Domains

The shared graph does not imply one universal data-flow state. Each analysis domain owns its lattice, transfer functions, merge rules,
direction, diagnostics, and durable result projection.

Initial domains include:

- reachability and control completion,
- storage initialization and partial initialization,
- ownership, movement, and partial movement,
- borrowing, aliasing, reborrowing, and mutation authority,
- lifecycle, destruction, finalization, cancellation, and joining obligations,
- nullable, active-variant, pattern, predicate, and other fact refinements,
- dependency-contract propagation,
- liveness needed for borrow shortening and lifecycle decisions.

Storage initialization, ownership, movement, borrowing, mutation authority, and lifecycle obligations are mutually dependent. They
use one composite storage-flow domain where separating them would require circular passes or duplicate state. This is a focused
domain, not a universal container for unrelated analyses.

Reachability can run first and provide a reachable-block and reachable-edge mask to later domains. Refinement results can feed
storage overlap and active-variant checks. Liveness and other naturally backward analyses use predecessor traversal over the same
graph. Dependency-contract propagation consumes the checked storage, capability, witness, and refinement conclusions it requires.

A reusable worklist engine is appropriate for domains that genuinely share fixed-point mechanics. Domain policy remains in concrete
checker modules and typed state. The engine must not force unrelated facts into one optional-field record or erase outcomes behind
untyped maps.

State propagation should use dense typed maps, bit sets, persistent sharing, deltas, or other representations appropriate to each
domain. It must not clone the whole graph or an entire large state for every edge merely to simplify the worklist implementation.

Loops and cyclic control use finite-height or otherwise provably convergent domain rules. Merge operations are deterministic,
monotone, and independent of hash iteration or worker completion order. Failure to converge under a domain's stated contract is a
compiler invariant failure, not a user diagnostic.

### Construction And Publication

Whole-unit analysis follows this boundary:

1. Binding commits all syntax and semantic decisions needed to establish source evaluation order.
2. `bray-bound-tree` provides a read-only bound unit view over the task-local unit without cloning its arenas.
3. `bray-checker` builds one immutable analysis topology from that view.
4. Focused domains run over the shared topology according to their explicit fact dependencies.
5. The checker returns typed conclusions, side tables, and structured diagnostic bags.
6. The binder incorporates those conclusions and publishes the complete checked unit atomically.

Abandoned speculative candidates never contribute nodes or edges to the final graph. Candidate-local checks can use focused temporary
state, but the shared whole-unit graph is constructed only from committed binding state.

Each nested anonymous callable or independently checked declaration-owned expression has its own semantic unit and therefore its own
graph when flow analysis is required. A graph never crosses semantic-unit ownership boundaries. Relationships to nested units use
their typed unit keys rather than embedding the nested graph.

Only durable conclusions promised by the checked-unit contract are copied into bound side tables. Full block-entry and block-exit
states, work lists, predecessor counts, temporary alias sets, and intermediate fixed-point iterations are discarded after checking
unless a separate tooling query explicitly requests a derived control-flow view.

A future tooling control-flow query can publish an immutable source-correlated projection keyed by the checked unit. That projection
is a separate lazy fact with its own stable contract. It does not expose checker-private IDs or make the analysis graph canonical
compiler state.

### Determinism, Parallelism, And Recovery

Blocks, operations, edges, and exits are assigned in deterministic source-semantic order. Worklist scheduling can use another order
for efficiency only when the domain's fixed point and diagnostics remain identical. Diagnostics are structured, task-local to their
domain, and merged by the binder through the ordinary deterministic diagnostic ordering.

Independent semantic units can build and analyze their graphs in parallel. Independent domains over one immutable graph can also run
in parallel once their declared input facts are available, but the implementation should not add coordination overhead for small
units merely to create parallel work.

Graph construction and every fixed-point engine observe compilation cancellation. Cancellation returns the ordinary cancellation
outcome and publishes no graph, analysis state, diagnostics, or partial checked unit.

Malformed bound nodes produce conservative recovery operations and edges. Unknown control, storage overlap, or refinement facts
degrade to typed unknown or error states. Ordinary malformed source must not cause graph construction, transfer, or merge code to
panic or loop forever.

Lowering consumes the published checked HIR and its durable conclusions. It does not consume checker-private block IDs or treat the
analysis topology as normalized execution. Any reusable control-structure helper must preserve this ownership boundary and cannot
make lowering depend on checker algorithms.

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
- checked storage access, ownership, borrow, and movement behavior,
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
- typed unit, node, scope, symbol, type, storage, storage-access, and target IDs,
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

- canonical type, constant value, constant term, and substitution interning,
- concurrent interning returning one canonical ID per structural key,
- checked access rejecting IDs from another semantic store,
- inference variables never appearing in published types or interfaces,
- concrete substitutions rejecting open, unresolved, or error arguments,
- closed constant folding and exact typed value identity,
- open term identity preserving selected operation and evaluation order,
- contextual proofs relating open terms without globally merging canonical IDs,
- numeric semantic ID assignment not affecting diagnostics or serialized ordering,
- storage identity remaining distinct from binding, node, and storage-access identity,
- dynamic access occurrences not becoming equal through structural interning,
- identical, disjoint, and potentially overlapping storage relationships requiring typed checker outcomes,
- borrow-capability identity remaining distinct from program-point activity,
- portable dependency-contract templates instantiating into unit-local bound contracts,
- guarded dependency requirements retaining their semantic conditions,
- moving values transferring their dependency contracts without changing allocation identity,
- query dependencies remaining distinct from language dependency contracts,
- one deterministic checker-internal control-flow topology being shared by focused analysis domains,
- forward and backward analyses observing the same operation and edge order,
- composite storage-flow analysis converging across loops without circular checker passes,
- nested semantic units retaining independent analysis graphs,
- abandoned speculation contributing no analysis blocks or edges,
- checker-private graph IDs remaining absent from published bound facts and package interfaces,
- malformed source producing conservative analysis states without nontermination or panics,
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

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Define canonical type, constant, open-term, substitution, dependency-contract-template, and semantic-store contracts in
   `bray-symbols`.
2. Define the injected binder fact context and binding-dependent symbol-fact provider contracts.
3. Define bound unit kinds, typed IDs, stable keys, origins, and checked accessor behavior.
4. Define storage identities, storage accesses, borrow capabilities, and portable and bound dependency-contract identities.
5. Define immutable per-unit bound storage and category-specific error nodes.
6. Define category-specific checked-unit results and `DiagnosticResult<T>` publication integration.
7. Implement local snapshot and lexical-scope builders against the contracts in `docs/design/symbols.md`.
8. Define binder request contexts, task-local builders, and deterministic query-dependency recording.
9. Implement typed name and path resolution over surface and local symbol APIs.
10. Implement patterns, locals, blocks, and anonymous callable unit boundaries.
11. Add expression, call, member, conversion, and control-flow bound nodes incrementally by grammar category.
12. Define the shared checker-internal control-flow topology, typed edge refinements, and reusable fixed-point mechanics.
13. Integrate focused checker domains and whole-unit analysis finalization.
14. Add deterministic diagnostic aggregation, cancellation, speculation, recovery, convergence, and parallel-query tests.
15. Establish the checked-HIR-to-lowered-bound and lowered-bound-to-`bray-ir` boundaries before implementing production lowering.

Each step must publish only complete immutable facts and must not add temporary eager workflow APIs.
