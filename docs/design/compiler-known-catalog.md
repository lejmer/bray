# Compiler-known catalog design

This document defines how Bray compiler-known declarations, compiler-provided declaration surfaces, special values, and recognized
standard-library identities should be represented and loaded by the compiler.

The language semantics are defined in `docs/language/compiler-known-and-standard-library.md`. The symbol model is defined in
`docs/design/symbols.md`. This document is the implementation contract for the catalog that supplies those symbols and identities.

---

## Goals

The compiler-known catalog should:

- keep language-defined declaration surfaces readable and reviewable,
- reuse the Bray parser for Bray declaration syntax instead of implementing a second declaration grammar,
- assign explicit stable identities independently of source spelling and file order,
- represent compiler-provided behavior through closed typed Rust hooks,
- represent protected storage and runtime forms through closed typed Rust representation roles,
- support target-conditional availability without making the catalog executable,
- publish one immutable, process-shareable catalog,
- support compiler-known symbol construction through the same kind-specific APIs as source and imported symbols,
- keep recognized standard-library declarations separate from ambient compiler-known declarations,
- fail as a compiler invariant when the checked-in catalog is invalid.

The catalog is compiler implementation input. It is not user source and is not part of a package dependency graph.

---

## Non-Goals

The catalog language does not:

- define executable compiler behavior,
- contain lowering, checking, constant-evaluation, or code-generation scripts,
- replace Rust exhaustiveness for implementation hooks,
- replace the Bray parser for declaration surfaces,
- create source packages or imports,
- represent structural type forms as fake named declarations,
- make recognized standard-library declarations ambient,
- expose catalog syntax through `bray-syntax`,
- provide recovery for malformed checked-in catalog files,
- preserve compatibility with obsolete catalog formats.

Bray is greenfield. The catalog format can be changed coherently with its checked-in definitions and compiler implementation.

---

## Terminology

### Catalog Source

A catalog source is a checked-in `.braydef` file containing compiler metadata and embedded Bray declaration-surface fragments.

Catalog syntax is private compiler infrastructure. It is not a Bray terminal or nonterminal and does not belong in the public Bray
syntax tree.

### Stable Catalog Key

A stable catalog key identifies one exact catalog entry across source layout changes.

Examples include `Bool`, `RawPointer`, `RawPointerRead`, and `CoreMemoryCopy`.

Keys are explicit data. They are not inferred from declaration spelling, path, source range, file name, ordinal, or insertion order.
Renaming a key is an identity change and must be reviewed as such.

Stable catalog keys are not compilation-local `SymbolId` values. Symbol construction maps stable catalog keys to compilation-local
typed symbol IDs.

### Declaration Surface

A declaration surface is the source-shaped contract of a compiler-known declaration. It can include:

- name and declaration kind,
- generic parameters,
- parameters and parameter modifiers,
- declared types and result clauses,
- visibility and declaration modifiers,
- directives,
- generic constraints,
- callable contract clauses,
- declaration-owned expressions such as defaults and constant definitions.

A compiler-provided implementation body is not part of the declaration surface and is not Bray source.

### Representation Role

A representation role is a closed Rust enum value that tells semantic and lowering code that a declaration has language-defined
representation behavior. Examples can include scalar Boolean, signed integer, raw pointer, or task handle roles.

The catalog names a role. Rust defines and implements it.

### Implementation Hook

An implementation hook is a closed Rust enum value identifying compiler-provided behavior. The catalog associates the hook with a
declaration surface. Checking, lowering, constant evaluation, and code generation implement the hook in their owning crates.

### Availability Rule

An availability rule is a closed Rust enum value identifying a language-defined predicate over target facts. The catalog selects the
rule. Target-profile logic evaluates it for a compilation.

---

## Architecture

### Crate Ownership

The catalog belongs to a dedicated `bray-compiler-known` crate.

`bray-compiler-known` owns:

- stable catalog key value types,
- catalog source inventory,
- the private catalog parser and source model,
- immutable catalog descriptors,
- representation-role enums,
- implementation-hook enums,
- availability-rule enums,
- structural catalog validation,
- lazy process-wide catalog publication.

It can depend on:

- `bray-source`,
- `bray-syntax`,
- `bray-parser`,
- `bray-diagnostics` for structured internal validation records.

It must not depend on:

- `bray-symbols`,
- `bray-binder`,
- `bray-bound-tree`,
- `bray-checker`,
- `bray-lowering`,
- `bray-ir`,
- `bray-compilation`.

The intended dependency direction is:

```text
bray-source --\
bray-syntax ----> bray-compiler-known ----> bray-symbols
bray-parser --/              |
                             +------------> later semantic consumers
```

Later consumers interpret typed metadata without adding reverse dependencies:

- `bray-symbols` materializes ordinary kind-specific symbols from declaration descriptors,
- `bray-binder` binds catalog declaration-owned syntax through ordinary semantic rules,
- `bray-checker` interprets checker-owned representation and behavior roles,
- `bray-lowering` maps implementation hooks to lowering behavior,
- `bray-compilation` evaluates target availability and coordinates lazy facts.

### Module Shape

The initial crate layout should follow the thin-root module rule:

```text
crates/bray-compiler-known/
|-- catalog/
|   |-- ambient/
|   |-- core/
|   `-- recognized/
`-- src/
    |-- lib.rs
    |-- catalog.rs
    |-- catalog/
    |   |-- descriptor.rs
    |   |-- entry.rs
    |   |-- key.rs
    |   |-- loader.rs
    |   |-- parser.rs
    |   |-- source.rs
    |   `-- validation.rs
    |-- availability.rs
    |-- implementation.rs
    `-- representation.rs
```

`lib.rs` and `catalog.rs` should contain only module declarations and reexports after their implementation is split into
submodules.

Catalog parser types remain private unless another compiler tool has a concrete need for them. Consumers use immutable descriptor
APIs, not the catalog parse tree.

### Definition Location

Checked-in definitions live under `crates/bray-compiler-known/catalog/` because they are owned by the compiler-known catalog crate.
They must not live under `bray-syntax/src/syntax/`, the language specification, or a source package directory.

Files should be grouped by semantic domain rather than by implementation consumer. Expected groups include:

- ambient scalar and fundamental declarations,
- compiler-known result and task declarations,
- compiler-known traits and operator contracts,
- `core.memory` declarations,
- `std.target` declarations,
- recognized standard-library identities.

The crate owns a canonical embedded source inventory using `include_str!`. That inventory is the complete ordered list of catalog
source files shipped with the compiler. The loader must not enumerate the build machine's filesystem or depend on directory
iteration order.

Catalog source order is retained for developer diagnostics and review only. Semantic identity and descriptor IDs are derived from
stable keys in canonical key order.

---

## Catalog Language

The catalog format is called Bray definitions and uses the `.braydef` extension.

It is a small declarative wrapper around embedded Bray declaration surfaces. It is intentionally not general-purpose.

### Example

```bray
catalog compiler_known;

scope Ambient at ambient {
    declaration Bool {
        representation ScalarBool;

        surface {
            struct bool {}
        }
    }

    value True {
        spelling true;
        type { bool }
        representation BooleanTrue;
    }
}

scope CoreMemory at core.memory {
    declaration MemoryCopy {
        availability RawMemory;
        implementation MemoryCopy;

        surface {
            trusted func copy(
                destination: RawPointer<u8>,
                source: RawPointer<u8>,
                count: usize,
            );
        }
    }
}
```

An independently identified member names its semantic owner explicitly:

```bray
catalog compiler_known;

scope Ambient at ambient {
    declaration RawPointer {
        representation RawPointer;

        surface {
            struct RawPointer<T> {}
        }
    }

    declaration RawPointerRead {
        owner RawPointer;
        implementation RawPointerRead;

        surface {
            trusted func read(offset: usize) -> u8;
        }
    }
}
```

The `RawPointerRead` surface is parsed in the member context implied by the `RawPointer` owner. Its generic references can resolve
against the owner's generic environment during later semantic completion.

### Lexical Rules

The catalog parser should reuse the Bray lexer for identifiers, punctuation, comments, whitespace, literals, and balanced
delimiters. Catalog words such as `catalog`, `scope`, `declaration`, and `surface` are recognized by spelling in catalog context.
They do not become new public `SyntaxKind` values.

Whitespace and comments are insignificant to catalog semantics but remain available in the internal source snapshot for developer
diagnostics.

Catalog files use UTF-8. Include paths are not part of the initial language because the canonical embedded source inventory is owned
by Rust and must work in an installed compiler without filesystem access.

### Grammar

The initial outer grammar should be:

```ebnf
catalog-file =
    "catalog" catalog-kind ";"
    { scope-declaration } ;

catalog-kind =
      "compiler_known"
    | "recognized_standard_library" ;

scope-declaration =
    "scope" stable-key "at" scope-location "{"
        { declaration-entry | value-entry }
    "}" ;

scope-location =
      "ambient"
    | path ;

declaration-entry =
    "declaration" stable-key "{"
        { declaration-field }
    "}" ;

declaration-field =
      owner-field
    | availability-field
    | representation-field
    | implementation-field
    | surface-field ;

value-entry =
    "value" stable-key "{"
        { value-field }
    "}" ;

value-field =
      spelling-field
    | type-field
    | availability-field
    | representation-field ;

owner-field =
    "owner" stable-key ";" ;

availability-field =
    "availability" identifier ";" ;

representation-field =
    "representation" identifier ";" ;

implementation-field =
    "implementation" identifier ";" ;

spelling-field =
    "spelling" token-spelling ";" ;

surface-field =
    "surface" braced-bray-declaration-fragment ;

type-field =
    "type" braced-bray-type-expression-fragment ;
```

The braces surrounding a Bray fragment belong to the catalog language. They are not part of the embedded fragment.

The parser should reject unknown fields, duplicate fields, missing required fields, unsupported entry kinds, and trailing tokens.
The initial format does not preserve unknown metadata for possible future interpretation.

### Catalog Kinds

`compiler_known` files define ambient compiler-known identities and compiler-known module scopes. Their descriptors can create
symbols before source package lookup begins.

`recognized_standard_library` files define recognition contracts for ordinary imported standard-library declarations. They do not
create ambient symbols and do not bypass ordinary package, import, visibility, or path rules.

The two catalog kinds can share parser and descriptor primitives, but their validated descriptor families remain distinct so a
recognized declaration cannot accidentally be materialized as compiler-known.

### Scopes

Every scope has an explicit stable key and location.

`ambient` denotes the compiler-known environment searched according to the language's ambient lookup rules. A path such as
`core.memory` or `std.target` denotes a compiler-known module path.

A scope declaration defines catalog ownership, not a source module declaration. Scope descriptors later produce the appropriate
typed module or compiler-known-root symbols.

The same logical scope can be split across several catalog files. Scope contributions merge by stable scope key only when their
locations and catalog kinds agree. Merge order is canonical and immutable.

### Declaration Entries

Every independently identified compiler-known declaration has one `declaration` entry.

An entry without `owner` belongs directly to its surrounding scope. An entry with `owner` belongs to the declaration identified by
that stable catalog key.

This supports fields, variants, callable members, lifecycle members, associated declarations, and other nested declaration spaces
without placing catalog-only annotations inside Bray syntax.

Container declaration surfaces use syntactically empty bodies when their independently identified members are supplied by owned
entries. Signature-owned children such as generic parameters and callable parameters remain in the declaration surface and do not
need separate catalog entries unless a future language rule requires an independent catalog identity.

Ownership is resolved after all files have been parsed, so owners can be declared in another file or later in source order. The
validated owner graph must be acyclic and compatible with the declaration contexts allowed by the Bray grammar.

### Value Entries

`value` entries represent language-known values that cannot honestly be expressed as ordinary declarations, including `true`,
`false`, `unit`, and `none` where required by the language model.

A value entry has an explicit spelling, an embedded Bray type expression, and a representation role. Whether a particular value is
exposed through lookup, literal parsing, or contextual typing is determined by its typed descriptor category and the owning language
rules. A value entry must not be forced into a fake constant declaration merely to reuse symbol machinery.

### Declaration Surface Fragments

`bray-parser` should expose a narrow declaration-fragment entry point for the catalog and other compiler infrastructure that has a
real need to parse one declaration outside a source unit.

Conceptually, the API accepts:

- a source snapshot and exact fragment range,
- a closed declaration context such as module, struct, union, trait, or implementation member,
- a closed surface mode,
- the expected end of the fragment.

It returns one closed typed declaration-fragment value plus ordinary lexical and syntax diagnostics.

The fragment parser must call the existing handwritten `Parser::parse_*` methods. It must not copy declaration grammar into a
catalog-specific parser.

The catalog surface mode can accept language-specified bodyless compiler-provided declaration surfaces where ordinary package
source requires a body. This is a narrow parser mode, not a public source-language extension. The same typed header, parameter,
constraint, contract, and type-expression parsers remain authoritative.

Catalog loading rejects any fragment that:

- produces lexical or syntax diagnostics,
- contains missing or skipped syntax,
- contains more than one declaration,
- does not consume the complete fragment,
- is invalid in the owner-derived declaration context,
- contains an executable body for a compiler-provided implementation hook.

Type-expression fragments use the same principle and reuse the ordinary type-expression parser.

### No Catalog Syntax In `bray-syntax`

Catalog scopes, entries, fields, and metadata are not Bray language nonterminals. Their private parse records belong to
`bray-compiler-known`.

Only embedded Bray fragments produce `bray-syntax` nodes. This preserves the rule that the `bray-syntax/src/syntax/` tree contains
the actual terminals and nonterminals of the Bray language.

---

## Typed Metadata

### Stable Keys

Stable keys are validated identifiers stored through a dedicated `CompilerKnownDeclarationKey`, `CompilerKnownValueKey`, or
equivalent typed key. Consumers must not pass unvalidated strings through semantic APIs.

Catalog keys remain serializable language identities. Compilation-local descriptor and symbol IDs can be compact integers assigned
in canonical stable-key order.

Compiler code should obtain frequently used symbols through typed environment APIs such as compiler-known type or trait accessors.
It should not repeatedly look up string spellings or catalog keys throughout checking and lowering.

### Representation Roles

Representation role spellings map exhaustively to a Rust enum owned by `bray-compiler-known`.

Representation roles describe semantic identity and protected representation. They do not contain target layout values. Target
layout remains a lazy compilation fact derived from the selected target profile and the declaration's role.

Unknown role spellings are catalog validation errors. Duplicate use of a unique role is rejected unless the role contract explicitly
permits several declarations.

### Implementation Hooks

Implementation hook spellings map exhaustively to a Rust enum owned by `bray-compiler-known`.

Each consuming phase uses an exhaustive match or an exhaustively validated registry for the hooks it owns. The hook enum contains no
function pointers, trait objects, closures, bound nodes, or backend objects.

A hook identifies observable compiler-provided behavior. It does not select one permanent lowering strategy. Different targets can
lower the same hook differently while preserving its specified Bray semantics.

Catalog validation rejects:

- unknown hooks,
- hooks attached to incompatible declaration kinds,
- missing hooks on declarations whose implementation is compiler-provided,
- executable Bray bodies attached to compiler-provided hooks.

### Availability Rules

Availability rule spellings map exhaustively to a Rust enum owned by `bray-compiler-known`.

The catalog does not contain an arbitrary boolean expression evaluator. Rust implements each language-defined predicate over typed
target facts. `Always` is the default when the field is absent.

The global catalog retains every declaration. A target-specific catalog view filters or marks entries through a lazy compilation
fact. Target selection must not mutate the process-wide catalog.

### Structural Type Forms

Tuple, fixed-size array, slice, nullable, borrow, trait-view, owned-indirection, and callable type forms are semantic constructors,
not named declaration symbols.

Their implementation remains in typed Rust semantic APIs. They should not receive fake catalog declaration entries. Catalog entries
can still describe named declarations that support those forms, such as a compiler-known `Storage<T>` trait.

---

## Immutable Descriptor Model

The private catalog parse tree is validated and converted into an immutable descriptor graph.

The public model should remain category-specific. Conceptually it includes:

```text
CompilerKnownCatalog
  compiler_known_scopes
  compiler_known_declarations
  compiler_known_values
  recognized_standard_library_scopes
  recognized_standard_library_declarations
  indexes
  internal_sources

CompilerKnownScopeDescriptor
  key
  location
  declaration_ids
  value_ids

CompilerKnownDeclarationDescriptor
  key
  owner
  declaration_kind
  surface
  representation_role
  implementation_hook
  availability_rule

CompilerKnownValueDescriptor
  key
  owner_scope
  spelling
  type_surface
  representation_role
  availability_rule
```

The actual Rust API should use typed IDs and closed owner enums. It should not expose one untyped property map or one generic list of
heterogeneous children.

Catalog descriptors can retain internal syntax handles or catalog-specific syntax anchors for lazy binding of type expressions,
constraints, contracts, and declaration-owned expressions. Those anchors refer to catalog-owned source snapshots, not user source
IDs.

Catalog source anchors are for compiler development and invariant reporting. They must never be emitted as locations in ordinary
user diagnostics.

---

## Loading And Validation

### Lazy Process-Wide Publication

The initial implementation should embed catalog source text with `include_str!` and initialize one immutable catalog through
`OnceLock` on first request.

The catalog is target-independent and can be shared by all compilations in the process. A target-specific available-surface view is
a separate compilation-owned lazy fact.

Concurrent first requests must publish one equivalent catalog. Demand order, worker count, and source inventory order must not alter
stable keys, descriptor IDs, or validation results.

### Structural Loading Stages

Catalog loading proceeds in deterministic stages:

1. Parse every embedded catalog source independently.
2. Parse each embedded Bray fragment through `bray-parser`.
3. Reject syntax diagnostics and recovered syntax.
4. Validate required fields, field cardinality, and typed metadata spellings.
5. Build the complete stable-key skeleton.
6. Resolve scope contributions and declaration owners.
7. Validate ownership cycles and owner-to-child declaration contexts.
8. Assign compact descriptor IDs in canonical stable-key order.
9. Build immutable typed indexes.
10. Publish the catalog atomically.

No partially validated catalog is observable.

### Semantic Validation

Some catalog validity requires symbols, binding, checking, or target facts and therefore cannot be implemented inside
`bray-compiler-known` without creating dependency cycles.

Those checks use ordinary later-phase APIs after the compiler-known identity skeleton exists. Examples include:

- resolving declaration type expressions against compiler-known symbols,
- checking generic constraints and callable contracts,
- validating compiler-provided signature requirements,
- checking target-availability dependencies,
- confirming recognized standard-library interface identities.

The compiler test suite and catalog-checking `xtask` command must force semantic completion of the complete catalog. Production
compilation can retain ordinary lazy completion, relying on checked-in catalog validation as a build invariant.

### Invalid Catalogs

Malformed user source must never panic the compiler. Malformed checked-in catalog source is different: it is a compiler defect.

The catalog parser and validator should still produce structured internal errors containing:

- error kind,
- catalog source file,
- source range,
- typed arguments,
- related catalog keys where applicable.

Tests and `xtask` render those errors for compiler developers. Runtime initialization fails the compiler invariant after rendering
or summarizing the complete deterministic error set. Catalog errors are not merged into the user's compilation diagnostic bag and
do not use user source locations.

---

## Symbol Construction

Compiler-known descriptors feed a dedicated symbol provider. They do not pass through source declaration discovery and do not
receive source `DeclarationId` values.

The provider:

1. Builds the dedicated `CompilerKnownEnvironmentSymbol` and compiler-known module scope skeleton.
2. Assigns compilation-local typed symbol IDs from canonical catalog descriptor order.
3. Creates ordinary kind-specific symbol records with `CompilerKnown` or `CompilerProvided` origin.
4. Publishes stable catalog-key-to-symbol-ID indexes.
5. Supplies lazy declaration-surface facts backed by catalog descriptors.
6. Evaluates target availability through compilation-owned facts.

A compiler-known struct produces the same `StructSymbol` API as a source struct. A compiler-provided function produces the same
`FunctionSymbol` API as a source function. Origin-specific storage remains behind symbol fact providers.

Stable catalog keys are retained as origin identities but are not substitutes for typed symbol IDs in ordinary semantic APIs.

The compiler-known environment is one root in the compilation's immutable symbol forest. It owns ambient compiler-known
declarations and compiler-known modules, has no ordinary source name, and is not a package. Source modules consult its ambient lookup
index through an explicit lookup relationship without changing their package containment.

Root-wide operations use the closed `SymbolRootId` family defined by `bray-symbols`. They do not require a generic root-symbol record
or a `CompilationRootSymbol`.

Special value descriptors and structural type constructors use their own typed semantic APIs and are not forced into declaration
symbol records when the language model does not define them as declarations.

---

## Recognized Standard-Library Declarations

Recognized standard-library descriptors are contracts for imported declaration identity. They do not create symbols before package
loading.

Recognition proceeds only after an imported package interface supplies the expected stable declaration identity. Spelling, path,
or signature resemblance alone is insufficient.

The recognized descriptor can associate that imported identity with checking, lowering, optimization, const-eligibility, contract,
or availability roles. The imported symbol remains an ordinary imported symbol for lookup, visibility, imports, overloads, and
implementation participation.

The compiler-known and recognized catalogs can share stable-key, availability, hook, parsing, and validation primitives. Their
published descriptor families and symbol-materialization behavior remain separate.

---

## Automation

Project automation belongs in `xtask`.

The initial command should be:

```text
cargo xtask compiler-known check
```

It should:

- load every embedded catalog source,
- report all structural catalog errors deterministically,
- construct the compiler-known symbol environment,
- force all catalog-owned semantic facts,
- validate exhaustive implementation-hook coverage,
- validate representation-role and availability-rule coverage,
- validate recognized standard-library fixtures where available,
- exit unsuccessfully when any invariant fails.

The command orchestrates existing crate APIs. Catalog parsing and domain validation remain in their owning crates rather than being
implemented directly in `xtask`.

### Future Precompilation

The initial implementation should parse the small embedded catalog once per process. It should not introduce checked-in generated
Rust, a build script, a binary descriptor blob, or a second serialization format without measured need.

If catalog loading becomes measurable, `xtask` can later generate immutable descriptors from the same validated source model. The
consumer boundary must remain `CompilerKnownCatalog`, so changing the loading strategy does not change symbol, binder, checker, or
lowering APIs.

Any future generator must provide a check mode that detects stale generated output and must preserve deterministic, reviewable
artifacts.

---

## Determinism And Concurrency

The catalog must satisfy these invariants:

- stable keys are globally unique within their catalog identity domain,
- descriptor IDs are assigned from canonical stable-key order,
- source file order does not define semantic identity,
- owner resolution does not depend on declaration order,
- scope contribution merging is deterministic,
- validation errors have deterministic ordering,
- target filtering does not mutate shared descriptors,
- no lazy request publishes a partially completed catalog or symbol fact,
- concurrent requests observe equivalent immutable values.

Catalog text, validated descriptors, and process-wide indexes can be shared through immutable references or `Arc`. Compilation-local
symbol IDs and target-specific views remain owned by their compilation or immutable symbol snapshot.

---

## Testing

Tests should cover:

- valid compiler-known declaration and value entries,
- embedded fragments using generics, constraints, contracts, directives, and defaults,
- bodyless compiler-provided callable surfaces in every supported owner context,
- explicit nested declaration ownership,
- declarations split across several catalog files,
- duplicate and unknown stable keys,
- duplicate, missing, and unknown fields,
- unknown representation, implementation, and availability roles,
- invalid owner kinds and ownership cycles,
- malformed embedded Bray fragments,
- rejection of recovered embedded syntax,
- deterministic descriptors under reversed source inventory order,
- deterministic errors under different worker schedules,
- target-specific views leaving the global catalog unchanged,
- compiler-known symbols exposing the same typed APIs as equivalent source symbols,
- recognized standard-library entries never becoming ambient symbols,
- complete implementation-hook registry coverage,
- full semantic force-completion through `cargo xtask compiler-known check`.

Tests should compare stable keys and typed relationships rather than relying on incidental source ordinals.

---

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Add `bray-compiler-known` with stable keys, metadata enums, immutable descriptor types, and the embedded source inventory.
2. Add the private catalog lexer cursor, parser, structural validation, and deterministic descriptor builder.
3. Add declaration and type-expression fragment entry points to `bray-parser` using existing parser methods.
4. Add representative ambient, module-scoped, nested, compiler-provided, and special-value catalog entries.
5. Publish the immutable process-wide catalog through `OnceLock`.
6. Add the compiler-known symbol provider and compilation-local stable-key-to-symbol-ID map.
7. Add lazy target-availability views.
8. Add checker and lowering registries for typed representation and implementation roles.
9. Add recognized standard-library descriptors and imported-identity matching.
10. Add `cargo xtask compiler-known check` and force-completion coverage.

Each step should preserve the descriptor boundary. Temporary hardcoded symbol construction paths should not become alternate sources
of compiler-known truth.
