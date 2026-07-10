# Compiled package interfaces design

This document defines how Bray library products publish compiler-readable package interfaces and how consuming compilations load
those interfaces into imported symbols.

Package and module semantics are defined in `docs/language/modules-and-packages.md`. Symbol identity and symbol construction are
defined in `docs/design/symbols.md`. Binder and checked representation contracts are defined in `docs/design/binder.md`. This
document is the implementation contract for the durable artifact boundary between separately compiled packages.

---

## Goals

Compiled package interfaces should:

- represent the selected library product's reachable public declaration graph,
- preserve stable external declaration identity across compilations,
- reconstruct imported symbols through the same kind-specific APIs as source symbols,
- make dependency source text and syntax trees unnecessary for normal consumption,
- carry all semantic surface facts required for separate compilation,
- carry checked declaration-owned templates that consumers must instantiate without rebinding,
- preserve implementation, overload, coherence, dependency-contract, target, layout, and ABI facts that affect consumers,
- support deterministic lazy decoding and parallel symbol queries,
- be compact, bounded, corruption-resistant, and independent of Rust memory layout,
- produce structured localized diagnostics for invalid or incompatible artifacts,
- support deterministic content-addressed caching.

The interface is semantic compiler data. It is not source syntax, a syntax-tree serialization, a symbol-table memory dump, or an
executable object format.

---

## Non-Goals

The package interface does not:

- create a parallel `Metadata*Symbol` hierarchy,
- preserve dependency syntax trees or trivia,
- serialize compilation-local `SymbolId`, `TypeId`, bound-node IDs, pointers, or arena indexes,
- expose private package declarations through imported lookup,
- contain unevaluated source text that the consuming compiler must parse or bind,
- carry ordinary executable bodies merely because a declaration is public,
- define package resolution, package naming, dependency version selection, or lock-file policy,
- define the code distribution strategy for generic executable bodies,
- serve as the object-code, debug-information, documentation, or source-map format,
- guarantee compatibility with obsolete interface format revisions.

The package and build layer supplies opaque package, product, and dependency identities. The interface records and validates those
identities but does not decide how a package manager obtains them. Each identity type used in an artifact must provide a canonical
serialized form and semantic equality contract.

Implementation payloads required by a future generic code-distribution model can be referenced by or colocated with an interface,
but they remain a separate artifact contract. They must not be smuggled into imported symbol surfaces as executable source bodies.

---

## Terminology

### Compiled Package Interface

A compiled package interface is an immutable `.brayi` artifact emitted for one successfully checked library product surface.

The artifact contains package identity, dependency references, imported symbol identity records, semantic surface facts, target
dependencies, and checked declaration-owned templates.

### Interface Symbol ID

`InterfaceSymbolId` is a compact artifact-local reference into one interface's symbol identity table.

It is not a `SymbolId`. It is meaningful only together with the exact interface identity and content hash that issued it.

### External Symbol Key

`ExternalSymbolKey` is the stable serialized semantic identity of an exported or otherwise interface-addressable symbol.

It survives compilation-local ID reassignment and file movement that does not change semantic ownership. It changes when the
language says declaration identity changes.

### Imported Symbol

An imported symbol is a normal Bray symbol in the consuming compilation whose origin is `Imported` and whose facts are backed by a
compiled package interface.

The imported symbol has a compilation-local typed symbol ID. The interface descriptor from which it was created is not itself a
symbol.

### Interface Fact

An interface fact is one immutable decoded semantic value associated with an interface symbol, relationship, target dependency, or
declaration-owned template.

Fact payloads can remain encoded until requested. Successful decoding publishes an immutable cached value.

### Support Entity

A support entity is an interface-private semantic or implementation record required to instantiate an exported checked template but
not visible through ordinary imported lookup.

Support entities do not become public imported declarations merely because an exported template refers to them.

---

## Ownership And Crate Boundaries

### `bray-symbols`

`bray-symbols` owns:

- `ExternalSymbolKey` and its category-specific construction rules,
- canonical semantic type, constant value, open constant term, and generic substitution contracts,
- typed imported symbol IDs and ordinary kind-specific symbol records,
- `SymbolOrigin::Imported`,
- artifact-local symbol and imported-fact key value types needed by symbol APIs,
- immutable imported identity-skeleton input types,
- normalized symbol-facing imported surface values,
- imported symbol completion contracts.

It must not know the `.brayi` byte layout, section tags, compression, checksums, file paths, readers, or codecs.

### `bray-package-interface`

A dedicated `bray-package-interface` crate owns:

- the `.brayi` artifact format,
- explicit wire tags and primitive encodings,
- deterministic encoding,
- bounded structural decoding,
- interface header and section-directory validation,
- immutable loaded-interface handles,
- lazy fact-directory and payload decoding,
- conversion between wire records and symbol-owned imported surface inputs,
- semantic content and whole-artifact hashes,
- interface-specific structured diagnostics,
- developer inspection and format-validation APIs.

It can depend on:

- `bray-base`,
- `bray-diagnostics`,
- `bray-source` for optional source provenance values,
- `bray-symbols` for semantic keys and imported symbol-surface contracts,
- `bray-bound-tree` for serializable checked-template contracts when required.

It must not depend on:

- `bray-compilation`,
- `bray-binder`,
- `bray-checker`,
- `bray-lowering`,
- `bray-codegen`,
- `bray-emitter`,
- `bray-driver`.

The important dependency direction is:

```text
bray-symbols -----------\
bray-bound-tree ---------> bray-package-interface
bray-diagnostics -------/            |
                                     v
                              bray-compilation
```

`bray-symbols` must not depend on `bray-package-interface`. This keeps semantic symbols independent of their durable encoding.

### Coordination And Emission

`bray-compilation` owns lazy fact coordination for:

- loading a dependency interface,
- validating the expected package and product identity,
- constructing the imported identity skeleton,
- resolving imported symbol facts,
- constructing the current library product's export bundle,
- requesting deterministic interface bytes.

`bray-emitter` writes already constructed interface bytes or an immutable interface artifact descriptor to the requested output
location. It does not decide semantic reachability or encode symbol facts itself.

---

## Imported Symbol Model

### No Metadata Symbol Hierarchy

Bray does not define public types such as `MetadataFunctionSymbol`, `MetadataStructSymbol`, or `MetadataTraitSymbol`.

A source function and imported function both use `FunctionSymbol` and `FunctionSymbolId`. A source struct and imported struct both
use `StructSymbol` and `StructSymbolId`. Origin is explicit and independent of symbol kind.

Conceptually, kind-specific records can use private origin backing:

```rust
enum FunctionSymbolBacking {
    Source(SourceFunctionDeclarationId),
    Imported(ImportedFunctionFactKey),
    CompilerKnown(CompilerKnownDeclarationKey),
}
```

The exact backing enum is private and category-specific. Bray must not introduce one universal backing record with unrelated optional
fields merely to share storage across symbol kinds.

Common completed facts should normalize to the same immutable semantic value types regardless of origin. Origin-specific backing is
retained only where later lazy facts or diagnostics need it.

### Imported Package Roots And Modules

Every loaded dependency has a normal `PackageSymbol` root in the consuming compilation. Imported `ModuleSymbol` values are contained
by that package and use the same full package-relative path model as source modules.

The interface contains logical merged modules. It does not expose source units, module parts, split-module contribution records, or
declaration discovery IDs.

Imported module and member relationships are reconstructed from interface identities and typed relationship tables. Re-exported
symbols retain their defining external identity and are projected through exported lookup edges rather than cloned into new symbols.

### Identity Skeleton Before Lazy Facts

The loader produces a complete deterministic imported identity skeleton before arbitrary imported facts are requested.

The skeleton establishes:

- the package root,
- logical module symbols,
- every imported declaration symbol that can participate in lookup or semantic relationships,
- containing-symbol relationships,
- typed member and parameter identities,
- stable external-key-to-symbol-ID mappings,
- exported lookup edges,
- overload, implementation, and provider identity edges required before signature completion.

Compilation-local IDs are assigned from canonical interface identity order, never first-request order. Lazy fact requests cannot add
new ordinary declaration symbol IDs to an already published imported identity skeleton.

Synthesized imported identities whose existence is part of the interface surface, such as runtime default provider symbols, are also
present in the skeleton. They remain absent from ordinary lookup according to their symbol contract.

---

## Stable External Identity

### Key Properties

An `ExternalSymbolKey` is structured semantic data, not a rendered fully qualified name or an untyped string.

Its category-specific components can include:

- defining package identity,
- full package-relative module path,
- semantic containing declaration key,
- symbol kind,
- declared ordinary name where the declaration has one,
- owner-relative role or ordinal where the language defines identity that way,
- normalized implementation identity for unnamed implementations,
- synthesized provider role for compiler-generated interface symbols.

Keys must not include:

- source file paths,
- source ranges,
- syntax node addresses,
- declaration discovery IDs,
- compilation-local symbol or type IDs,
- worker order,
- lazy request order,
- a hash of the declaration's current signature as its sole identity.

A surface change and an identity change are different. Changing a function result type changes its interface facts and interface
content hash, but does not automatically create a different function identity. Moving a declaration to another semantic module or
owner changes identity.

### Category-Specific Construction

External keys follow the language's symbol identity rules:

- modules use package identity plus full package-relative module path,
- named module declarations use module owner plus symbol kind plus ordinary name,
- fields and variants use defining type plus their declared name,
- parameters and generic parameters use owner plus category and ordinal,
- overload families use owner plus family name,
- overload arms retain their independently declared identities,
- named implementations use their semantic owner and implementation name,
- unnamed implementations use their normalized exact implementation identity rather than a source ordinal,
- synthesized runtime default providers use owner plus provider role and owner-relative ordinal where required.

Only valid successfully checked public interfaces are emitted, so duplicate or recovered source declarations never require unstable
external disambiguation keys.

### Local And Cross-Package References

Inside one artifact, records refer to local symbols by `InterfaceSymbolId` for compactness.

A reference to a symbol defined by a dependency uses:

- a dependency-table slot,
- the dependency's `ExternalSymbolKey`,
- the dependency interface content hash expected by the producer where exact semantic identity is required.

The loader resolves every external reference against the selected dependency interfaces before publishing the imported symbol
skeleton. A digest can accelerate key lookup, but equality always verifies the full structured key.

---

## Interface Surface Selection

### Product Boundary

An importable interface is emitted for one library product after its selected source graph, dependency graph, target gates, module
merging, visibility, exports, implementations, overloads, contracts, and public API validity have been checked.

Package identity is shared by package products, but product identity remains in the interface header because different products can
select different public surfaces and dependencies.

Executable and test products do not become importable merely because the compiler can describe their symbols. Any future tooling
snapshot for those products is a different artifact contract.

### Exported Symbol Graph

The exported symbol graph contains:

- public logical modules reachable through the package surface,
- public declarations reachable through those modules,
- public re-export lookup edges,
- type, trait, implementation, overload, variant, field, member, generic-parameter, and callable-parameter relationships needed to
  describe those declarations,
- synthesized provider symbols whose stable identity is referenced by an exported declaration surface,
- implementation and coherence facts that can affect a consuming package,
- semantic dependencies required to interpret every exported fact.

An export edge does not create a new declaration identity. The interface records the target external key and projected lookup name.

Private declarations do not enter imported ordinary lookup. A public declaration whose public surface illegally requires private or
internal access must be rejected before interface construction.

### Support Graph

Checked declaration-owned templates can require implementation records that are not user-visible declarations. Those records belong
to a separate support graph.

Support graph entries:

- use `InterfaceSupportEntityId`, not `InterfaceSymbolId`, unless the language model says the entity is a synthesized symbol,
- cannot be reached through imported ordinary lookup,
- cannot be imported, re-exported, reflected over as declarations, or returned by symbol enumeration,
- exist only when a checked exported fact refers to them,
- are included in the interface content hash and target compatibility checks.

This separation prevents a private helper needed by a runtime default or constant template from becoming an accidental public symbol.

### Facts That Must Be Present

For every exported declaration, the interface records the applicable checked surface facts, including:

- exact declaration kind and stable identity,
- semantic owner and typed member relationships,
- ordinary name and exported lookup names,
- visibility and module visibility,
- generic parameter kinds, order, variance where applicable, and defaults,
- generic constraints and normalized trait applications,
- callable parameter names, order, modifiers, defaults, and declared types,
- result type,
- callable contracts, trusted obligations, capabilities, effects, and ABI,
- inferred dependency contracts required by consumers,
- type layout and representation contracts exposed by the public surface,
- constant eligibility, definition template or closed value as required,
- predicate definitions and callable-contract facts required by consumers,
- overload family and arm relationships,
- implementation subject, trait application, fulfillment, and coherence facts,
- lifecycle obligations and default availability,
- target-fact dependencies,
- runtime default provider identity and checked template reference.

The interface records semantic answers, not the source syntax from which they were derived.

---

## Semantic Encoding

### Dedicated Wire Values

The wire format uses explicit interface value types. It must not serialize Rust structs by memory layout or derive a long-term format
directly from private Rust field names.

Every enum has an explicit stable wire tag. Every collection is length-delimited. Every reference names its table and is bounds
checked. Reserved tags are rejected for the current exact format revision.

### Types

`InterfaceType` encodes canonical checked semantic types. Its closed variants include the forms required by the language, such as:

- a named declaration reference with ordered type and const arguments,
- a generic type or const parameter reference,
- tuple,
- fixed-size array,
- slice,
- nullable,
- borrow and mutability form,
- trait view,
- owned indirection,
- callable type including contracts and effects,
- other structural forms defined by the type system.

The exact variants should mirror durable semantic type categories, not parser productions. Error, unresolved, inferred-placeholder, and
recovery types are forbidden in a successfully emitted interface.

Encoding traverses canonical semantic type records owned by `bray-symbols`. Decoding validates interface type records and interns
equivalent local `TypeId` values in the consuming semantic store. Compilation-local numeric IDs never appear in the artifact.

Recursive types use table references and are validated as graphs. The decoder must not recurse through untrusted nesting without a
configured depth limit.

### Constants And Const Expressions

Closed constant values use a canonical `InterfaceConstantValue` representation independent of host endianness and Rust primitive
layout.

Encoding traverses `ConstantValueId` records. Decoding interns equivalent local constant values and returns `ConstantValueId` values
to imported symbol facts. Open const arguments use checked interface term records corresponding to `ConstantTermId`, not fake closed
values.

The representation distinguishes exact language value categories, including arbitrary-width integer or other numeric forms where
the language requires them. Floating, real, and complex values use language-defined bit or canonical numeric encodings rather than
locale-dependent text.

A generic-dependent constant is not falsely serialized as a closed value. It uses a checked const template whose references are
stable external keys, generic parameters, target facts, and template-local IDs.

### Constraints, Contracts, And Dependency Contracts

Generic constraints, callable contracts, trusted obligations, effects, capabilities, and inferred dependency contracts use typed
normalized interface records.

Consumers do not parse contract source or infer an exported dependency contract again. They instantiate and check the published
semantic contract against local arguments and selected implementations.

An exported dependency contract encodes the structural form of a `DependencyContractTemplateId`, not its compilation-local numeric
ID. Its formal subjects reference receivers, parameters by stable ordinal, results, projections, scoped capabilities, and required
implementation witnesses through interface-stable identities. It contains no bound-unit storage identities, storage-access IDs,
borrow-capability IDs, or checker-local flow state.

The consuming compiler decodes the structural template into its local `bray-symbols` semantic store. Binding then instantiates that
portable template into a unit-local `BoundDependencyContractId` using the exact receiver, argument, result, capability, and selected
implementation facts for the use site.

### Checked Declaration-Owned Templates

Runtime defaults, generic constant definitions, predicate definitions, contract expressions, and other declaration-owned facts that
must execute or instantiate in a consuming compilation use source-independent checked templates.

A template:

- uses template-local node and temporary IDs,
- references declarations by local interface ID or external symbol key,
- declares its generic and contextual inputs explicitly,
- carries the checked type, effects, capabilities, dependency contracts, and required implementation witnesses,
- contains no source syntax IDs, compilation-local symbol IDs, bound-node IDs, or captured call-site locals,
- carries enough normalized behavior for lowering without rebinding.

The serializable template contract belongs to `bray-bound-tree` or another lower semantic representation owner. The
`bray-package-interface` codec encodes that contract but does not define its semantics.

The consuming compiler validates references, substitutes generic and contextual inputs, and lowers the already checked template. It
must not rerun definition-site name lookup, overload resolution, or semantic diagnostics.

### Executable Bodies

Ordinary executable bodies are not declaration-surface facts and are not included in the symbol interface merely because their
declarations are public.

If a future generic or cross-package code-distribution strategy requires checked executable templates, machine code, or lower-level
IR, those payloads use a separate implementation section or artifact with its own reachability and compatibility contract. Imported
symbol APIs expose only a stable implementation reference needed by that strategy.

---

## Target And ABI Compatibility

### Recorded Inputs

The interface records:

- the target profile identity used to produce the selected surface,
- the exact target facts that affect any exported semantic fact,
- target facts required by checked templates and support entities,
- public layout and ABI dependencies,
- target-conditional declarations and implementations included in the surface,
- code or implementation payload compatibility keys when such payloads exist.

Target-fact dependencies are recorded by typed fact identity and canonical value. They are not flattened into a target-triple string.

### Compatibility Check

An interface can be reused for a consuming target profile when every target fact relevant to its public semantic surface has the
required value and every ABI compatibility requirement is satisfied.

The consumer need not reject an otherwise portable interface merely because an irrelevant target fact differs. Conversely, matching
target names do not make incompatible fact values safe.

Target compatibility is checked before imported facts that depend on those values are published. A mismatch produces a dependency
interface diagnostic, not a later code-generation failure.

### Per-Fact Dependencies

The artifact retains per-fact target dependencies so incremental and lazy queries can use precise keys. It can also publish a
canonical interface-wide compatibility summary for fast rejection.

The summary is derived from the per-fact records and must not discard information needed to validate an individual lazy fact.

---

## Artifact Format

### File Identity

The initial artifact extension is `.brayi`.

The binary header contains:

- fixed magic bytes,
- exact interface format revision,
- language semantic revision,
- canonical byte-order marker,
- package identity,
- product identity and product kind,
- public-surface identity or build-surface key supplied by the package layer,
- section-directory offset and length,
- declared file length,
- `InterfaceContentHash`,
- `InterfaceArtifactHash`,
- required compatibility flags.

The initial format uses little-endian fixed-width primitives where fixed width is appropriate and explicitly bounded variable-length
integers where compactness materially helps. The codec, not Rust layout, defines every byte.

### Section Directory

The artifact is sectioned to support bounded validation and lazy decoding. Initial semantic sections include:

1. string table,
2. package and product metadata,
3. dependency table,
4. symbol identity skeleton,
5. containment and typed relationship tables,
6. exported lookup and re-export edges,
7. symbol fact directory,
8. canonical semantic type table,
9. constant values and checked const templates,
10. constraints, contracts, effects, capabilities, and dependency contracts,
11. runtime default and other declaration-owned checked templates,
12. implementation and coherence records,
13. target-fact and ABI dependencies,
14. optional source provenance,
15. support graph and implementation references.

Each section has an explicit tag, byte range, record count where applicable, and section checksum or digest contribution. Sections
must not overlap or extend beyond the declared file length.

The initial format is uncompressed. This keeps offsets stable, permits direct bounded reads, and avoids making all lazy access depend
on decompression. Compression can be added only as an explicitly identified container or section encoding after measurement.

### Format Revision

The compiler accepts only format revisions it explicitly implements. The initial reader does not attempt best-effort decoding of a
different revision and does not preserve unknown semantic fields.

Changing the meaning or required encoding of a semantic record increments the format revision. Compatibility shims are added only
when explicitly required by distribution policy, not by default during greenfield development.

Unknown sections, tags, or required flags in the current exact revision are errors. Optional non-semantic tooling data can be added
later only through an explicitly skippable extension mechanism.

### Hashes

`InterfaceContentHash` is a domain-separated BLAKE3 digest of the format revision followed by canonical semantic and support section
tags, lengths, and payloads in tag order. It excludes header offsets, section-directory offsets, optional provenance, and other
explicitly non-semantic tooling sections.

The content hash covers package and product identity, language semantic revision, every symbol, relationship, semantic fact, support
entity, dependency reference, and target dependency that can affect a consumer. Dependency tables record expected content hashes
when exact dependency semantics are required.

`InterfaceArtifactHash` is a BLAKE3 digest of the complete canonical artifact bytes with the artifact-hash field treated as zero. It
covers optional provenance and detects corruption or byte-level substitution of the exact file.

The semantic content hash keys imported semantic reuse. The artifact hash keys exact byte storage and provenance-aware tooling.
Neither hash is a package signature or trust proof. Artifact authenticity belongs to the package and distribution layer.

---

## Encoding

### Lazy Compilation Fact

The current library product's compiled interface is a lazy compilation fact, conceptually keyed by:

- package identity,
- product identity and selected public source graph,
- selected dependency interface identities and hashes,
- selected target profile facts relevant to the surface,
- language semantic revision,
- interface format revision,
- completed public symbol and declaration-owned fact dependency keys.

A caller requests the interface artifact or bytes. It does not issue a command that manually runs symbol completion, checking,
interface construction, and encoding in sequence.

### Export Bundle

`bray-compilation` requests and freezes an immutable export bundle before encoding. The bundle contains category-specific semantic
records and checked templates. It does not expose compilation caches or mutable symbol providers to the encoder.

Interface construction forces exactly the facts required by the reachable public graph. Executable bodies remain unforced unless a
separate implementation payload contract requires them.

The export bundle must be error-free for all facts required by the interface. A package with invalid public surface facts does not
emit a successful importable interface.

Constructing an export bundle does not waive diagnostics outside the public graph. Final library emission publishes the `.brayi`
artifact only when the complete library product check required by emission succeeds.

### Canonical Ordering

Encoding order is deterministic:

- strings use canonical byte ordering after deduplication,
- dependencies use stable package identity order,
- symbols use canonical `ExternalSymbolKey` order,
- typed owner collections preserve their language-defined stable order where that order is semantically or diagnostically relevant,
- maps serialize as sorted records,
- target facts use typed fact-key order,
- templates use deterministic owner and role order.

Hash maps, source file order, memory addresses, worker completion order, and lazy request order must not affect bytes.

### Atomic Publication

The complete byte sequence, semantic content hash, and artifact hash are produced before the interface fact is published.
Cancellation, encoding failure, or semantic failure publishes no partial artifact.

`bray-emitter` writes through its ordinary atomic artifact-emission policy. A failed write must not leave a file that appears to be a
valid completed interface.

---

## Loading And Lazy Decoding

### Load Boundary

The package and build layer supplies:

- the expected package identity,
- the selected dependency/product identity,
- an artifact path or immutable byte source,
- the dependency relationship used for diagnostics.

`bray-package-interface` validates and returns an immutable loaded interface. It never discovers arbitrary interfaces by searching
the filesystem.

### Eager Structural Validation

Before publishing a loaded interface, the reader validates:

- magic, declared length, format revision, and language revision,
- semantic content and artifact hashes,
- expected package and product identity,
- section directory bounds, ordering, uniqueness, and overlap,
- record counts against configured limits,
- string and blob bounds,
- explicit wire tags,
- local table-reference bounds,
- duplicate external keys,
- identity and containment graph integrity,
- dependency slots and expected dependency hashes,
- target and ABI compatibility summary,
- fact-directory ranges.

This validation makes later lazy reads memory-safe and bounded. It does not eagerly decode every semantic fact.

### Imported Skeleton Construction

After structural validation, the loader decodes the identity and relationship sections into an immutable
`ImportedPackageIdentitySurface` owned by `bray-symbols`.

Symbol construction maps every `InterfaceSymbolId` to a compilation-local typed symbol ID in canonical order. The map is immutable
after publication.

Imported symbols store symbol-owned fact keys such as an interface identity plus artifact-local symbol and fact category. They do
not store package-interface reader objects in public records.

### Lazy Fact Decoding

When an imported symbol fact is requested:

1. `bray-compilation` resolves the imported fact key to the loaded interface.
2. `bray-package-interface` validates and decodes the exact length-delimited payload.
3. External symbol references are mapped through the immutable imported skeleton and dependency maps.
4. The decoded value is converted to the ordinary symbol-owned or bound-representation-owned fact type.
5. The immutable `DiagnosticResult<T>` is cached under the compilation fact key.

Repeated and concurrent requests publish one equivalent immutable value. A canceled request publishes nothing.

The decoder must not trigger arbitrary symbol completion while holding an internal decode lock. It returns explicit reference keys
whose semantic dependencies are requested through the compilation query graph.

### Sharing

A structurally validated loaded byte artifact can be shared across compilations when its artifact hash and format revision match.
Decoded semantic tables can also be reused across provenance variants when their semantic content hashes match. Target-specific
decoded facts can be shared only when their precise target dependency keys match.

Compilation-local symbol ID maps are not stored in the process-wide loaded interface. They belong to the consuming symbol snapshot.

---

## Validation And Failure Handling

### External Input Is Untrusted

A dependency interface can be truncated, corrupt, malicious, stale, incompatible, or produced by a defective compiler. Ordinary
interface loading must never panic for such input.

The reader uses checked arithmetic, validates every offset and count before allocation or indexing, limits recursive depth and total
decoded size, and rejects impossible graph shapes. Unsafe code is not required for the initial implementation.

Configured loader limits include at least:

- file size,
- section count,
- record count per section,
- string and blob length,
- total decoded allocation,
- semantic type nesting depth,
- template graph size,
- external-reference count.

Limit failures produce structured diagnostics.

### Semantic Validation

Semantic validation rejects interfaces containing:

- impossible symbol kinds or owner relationships,
- containment cycles,
- duplicate exported names in one ordinary lookup surface,
- missing typed member targets,
- incompatible external-key categories,
- unresolved dependency references,
- error or recovery semantic values,
- malformed type, constraint, contract, implementation, or overload graphs,
- template references outside their declared inputs,
- invalid target-fact or ABI requirements,
- private support entities exposed as declarations,
- implementation or coherence records inconsistent with the exported symbol graph.

The producing compiler should never emit these states. The consuming compiler still validates them because artifacts are external
input.

### Diagnostics

Interface diagnostics are structured through `bray-diagnostics` and rendered through `bray-messages`.

Expected diagnostic categories include:

- invalid interface magic or format revision,
- unsupported language semantic revision,
- truncated or corrupt interface,
- content-hash mismatch,
- package or product identity mismatch,
- dependency interface mismatch,
- incompatible target facts or ABI,
- malformed symbol identity or containment graph,
- malformed lazy fact payload,
- configured resource limit exceeded.

When available, the primary or related location points to the dependency declaration or package configuration that selected the
artifact. Typed diagnostic arguments identify the artifact path, package identity, section, record, and external symbol key. The
compiler must not invent a Bray source span inside a binary artifact.

Optional source provenance can improve dependency diagnostics and tooling, but interface correctness cannot depend on source files
being installed. Provenance locations are related locations, not syntax handles used for rebinding.

Diagnostics produced while the dependency itself was compiled are not replayed to every consumer. A valid interface represents a
successfully checked public surface. Consumers diagnose loading, compatibility, local use, and instantiation failures they own.

---

## Provenance And Tooling

The semantic interface is sufficient without source provenance. An optional provenance section can record:

- normalized source document identity,
- declaration source range,
- documentation source reference,
- generated-origin chain,
- repository or source-package mapping key.

Provenance must not affect semantic symbol identity. Moving a source file without changing semantic ownership changes provenance and
the artifact hash but not `ExternalSymbolKey` or the semantic content hash.

Tooling can use provenance for navigation when dependency source is available. When it is unavailable, tooling still exposes the
imported symbol's package, module, declaration kind, name, signature, and external key.

Developer inspection should use a semantic inspector rather than raw binary parsing. Project automation should eventually expose:

```text
cargo xtask package-interface inspect <path>
cargo xtask package-interface validate <path>
```

The inspector renders deterministic structured records. It must not become an alternate parser or source of interface truth.

---

## Caching And Incrementality

### Interface Identity

The cache identity of a loaded interface includes:

- exact artifact hash for loaded bytes,
- semantic content hash for imported semantic facts,
- format revision,
- language semantic revision,
- package and product identity,
- required dependency hashes,
- relevant target compatibility facts.

Paths and file timestamps are discovery hints, not semantic cache keys.

### Fact Reuse

Decoded identity tables and target-independent facts can be reused when the semantic content hash is unchanged. Target-dependent
facts require matching typed target dependencies.

An imported symbol record or fact can be reused across compilation snapshots only when its external key, interface semantic content
hash, and all fact dependency keys remain valid. Compilation-local numeric symbol IDs are remapped and never persisted as reuse
identity.

### Public Surface Changes

The semantic content hash changes whenever any encoded semantic or support fact changes. The artifact hash also changes for
provenance-only changes. Tools can additionally compare structured external keys and fact hashes to classify changes as:

- identity addition or removal,
- declaration movement or owner change,
- compatible or incompatible surface change,
- target-compatibility change,
- implementation/coherence change,
- checked-template change,
- provenance-only change.

Compatibility classification is a separate semantic tool. The loader validates exact artifact consistency and does not silently
substitute a merely similar interface.

---

## Public API Direction

Conceptually, `bray-package-interface` publishes APIs shaped like:

```rust
pub struct LoadedPackageInterface {
    identity: PackageInterfaceIdentity,
    content_hash: InterfaceContentHash,
    artifact_hash: InterfaceArtifactHash,
    // Private validated bytes, section directory, and lazy decode caches.
}

impl LoadedPackageInterface {
    pub fn imported_identity_surface(&self) -> DiagnosticResult<ImportedPackageIdentitySurface>;

    pub fn decode_symbol_fact<T>(
        &self,
        key: ImportedSymbolFactKey<T>,
    ) -> DiagnosticResult<T>;
}
```

The exact Rust shape can use category-specific methods instead of a generic method where that preserves stronger typing. Public APIs
must not expose byte offsets, unchecked table indexes, raw maps, or codec internals.

Encoding conceptually accepts an immutable bundle:

```rust
pub fn encode_package_interface(
    bundle: &PackageInterfaceExportBundle,
) -> DiagnosticResult<EncodedPackageInterface>;
```

The encoder does not receive a mutable compilation or invoke binder/checker workflows itself. `bray-compilation` supplies completed
facts through the bundle.

---

## Determinism And Concurrency

The interface system must satisfy these invariants:

- equivalent semantic inputs produce byte-identical interfaces,
- external keys and interface IDs are independent of worker and request order,
- structural validation publishes no partial loaded interface,
- lazy fact decoding publishes no partial value,
- diagnostic ordering is deterministic,
- decode caches do not alter semantic answers,
- loaded bytes and validated directories are immutable,
- compilation-local symbol maps are immutable after skeleton publication,
- no decoder lock is held across compilation query calls,
- source provenance cannot affect semantic lookup or identity.

Parallel encoding can build independent sections or record batches with task-local buffers, followed by deterministic canonical
assembly. Parallel decoding can evaluate independent facts after eager structural validation.

---

## Testing

### Round-Trip Tests

Round-trip tests should cover every exported symbol category, relationship, semantic type form, constant value, contract,
implementation record, target dependency, and checked-template category.

Round-trip equality compares normalized semantic values, not private byte-buffer object identity.

### Determinism Tests

Tests should prove byte-identical output under:

- reversed source unit and declaration discovery chunk order,
- different symbol fact request orders,
- serial and parallel completion,
- different hash-map insertion orders,
- repeated encoding,
- equivalent loaded dependency order supplied through differently ordered inputs.

Tests should also prove that provenance-only changes preserve `InterfaceContentHash` while changing `InterfaceArtifactHash`.

### Imported Symbol Tests

Tests should verify:

- source and imported symbols expose equivalent kind-specific APIs,
- imported and source symbols retain distinct origins,
- external keys map deterministically to compilation-local IDs,
- imported modules are contained by package symbols,
- re-exports preserve target identity,
- private and support entities never enter ordinary lookup,
- imported defaults instantiate without source parsing or rebinding,
- imported contracts and dependency contracts affect local checking,
- force completion of imported package roots decodes every required fact deterministically,
- imported executable bodies are not requested during declaration-surface completion.

### Corruption And Resource Tests

Every offset, length, count, tag, reference, graph edge, and digest field needs malformed-input coverage. Tests should include:

- truncation at every structural boundary,
- integer overflow attempts,
- overlapping and out-of-range sections,
- duplicate sections and keys,
- invalid UTF-8 where text requires UTF-8,
- deep recursive type and template graphs,
- oversized allocation requests,
- containment and semantic graph cycles,
- invalid external references,
- hash and dependency mismatches,
- incompatible target facts,
- repeated and concurrent decoding of malformed facts.

Ordinary malformed interface input must return structured diagnostics and never panic.

### Compatibility Fixtures

Checked-in binary fixtures should cover the current exact format revision. Fixtures are regenerated intentionally when that revision
changes. Tests do not require the compiler to continue reading obsolete fixtures unless compatibility support is explicitly added.

---

## Initial Implementation Sequence

Implementation should proceed in dependency order:

1. Define `ExternalSymbolKey`, `InterfaceSymbolId`, imported fact keys, and imported identity-surface inputs in `bray-symbols`.
2. Define source-independent serializable checked-template contracts in their semantic owner crate.
3. Add `bray-package-interface` with header, section directory, explicit wire primitives, loader limits, and structured diagnostics.
4. Implement package, dependency, string, identity, containment, relationship, and lookup sections.
5. Construct imported package, module, and declaration symbol skeletons from validated identity surfaces.
6. Add canonical semantic type, constant, constraint, contract, dependency-contract, implementation, and target-fact sections.
7. Add checked declaration-owned templates and support graph records.
8. Add lazy imported fact queries through `bray-compilation`.
9. Add deterministic export-bundle construction and encoding for valid library products.
10. Integrate interface artifact writing with `bray-emitter`.
11. Add corruption, limits, concurrency, round-trip, and byte-determinism coverage.
12. Add `cargo xtask package-interface inspect` and `validate` commands.

Each step should preserve the boundary between semantic symbol contracts and durable wire encoding. Temporary metadata-specific
symbol classes, eager full-interface decoding, or syntax-rebinding paths must not become alternate architectures.
