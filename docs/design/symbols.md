# Symbols design

This document defines how Bray symbols and symbol construction should be implemented.

Symbols belong to `bray-symbols`. They consume declaration surfaces from `bray-declarations` and provide precise semantic
identities and typed symbol relationships to binding, checking, tooling, lowering, and emission.

The compiler architecture overview is defined in `docs/design/compiler-architecture.md`. Declaration discovery is defined in
`docs/design/declaration-discovery.md`. This document is the implementation contract for symbols and symbol-owned lazy facts.

---

## Goals

The symbol model should:

- represent each declared semantic entity with a precise symbol category,
- give every symbol instance a stable typed identity within one compilation,
- expose kind-specific children, members, parameters, and relationships,
- make name lookup and binding operate on semantic identities instead of syntax strings,
- support source, imported, compiler-known, compiler-provided, and synthesized symbols through the same typed contracts,
- compute expensive symbol facts only when requested,
- support deterministic force-completion of one symbol or a symbol-owned subtree,
- keep published semantic values immutable,
- support concurrent queries without changing IDs, diagnostics, or ordering,
- recover from malformed source with error-aware symbols and lookup results,
- preserve enough source correlation for precise diagnostics and tooling.

The design takes inspiration from compilers that model symbols deeply and complete them lazily. Bray should keep that semantic
precision without reproducing an inheritance-heavy object model.

---

## Non-Goals

The symbol layer does not:

- represent syntax-tree structure,
- replace declaration discovery,
- expose one generic record with optional fields for every symbol category,
- bind or check executable callable bodies as part of ordinary symbol completion,
- perform expression flow analysis,
- represent temporary values or storage places as symbols,
- lower semantic behavior into IR,
- assign backend names or emit artifacts,
- make demand order observable through symbol IDs or diagnostics.

---

## Terminology

### Declaration

A declaration is a syntax-backed surface recorded by `bray-declarations`.

A declaration can create a symbol, contribute to a symbol, describe a relationship between symbols, or create no symbol.

Examples:

- multiple module declarations contribute to one logical module symbol,
- an export declaration creates a reachability edge and does not create a new symbol identity,
- a using declaration records an intentional dependency and does not create an alias symbol,
- a field declaration creates a field symbol,
- a binding pattern can create several local binding symbols.

### Symbol

A symbol is the semantic identity of one declared, imported, compiler-known, or synthesized program entity.

A symbol answers questions such as:

- which exact module, type, function, field, parameter, trait, implementation, or local binding is this,
- what semantically contains it,
- where it was declared or synthesized,
- which typed members or parameters it owns,
- which semantic facts describe its declared surface.

A symbol is not a source string, syntax node, type value, storage location, or bound expression.

### Symbol Kind

`SymbolKind` classifies a symbol.

Two symbols can have the same `SymbolKind` and still be different symbols.

For example, two functions both have `SymbolKind::Function`, but each has its own function symbol ID.

### Symbol ID

A symbol ID identifies one exact symbol instance within one compilation.

IDs and kinds are different concepts:

- `SymbolKind` answers "what category is this",
- `SymbolId` answers "which exact symbol is this",
- `FunctionSymbolId` answers "which exact function symbol is this".

Numeric symbol IDs are compilation-local handles. They are not serialized language identities and are not required to remain
numerically equal across different compilation snapshots.

### Symbol Key

A symbol key is a deterministic semantic construction key used to assign or recover a symbol identity.

A key can include:

- package identity,
- logical module path,
- containing symbol identity,
- declaration kind and declaration identity,
- a synthesized role,
- a stable source-order ordinal where the language permits several unnamed entities.

Keys must not include memory addresses, thread IDs, worker completion order, or lazy query order.

Public symbol APIs should normally use typed IDs. Symbol keys are infrastructure for deterministic construction, caches, imported
interfaces, and future incremental remapping.

### Symbol Fact

A symbol fact is immutable semantic information associated with a symbol.

Facts can be eager or lazy. Examples include members, generic parameters, a resolved declared type, a callable signature, generic
constraints, an implementation subject, a trait application, a constant value, or a decoded directive surface.

### Completion

Completion is the act of requesting all facts promised by a defined symbol-completion boundary.

Completion is a query fanout over ordinary lazy fact APIs. It is not a separate mutable compiler workflow.

---

## Phase Boundary

Symbol construction consumes:

- the selected package and product identity,
- the selected target profile,
- compiler-known declaration catalogs,
- imported package symbol surfaces,
- the immutable declaration table,
- the enabled module contributions for the selected product and target.

It publishes:

- a deterministic symbol identity map,
- typed symbol records or handles,
- typed containment and member relationships,
- lookup indexes for symbol-owned declaration spaces,
- lazy symbol-fact queries,
- symbol-construction and symbol-completion diagnostics.

Symbol construction does not consume executable bodies to create the global symbol identity graph.

Binding resolves syntax references to symbols and computes binding-dependent symbol facts through compilation-owned queries. Checking
owns type compatibility, ownership, borrowing, effects, contracts, constant-evaluation validity, and body correctness. A fact can be
presented as a property of a symbol without requiring `bray-symbols` to compute that fact directly.

This separation prevents a crate dependency cycle:

```text
bray-declarations -> bray-symbols <- bray-binder
                             ^
                             |
                     bray-compilation queries
```

`bray-symbols` owns symbol types and fact contracts. `bray-binder` and checker services can compute facts whose implementation
requires binding or checking. `bray-compilation` coordinates and caches those facts.

---

## Identity Construction

### Eager Identity Skeleton

The compilation constructs a cheap deterministic symbol identity skeleton before expensive symbol facts are demanded.

The skeleton establishes:

- package and compiler-known roots,
- logical module identities,
- a deterministic mapping from symbol-producing declarations to typed symbol IDs,
- symbol origins,
- immediate semantic containers,
- stable keys for synthesized declaration-surface symbols.

This work must not require resolved type expressions, checked constraints, evaluated constants, or executable body binding.

Identity assignment cannot happen in arbitrary first-request order. Otherwise two equivalent compilations could assign different IDs
because different IDE requests or worker schedules demanded different symbols first.

### Source Declarations

A source declaration that introduces an independent semantic entity receives its own symbol identity even when the program is
invalid because another declaration conflicts with it.

Invalid duplicate declarations must remain distinguishable so diagnostics and ambiguous lookup can point to every participant.

Declarations merge into one symbol only when the language explicitly defines merging behavior.

Current merging behavior includes logical modules:

- every enabled part of one package-relative module path contributes to one `ModuleSymbol`,
- the module symbol retains all contributing declaration IDs and source locations,
- a module part is not itself a symbol.

Overload declarations do not merge their arms into one arm symbol. An overload family symbol references independently named arm
symbols, and every arm retains its own identity.

### Synthesized Symbols

Synthesized symbols use deterministic keys based on their semantic owner, synthesized role, and stable ordinal where needed.

Examples include:

- an implicit receiver parameter,
- inferred implementation generic parameters,
- anonymous callable symbols,
- contextual postcondition result bindings,
- compiler-generated declaration-surface helpers when the language contract requires them.

Compiler implementation temporaries, lowering blocks, hidden storage places, and backend artifacts are not synthesized symbols.

### Lifetime Of IDs

A typed symbol ID is valid only for the compilation or immutable symbol snapshot that issued it.

Cross-compilation tooling and persisted interfaces must use stable symbol keys or a dedicated serialized identity format rather than
persisting raw numeric IDs.

---

## Origins

Symbol origin is explicit and separate from symbol kind.

The initial origin model should distinguish:

- source symbols from the current package,
- imported symbols loaded from compiled package interfaces,
- compiler-known symbols defined by the language,
- compiler-provided symbols whose implementation is supplied by the compiler,
- synthesized symbols introduced by semantic rules.

A standard-library declaration remains an ordinary source or imported symbol unless the language specification explicitly marks it
compiler-known.

Origin does not create parallel symbol class hierarchies. A source function and an imported function should provide the same
`FunctionSymbol` contract, with origin-specific backing data hidden behind symbol fact providers.

---

## Rust Representation

### Typed IDs

Each symbol category uses a distinct ID type.

Conceptually:

```rust
pub struct SymbolId(u32);
pub struct ModuleSymbolId(SymbolId);
pub struct StructSymbolId(SymbolId);
pub struct FunctionSymbolId(SymbolId);
pub struct ParameterSymbolId(SymbolId);
```

The exact representation can be generated mechanically, but distinct public types must prevent accidental interchange.

Family IDs can exist where a language operation intentionally accepts several specific symbol kinds. Examples can include
`NamedTypeSymbolId`, `CallableSymbolId`, `TypeMemberSymbolId`, or `ValueSymbolId`.

Family IDs must be closed enums or validated wrappers. They must not weaken every API back to `SymbolId`.

### Kind-Specific Records

Each substantial symbol category has a specific record or typed view.

Conceptually:

```rust
pub struct ModuleSymbol { /* module identity */ }
pub struct StructSymbol { /* struct identity */ }
pub struct FunctionSymbol { /* function identity */ }
pub struct CallableParameterSymbol { /* parameter identity */ }
```

Bray must not use a public structure such as:

```text
Symbol {
    kind,
    optional_members,
    optional_parameters,
    optional_type,
    optional_trait,
    optional_body,
    ...
}
```

That shape makes invalid states easy to represent and forces every consumer to rediscover which fields are meaningful.

### Shared Composition

Private components can hold truly common identity data, such as:

- the exact symbol ID,
- origin,
- containing symbol,
- declaration IDs,
- source locations,
- recovery state.

Common composition must not absorb category-specific facts merely to reduce field repetition.

Macros can generate repetitive ID declarations, conversions, common accessors, visitors, and typed table plumbing. They must not
generate or hide language policy.

### Any-Symbol Erasure

An `AnySymbolId` closed enum is useful for:

- diagnostics,
- debugger output,
- typed visitors,
- heterogeneous lookup diagnostics,
- generic containment queries,
- tooling protocols that explicitly request any symbol.

It is an identity-erasing adapter, not canonical symbol storage.

Core APIs must not return `Vec<AnySymbolId>` when a semantic relationship has a more precise type.

### Context-Bound Views

Many useful symbol properties require lazy compilation facts. A context-bound symbol view can pair a typed ID with read-only access
to the compilation fact graph.

Conceptually:

```text
FunctionSymbolView<'compilation>
  id: FunctionSymbolId
  facts: read-only compilation symbol facts
```

Calling `parameters()`, `return_type()`, or `contracts()` can request the corresponding cached fact without placing binder logic in
`bray-symbols`.

The exact Rust API can use views, query methods, or another ownership-safe shape. It must preserve these properties:

- symbol methods appear kind-specific to callers,
- fact computation remains owned by the correct compiler phase,
- no mutable symbol object is exposed,
- the crate dependency graph remains acyclic.

---

## Symbol Categories

The initial model should cover the full language even when implementation proceeds incrementally.

### Roots And Modules

- `PackageSymbol`
- `ModuleSymbol`

A package symbol represents package identity supplied by the package layer. A product is a selected compilation surface and does not
create a namespace, so a product is not a symbol.

A module symbol represents one logical package-relative module path and aggregates all enabled split-module contributions.

Module declarations are package-level and cannot nest. Every module symbol is semantically contained by its package symbol, including
modules with dotted paths such as `net.http`. Path-prefix indexes support module path resolution but do not invent containing module
symbols for undeclared path prefixes.

The compilation root and compiler-known environment can be graph roots without pretending they are source-declared packages.

### Module-Level Symbols

- `ConstantSymbol`
- `FunctionSymbol`
- `PredicateSymbol`
- `CallableContractSymbol`
- `CallableOverloadSymbol`
- `ImplementationOverloadSymbol`
- `StructSymbol`
- `UnionSymbol`
- `TraitSymbol`
- `InherentImplementationSymbol`
- `UnnamedTraitImplementationSymbol`
- `NamedTraitImplementationSymbol`

Using and export declarations do not create symbols.

### Type Symbols And Members

- `StructFieldSymbol`
- `UnionVariantSymbol`
- `UnionPayloadFieldSymbol`
- `TypeCallableMemberSymbol`
- `ConstructorSymbol`
- `FinalizerSymbol`
- `DestructorSymbol`
- `ScopeEnterSymbol`
- `ScopeExitSymbol`
- type-associated `ConstantSymbol`
- type-associated `PredicateSymbol`
- `InherentTypeMemberSymbol`
- type-associated `CallableOverloadSymbol`

Struct and union declarations are named type definition symbols. Their concrete generic applications are type values, not additional
declaration symbols.

An inherent implementation is a symbol even though its members become associated with the implementing type for lookup.

### Trait Symbols And Members

- `TraitCallableMemberSymbol`
- `TraitConstantMemberSymbol`
- `TraitTypeMemberSymbol`
- `TraitPredicateMemberSymbol`
- `TraitFinalizerRequirementSymbol`
- `TraitDestructorRequirementSymbol`
- `TraitScopeEnterRequirementSymbol`
- `TraitScopeExitRequirementSymbol`

Required and defaulted members use the same member-symbol identity. Required/defaulted state is a fact on the specific member kind.

### Implementation Fulfillment Symbols

Declarations inside a trait implementation retain source identities even when they fulfill trait members.

The implementation fulfillment categories are:

- `TraitCallableFulfillmentSymbol`,
- `TraitConstantFulfillmentSymbol`,
- `TraitTypeFulfillmentSymbol`,
- `TraitPredicateFulfillmentSymbol`,
- `TraitScopeEnterFulfillmentSymbol`,
- `TraitScopeExitFulfillmentSymbol`.

The symbol category is selected from the implementation context and the trait member being fulfilled, not only from the syntax kind
used by the member declaration.

Constructors, finalizers, destructors, and callable overload declarations can occur in inherent implementations according to their
ordinary type-associated rules. They cannot become trait implementation fulfillments because the language does not permit those
fulfillment forms.

An implementation member symbol links to the exact trait member it fulfills after implementation binding. It is not replaced by the
trait member symbol.

This distinction is needed for:

- source locations,
- fulfillment diagnostics,
- default-member selection,
- public API analysis,
- implementation-specific constant and type values,
- navigation between a requirement and its fulfillment.

### Parameters

- `GenericTypeParameterSymbol`
- `GenericConstParameterSymbol`
- `CallableParameterSymbol`
- `PredicateParameterSymbol`
- `ReceiverParameterSymbol`

Parameter identity includes its semantic owner and stable ordinal. Parameter names remain facts used by named argument binding and
diagnostics.

The implicit method receiver is represented by a receiver parameter symbol because it has a type, receiver mode, capability rules,
and ownership behavior. It is not part of the written ordinary parameter list.

Inferred implementation parameters are generic parameter symbols with synthesized origin and syntax-correlated inference sources.

### Body-Local Symbols

- `LocalBindingSymbol`
- `LocalConstantSymbol`
- `AnonymousCallableSymbol`
- `PostconditionResultSymbol`
- parameters owned by an anonymous callable,
- pattern-introduced local binding symbols,
- contextual postcondition result bindings where semantic lookup requires identity.

A binding pattern creates one local binding symbol for each binding name, not one symbol for the entire pattern.

Body-local symbols are created deterministically by body binding and belong to body-local symbol tables. They can participate in the
same erased symbol and diagnostic APIs without forcing every local into the global declaration symbol table.

Lambdas receive anonymous callable symbols because they own callable parameters, result and contract facts, an execution scope, and
a body. Their lack of a source-level name does not remove semantic identity.

The contextual type `Self` is a semantic type value tied to a trait or implementation context, not a separately declared named type
symbol. The contextual receiver value `self` is represented by the receiver parameter symbol. The postcondition name `result` is a
compiler-introduced contextual binding, not an ordinary source parameter.

### Compiler-Known And Imported Symbols

Compiler-known scalar types, traits, functions, constants, unions, variants, and other declared surfaces use the same specific symbol
categories as source declarations.

Compiler-provided implementation bodies are not Bray source bodies, but their declaration surfaces still produce ordinary typed
symbols.

Imported package interfaces reconstruct the same public symbol categories and relationships without requiring source syntax.

---

## Values That Are Not Symbols

The following concepts require precise semantic models but are not declaration symbols:

- a constructed generic type,
- a structural type form,
- a trait application,
- a substituted generic callable instance,
- a selected implementation instance or witness,
- an overload candidate after generic substitution,
- an expression value,
- an access path or storage place,
- a temporary,
- a module part,
- a using edge,
- an export edge,
- a directive,
- a bound node,
- an IR value.

These concepts should use separate typed identities where interning or cross-reference is needed:

- `TypeId`,
- `TraitApplicationId`,
- `CallableInstanceId`,
- `ImplementationInstanceId`,
- bound-node and place IDs owned by the bound representation.

A constructed entity references its original definition symbol and ordered arguments. It does not reuse the definition's symbol ID
as though construction had not occurred.

---

## Containment And Relationships

### Containment Tree

Every symbol has at most one semantic containing symbol.

The containment relation is acyclic and forms a tree or forest rooted in package and compiler-known roots.

Semantic references create additional graph edges but do not change containment.

Examples:

- a module is contained by its package,
- a function is contained by its module,
- a field is contained by its struct,
- a callable parameter is contained by its callable,
- an implementation fulfillment is contained by its implementation,
- an overload family references arm symbols but does not contain those independently declared arms,
- an export references a symbol but does not reparent it,
- a trait implementation references a trait application and implementing subject but does not contain either definition.

### No Generic Child List

Canonical storage and public APIs must use typed semantic relationships.

A generic `children()` operation can exist only as a derived visitor or tooling projection. It must not be the source of truth used by
binding or diagnostics.

### Package And Module Relationships

A package exposes logical modules by full package-relative module path.

A module exposes typed collections such as:

- constants,
- functions,
- predicates,
- callable contracts,
- callable overload families,
- implementation overload families,
- structs,
- unions,
- traits,
- inherent implementations,
- named trait implementations,
- unnamed trait implementations,
- using edges,
- export edges.

Using and export edges are typed module facts even though they are not symbols.

### Struct Relationships

A struct exposes:

- generic type parameters,
- generic const parameters,
- fields,
- constructors,
- lifecycle members,
- callable members,
- associated constants,
- associated predicates,
- associated type-valued members,
- callable overload families,
- inherent implementations associated with the struct.

Members declared directly in the type body and members contributed by inherent implementations retain their declaring-symbol and
implementation ownership while participating in the type-associated lookup surface defined by the language.

### Union Relationships

A union exposes:

- generic type parameters,
- generic const parameters,
- variants,
- constructors,
- lifecycle members,
- callable members,
- associated constants,
- associated predicates,
- associated type-valued members,
- callable overload families,
- inherent implementations associated with the union.

A variant exposes ordered payload fields.

### Trait Relationships

A trait exposes:

- generic type parameters,
- generic const parameters,
- callable members,
- constant-valued members,
- type-valued members,
- predicate members,
- lifecycle requirements.

A trait application is a separate semantic value that references one trait symbol and ordered generic arguments.

### Implementation Relationships

An implementation exposes:

- inferred generic type and const parameters,
- its implementing-subject fact,
- its optional implemented trait-application fact,
- static constraints,
- implementation members and fulfillments,
- its coherence-key or coherence-key-set fact,
- its optional implementation-overload-family membership.

An inherent implementation has no implemented trait application.

A named trait implementation has a source-level implementation name. An unnamed implementation remains a symbol but is not
referenced through an implementation name.

### Callable Relationships

A callable symbol exposes the relationships meaningful to its exact kind:

- generic type parameters,
- generic const parameters,
- optional receiver parameter,
- ordered callable parameters,
- result type,
- static constraints,
- callable contract clauses,
- execution mode,
- ABI,
- effects and capabilities,
- trusted obligations,
- optional executable body reference.

Predicate symbols expose predicate parameters and predicate-context facts rather than pretending to be ordinary runtime functions.

Callable contract symbols describe callable surfaces and do not own executable bodies.

---

## Names And Lookup

### Explicit Lookup Namespaces

Lookup indexes must key by both name and lookup namespace.

The symbol layer must not rely on one `Map<String, AnySymbolId>` for all declarations.

The exact `LookupNamespace` variants must follow the language's name-resolution rules. The language already distinguishes at least
value lookup and package or module declaration lookup, and future implementation must finish documenting the complete namespace
partition before lookup APIs are finalized.

### Typed Indexes

Symbol owners should expose typed lookup operations and typed collections.

Examples:

- module lookup by declaration namespace and name,
- type field lookup by field name,
- type-associated callable lookup by name,
- trait member lookup by member category and name,
- callable parameter lookup by name and ordinal,
- implementation fulfillment lookup by the trait member being fulfilled,
- overload family arm lookup by stable arm order.

Indexes are derived facts over canonical typed child collections.

### Lookup Results

Lookup must preserve failure shape instead of returning an arbitrary symbol.

Conceptually, a typed lookup result distinguishes:

- found exactly one symbol,
- not found,
- ambiguous candidates,
- inaccessible candidates,
- malformed or error-recovered candidates.

Ambiguous and inaccessible results retain candidate IDs and locations for diagnostics.

Lookup order and diagnostic order must be deterministic.

### Name And Identity

Names do not define identity by themselves.

Identity also depends on the semantic owner and declaration rules. For example:

- module identity includes package identity and logical module path,
- type member identity includes the declaring type or implementation relationship,
- a named implementation uses its implementation name and owner,
- an unnamed implementation has a declaration symbol and a separately computed exact coherence key,
- parameters use owner plus ordinal even when malformed source duplicates a name.

---

## Building The Symbol Graph

Symbol graph construction proceeds through lazy facts with a deterministic identity prerequisite.

The logical dependency order is:

1. Obtain package, product, dependency, and target-profile facts.
2. Construct the compiler-known symbol environment.
3. Load imported package symbol surfaces.
4. Evaluate module-contribution gates for the selected product and target.
5. Build deterministic source symbol keys and typed IDs from enabled declaration contributions.
6. Publish the identity skeleton and immediate containment relationships.
7. Materialize typed members, lookup indexes, and semantic declaration facts on demand.

This is a dependency order, not a requirement to eagerly complete every step for every symbol.

### Conditional Contributions

Only enabled `@test` and `@target(...)` module contributions participate in source symbol identity, module surface agreement, name
lookup, overload families, implementation coherence, and symbol diagnostics for the selected product.

Gate evaluation is a prerequisite fact for the affected module contribution. It must use the selected product and target facts and
the restricted semantic environment allowed by directive rules.

Disabled declarations remain available through syntax and declaration-discovery APIs but do not produce active source symbols for
that product.

### Cyclic Module References

The package dependency graph is acyclic, but the module declaration and reference graph can be cyclic.

Module identities and named member skeletons must therefore be available before import/export and signature resolution requires the
referenced modules to be complete.

Construction must not recursively force an entire referenced module merely to establish a name edge.

### Imported Symbols

Compiled package interfaces provide immutable declaration-surface facts sufficient to reconstruct public symbols and relationships.

Imported symbols use deterministic identities within the consuming compilation and preserve stable external keys for interface and
tooling references.

Import loading must not require executable dependency bodies.

---

## Lazy Facts

### Logical Immutability

A lazy symbol fact is computed at most once successfully for one compilation fact key and publishes an immutable value.

Internal cache mutation is permitted only to transition from absent to a completed immutable result. It must not change an already
published semantic answer.

Repeated requests return the same semantic value and diagnostics.

### Fact Results

Every fact that can diagnose source owns its diagnostic bag.

Conceptually:

```rust
pub struct SymbolFactResult<T> {
    value: T,
    diagnostics: DiagnosticBag,
}
```

The exact shared result abstraction can be generalized at the compilation query layer. Symbol logic must not append diagnostics into
one global mutable bag according to request timing.

### Fact Keys

A symbol fact key includes the exact symbol identity and exact fact category.

Examples:

- module members,
- module imports,
- decoded directives,
- generic parameters,
- generic constraints,
- callable signature,
- callable contracts,
- constant declared type,
- constant value,
- field type and default,
- union variant payload,
- implementation subject,
- implemented trait application,
- implementation coherence keys,
- overload family arms.

Fact categories should be explicit enums or typed query functions. String keys are not acceptable.

### Common Fact Groups

The implementation can group related work when one computation naturally produces several inseparable facts. It should not compute
unrelated expensive facts merely because they belong to the same symbol.

For example, constructing ordered callable parameters and the parameter-name index can be one fact. Checking an executable function
body is not part of that fact.

### Binding-Dependent Facts

Some symbol properties require name binding or checking:

- declared parameter and result types,
- generic constraint meanings,
- implementation subjects and trait applications,
- overload arm targets,
- constant values,
- checked default values,
- decoded contract meanings.

These remain symbol-facing facts but are computed by the compiler phase that owns the semantic operation. The compilation query graph
bridges the symbol API to the binder or checker without introducing a crate cycle.

---

## Completion Model

### Completion Levels

Completion has explicit semantic levels.

#### Identity Completion

Identity completion guarantees:

- typed symbol ID,
- symbol kind,
- origin,
- containing symbol,
- declaration and location anchors,
- recovery or error state.

Identity completion does not bind types or build all members.

#### Declaration-Surface Completion

Declaration-surface completion guarantees all facts needed to describe and use the declared semantic surface, including the facts
applicable to that symbol kind:

- typed child symbols,
- member and parameter indexes,
- visibility and modifiers,
- decoded directives,
- generic parameters and constraints,
- parameter and result types,
- receiver mode,
- callable contracts, ABI, effects, and capabilities,
- implementation subject and trait application,
- overload family membership and arms,
- constant values,
- checked declaration-level defaults,
- checked predicate bodies and other non-executable contract expressions,
- declaration-surface diagnostics.

This level can invoke binder and checker facts through the compilation query graph.

#### Body Completion

Executable body binding and checking are not symbol completion.

Body completion belongs to checked bound-body facts owned by binding and checker services.

This includes function, method, constructor, lifecycle, lambda, and default executable trait-member bodies.

A symbol can be declaration-surface complete while its executable body has never been requested.

### Force Complete

`force_complete(symbol)` requests declaration-surface completion for the symbol and every symbol it semantically contains.

It traverses typed containment relationships in deterministic order.

It does not recursively complete symbols that are only referenced.

Examples:

- completing a function completes its generic parameters, receiver, and ordinary parameters,
- completing a struct completes its fields and type-owned members,
- completing an implementation completes its own parameters and fulfillment members but does not recursively complete the trait it
  references,
- completing an overload family resolves and validates its arm references but does not reparent those arms,
- completing a package completes all active modules and their contained source symbols,
- completing a callable symbol does not bind its executable body.

Force completion is idempotent. It returns or exposes diagnostics through the same cached fact results used by ordinary requests.

Compilation-wide symbol diagnostics are obtained by forcing the selected package symbol roots to declaration-surface completion and
deterministically merging the diagnostics of all requested symbol facts.

### Partial Completion

Ordinary callers should request only the fact they need.

Examples:

- name lookup can request module member names without binding every signature,
- completion can request a function's parameter names without checking contracts,
- hover can request one symbol signature,
- an implementation query can request one coherence key,
- emission can force all required declaration surfaces and then request checked bodies.

No API should pretend a partial result satisfies a stronger completion contract.

---

## Cycles And Reentrancy

Lazy symbol facts can form dependency cycles.

Examples include:

- mutually referring declarations,
- recursive types,
- generic constraint cycles,
- constant initializer cycles,
- implementation and trait relationships,
- import/export cycles inside a package.

Plain nested one-time initialization is insufficient when a query can recursively request itself. It can deadlock, recurse forever,
or publish an incomplete value.

The fact engine must track:

- the current fact key,
- its evaluation state,
- the dependency stack for the current request,
- cycle recovery policy for that fact category,
- waiting relationships between workers.

A same-request cycle must be detected before waiting.

Cross-worker dependencies must not deadlock. The implementation must not hold a fact lock while recursively computing dependencies.

Cycle policy is fact-specific:

- legal recursive type references preserve the recursive symbol or type edge,
- illegal constant cycles produce structured diagnostics and an error constant value,
- invalid constraint cycles produce structured diagnostics and error facts,
- module reference cycles remain legal when the language permits them,
- compiler invariant cycles fail loudly rather than being reported as user errors.

Canceled or aborted computations must not publish a fact as complete and must not leak diagnostics into completed results.

---

## Concurrency And Determinism

Published symbol records, fact values, child collections, and indexes are immutable and shareable.

The symbol graph and public symbol views must be safe for concurrent read-only use.

Independent symbol facts can be evaluated in parallel when their dependencies are available.

Observable behavior must not depend on worker scheduling:

- symbol IDs come from deterministic keys and source order,
- typed child lists use stable semantic or source order,
- lookup candidate order is stable,
- diagnostic IDs and ordering are stable,
- force completion produces the same facts and diagnostics as requesting the same facts individually,
- serial and parallel execution produce equivalent published symbol graphs.

Thread-safe caches are implementation state. They must not become hidden semantic state.

---

## Diagnostics

### Ownership

Symbol construction owns diagnostics for violations that require semantic identity or symbol-table construction but do not require
executable body checking.

Examples include:

- conflicts with compiler-known declarations,
- duplicate semantic identities not fully decidable during declaration discovery,
- invalid partial-symbol surface combinations,
- duplicate or inconsistent synthesized symbol identities,
- malformed imported symbol surfaces,
- invalid symbol directive targets or combinations when their meaning is symbol-owned,
- symbol graph cycles that are illegal for the affected fact category.

Binding owns name and path resolution diagnostics, including unresolved or ambiguous references used by a symbol-facing fact.
Checker services own overload validity and selection, implementation coherence and fulfillment validity, type compatibility,
ownership, borrowing, contracts, effects, constant evaluation, target availability, and body-validity diagnostics.

A diagnostic remains owned by the phase that performs the semantic operation even when its immutable result is cached and exposed as
a property of a symbol.

### Structure

Symbol diagnostics use `bray-diagnostics` kinds, typed arguments, labels, notes, and source spans.

Compiler logic must not construct user-facing English. Rendering goes through `bray-messages`.

Diagnostics should carry typed symbol IDs only when the diagnostic contract remains valid for the lifetime of the consuming symbol
snapshot. Persisted or external output uses rendered names, stable keys, and source locations according to its contract.

### Lazy Collection

Each fact result owns the diagnostics produced while computing that fact.

Compilation diagnostic queries request the required completion boundary and merge fact diagnostics deterministically.

Repeated fact requests do not duplicate diagnostics.

Abandoned speculative computations do not publish semantic diagnostics. Diagnostics become observable only with a successfully
published fact result.

---

## Recovery And Error Symbols

Malformed source must not crash symbol construction.

The symbol layer preserves recoverable declarations when doing so gives later phases a stable semantic anchor.

A recovered symbol records:

- its exact symbol kind when known,
- its declarations and source locations,
- which identity or surface facts are missing or erroneous,
- diagnostics already owned by earlier phases,
- error facts needed to continue lookup and binding.

Missing names do not become ordinary empty-string names.

Conflicting declarations are not silently collapsed. Lookup returns ambiguity or a typed conflict result containing the relevant
symbols.

Error modeling should remain category-specific:

- an error type participates as a type result,
- an error callable preserves callable shape where recoverable,
- an error constant preserves declared type information where recoverable,
- an error implementation preserves subject or trait information where recoverable.

One universal error symbol must not erase the semantic category needed for recovery.

Ordinary user source must never cause symbol construction to panic. Panics are reserved for violated compiler invariants such as an
invalid internal ID or impossible symbol-kind/table mismatch.

---

## Binding Integration

Binding should receive typed symbol APIs designed around its actual questions.

Examples include:

- resolve an unqualified value name in a lexical and module context,
- resolve a type path,
- resolve a trait path and construct a trait application,
- obtain callable candidates from a callable overload family,
- obtain type-associated members for a receiver type,
- find participating implementations for an exact coherence key,
- map an implementation fulfillment to its trait member,
- retrieve parameter names, modes, and types for argument binding,
- retrieve constant and predicate symbols valid in a restricted context.

Binding must not reconstruct symbol ownership or declaration groups by walking syntax.

Symbol APIs should return typed facts and typed lookup results rather than loosely structured maps or strings.

The binder can create body-local symbols while constructing a bound body. Those local symbols use the same identity and diagnostic
principles but remain owned by the body-local semantic snapshot.

---

## Type And Application Separation

Named type definition symbols and semantic types are separate models.

Examples:

```text
StructSymbolId(Pair)
TypeId(Pair<i32, string>)
TypeId(Pair<string, i32>)
```

The two `TypeId` values reference the same definition symbol with different ordered arguments.

The same separation applies to:

- trait symbols and trait applications,
- generic function symbols and substituted callable instances,
- implementation declaration symbols and selected implementation instances or witnesses,
- overload family symbols and selected overload candidates.

This prevents definition identity, generic substitution, and use-site selection from being conflated.

Original-definition queries are explicit on application values. They must not rely on stripping information from a reused symbol ID.

---

## Incrementality And Sharing

The first implementation does not need a complete incremental compiler, but symbol contracts must not prevent one.

Requirements:

- symbol inputs and published outputs are immutable,
- facts have explicit typed keys and dependencies,
- raw pointers and process-global mutable state are not identities,
- imported and compiler-known symbols can be shared when their inputs are unchanged,
- compilation-local caches can be discarded without changing semantics,
- a future compilation snapshot can reuse symbol records or fact results only when their dependency keys remain valid.

Numeric symbol IDs need not survive edits. Stable keys and explicit remapping support tooling or cache reuse when needed.

---

## Public API Direction

The public cross-crate API should favor:

- typed IDs,
- immutable typed records or context-bound views,
- typed child collections,
- typed lookup results,
- explicit fact queries,
- explicit completion contracts,
- source-correlated diagnostics.

The public API should avoid:

- trait-object inheritance as canonical symbol storage,
- downcasting from a universal symbol object,
- public green or syntax storage internals,
- one untyped child list,
- optional fields that are meaningful only for unrelated kinds,
- callers mutating symbol completion state,
- callers manually sequencing phase workflows,
- eager whole-program completion as a prerequisite for ordinary queries.

Crate-private traits can express genuinely shared behavior when static dispatch or testability benefits. They must not recreate an
inheritance hierarchy whose primary purpose is field reuse.

---

## Testing

Symbol tests should validate semantic contracts rather than cache implementation details.

Required coverage includes:

- kind and exact-ID distinction,
- deterministic IDs under different request orders,
- deterministic IDs under serial and parallel construction,
- multiple module parts producing one module symbol,
- disabled module contributions producing no active symbols,
- conflicting declarations remaining distinguishable,
- source, imported, and compiler-known symbols exposing equivalent typed APIs,
- typed module, type, trait, implementation, callable, and variant relationships,
- parameter owner and ordinal identity,
- receiver parameter synthesis,
- inferred implementation parameter synthesis,
- anonymous callable and pattern-binding local symbols,
- overload family symbols retaining independent arm identities,
- trait implementation fulfillments linking to exact trait members,
- type definitions remaining distinct from constructed type IDs,
- lazy facts not being computed before demand,
- repeated requests returning equivalent cached facts without duplicate diagnostics,
- force completion matching the union of ordinary fact requests,
- force completion excluding executable body binding,
- legal recursive references completing without deadlock,
- illegal constant and constraint cycles producing deterministic diagnostics,
- malformed declarations producing error-aware symbols without panics,
- cancellation not publishing partial facts,
- stable diagnostics under different worker schedules.

Integration tests should verify that `Compilation` exposes symbol roots and diagnostics as lazy facts derived from declaration tables.

---

## Decisions Requiring Follow-Up

The following language or API details need to be settled before the corresponding implementation surface is finalized.

### Complete Lookup Namespace Taxonomy

The language documents refer to value lookup, declaration namespaces, and module namespaces, but do not yet define one exhaustive
lookup-namespace taxonomy.

Before implementing general lookup indexes, Bray needs an explicit list of lookup namespaces and the declaration categories that
occupy each namespace.

### Type-Associated Member Aggregation

The language states that inherent implementation members become associated with the implementing type. The exact conflict and
ordering rules between directly declared type members and members from one or more inherent implementations should be written as a
single lookup contract.

### Declaration-Level Defaults

This design treats constant values, parameter defaults, field defaults, and contract-level predicate bodies as declaration-surface
completion because they affect the usable declared surface and can produce declaration diagnostics.

Executable callable and lifecycle bodies remain separate checked-body facts. This boundary should be confirmed against the planned
constant evaluator and checker APIs.

### Body-Local Symbol Storage

This design keeps body-local symbols in immutable body-local tables while allowing them to participate in shared erased-symbol and
diagnostic APIs. The bound-tree design should confirm whether local IDs wrap global `SymbolId` values or use a separate typed local
identity space.

### Compiler-Known Root Shape

Compiler-known declarations need normal typed symbol APIs without pretending they belong to a source package dependency. The exact
root representation should be finalized with package and imported-interface design.

---

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Define symbol kinds, typed IDs, origins, keys, and common immutable identity data.
2. Define the symbol graph, package roots, module symbols, and deterministic source declaration-to-symbol identity mapping.
3. Define typed module member collections and lookup-result primitives.
4. Define the compilation-owned lazy fact and completion protocol with cycle and concurrency contracts.
5. Add named type, trait, implementation, overload, member, and parameter symbol records.
6. Add compiler-known and imported symbol providers through the same typed contracts.
7. Add binding-dependent signature, constraint, contract, implementation, and constant fact queries.
8. Add body-local and anonymous callable symbols with bound-body integration.
9. Add recursive force completion and deterministic symbol diagnostics.

Each step should preserve lazy evaluation and avoid temporary eager APIs that callers would later depend on.
