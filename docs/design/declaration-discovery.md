# Declaration discovery design

This document defines how Bray declaration discovery should be implemented.

Declaration discovery belongs to `bray-declarations`. It consumes parsed syntax trees from `bray-syntax` and produces an
immutable declaration surface for later compiler phases. It must not create symbols, resolve names, bind bodies, type check
declarations, or evaluate expressions.

The compiler architecture overview is defined in `docs/design/compiler-architecture.md`. This document is the implementation
contract for the declaration discovery phase.

---

## Goals

Declaration discovery should:

- record every declared surface that later phases need,
- preserve source order and source correlation,
- support partial modules and other declaration containers,
- keep discovered declarations distinct from semantic symbols,
- publish immutable outputs,
- support deterministic parallel source-unit discovery,
- keep diagnostics structured and locale-neutral.

Malformed or duplicated source declarations should not cause compiler panics.
Discovery should record the recoverable surface it can see and emit diagnostics for errors it owns.

---

## Phase Boundary

Declaration discovery records syntax-backed declaration facts.

It owns the early catalog of:

- modules,
- imports and exports,
- constants,
- functions,
- predicates,
- callable contracts,
- overload declarations,
- structs and unions,
- traits,
- implementations,
- fields,
- variants,
- parameters,
- type, trait, and implementation members.

Declaration discovery does not own:

- symbol creation,
- name resolution,
- import or export resolution,
- type expression binding,
- expression body binding,
- overload resolution,
- trait coherence,
- type checking,
- borrow checking,
- code generation.

A declaration can be invalid and still be present in the declaration table.
Symbol construction decides whether a discovered declaration becomes a normal symbol, an error symbol, or no symbol.

---

## Declaration IDs And Symbol IDs

`DeclarationId` identifies a discovered syntax declaration.

`SymbolId` identifies a semantic entity created later by symbol construction.

These must stay separate.
A duplicate function declaration, malformed struct declaration, or recovered trait member still needs a stable `DeclarationId`
so diagnostics can point to the exact syntax that was discovered.

Declaration IDs are assigned by the deterministic merge step.
Per-source-unit discovery chunks should use local IDs or source-order records that are remapped into final IDs during merge.

---

## Core Data Model

The published output should be an immutable `DeclarationTable`.

The table should contain:

- declaration records keyed by `DeclarationId`,
- container records keyed by `ContainerId`,
- module part records keyed by `ModulePartId`,
- source-order indexes for deterministic iteration,
- lookup indexes needed by later phases.

Initial record shapes should be close to:

```text
DeclarationRecord
  id
  kind
  owning_container
  name
  source_id
  syntax_kind
  full_range
  is_recovered
  syntax
  child_container

ContainerRecord
  id
  kind
  parent
  name_or_path
  parts
  declarations

ModulePartRecord
  id
  module_container
  source_id
  syntax_kind
  full_range
  declarations
```

The exact Rust fields can be refined during implementation, but the ownership should stay the same:

- declarations are source-backed,
- containers own source-order member lists,
- module parts represent syntax contributions to a logical module,
- the final table is immutable.

Syntax references should not expose green internals.
Use typed syntax nodes or a syntax-owned handle shape when the record needs to preserve access to the original declaration
syntax.

---

## Containers

Containers are declaration-discovery concepts.
They group declarations before symbols exist.

Expected container kinds:

- package or compilation root,
- logical module,
- type body,
- trait body,
- implementation body,
- callable, predicate, contract, and lifecycle signatures,
- union variant payloads.

Function and expression-local declarations can be added later if the binder needs a declaration surface before body binding.
Signature containers record declaration-surface children such as generic parameters and callable or predicate parameters, but
they do not imply that callable bodies are walked during discovery.

Every declaration belongs to exactly one owning container.
Declarations that introduce nested declaration spaces also point at a child container.

Examples:

```text
module app;
struct Point { x: Int; }
```

```text
ModuleContainer app
  Declaration Struct Point
    TypeContainer Point
      Declaration Field x
```

```text
trait Display { func show(); }
```

```text
TraitContainer Display
  Declaration TraitCallableMember show
    SignatureContainer show
```

```text
union Maybe { Some(value: Int); }
```

```text
TypeContainer Maybe
  Declaration UnionVariant Some
    VariantContainer Some
      Declaration PayloadField value
```

---

## Partial Modules

Modules are logical containers with one or more source parts.

A source-unit module declaration contributes a module part whose body is the loose module-level declarations in that source
unit.
A block module declaration contributes a module part whose body is the braced module body.

Multiple source units can contribute to the same logical module path.
Multiple top-level block module declarations can also contribute to the same logical module path.

Discovery should model this as:

```text
ModuleContainer core.io
  ModulePart source A
  ModulePart source B
  ModulePart source C
```

The module container is the logical declaration space.
The module parts preserve the individual syntax declarations and source ranges that contributed to that space.

Module paths are syntactic paths during declaration discovery.
Do not resolve imports, aliases, package roots, or visibility through them in this phase.

---

## Chunks And Merge

Discovery should run in two stages.

1. Source-unit discovery walks one source unit and produces an immutable `DeclarationChunk`.
2. Table merge consumes immutable chunks and constructs a new immutable `DeclarationTable`.

Source-unit discovery may use mutable local builders internally.
Once a chunk is published, it must not be mutated.

The merge step must treat chunks as immutable input.
It should allocate final declaration, container, and module-part IDs in deterministic order and copy or transform chunk records
into final table records.

Do not build a shared mutable declaration table from parallel worker tasks.
Worker tasks should return chunks.
The owning phase merges chunks through one deterministic operation.

Deterministic order should be based on:

- package or compilation input order,
- source-unit order,
- source order within each source unit,
- stable child order within each container.

The final result must not depend on worker completion order.

---

## Syntax Walking

Declaration discovery should use `bray-syntax` walker primitives.

Use `walk_source_unit` inside source-unit discovery tasks.
Use `SkipChildren` when a declaration has been recorded and its children are discovered through typed syntax APIs or a
container-specific helper.

The walker should not own declaration policy.
The declaration phase decides which syntax kinds are declaration roots, which containers they belong to, and which child
containers they introduce.

---

## Diagnostics

Declaration discovery can report diagnostics for errors it has enough information to diagnose accurately, such as:

- duplicate declarations in one declaration space,
- invalid duplicate module part shapes,
- declaration forms in a container that cannot contain them,
- conflicting declaration surfaces that do not require name binding or type checking.

Diagnostics must be structured records with typed arguments. User-facing English must be rendered through `bray-messages`.

When an error depends on symbol construction, name resolution, type binding, or body checking, discovery should record the
surface and leave the diagnostic to the owning later phase.

---

## Testing

Declaration discovery tests should validate the published declaration surface.

Important tests:

- one source-unit module with module-level declarations,
- multiple source units contributing to one partial module,
- multiple block module declarations contributing to one partial module,
- type, trait, and implementation declarations introducing child containers,
- fields, variants, and members assigned to the correct container,
- deterministic merge output when chunks arrive in different orders,
- malformed recovered declarations preserved with source ranges,
- diagnostics for duplicate declarations owned by discovery.

Tests should assert IDs, kinds, container membership, source order, and source ranges where relevant.
Do not assert symbol IDs in declaration discovery tests.
