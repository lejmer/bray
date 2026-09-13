# Compiled package interfaces

Separate compilation publishes semantic information independently of source syntax and executable implementation.
Consumers reconstruct ordinary imported symbols and demand the records they need without installing dependency source or
repeating definition-site binding.

## Artifact and component boundaries

A `.brayi` interface describes one checked library product's public declaration graph, relationships, contracts, and
required declaration-owned templates. A separate `.brayimpl` bundle carries executable templates needed for downstream
generic specialization or constant evaluation. Interface-only queries do not load those bodies.

| Owner | Responsibility |
| --- | --- |
| `bray-symbols` | Stable external identity, imported identity inputs, and normalized semantic values |
| Semantic representation crates | Meaning of checked templates and their inputs |
| `bray-package-interface` | Durable encoding, bounded validation, lazy decoding, and artifact inspection |
| `bray-compilation` | Query coordination, imported maps, and completed export bundles |
| `bray-emitter` | Product staging and publication |

Symbols have no dependency on the codec. The codec consumes completed immutable semantic data without invoking binder or
checker workflows. Interfaces are not native link inputs, and test catalogs remain separate artifacts owned by the test
protocol.

## Stable identity and imported symbols

Source and imported declarations use the same kind-specific symbol APIs with explicit origin backing. Loading creates a
complete deterministic identity skeleton before arbitrary semantic completion. Lazy requests cannot add ordinary symbol
IDs according to request order.

Structured external keys follow semantic ownership and declaration category. They survive source movement and local ID
reassignment without reducing identity to a rendered path or signature hash. Artifact-local IDs provide compact
references only within the exact issuing interface. Dependency references preserve the defining external key and
selected dependency identity.

Imported modules represent logical merged modules, not source contributions. Re-exports project lookup edges to the
original declaration rather than cloning it. Synthesized identities required by public contracts belong in the skeleton
without becoming ordinary lookup candidates.

## Public and support graphs

The public graph includes the relationships needed for lookup, overloads, implementations, coherence, and contract use.
Checked templates may also require private support entities. Those entities occupy a separate graph and do not become
importable, reflectable declarations simply because a public template refers to them.

Export construction requests the reachable semantic records, then freezes an error-free bundle. That narrow query does
not waive the complete product check required for final emission. Recovery values and unresolved public dependencies
cannot become a successful importable artifact.

Private runtime role bindings stay in trusted product metadata. A public wrapper exports its ordinary inferred contract,
not its binary role identity. The interface records the answers consumers need without distributing private source
syntax or checker-local flow state.

## Semantic records and checked templates

Types, constants, open terms, constraints, callable contracts, and dependencies use durable typed representations.
Decode interns equivalent values in the consumer's semantic store rather than preserving producer-local numeric IDs.
Recursive relationships are graphs with validated references.

Portable dependency templates use formal receiver, parameter, result, capability, projection, and witness identities.
Binding instantiates them into local storage and access identities. Open run-transfer requirements therefore survive
separate generic compilation without rechecking the body or recognizing a library operation by name.

Declaration-owned templates preserve checked behavior and explicit contextual inputs for defaults, predicates,
constraints, and constant definitions. Definition states remain explicit so absence of a body cannot imply trust.
Consumers substitute and evaluate or lower these templates without definition-site lookup, overload resolution, or
replayed source diagnostics.

Provider-checked execution evidence remains distinct from declared promises. Imported dependencies extend the same proof
graph, including termination evidence and foreign provenance. Async records similarly preserve the distinction between
invocation and deferred execution, along with portable frame and runtime requirements. Libraries describe requirements
without selecting a runtime implementation.

## Executable bodies

Implementation bundles bind their independently addressable payloads to the exact public interface, dependencies,
semantic revision, and template schema. They contain checked source-independent templates, not syntax, machine code, or
mutable compiler arenas. Private implementation entities stay behind implementation references in symbol APIs.

Specialization identity includes the declaration, substitutions, selected witnesses, relevant target and runtime inputs,
schema, and dependency hashes. Optional target-specific pre-specialized MIR must validate against that complete
identity. The same dependency model supports lazy constant-callable bodies without requiring source in the consumer.

## Format and validation

Explicit wire tags and bounded length-delimited records define the format independently of Rust layout. Section and
record directories make narrow reads possible. Structural validation establishes safe framing and graph identity before
publication, while requested semantic records decode only their transitive record closure.

Per-section compression is deterministic and bounded. Storage encoding does not change decoded semantic identity.
Unknown required semantics are rejected. Optional non-semantic data has an explicit discard-or-preserve policy for tools
that rewrite artifacts, rather than an unknown field silently affecting compilation.

The semantic content hash commits to every consumer-relevant record and support dependency. The artifact hash identifies
exact stored bytes, including provenance. These hashes support reuse and corruption detection, not authenticity. Package
and distribution policy owns trust in the selected input.

External artifacts are untrusted input. The reader validates counts, offsets, nesting, allocation bounds, references,
identities, and compatibility before using them. Rejected input retains the most specific field, region, and cause for
structured diagnostics. Source provenance can improve navigation but is never needed for correctness or rebinding.

## Target compatibility and reuse

Records retain exact typed target-property dependencies with their semantic owner. An aggregate compatibility summary
can reject a mismatch early without replacing record-level evidence. Irrelevant target differences need not invalidate
portable semantics, while equal target names cannot excuse incompatible properties.

Loaded immutable bytes can be shared by artifact identity. Decoded semantic records can be shared by content identity
and precise dependencies, including across provenance-only changes. Compilation-local symbol maps remain with their
snapshot and are remapped when reused. Paths and timestamps are discovery hints rather than semantic identity.

## Parallel construction and lazy publication

Independent export fragments use structural identities and private buffers under the compilation worker budget. Stable
assembly assigns artifact-wide IDs and resolves shared semantic values before freezing the bundle. Worker completion
order cannot determine references or bytes.

Imported queries coordinate through the ordinary compilation dependency graph. Decode locks do not span arbitrary
semantic queries. Equal concurrent requests share immutable results, and failed or cancelled work publishes no partial
record, loaded interface, or artifact. The emitter commits completed artifacts through its normal product transaction.

## Related documents

- [Package semantics](../language/modules-and-packages.md)
- [Symbols](symbols.md)
- [Binder](binder.md)
- [Async runtime](async-runtime.md)
- [Emission](emitter.md)
