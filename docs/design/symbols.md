# Symbols design

This document defines how Bray symbols and symbol construction should be implemented.

Symbols belong to `bray-symbols`. They consume declaration surfaces from `bray-declarations` and provide precise semantic
identities and typed symbol relationships to binding, checking, tooling, lowering, and emission.

The compiler architecture overview is defined in `docs/design/compiler-architecture.md`. Declaration discovery is defined in
`docs/design/declaration-discovery.md`. The compiler-known catalog is defined in
`docs/design/compiler-known-catalog.md`. Compiled package interfaces are defined in
`docs/design/compiled-package-interfaces.md`. This document is the implementation contract for symbols and symbol-owned lazy facts.

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
- represent temporary values, storage identities, or storage accesses as symbols,
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
- the compiler-known declaration catalog,
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

Compiler implementation temporaries, lowering blocks, hidden storage, and backend artifacts are not synthesized symbols.

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

- `CompilerKnownEnvironmentSymbol`
- `PackageSymbol`
- `ModuleSymbol`

A compiler-known environment symbol is the unnamed semantic owner of ambient compiler-known declarations and compiler-known module
symbols. It has a dedicated `CompilerKnownEnvironmentSymbolId` and `SymbolKind::CompilerKnownEnvironment`. It is not a source or
imported package, does not occupy the ordinary lookup namespace, and has no visibility or source declaration location.

A package symbol represents package identity supplied by the package layer. A product is a selected compilation surface and does not
create a declaration container or lookup scope, so a product is not a symbol.

A module symbol represents one logical module path. A source or imported module aggregates enabled package module contributions. A
compiler-known module is supplied by the compiler-known catalog.

Module declarations are package-level and cannot nest. Source and imported module symbols are semantically contained by their
package symbol. Compiler-known module symbols such as `core.memory` and `target` are semantically contained by the compiler-known
environment symbol. Modules with dotted paths remain directly contained by their semantic owner. Path-prefix indexes support module
path resolution but do not invent containing module symbols for undeclared path prefixes.

The compilation does not create a `CompilationRootSymbol`. An immutable symbol graph owns a forest of package roots plus exactly one
compiler-known environment root. APIs that intentionally accept either root use a closed family:

```rust
pub enum SymbolRootId {
    Package(PackageSymbolId),
    CompilerKnown(CompilerKnownEnvironmentSymbolId),
}
```

`SymbolRootId` is a typed family ID, not a generic root-symbol record. Package and compiler-known environment symbols retain their
own kind-specific storage and APIs.

Conceptually, the graph publishes its roots through:

```rust
pub struct SymbolGraphRoots {
    compiler_known: CompilerKnownEnvironmentSymbolId,
    packages: Arc<[PackageSymbolId]>,
}
```

The process-wide compiler-known catalog is not a symbol and has no `SymbolId`. Each compilation or immutable symbol snapshot maps it
to exactly one compilation-local compiler-known environment symbol.

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

Receiver signature facts use the shared, mutable, consuming, and consuming-mutable modes. Constructors and scope exits have no
receiver. Finalizers use mutable receivers, destructors use consuming-mutable receivers, and scope enter declarations use the mode
selected by their receiver modifiers.

Inferred implementation parameters are generic parameter symbols with synthesized origin and syntax-correlated inference sources.

### Runtime Default Provider Symbols

- `CallableParameterDefaultProviderSymbol`
- `StructFieldDefaultProviderSymbol`
- `UnionPayloadDefaultProviderSymbol`

A runtime default provider is a synthesized declaration-surface callable used to preserve runtime default behavior across package
boundaries. It has a typed symbol ID and deterministic synthesized key derived from the exact parameter, struct field, or union
payload field that owns the default.

A parameter provider is semantically contained by the callable that owns the parameter and references that parameter. A struct field
provider is contained by the struct and references its field. A union payload provider is contained by the union variant and
references its payload field.

Providers do not have source-level names, do not enter ordinary lookup, and do not appear in typed member collections. The owning
parameter or field exposes the provider through its checked default fact. Any-symbol erasure can include providers for diagnostics,
debugging, interface serialization, and tooling that explicitly requests synthesized symbols.

The provider body is the already checked declaration-owned default expression. It is not checked again as an ordinary callable body.
Lowering and emission request the provider lazily only when a reachable call or construction can use the default.

### Body-Local Symbols

- `LocalBindingSymbol`
- `LocalConstantSymbol`
- `AnonymousCallableSymbol`
- `AnonymousCallableParameterSymbol`
- `PostconditionResultSymbol`
- pattern-introduced local binding symbols,
- contextual postcondition result bindings where semantic lookup requires identity.

A binding pattern creates one local binding symbol for each logical binding name, not one symbol for the entire pattern. Occurrences of
the same coherent binding across alternative patterns contribute syntax anchors to one symbol rather than creating competing locals.

Body-local and contextual local symbols are created deterministically by binding and belong to immutable local semantic-region
snapshots. They use a separate typed local identity space rather than consuming compilation-wide declaration `SymbolId` values.
They can still participate in erased symbol and diagnostic APIs.

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

`Future<T>`, `Task<T>`, `RunResult<T>`, `PanicReport`, `blocking_execution()`, `compute_execution()`,
`main_thread_execution()`, and the inherent `start`, `join`, and `cancel` members use ordinary category-specific symbols backed by
closed compiler-known roles. Their semantic phases select them by role identity, never spelling. Hidden async frame identities are
semantic values or lowering identities, not declaration symbols.

Compiler-known surfaces are supplied by the immutable descriptor catalog defined in
`docs/design/compiler-known-catalog.md`. Stable catalog keys identify language-defined entries across compilations. Symbol
construction maps those keys to compilation-local typed symbol IDs and exposes catalog-backed facts through the same kind-specific
contracts used by source and imported symbols.

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
- a storage identity or storage access,
- a temporary,
- a module part,
- a using edge,
- an export edge,
- a directive,
- a bound node,
- an IR value.

These concepts should use separate typed identities where interning or cross-reference is needed:

- `TypeId`,
- `ConstantValueId`,
- `ConstantTermId`,
- `GenericSubstitutionId`,
- `ConcreteGenericSubstitutionId`,
- `DependencyContractTemplateId`,
- `TraitApplicationId`,
- `CallableInstanceId`,
- `ImplementationInstanceId`,
- bound-node, storage-identity, and storage-access IDs owned by the bound representation.

A constructed entity references its original definition symbol and ordered arguments. It does not reuse the definition's symbol ID
as though construction had not occurred.

`bray-symbols` owns canonical semantic types, constant values, open constant terms, substitutions, portable dependency-contract
templates, and their interner APIs because they directly compose from typed symbol IDs and are returned by symbol facts. They remain
separate semantic categories and do not become symbols merely because the symbol crate owns their dependency-safe representation.
The full contract is defined in `docs/design/binder.md`.

---

## Containment And Relationships

### Containment Tree

Every symbol has at most one semantic containing symbol.

The containment relation is acyclic and forms a tree or forest rooted in package and compiler-known roots.

Semantic references create additional graph edges but do not change containment.

Examples:

- a source or imported module is contained by its package,
- a compiler-known module is contained by the compiler-known environment,
- an ambient compiler-known declaration is contained directly by the compiler-known environment,
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

A module's semantic owner is represented by a closed family rather than an untyped symbol ID:

```rust
pub enum ModuleOwnerId {
    Package(PackageSymbolId),
    CompilerKnownEnvironment(CompilerKnownEnvironmentSymbolId),
}
```

Only module symbols supplied by the compiler-known catalog can use the compiler-known environment owner. Ordinary source and
imported modules always use a package owner.

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

### Compiler-Known Environment Relationships

The compiler-known environment exposes:

- typed ambient declaration collections,
- ordinary-name lookup over those ambient declarations,
- compiler-known modules by full module path,
- stable catalog-key-to-symbol-ID indexes,
- typed accessors for compiler-known roles needed by semantic phases.

The compiler-known symbol provider must publish one immutable compilation-local role registry derived from the generated catalog
role indexes. It must map representation roles and implementation hooks to exact symbol IDs, retain special-value targets as typed
catalog value IDs, and support reverse symbol-to-role classification. The registry must never infer roles from symbol names or
stable-key strings.

Target-available views must apply their existing availability result to both forward and reverse role queries for symbols and
special values. The complete provider and its registry remain unchanged.

Its ambient collections remain category-specific. They must not use a generic canonical child list merely because several symbol
kinds are ambient.

Declaration categories that can be owned either by a module or directly by the compiler-known environment use a closed owner family
specific to that relationship. Conceptually:

```rust
pub enum ModuleLevelOwnerId {
    Module(ModuleSymbolId),
    CompilerKnownEnvironment(CompilerKnownEnvironmentSymbolId),
}
```

An owner family should exist only where both cases are semantically legal. It must not become a replacement for precise
kind-specific owner types throughout the symbol model.

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

### Type-Associated Surface Aggregation

Every named type definition has one lazy immutable type-associated surface fact. The fact aggregates:

- representation and behavior members declared directly in the type body,
- members contributed by enabled inherent implementations associated with the type,
- callable overload families from either declaration location,
- typed lifecycle slots,
- the inherent implementation symbols that contributed members.

A source type's defining package is its semantic owner. A compiler-known type is owned by the compiler-known environment. Only the
semantic owner can contribute inherent implementations unless a language rule explicitly defines another owner. All enabled inherent
implementations from the owning package's selected source graph contribute automatically, regardless of their source module. A
`using` declaration or export does not activate or deactivate an inherent implementation.

Trait implementation fulfillments are not added to the type-associated surface. Trait method and member resolution query exact
participating trait implementations separately. Union payload fields remain in their variant payload scope rather than the
union-wide surface.

Aggregation stores existing typed symbol IDs. It does not clone or reparent symbols. A direct member remains contained by its type,
an inherent member remains contained by its inherent implementation, and both expose the associated type-definition ID. This keeps
source provenance and implementation ownership available to diagnostics, navigation, and public API analysis.

The aggregate ordinary-name index enforces one name across direct members and every contributing inherent implementation. Member
kind, source location, visibility, and generic constraints do not partition that index. A direct declaration has no precedence over
an inherent declaration, and one inherent implementation has no precedence over another. An explicit callable overload family is
the only mechanism that places separately named callables behind a shared call name.

Primary construction, finalization, destruction, scope entry, and scope exit use typed lifecycle slots rather than the ordinary-name
index. One declaration template can occupy each slot for a type definition. Named constructors occupy ordinary names. Generic
constraints do not create alternative declarations for the same ordinary name or lifecycle slot.

Aggregation occurs at the type-definition level. Generic inherent members remain declaration templates in that fact. A constructed
`TypeId` requests an applicable view that substitutes the type arguments, matches the implementation subject, and proves the
implementation constraints. A member with unproved constraints is not applicable, but its declaration still reserves its ordinary
name or lifecycle slot in the definition-level surface. A generic checking context can use the member only when its available static
facts prove those constraints.

The full surface retains inaccessible and inapplicable entries so lookup can distinguish not found, wrong semantic category,
inaccessible, unsatisfied constraints, malformed, and conflicting declarations. Effective reachability is capped by the associated
type, the member's declaring module, and the member's own visibility. An inherent implementation does not create another visibility
or activation boundary.

Stable enumeration lists direct members in source order, followed by inherent implementations in canonical declaration-table order
and each implementation's members in source order. This order is observable only for deterministic metadata, diagnostics, tooling,
and tests. It is never lookup precedence. A conflict retains every candidate and emits diagnostics in canonical order rather than
selecting the first declaration.

The type-associated surface fact owns aggregation diagnostics. Successful publication caches the immutable surface and its
diagnostic bag together so concurrent requests cannot observe a member index without the diagnostics produced while building it.

If an inherent implementation subject cannot be resolved to an owned named type definition, the implementation remains an
error-aware symbol with its own diagnostics and is not attached to an arbitrary type surface.

Public APIs over this fact remain kind-specific. Types expose typed field, variant, callable, constructor, lifecycle, constant,
predicate, type-valued-member, overload-family, and inherent-implementation collections rather than a canonical generic child list.

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

The implementation-coherence fact publishes a typed key containing the checked subject type and the optional checked trait
application. It must force those prerequisite facts and must not report success after merely requesting them. Candidate aggregation
and conflict diagnostics consume this key through checker-owned coherence queries.

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

Body-bearing symbol views expose cheap body presence and lazy category-specific checked-body access according to
`docs/design/binder.md`. Symbol completion records the body relationship but does not force that checked-body fact.

---

## Names And Lookup

### Ordinary Lookup Namespace

Bray currently has one general identifier lookup namespace: the ordinary lookup namespace.

The namespace determines whether the same spelling may identify more than one entity in the same lookup scope. Symbol kind and
lookup context do not create additional namespaces. Types, values, traits, predicates, callable contracts, named implementations,
members, generic parameters, callable parameters, local bindings, and other named declarations therefore compete for the same
ordinary name in a scope.

For example, a type and constant cannot share a name in one module, a generic type parameter and generic const parameter cannot
share a name on one declaration, and a field and callable member cannot share a name on one type. An explicit overload family
occupies its ordinary name once. Its arms retain separate identities without introducing the family name again.

Packages and modules require specialized path indexes, but they are not additional lookup namespaces. A visible package identity,
module path component, or declaration that can occupy the same path position participates in the same ordinary name surface. A
module remains a declaration container and lookup provider rather than a separate name-collision partition.

The implementation need not introduce a one-variant `LookupNamespace` enum. Such an enum should be added only if the language later
introduces a name category, such as labels, whose spelling may legally coexist with an ordinary name in the same scope.

Ambient compiler-known visibility is an explicit lookup relationship, not semantic containment. A source module remains contained
by its package while its lookup context consults the compiler-known environment's ambient index. The compiler-known environment is
not imported into, cloned into, or made the semantic parent of each source module.

Ambient declarations participate in the module's effective ordinary-name surface according to the language's collision and lookup
rules. The environment symbol itself has no ordinary name and is never returned as an ordinary lookup candidate.

The symbol layer must not interpret the ordinary namespace as permission to use one global `Map<String, AnySymbolId>`. Lookup remains
owner-specific and typed. The namespace defines collision behavior, while the owner and lookup operation define which index is
queried and which result kinds are valid.

The following relationships are not ordinary name lookup:

- unnamed implementation selection by coherence key,
- overload-arm selection by family and signature,
- implementation fulfillment lookup by trait member identity,
- lifecycle member selection through typed owner slots,
- tuple element selection by ordinal,
- contextual `Self`, `self`, and `result` bindings,
- directives and `using` declarations, which introduce no ordinary names.

An export does not create a symbol or introduce an unqualified name inside the exporting module. It projects the target symbol's
ordinary name into the module's exported lookup surface, where that name participates in ordinary collision checking.

### Typed Indexes

Symbol owners should expose typed lookup operations and typed collections.

Examples:

- module declaration lookup by ordinary name,
- type field lookup by field name,
- type-associated callable lookup by name,
- trait member lookup by ordinary name followed by typed classification,
- callable parameter lookup by name and ordinal,
- implementation fulfillment lookup by the trait member being fulfilled,
- overload family arm lookup by stable arm order.

Indexes are derived facts over canonical typed child collections.

A context-specific lookup such as type lookup or value lookup first resolves the ordinary name and then validates the resolved
entity's semantic category. Finding an entity of the wrong category must remain distinguishable from not finding the name. Typed
entry points should expose that distinction without creating parallel type and value namespaces.

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

The complete artifact, stable-key, identity-skeleton, lazy-decoding, target-compatibility, and checked-template contracts are defined
in `docs/design/compiled-package-interfaces.md`.

`bray-symbols` owns imported identity-surface input values and imported fact keys, but it does not depend on the package-interface
codec. `bray-package-interface` depends on symbol contracts and translates validated wire records into those inputs.

---

## Lazy Facts

### Logical Immutability

A lazy symbol fact is computed at most once successfully for one compilation fact key and publishes an immutable value.

Internal cache mutation is permitted only to transition from absent to a completed immutable result. It must not change an already
published semantic answer.

Repeated requests return the same semantic value and diagnostics.

### Fact Results

Every fact that can diagnose source owns its diagnostic bag.

Lazy symbol facts use the shared `DiagnosticResult<T>` contract defined in
[Compiler diagnostics](compiler-diagnostics.md#diagnostic-results). Compilation owns lazy caching and publication around it. Symbol
logic must not append diagnostics into one global mutable bag according to request timing.

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
- callable-contract declared type,
- constant declared type,
- constant definition template,
- constant instance value,
- callable parameter default,
- struct field type and default,
- implementation type-member value,
- union payload field type and default,
- predicate definition,
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
- checked constant definition templates and instance values,
- checked parameter, struct field, and union payload defaults,
- checked predicate definitions,
- decoded contract meanings.

These remain symbol-facing facts but are computed by the compiler phase that owns the semantic operation. The compilation query graph
bridges the symbol API to the binder or checker without introducing a crate cycle.

### Declaration-Owned Expression Facts

Declaration-owned expressions are symbol-facing semantic facts even when their full checked representation belongs to binding and
checking. They include runtime defaults, constant definition templates, predicate definitions, constraints, and contract clauses.

The completion boundary is based on semantic ownership rather than syntax shape:

| Expression category         | Declaration-surface result                     | Deferred operation                          |
|-----------------------------|------------------------------------------------|---------------------------------------------|
| parameter default           | checked parameter-default surface and provider | runtime evaluation when omitted             |
| struct field default        | checked field-default surface and provider     | runtime evaluation when omitted             |
| union payload default       | checked payload-default surface and provider   | runtime evaluation when omitted             |
| constant initializer        | checked constant definition template           | concrete constant-instance evaluation       |
| trait constant default      | checked selected-value template                | evaluation after implementation selection   |
| predicate body              | checked semantic predicate definition          | application or proof for concrete arguments |
| constraints and contracts   | checked semantic predicate facts               | use during checking and inference           |
| default trait callable body | body-presence fact only                        | ordinary executable-body checking           |

#### Crate Ownership

`bray-symbols` owns:

- typed owner and provider symbol IDs,
- default-presence and predicate-definition state enums,
- immutable symbol-facing semantic summary contracts,
- typed context-bound view methods,
- completion requirements for each symbol kind.

`bray-compilation` owns exact fact-key composition, thread-safe caches, dependency scheduling, cancellation, and publication of
immutable fact results. Typed domain key components remain owned by the lower representation that defines their meaning so those
representations do not depend back on compilation.

`bray-binder` and checker services bind and validate declaration-owned expressions. `bray-bound-tree` owns their full checked
source-shaped representation. A symbol record or `bray-symbols` summary must not store or depend on a bound-tree node ID because the
bound tree already depends on symbols.

Lowering consumes the binder-owned checked HIR when it materializes a reachable runtime default provider. Symbol-facing summaries
are not a second executable representation.

#### Runtime Default API

Default presence is cheap identity-level information derived from the declaration syntax. It must distinguish absence from written
or recovered default syntax without forcing semantic checking.

Conceptually:

```rust
pub enum RuntimeDefaultPresence {
    Absent,
    Present,
    Recovered,
}
```

Context-bound symbol views expose kind-specific methods:

```rust
impl CallableParameterSymbolView<'_> {
    pub fn default_presence(&self) -> RuntimeDefaultPresence;

    pub fn default(
        &self,
    ) -> Option<Arc<DiagnosticResult<CheckedCallableParameterDefault>>>;
}

impl StructFieldSymbolView<'_> {
    pub fn default_presence(&self) -> RuntimeDefaultPresence;

    pub fn default(
        &self,
    ) -> Option<Arc<DiagnosticResult<CheckedStructFieldDefault>>>;
}

impl UnionPayloadFieldSymbolView<'_> {
    pub fn default_presence(&self) -> RuntimeDefaultPresence;

    pub fn default(
        &self,
    ) -> Option<Arc<DiagnosticResult<CheckedUnionPayloadDefault>>>;
}
```

The exact implementation can avoid `Arc` in the public signature when a borrowed immutable result has a sufficient lifetime. It must
not return an untyped `CheckedDefault` that forces callers to inspect the owner kind.

`default()` returns `None` exactly when `default_presence()` is `Absent` and must not create or request a default fact in that case.
Both `Present` and `Recovered` return `Some(...)`. Recovered or semantically invalid defaults publish their diagnostics and an
error-aware value through the same owner-specific fact contract.

Each checked result exposes its exact provider ID and an error-aware typed value. Conceptually:

```rust
pub struct CheckedCallableParameterDefault {
    provider: CallableParameterDefaultProviderSymbolId,
    value: CallableParameterDefaultValue,
}

pub enum CallableParameterDefaultValue {
    Valid(CallableParameterDefaultSurface),
    Error(ErrorCallableParameterDefault),
}

impl CheckedCallableParameterDefault {
    pub const fn provider(&self) -> CallableParameterDefaultProviderSymbolId;
    pub const fn value(&self) -> &CallableParameterDefaultValue;
}
```

Struct field and union payload defaults use their corresponding provider IDs, valid surface types, and error types. A macro can
generate common storage and accessors, but the public result types remain specific.

A valid runtime-default surface contains at least:

- resulting type,
- ordered generic and contextual dependencies,
- receiver and earlier-parameter dependencies where permitted,
- ownership and borrowing behavior,
- effects and capabilities,
- trusted obligations,
- finalization obligations,
- source and declaration anchors needed by diagnostics and tooling.

The provider ID is assigned deterministically from the owner identity when written or recovered default syntax is discovered. An
erroneous default retains that provider identity but cannot be lowered or emitted as a valid provider.

#### Constant Definition And Instance API

Constant checking is split between a definition template and a concrete instance value.

Context-bound constant, trait constant member, and trait constant fulfillment views expose kind-specific definition methods returning
checked template facts. The constant evaluator accepts an internal constant-definition erasure only at the shared evaluation
boundary.

Conceptually, a concrete value query uses:

```rust
pub struct ConstantInstanceKey {
    definition: AnyConstantDefinitionId,
    substitution: ConcreteGenericSubstitutionId,
    selected_implementation: Option<ImplementationInstanceId>,
    target_profile: TargetProfileId,
}

impl Compilation {
    pub fn constant_value(
        &self,
        key: ConstantInstanceKey,
    ) -> Arc<DiagnosticResult<ConstantValueId>>;
}
```

`AnyConstantDefinitionId` is a closed internal adapter over ordinary constant, trait constant member, and trait constant fulfillment
IDs. The fields of `ConstantInstanceKey` remain private and category-specific symbol views construct the key. Typed symbol APIs must
not expose the erased definition ID when the exact constant category is known.

A non-generic closed constant uses an empty substitution and no selected implementation. Declaration-surface completion evaluates
that one concrete instance. Generic and trait-selected templates are checked at definition completion but produce concrete values
only for requested instance keys.

Definition diagnostics belong to the checked template fact. Substitution-, implementation-, or target-specific diagnostics belong
to the concrete instance fact and are not published as diagnostics for unrelated instances.

#### Predicate And Contract API

Predicate symbols and trait predicate members expose a typed definition state:

```rust
pub enum PredicateDefinitionState<T> {
    Defined(T),
    Required,
    OpaqueTrusted,
    Error(ErrorPredicateDefinition),
}
```

The public implementation can use separate ordinary-predicate and trait-predicate state enums if that prevents impossible variants
for either category. A defined state contains a checked semantic predicate summary, not one evaluated Boolean value.

Callable and declaration views expose checked contract and constraint collections through their existing typed `contracts()` and
`constraints()` facts. Predicate application and proof queries are separate facts keyed by the checked definition and exact semantic
arguments.

#### Runtime Default Provider Surface

A runtime default provider exposes only the compiler-facing callable surface needed by interface emission, lowering, and codegen:

- its typed provider symbol ID,
- its owning parameter or field ID,
- ordered provider inputs,
- generic parameters and substitutions,
- result type,
- effects, capabilities, trusted obligations, and finalization behavior,
- source-independent checked provider representation or stable interface reference.

Provider inputs are explicit. A parameter default can depend on the receiver and earlier parameters, but not itself, later parameters,
or arbitrary call-site locals. Field and payload defaults cannot depend on `self` or sibling fields.

Imported package interfaces reconstruct provider symbols and their checked surfaces without dependency source syntax. Generic
providers include a source-independent checked or lowerable template sufficient for downstream instantiation. The consuming compiler
must not rebind a dependency's default expression.

Compiler-known runtime construction defaults use the same checked-default and provider contract through their compiler-known
construction surfaces. Their typed parameter and provider categories follow the owning declaration kind, and their symbols are
contained beneath the compiler-known environment through ordinary typed owner relationships. They must not be forced into a source
callable parameter ID.

#### Fact Dependencies And Cycles

Declaration-owned expression queries depend on identity, generic parameters, declared types, and the minimum contract facts needed by
that expression. They must use signature-only facts when resolving a recursive reference to the owning declaration rather than
forcing the owner's defaults again.

Parameter defaults are checked in parameter declaration order and can depend only on the receiver and earlier parameters. Field and
payload defaults are checked in their declaration order but cannot depend on siblings.

Constant definition templates form a checked dependency graph. Concrete constant instances form a separate evaluation graph. Illegal
constant cycles produce structured diagnostics and error constant values. Predicate recursion and termination follow predicate
checking rules rather than being treated as cache deadlocks.

A declaration-owned fact can request a checked executable body when its semantics require execution. Constant evaluation can, for
example, request a const callable body. The body remains a body-checker-owned fact and is not added to every symbol's declaration
completion boundary.

#### Diagnostics And Publication

Each checked default, constant template, constant instance, predicate definition, and contract fact owns its diagnostic bag.
Definition diagnostics are published once with the definition fact. A call or construction that encounters an error-aware default
uses the error result without duplicating the original definition diagnostic.

Successful publication caches the immutable semantic summary, provider relationship where applicable, and diagnostics together.
Canceled work and failed speculative work publish none of them.

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
- checked declaration-owned runtime defaults,
- checked constant definition templates and closed constant values,
- checked predicate bodies and other non-executable contract expressions,
- declaration-surface diagnostics.

This level can invoke binder and checker facts through the compilation query graph.

#### Body Completion

Executable body binding and checking are not symbol completion.

Body completion belongs to checked bound-body facts owned by binding and checker services.

This includes function, method, constructor, lifecycle, lambda, and defaulted executable trait-member bodies.

A symbol can be declaration-surface complete while its executable body has never been requested.

### Force Complete

`force_complete(symbol)` requests declaration-surface completion for the symbol, every symbol it semantically contains, and any
explicitly owned declaration-surface relationship defined for that symbol kind.

It traverses typed containment relationships in deterministic order.

It does not recursively complete symbols that are only referenced.

Examples:

- completing a function completes its generic parameters, receiver, ordinary parameters, parameter defaults, and contracts,
- completing a struct or union materializes its type-associated surface and completes the direct and inherent members that
  contribute to that surface without changing their containment,
- completing a struct or union checks its field or payload defaults in declaration order,
- completing a constant checks its definition template and evaluates its value when the instance is closed,
- completing a predicate checks its predicate definition but does not evaluate it once to a Boolean value,
- completing an implementation completes its own parameters and fulfillment members but does not recursively complete the trait it
  references,
- completing an overload family resolves and validates its arm references but does not reparent those arms,
- completing a package completes all active modules and their contained source symbols,
- completing the compiler-known environment completes its ambient declarations and compiler-known modules,
- completing a callable symbol does not bind its executable body.

Compiler-known completion must use the same immutable containment traversal as other symbol roots. Its environment root owns both
top-level compiler-known modules and ambient declarations for completion purposes, while each module or declaration contributes its
ordinary typed semantic children. A complete catalog audit must prove that this traversal reaches every generated compiler-known
declaration exactly once.

Force completion is idempotent. It returns or exposes diagnostics through the same cached fact results used by ordinary requests.

Compilation-wide symbol diagnostics are obtained by forcing the compiler-known environment and selected package symbol roots to
declaration-surface completion and deterministically merging the diagnostics of all requested symbol facts.

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

## Local Symbol Storage

### Storage Boundary

Compilation-wide symbol storage contains declaration-surface, imported, compiler-known, compiler-provided, and synthesized surface
symbols. It does not append locals when an executable body or declaration-owned expression is requested.

Each independently checked semantic region owns an immutable `LocalSymbolSnapshot`. Regions include:

- a declared executable body,
- an anonymous callable together with its signature and body,
- a declaration-owned expression that introduces contextual lookup symbols.

The binder constructs the local snapshot together with the region's checked bound representation. The compilation query publishes
the bound representation, local snapshot, and diagnostic bag atomically as one immutable fact result. Cancellation, failed
speculation, or abandoned work publishes none of them.

This boundary allows bodies and declaration-owned expressions to be requested, cached, replaced, and checked in parallel without
mutating the compilation-wide symbol graph.

### Region Identity

`LocalSymbolRegionId` identifies one exact local semantic region in one compilation snapshot. It is not a symbol ID.

Every region has a deterministic `LocalSymbolRegionKey` derived from:

- the exact declared or synthesized semantic owner,
- the bound fact category,
- the region's stable `SyntaxAnchor`,
- a canonical role or ordinal when one owner has multiple regions at the same anchor.

A declared callable body uses its callable symbol and body anchor. An anonymous callable region uses the nearest declared or
synthesized root plus the canonical path of lambda anchors leading to that lambda. A declaration-owned expression uses its owning
symbol, exact fact category, and expression anchor.

The lambda region owns the anonymous callable symbol, its parameter symbols, its contracts, and its body-local symbols. The enclosing
bound lambda expression references that deterministic anonymous callable ID. The ID can therefore be derived before the lambda body
is checked without publishing a partial enclosing snapshot or allocating a global symbol.

Region keys must not contain memory addresses, worker IDs, cache insertion order, or lazy request order. Numeric region IDs are
compilation-local handles and need not survive source edits.

### Typed Local IDs

Local IDs contain the owning region ID and a category-specific slot. They do not wrap the compilation-wide `SymbolId`.

Conceptually:

```rust
pub struct LocalBindingSymbolId {
    region: LocalSymbolRegionId,
    slot: LocalBindingSlot,
}

pub struct LocalConstantSymbolId {
    region: LocalSymbolRegionId,
    slot: LocalConstantSlot,
}

pub struct AnonymousCallableSymbolId {
    region: LocalSymbolRegionId,
    slot: AnonymousCallableSlot,
}

pub struct AnonymousCallableParameterSymbolId {
    region: LocalSymbolRegionId,
    slot: AnonymousCallableParameterSlot,
}

pub struct PostconditionResultSymbolId {
    region: LocalSymbolRegionId,
    slot: PostconditionResultSlot,
}

pub struct LocalScopeId {
    region: LocalSymbolRegionId,
    slot: LocalScopeSlot,
}
```

Fields remain private. Typed constructors are available only to the local-symbol builder. Public accessors can expose the region ID
when routing a reference to its owning snapshot is necessary.

`LocalScopeId` uses the same region protection but is not a symbol ID.

`AnyLocalSymbolId` is a closed erasure over the exact local categories. `AnySymbolId` can include local variants for diagnostics,
debugging, visitors, and tooling, but core binding and checking APIs use exact IDs or narrow family IDs.

An ID from one region is never valid against another region's snapshot. Checked accessors return `None` or a typed lookup error for
a mismatched region. Unchecked indexing is crate-private and reserved for compiler invariants already established by the caller.

### Snapshot Shape And API

Conceptually:

```rust
pub struct LocalSymbolSnapshot {
    region: LocalSymbolRegionId,
    scopes: Box<[LocalScope]>,
    bindings: Box<[LocalBindingSymbol]>,
    constants: Box<[LocalConstantSymbol]>,
    anonymous_callables: Box<[AnonymousCallableSymbol]>,
    anonymous_parameters: Box<[AnonymousCallableParameterSymbol]>,
    postcondition_results: Box<[PostconditionResultSymbol]>,
}

impl LocalSymbolSnapshot {
    pub const fn region(&self) -> LocalSymbolRegionId;
    pub fn scope(&self, id: LocalScopeId) -> Option<&LocalScope>;
    pub fn binding(&self, id: LocalBindingSymbolId) -> Option<&LocalBindingSymbol>;
    pub fn constant(&self, id: LocalConstantSymbolId) -> Option<&LocalConstantSymbol>;
    pub fn anonymous_callable(
        &self,
        id: AnonymousCallableSymbolId,
    ) -> Option<&AnonymousCallableSymbol>;
    pub fn anonymous_parameter(
        &self,
        id: AnonymousCallableParameterSymbolId,
    ) -> Option<&AnonymousCallableParameterSymbol>;
    pub fn postcondition_result(
        &self,
        id: PostconditionResultSymbolId,
    ) -> Option<&PostconditionResultSymbol>;
}
```

The exact storage can use dense per-category tables generated by shared infrastructure. Public APIs remain category-specific and do
not expose one canonical heterogeneous child list.

The checked-region API exposes its local snapshot directly. Lowering receives the checked bound HIR and its local snapshot together
rather than resolving locals through a mutable compilation-wide registry.

### Lexical Scopes

`LocalScopeId` identifies a lexical lookup scope inside one local snapshot. A scope is not a symbol.

The immutable scope graph records:

- the parent lexical scope where one exists,
- callable, pattern-arm, guard, block, and contract-context boundaries,
- source visibility start points,
- typed ordinary-name indexes,
- source anchors needed by tooling and diagnostics.

Published scope indexes support deterministic tooling queries. The binder can use a mutable scope stack while constructing them, but
that mutable stack is not durable compiler state.

Semantic containment and lexical lookup ancestry are separate relationships. An anonymous callable symbol is semantically contained
by its nearest declared or anonymous callable owner, while its symbols are stored in its own lambda region snapshot. Its body begins a
new callable lookup boundary. Because Bray lambdas are capture-free, the lambda body does not inherit the enclosing region's local
names or receiver. It receives only its own parameters and the declarations available from its declaration context.

Named callable parameters, predicate parameters, generic parameters, and receiver parameters remain declaration-surface symbols.
The root local scope references the applicable surface symbols without cloning them into local storage.

### Deterministic Construction And Recovery

Local slots are assigned in canonical syntax order within each category. Pattern bindings use their language-defined logical binding
order. Stable keys combine the region key, introducing syntax anchor, exact local category, and a role or ordinal where one syntax
form introduces multiple symbols.

Construction follows these rules:

- discard patterns create no symbol,
- a simple binding occurrence creates one local binding symbol,
- destructuring creates one symbol for each logical binding name,
- alternative-pattern occurrences for one coherent binding share one symbol and retain all contributing anchors,
- duplicate or incompatible bindings retain distinct error-aware records for diagnostics,
- a missing name does not create an empty-string symbol,
- recovered names create recovered symbols when their identity remains usable,
- synthesized contextual symbols use fixed semantic roles rather than invented source names as identity.

Name-index insertion and symbol retention are separate. An invalid duplicate remains addressable for diagnostics and bound recovery
without silently replacing the valid lookup entry.

### Local Symbol Lifetime And Resolution

A local ID is valid only while its owning `LocalSymbolSnapshot` and compilation snapshot are valid. Persisted tooling data uses the
region key, local stable key, and source anchors rather than raw local slots.

Bound name, declaration, pattern, and callable nodes store exact typed local or surface symbol references. A narrow resolved-value or
resolved-callable family can close over both identity spaces where a language operation accepts either. It must not erase every
reference to `AnySymbolId` merely for storage convenience.

Diagnostics owned by a checked region can carry local IDs because the local snapshot and diagnostic bag are published and retained
together. External diagnostic formats use source locations, rendered names, and stable keys according to their lifetime contract.

Local bindings are symbols, but storage identities, storage accesses, projections, temporaries, control-flow blocks, and borrow-state
records are not.
Those remain owned by the source-shaped bound representation, checker state, or backend-independent MIR as appropriate.

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

The binder creates local symbols while constructing a checked semantic region. Those symbols remain owned by the region's immutable
local snapshot and are published atomically with the checked bound representation.

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

Canonical semantic types, constant values, open constant terms, and generic substitutions are stored in a compilation- or immutable
symbol-snapshot-scoped semantic value store owned by `bray-symbols`. The store interns immutable structural keys and returns opaque
typed IDs. Numeric IDs are never persisted or used for deterministic output ordering.

Inference variables are checker-local and never appear as `TypeId`. Closed constant values use `ConstantValueId`; open const
parameters and checked terms used in generic type identity use `ConstantTermId`. An open `GenericSubstitutionId` is distinct from a
validated `ConcreteGenericSubstitutionId` required by concrete constant evaluation and code generation.

Portable inferred dependency contracts use `DependencyContractTemplateId`. Their formal subjects use stable receiver, parameter,
result, projection, capability, and implementation-witness identities. Unit-local storage identities, storage accesses, borrow
capabilities, and instantiated contracts remain owned by `bray-bound-tree` and never enter symbol-store templates.

The detailed representation, equality, ownership, and interning contract is defined in `docs/design/binder.md`.

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
- a closed `SymbolRootId` family with kind-specific root APIs,
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
- canonical semantic values reusing one ID per structural key within a store,
- semantic value IDs remaining store-local and absent from persisted interfaces,
- open and concrete generic substitutions remaining type-distinct,
- dependency-contract templates using formal subjects and structural interface encoding,
- dependency-contract templates containing no bound-unit storage, access, or borrow-capability IDs,
- source, imported, and compiler-known facts sharing canonical type and constant APIs,
- exactly one compiler-known environment root and no compilation-root symbol,
- package and compiler-known module owners remaining distinguishable through `ModuleOwnerId`,
- ambient compiler-known lookup not changing source-module package containment,
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
- deterministic callable parameter, struct field, and union payload default-provider IDs,
- runtime default providers remaining absent from ordinary lookup and typed member collections,
- kind-specific default APIs retaining recovered and error-aware defaults,
- invalid runtime defaults diagnosing even when every use supplies an explicit value,
- parameter defaults accepting receiver and earlier-parameter dependencies while rejecting self and later-parameter dependencies,
- force completion checking declaration-owned defaults, predicates, contracts, and closed constants,
- force completion recording but not checking defaulted trait callable bodies,
- generic constant definition templates producing separately cached concrete instance values,
- substitution- and target-specific constant diagnostics remaining isolated to their exact instance facts,
- imported runtime defaults remaining usable without dependency syntax rebinding,
- local symbol IDs remaining separate from compilation-wide declaration `SymbolId` values,
- deterministic local region and symbol keys under different body request and worker orders,
- local snapshots being published atomically with their bound region and diagnostics,
- mismatched-region local IDs being rejected by checked accessors,
- scopes remaining distinct from symbols and retaining deterministic parent and visibility relationships,
- named callable parameters remaining surface symbols referenced by body scopes rather than copied locals,
- anonymous callables introducing capture-free callable lookup boundaries,
- alternative-pattern occurrences sharing one logical binding symbol,
- duplicate, recovered, and malformed local bindings retaining error-aware identities without corrupting lookup indexes,
- cancellation and abandoned speculative work publishing no local snapshot or local diagnostics,
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

Integration tests should verify that `Compilation` exposes symbol roots and diagnostics as lazy facts derived from declaration
tables, the compiler-known catalog, and compiled dependency interfaces.

---

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Define symbol kinds, typed IDs, semantic value IDs, origins, keys, and common immutable identity data.
2. Define canonical semantic type, constant value, open constant term, generic substitution, dependency-contract-template, and
   semantic-store contracts.
3. Define the symbol graph, `SymbolRootId`, package roots, the compiler-known environment root ID, module owner families, module
   symbols, and deterministic source declaration-to-symbol identity mapping.
4. Define typed module member collections and lookup-result primitives.
5. Define the compilation-owned lazy fact and completion protocol with cycle and concurrency contracts.
6. Add named type, trait, implementation, overload, member, and parameter symbol records.
7. Add the compiler-known environment and imported symbol providers through the same typed contracts.
8. Add binding-dependent signature, constraint, contract, implementation, and constant fact queries according to
   `docs/design/binder.md`.
9. Add local semantic-region snapshots, anonymous callable symbols, and checked-region integration.
10. Add recursive force completion and deterministic symbol diagnostics.

Each step should preserve lazy evaluation and avoid temporary eager APIs that callers would later depend on.
