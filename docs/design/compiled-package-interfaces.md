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
- contain generic executable implementation payloads, which belong to the separate `.brayimpl` contract,
- serve as the object-code, debug-information, documentation, or source-map format,
- guarantee compatibility with obsolete interface format revisions.

Private product/runtime ABI semantic-contract tables are not ordinary library package interfaces. They belong to the selected
trusted ABI artifact, use closed binary ABI role identities, and are consumed only while checking private standard-library or
product bindings. A public wrapper's inferred portable contract can enter `.brayi`; its private binding role and trusted ABI
contract record cannot.

The package and build layer supplies opaque package, product, and dependency identities. The interface records and validates those
identities but does not decide how a package manager obtains them. Each identity type used in an artifact must provide a canonical
serialized form and semantic equality contract.

Generic implementation payloads are referenced by the interface content hash and published as a separate `.brayimpl` artifact.
They must not be smuggled into imported symbol surfaces as executable source bodies.

---

## Terminology

### Compiled Package Interface

A compiled package interface is an immutable `.brayi` artifact emitted for one successfully checked library product surface.

The artifact contains package identity and version, dependency references, imported symbol identity records, semantic surface facts,
target-property dependencies, and checked declaration-owned templates.

### Compiled Implementation Bundle

A compiled implementation bundle is an immutable `.brayimpl` artifact containing the checked target-independent implementation
templates required to instantiate exported generic executable bodies. It is separate from `.brayi` so an interface-only consumer
does not load executable implementation data and so implementation changes that preserve the public surface do not change imported
symbol identity.

The complete bundle and specialization contract is defined under [Executable Bodies](#executable-bodies).

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
- `bray-tooling`,
- `bray-driver`,
- `bray`.

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

`bray-emitter` includes an already constructed immutable interface artifact in the emission plan, assigns its deterministic output,
and publishes it through the ordinary staging and atomic artifact policy. It does not pass `.brayi` through a codegen backend,
include it in a native link plan, decide semantic reachability, or encode symbol facts itself.

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

Executable and test products do not become importable merely because the compiler can describe their symbols. A tooling snapshot
for those products is a different artifact contract.

Test products publish a bounded test catalog beside their native host. The catalog contains checked entry identities and runner
metadata but no importable declaration surface. Its host identity and digest bind it to one emitted test product, and its format is
owned by the testing protocol rather than the compiled-package-interface codec.

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
- checked implicit abnormal-control summaries such as `may_cancel_current_run`,
- inferred dependency contracts required by consumers,
- type layout, union tags, exact generic copy dependencies, and representation contracts exposed
  by the public surface,
- constant eligibility, definition template or closed value as required,
- predicate definitions and callable-contract facts required by consumers,
- overload family and arm relationships,
- implementation subject, trait application, fulfillment, and coherence facts,
- lifecycle obligations and default availability,
- target-property dependencies,
- runtime default provider identity and checked template reference.

Export validation rejects a public semantic fact that refers to a private product/runtime ABI role. The producing compilation must
have reduced such a dependency to the ordinary inferred public contract of the wrapper declaration.

The interface records semantic answers, not the source syntax from which they were derived.

---

## Semantic Encoding

### Dedicated Wire Values

The wire format uses explicit interface value types. It must not serialize Rust structs by memory layout or derive a long-term format
directly from private Rust field names.

Every enum has an explicit stable wire tag. Every collection is length-delimited. Every reference names its table and is bounds
checked. Reserved tags are rejected for the current exact format revision.

Semantic value tables use canonical record directories followed by contiguous payload bytes. Each directory entry stores the
record's relative offset and length. Structural validation checks the complete directory before any record is read, while exact-fact
decoding reads only the records in the requested fact's transitive dependency closure. Record references are remapped into compact
artifact-local tables before the ordinary semantic model is published.

Implementation records identify the coherence records required by that implementation. This makes implementation selection
addressable without scanning unrelated coherence payloads. Full semantic decoding iterates the same record directories in canonical
order, so narrow and complete decoding share one wire representation.

### Types

`InterfaceType` encodes canonical checked semantic types. Its closed variants include the forms required by the language, such as:

- a named declaration reference with ordered type and const arguments,
- a generic type or const parameter reference,
- contextual `Self` with its exact named type, trait, or implementation context,
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
stable external keys, generic parameters, target properties, and template-local IDs.

### Constraints, Contracts, And Dependency Contracts

Generic constraints, callable contracts, trusted obligations, effects, capabilities, and inferred dependency contracts use typed
normalized interface records.

Consumers do not parse contract source or infer an exported dependency contract again. They instantiate and check the published
semantic contract against local arguments and selected implementations.

Each exported constant predicate constraint has one declaration-owned `GenericConstraint` checked template at the same stable
owner-relative ordinal. Trait-satisfaction constraints instead retain their normalized subject type and exact trait application
directly. This lets consumers evaluate concrete predicate substitutions without rebinding source while preserving demand-driven
constraint checking.

An exported dependency contract encodes the structural form of a `DependencyContractTemplateId`, not its compilation-local numeric
ID. Its formal subjects reference receivers, parameters by stable ordinal, results, projections, scoped capabilities, and required
implementation witnesses through interface-stable identities. It contains no bound-unit storage identities, storage-access IDs,
borrow-capability IDs, or checker-local flow state.

The consuming compiler decodes the structural template into its local `bray-symbols` semantic store. Binding then instantiates that
portable template into a unit-local `BoundDependencyContractId` using the exact receiver, argument, result, capability, and selected
implementation facts for the use site.

Portable dependency templates include open transfer terms for generic subjects published to synchronized shared ownership, an
independent task or thread, or a typed child-process protocol. Each term records the subject projection and destination class. A
consumer instantiates it with the concrete argument's storage, affinity, synchronization, encoding, process-locality, and lifecycle
dependencies; it does not re-check the generic body or look for a marker trait. This representation is what permits ordinary
separately compiled generic `std.channel`, `std.thread`, `std.process`, and `std.parallel` declarations to enforce cross-run safety
without compiler recognition of their names.

### Async Declaration Metadata

An exported async callable records its declared completion type, async callable contract, normalized invocation contract, deferred
body effects, capabilities, execution requirements and lifecycle behavior, normal-completion postcondition template, portable
dependency contract, hidden frame descriptor compatibility reference, state-indexed affinity requirements, and required runtime
ABI features.

The hidden frame representation is not encoded as an ordinary source generic argument or public field. Concrete non-generic frames
publish target-specific size, alignment, move, resume, cancellation entry, phase-one owned-task broadcast, phase-two lifecycle
resolution, result-move, and destruction descriptor references.
Generic async declarations publish checked frame templates or implementation references under the same generic distribution policy
as other executable generic bodies.

A consuming compiler rejects an incompatible frame descriptor or runtime ABI revision before lowering imported use. Libraries record
requirements but never select a runtime implementation. Executable and test product formation unions reachable requirements before
code generation and linking.

Runtime requirements are owner-correlated semantic facts that remain independently queryable. They retain the portable
compatibility facts required by consumers, including hidden-frame compatibility where applicable. Runtime identities, private ABI
roles, and binary bindings are product-selection facts and must not be exported as library requirements.

The complete metadata and compatibility contract is defined in `docs/design/async-runtime.md`.

### Checked Declaration-Owned Templates

Runtime defaults, generic constant definitions, predicate definitions, contract expressions, and other declaration-owned facts that
must execute or instantiate in a consuming compilation use source-independent checked templates.

Every exported predicate must carry an explicit definition state. A defined predicate has exactly one predicate-definition template,
a required trait predicate member has no definition template, and an opaque trusted predicate has no definition template. Importers
must reconstruct these states directly and must never infer trusted behavior from an absent template.

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

Checked executable templates use the separate `.brayimpl` package implementation artifact. Imported symbol APIs expose only stable
implementation references and do not expose implementation payload storage. The bundle never contains source text, syntax,
backend IR, machine code, or mutable compiler arenas.

The bundle header binds it to the exact package, product, public-interface semantic content hash, language semantic revision,
compiler template-schema revision, dependency interface hashes, and required runtime and ABI identities. A consumer rejects a
missing or mismatched bundle when a requested semantic operation requires an implementation payload. Loading the public interface
does not require loading the bundle.

The artifact contains a deterministic directory keyed by stable declaration identity and implementation-payload category. Payloads
are independently length-delimited, independently encoded, and validated so a query can load one body without decoding unrelated
bodies. Every payload uses canonical checked target-independent template IR sufficient for downstream specialization and lowering.
It records its external declaration key, support graph, referenced private implementation entities, exact dependency hashes,
target-property dependencies, and implementation witnesses.

A specialization key consists of the declaring external symbol key, canonical concrete substitution, selected implementation
witnesses, target identity and target-property values, panic and runtime configuration, template-schema revision, and every
template dependency hash. The same key names the downstream checked specialization and its content-addressed cache entry. Optional
pre-specialized MIR may appear in independently identified target-specific sections only when validation binds it to that full key
and the selected MIR schema.

The `.brayimpl` codec uses the same canonical framing, revision, hashing, resource bounds, memory-safe decoding, unknown-section
policy, and corruption handling as `.brayi`. Distribution may add an external authenticity record over artifact hashes, but neither
artifact treats a checksum as trust. Consumers match the selected interface and dependency graph exactly before publishing any
payload.

An exported const callable whose body can be requested by another compilation publishes a checked constant-evaluation body in the
implementation artifact. The body:

- declares callable parameters, generic parameters, and contextual inputs explicitly,
- carries checked operations, control flow, local constant storage, patterns, calls, and result production,
- references declarations and selected implementations through stable package identities,
- uses artifact-local IDs only inside its own validated payload,
- contains no source syntax IDs, compilation-local symbol IDs, or dependency source text,
- carries enough checked semantic information for constant evaluation without parsing, binding, overload resolution, or
  definition-site diagnostics.

The consuming compilation resolves the stable owner through the imported symbol skeleton, validates and interns the selected body
against the already loaded semantic interface, and evaluates it with the concrete call arguments. Loading and interning remain lazy
facts keyed by artifact identity and body identity. Equivalent concurrent requests share the published immutable result.

---

## Target And ABI Compatibility

### Recorded Inputs

The interface records:

- the target profile identity used to produce the selected surface,
- the exact target properties that affect any exported semantic fact,
- target properties required by checked templates and support entities,
- public layout and ABI dependencies,
- target-conditional declarations and implementations included in the surface,
- code or implementation payload compatibility keys when such payloads exist.

Target-property dependencies are recorded by typed property identity and canonical value. They are not flattened into a
target-triple string.

### Compatibility Check

An interface can be reused for a consuming target profile when every target property relevant to its public semantic surface has the
required value and every ABI compatibility requirement is satisfied.

The consumer need not reject an otherwise portable interface merely because an irrelevant target property differs. Conversely,
matching target names do not make incompatible property values safe.

Target compatibility is checked before imported facts that depend on those values are published. A mismatch produces a dependency
interface diagnostic, not a later code-generation failure.

### Per-Fact Dependencies

The artifact retains per-fact target dependencies so incremental and lazy queries can use precise keys. Every dependency records the
exact semantic-fact owner that consumes it, the required target-property declaration, and the required canonical value. An
implementation-header query therefore obtains only the target dependencies owned by that exact implementation.

The artifact can also publish a canonical interface-wide compatibility summary for fast rejection. The summary is derived from the
per-fact records and must not discard information needed to validate an individual lazy fact.

---

## Artifact Format

### File Identity

The canonical package-interface extension is `.brayi`.

The binary header contains:

- fixed magic bytes,
- exact interface format revision,
- language semantic revision,
- canonical byte-order marker,
- package identity and semantic version,
- product identity and product kind,
- public-surface identity or build-surface key supplied by the package layer,
- section-directory offset and length,
- declared file length,
- `InterfaceContentHash`,
- `InterfaceArtifactHash`,
- required compatibility flags.

The format uses little-endian fixed-width primitives where fixed width is appropriate and canonical explicitly bounded
variable-length integers where compactness materially helps. Encoders use the shortest valid variable-length representation and
decoders reject non-canonical encodings. The codec, not Rust layout, defines every byte.

### Section Directory

The artifact is sectioned to support bounded validation and lazy decoding. The required semantic and support sections are:

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
11. callable signatures, generic declarations, and callable parameter default availability,
12. runtime default and other declaration-owned checked templates,
13. implementation and coherence records,
14. target-property and ABI dependencies,
15. support graph and implementation references.

Source provenance and other tooling metadata use optional non-semantic sections.

Each directory entry has an explicit tag, section revision, compatibility class, encoding, byte range, decoded length, record count
where applicable, stored-byte checksum, and decoded-content digest. Sections must not overlap or extend beyond the declared file
length.

Each section independently selects the canonical raw encoding or a registered deterministic compressed encoding. Compression does
not change the decoded semantic bytes or semantic content hash. A reader validates compressed and decoded length bounds before
allocation, authenticates the stored section bytes, and decompresses only the requested section. When a section is requested, the
reader verifies its decoded bytes against the directory's decoded-content digest before publishing them. The encoder selects raw
or compressed storage using one revisioned deterministic size policy recorded in artifact compatibility identity.

The encoding registry contains `raw` and `zstd_frame`. A `zstd_frame` section declares its decoded content size, uses no external
dictionary, carries a frame checksum, and stays within the format revision's maximum window size. The encoder-policy revision fixes
compression parameters and the minimum deterministic size saving required to select it. Readers do not depend on the encoder policy
to decode a valid registered frame.

### Format Revision

The compiler accepts only exact format and section revisions it explicitly implements. It never attempts best-effort semantic
decoding of another revision.

Changing the meaning or required encoding of a semantic record increments the format revision. Compatibility shims are added only
when explicitly required by distribution policy, not by default during greenfield development.

Unknown required semantic sections, record tags, fields, flags, encodings, and section revisions are errors. An optional
non-semantic directory entry declares either `discardable` or `preserve_opaque` compatibility. Readers validate its framing,
bounds, stored-byte checksum, and declared resource limits, exclude it from semantic content identity, and do not interpret it. A
tool that rewrites an artifact must retain `preserve_opaque` bytes exactly and may omit `discardable` bytes. Known semantic sections
cannot use either compatibility class. There is no optional semantic field whose unknown meaning can affect compilation.

### Hashes

Each semantic or support section has a domain-separated BLAKE3 decoded-content digest over its tag, decoded length, and decoded
payload bytes. `InterfaceContentHash` is a domain-separated BLAKE3 digest of the format revision followed by the canonical semantic
and support section tags, decoded lengths, and decoded-content digests in tag order. A reader validates that aggregate commitment
before exposing the interface, then validates each requested section's decoded bytes against its committed digest. The content hash
excludes section revisions, storage encodings, header offsets, section-directory offsets, optional provenance, and other explicitly
non-semantic tooling sections.

The content hash covers package identity and version, product identity, language semantic revision, every symbol, relationship, semantic fact, support
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

- package identity and semantic version,
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
- target properties use typed property-key order,
- templates use deterministic owner and role order.

Hash maps, source file order, memory addresses, worker completion order, and lazy request order must not affect bytes.

### Atomic Publication

The complete byte sequence, semantic content hash, and artifact hash are produced before the interface fact is published.
Cancellation, encoding failure, or semantic failure publishes no partial artifact.

`bray-emitter` stages the completed artifact in its managed product generation and exposes it only when that generation is
atomically published. An explicitly independent inspection write uses the emitter's per-artifact transaction. A failed write cannot
leave a file that appears to be a valid completed interface.

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
2. `bray-package-interface` validates the relevant record directories and decodes the exact fact's transitive record closure.
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

The canonical reader is memory-safe code. It uses checked arithmetic, validates every offset and count before allocation or
indexing, limits recursive depth and total decoded size, and rejects impossible graph shapes. An optimized unsafe view is permitted
only behind the fully validated immutable bounded-byte abstraction, with a separately audited safety contract, fuzz and Miri
coverage, and the canonical safe reader retained as the conformance oracle and fallback.

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
- invalid target-property or ABI requirements,
- private support entities exposed as declarations,
- implementation or coherence records inconsistent with the exported symbol graph.

The producing compiler should never emit these states. The consuming compiler still validates them because artifacts are external
input.

### Diagnostics

Interface diagnostics are structured through `bray-diagnostics` and rendered through `bray-messages`.

The diagnostic categories are:

- invalid interface magic or format revision,
- unsupported language semantic revision,
- truncated or corrupt interface,
- content-hash mismatch,
- package or product identity mismatch,
- dependency interface mismatch,
- incompatible target properties or ABI,
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

Developer inspection uses a semantic inspector rather than raw binary parsing. Project automation exposes:

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
- package identity and semantic version, and product identity,
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
) -> DiagnosticResult<InterfaceArtifact>;
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
assembly. Parallel decoding can evaluate independent facts after eager structural validation. An exact implementation-header query
decodes its implementation record, coherence evidence, generic constraints, semantic value dependencies, and owner-scoped target
dependencies without decoding unrelated declaration templates, callable contracts, or implementation records.

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
- incompatible target properties,
- repeated and concurrent decoding of malformed facts.

Ordinary malformed interface input must return structured diagnostics and never panic.

### Compatibility Fixtures

Checked-in binary fixtures should cover the current exact format revision. Fixtures are regenerated intentionally when that revision
changes. Tests do not require the compiler to continue reading obsolete fixtures unless compatibility support is explicitly added.

---

## Dependency And Conformance Order

Delivery follows this dependency order. Every completed step must use the final contracts and ownership boundaries defined above:

1. Define `ExternalSymbolKey`, `InterfaceSymbolId`, imported fact keys, and imported identity-surface inputs in `bray-symbols`.
2. Define source-independent serializable checked-template contracts in their semantic owner crate.
3. Add `bray-package-interface` with header, section directory, explicit wire primitives, loader limits, and structured diagnostics.
4. Implement package, dependency, string, identity, containment, relationship, and lookup sections.
5. Construct imported package, module, and declaration symbol skeletons from validated identity surfaces.
6. Add canonical semantic type, constant, constraint, contract, dependency-contract, implementation, and target-property sections.
7. Add checked declaration-owned templates and support graph records.
8. Add lazy imported fact queries through `bray-compilation`.
9. Add deterministic export-bundle construction and encoding for valid library products.
10. Add `.brayimpl` template, specialization-key, dependency-binding, and validation contracts.
11. Integrate interface and implementation-bundle artifact writing with `bray-emitter`.
12. Add corruption, limits, concurrency, round-trip, and byte-determinism coverage.
13. Add `cargo xtask package-interface inspect` and `validate` commands.

Each step should preserve the boundary between semantic symbol contracts and durable wire encoding. Temporary metadata-specific
symbol classes, eager full-interface decoding, or syntax-rebinding paths must not become alternate architectures.
