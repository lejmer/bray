# Emitter design

This document defines the goal-state architecture for planning, serializing, staging, and publishing Bray compilation
artifacts.

`docs/design/compiler-architecture.md` defines the compiler-wide phase and lazy emission input model.

`docs/design/codegen.md` defines backend selection, codegen units, backend IR construction, and backend-specific
serialization.

`docs/design/compiled-package-interfaces.md` defines `.brayi` construction, encoding, validation, and hashes.

`docs/design/linker.md` defines final native linker and archiver integration.

This document defines the emission lifecycle that coordinates those completed compiler emission inputs into
deterministic external artifacts.

---

## Goals

The emitter architecture should:

- own one coherent lifecycle from an emission request through published artifact records,
- let artifact policy be established before backend serialization begins,
- keep emission planning immutable and safe to share across parallel codegen tasks,
- request compiler emission inputs lazily rather than execute earlier phases through workflow commands,
- coordinate backend-specific serialization without inspecting backend modules,
- publish package-interface artifacts without moving their encoding policy out of `bray-package-interface`,
- own deterministic output names, sinks, staging, atomic publication, and artifact bookkeeping,
- construct complete typed link plans without performing the final native link,
- preserve complete diagnostics when artifact construction or publication cannot continue,
- publish every required product artifact as one generation or publish none of that generation,
- publish no incomplete individual artifact and never report a partial product as complete.

---

## Non-Goals

The emitter does not:

- parse source, discover declarations, bind names, check semantics, lower HIR, or construct MIR,
- select reachable declarations, generic instances, implementations, or runtime behavior,
- translate Bray MIR into backend-specific instructions,
- inspect, mutate, or retain LLVM modules or another backend's private IR,
- define LLVM optimization pipelines or perform backend-specific serialization itself,
- encode `.brayi` records or decide which semantic emission inputs belong in a package interface,
- discover native libraries by reading source directives,
- implement or invoke the final linker directly,
- make the command driver enumerate compiler phases,
- publish partial success as a completed product.

---

## Terminology

### Emission Request

An `EmissionRequest` describes the product and external artifacts requested by the compiler host. It contains typed
product, target identity, destination, artifact, and replacement intent.

Target-output emission inputs and compilation diagnostic policy remain emission inputs of their owning target and
compilation contexts. Planning requests those emission inputs rather than duplicating them in the host's artifact
request.

It does not contain mutable compiler state or backend-private values.

### Emission Plan

An `EmissionPlan` is the complete validated immutable plan derived from one request and the selected product, target,
backend capabilities, completed package-interface artifact, and completed implementation bundle when the product
requires one.

It fixes artifact identities, required and optional artifact kinds, logical ordering, output names, sinks, per-unit
backend artifact requests, package-interface output, staging requirements, and prospective link outputs before backend
emission or publication begins.

The planning boundary is an immutable `EmissionPlanner` composed from target-output emission inputs, an optional
selected backend with its stable codegen-unit keys and output policy, an optional completed `InterfaceArtifact`, and an
optional completed `ImplementationBundle` for the selected product. Planning consumes one `EmissionRequest` and returns
either the complete `EmissionPlan` or a typed planning error. Callers do not preassemble planned artifacts or backend
artifact requests.

### Artifact Contribution

An artifact contribution is immutable content produced for one planned artifact identity. Backend contributions come
from codegen units. Package-interface contributions come from `bray-package-interface`. Runtime or compiler-provided
contributions come from their owning compilation emission inputs.

### Output Sink

An output sink is one of the closed typed destinations selected by the plan: a managed filesystem product root, a
host-provided transactional stream, or an in-memory transactional collector. A separately requested inspection artifact
may use an explicit filesystem artifact path.

A sink defines where bytes go, not what those bytes mean.

Memory collectors and streams are resolved only at publication through a host-provided transactional write boundary.
Bytes written to that operation remain hidden until an explicit commit succeeds. Dropping an uncommitted operation
discards its buffered bytes, so an indirect sink cannot expose a partial artifact after a write, flush, cancellation, or
commit failure.

### Emitted Artifact

An `EmittedArtifact` records one successfully completed external artifact, including its identity, kind, destination,
byte length, content digest, producer identity, and relationship to a product or link plan.

### Emission Outcome

An `EmissionOutcome` contains complete emitted artifact records, the `PublishedProductGeneration` when product
publication succeeds, an optional link plan, and the emission-owned diagnostic bag. `PublishedProductGeneration` records
the stable generation identity, manifest digest, publication-reference identity, and resolved artifact records. A
cancelled or failed outcome exposes no product-level success claim.

---

## Lifecycle

The logical lifecycle is:

```text
EmissionRequest
    -> request completed package-interface and implementation-bundle artifacts selected by the product
    -> validate product, target, backend capabilities, interface identity, and destinations
    -> freeze immutable EmissionPlan
    -> request required package and semantic diagnostics
    -> request planned MIR and codegen-unit emission inputs
    -> backend constructs task-local modules
    -> backend serializes requested artifact contributions
    -> validate and merge contributions in plan order
    -> stage required contributions
    -> construct immutable LinkPlan when required
    -> invoke bray-linker through the product emission operation
    -> validate the complete product result
    -> atomically publish the complete product generation
    -> return EmissionOutcome
```

The emitter is a lifecycle orchestrator. Orchestration does not transfer ownership of earlier compiler semantics into
the emitter. It requests typed completed emission inputs and applies emission-owned policy to them.

The lifecycle can logically bracket code generation without sharing a mutable emission session. `EmissionPlan` is
immutable. Each codegen task receives only its derived immutable request and publishes one immutable contribution.

---

## Compilation Integration

`bray-compilation` owns the top-level product emission operation because it owns lazy emission input coordination,
cancellation, and deterministic diagnostic aggregation.

Conceptually, its public boundary is:

```rust
pub fn emit(
    &self,
    request: &EmissionRequest,
    emitter: &Emitter,
    linker: &Linker,
) -> EmissionOutcome;
```

This is a design shape rather than a requirement to preserve the exact signature.

The operation asks the emitter to create the plan, requests the compilation emission inputs named by that plan, returns
immutable contributions to the emitter, invokes `bray-linker` when a link plan exists, and asks the emitter to publish
the linked staging result.

The operation does not manually call parser, binder, checker, lowering, and codegen entry points. It asks for final
typed emission inputs, and those emission inputs request their own dependencies.

The command driver performs one product emission request and renders the resulting merged diagnostics. It does not
implement the lifecycle itself.

Emission is effectful and is not cached as if filesystem state were an immutable compiler emission input. The MIR,
codegen contributions, package-interface artifact, and other pure inputs remain lazy cached emission inputs. Repeating
emission can reuse them while reattempting publication against current external state.

---

## Immutable Planning

Planning completes before any output is created.

The planner validates:

- product kind and selected target,
- backend artifact capabilities,
- required linkable artifact kinds,
- requested optional inspection artifacts,
- package-interface eligibility,
- logical artifact identities and deterministic order,
- final output names and sink compatibility,
- path and sink collisions,
- overwrite and replacement policy,
- whether linked and non-linked outputs conflict,
- whether every required contribution has one producer.

Planning failure produces structured diagnostics and no staging files.

The plan uses typed artifact identities rather than paths as cross-task keys. Paths and streams are sink properties
resolved only by the emitter.

An emission plan is `Send` and `Sync`. It contains no mutable output stream, backend module, temporary file handle,
linker process, or compilation cache reference.

---

## Backend Interaction

For every planned codegen unit, the emitter derives a backend-neutral `BackendArtifactRequest` owned by `bray-codegen`.

The request identifies:

- required artifact kinds,
- optional artifact kinds enabled by the request,
- debug-information output mode,
- whether linkable object or bitcode content is required,
- logical contribution identities,
- serialization options already validated against backend capabilities.

It does not contain final paths or writable sinks. Backend workers produce immutable content contributions so
serialization can remain task-local and independent of filesystem publication.

`bray-codegen-llvm` physically serializes LLVM IR, bitcode, assembly, object, and LLVM-owned debug content because only
it understands LLVM modules and target machines. The emitter owns why those formats were requested and where the
resulting contributions are published.

Verification, optimization, target-machine setup, and backend metadata finalization remain backend responsibilities. The
emitter does not call instruction-level backend APIs.

---

## Package Interface And Implementation Integration

`.brayi` and `.brayimpl` are independently constructed artifact contributions that participate in the product
publication transaction.

`bray-package-interface` owns:

- public semantic surface construction contracts,
- deterministic record and section encoding,
- format and semantic revision handling,
- semantic content and artifact hashes,
- the immutable completed `InterfaceArtifact`.

The same crate owns the corresponding `.brayimpl` template selection, encoding, revision, hashing, and immutable
completed `ImplementationBundle` contracts defined in `compiled-package-interfaces.md`.

`bray-emitter` owns:

- whether the selected product request includes `.brayi` and `.brayimpl`,
- its logical artifact identity and deterministic output name,
- its output sink,
- validation that the completed interface artifact matches the planned product,
- staging, atomic publication, emitted length and digest bookkeeping,
- publication diagnostics.

Neither artifact passes through `CodeGenerator` or participates in the native link plan, and the emitter never rebuilds
either one.

Library emission publishes these artifacts only as part of a complete validated product generation. A standalone
independent inspection request still obtains every compilation diagnostic required by its package-interface or
implementation-bundle contract.

---

## Artifact Policy

The complete externally requestable artifact taxonomy is:

- backend IR inspection output,
- backend opaque representation,
- assembly,
- relocatable native object,
- directly executable target module,
- codegen-owned debug companion data,
- compiled package interface,
- compiled implementation bundle,
- compiler-owned dependency metadata,
- final executable,
- static library,
- shared library,
- target-required linked companion artifacts.

A backend can add a namespaced inspection kind through the typed extension contract in `codegen.md`. The emitter accepts
it only for the same selected backend identity and never substitutes it for a required backend-neutral kind. Adding a
compiler, linked, or product artifact kind requires extending this closed taxonomy, capability negotiation, naming,
publication, and manifest encoding together.

The emitter validates kinds against the selected backend, target, and product. An unavailable required format produces a
structured capability diagnostic. An unavailable optional format is omitted deterministically rather than turning the
complete request into a failure.

One emission plan contains at most one linked product. Its target-required linked companion, when requested, belongs to
the same typed linker operation. Staged link inputs preserve the linked product's required or optional completion
contract.

Intermediate objects needed only for linking are planned staging artifacts. They are not published as user-visible
outputs unless the request explicitly asks for object artifacts.

Inspection artifacts do not become required inputs to later compiler phases merely because they can be emitted.

---

## Naming And Layout

The emitter owns deterministic external artifact naming and product layout.

Names derive from typed package, product, target, artifact-kind, and codegen-unit identities. They do not derive from
worker order, memory addresses, temporary names, or compilation-local numeric IDs whose assignment depends on lazy
demand.

Target-specific extensions and naming conventions are typed target-output emission inputs. They are not raw strings
assembled by codegen. `bray-target` represents these as a target identity, machine properties, and stable per-artifact
prefix and suffix rules. The emitter validates product-derived filename stems and applies those rules while resolving
final sinks.

Explicit user-selected output names and target-derived final filenames are validated against host filename rules before
use. They must not create path traversal, filesystem-equivalent sink collisions, ambiguous multi-artifact destinations,
or incompatible extensions without a deliberate target policy.

Temporary filenames are private implementation details and need not be stable, but they must not enter emitted content,
content digests, diagnostics ordering, or cache keys.

---

## Product Staging And Atomic Publication

Every filesystem product is published through one managed generation under `<output-root>/.bray/`. The emitter creates a
private generation directory on that filesystem, stages every required compiler, backend, linker, interface,
implementation bundle, and companion artifact there, and writes a stable generation manifest containing their logical
identities, private and stable public relative paths, lengths, digests, permissions, producer identities, and
relationships. Each requested product artifact is also projected to its deterministic public filename beneath the
host-selected public directory.

The publication sequence is:

1. Validate the complete plan, managed root, replacement policy, and every normalized relative path.
2. Create a unique private generation that cannot replace an existing path.
3. Write or link every required artifact into that generation.
4. Close producers, flush content, and validate kinds, lengths, digests, permissions, and cross-artifact relationships.
5. Write, flush, and validate the stable generation manifest.
6. Hash the normalized manifest to obtain the generation identity, make the generation immutable to emitter writers, and
   place it at its content-addressed generation identity with atomic no-replace publication.
7. While holding the product publication lock, prepare and atomically replace each stable public artifact, retaining
   rollback copies until the transaction commits. Remove public paths absent from the replacement manifest through
   the same rollback set.
8. Atomically replace the product's private published-generation reference with a reference to the complete generation.
9. Retain the current and immediately preceding generation, then remove older validated generation directories.
10. Record the product and all emitted artifacts from the now-published manifest.

Ordinary users and automation consume the invariant stable public path directly after the build command completes. A
reader that can race another publisher holds the shared product publication lock while opening stable paths. Bray Tack
follows this protocol for run and test execution. The emitter holds the exclusive lock across public projection,
reference replacement, rollback, and retention. This gives readers one complete public path set and prevents two
compiler processes from interleaving one product transaction. Compiler readers also validate the projection through the
private published-generation reference and manifest.

The private reference observes either the preceding complete generation or the replacement complete generation. Each
public file replacement is independently atomic inside the locked transaction. If a later projection or the reference
commit fails, the emitter restores every public file already replaced. A host filesystem that cannot provide the
required same-filesystem atomic operations does not support managed product publication and produces a structured
capability diagnostic. A require-absent request uses atomic no-replace operations or fails.

If the content-addressed generation already exists, the emitter validates its manifest and artifact identities and
reuses it only when they match exactly. A collision or mismatched existing generation is a publication failure. The
published-generation reference contains only the generation identity and normalized manifest digest, never private
staging paths.

Cancellation or failure before step 8 restores the preceding stable public files and leaves the preceding generation
reference visible while removing only explicitly recorded private staging state. Cancellation after the atomic reference
replacement cannot retract the published generation. New Unix artifacts use artifact-appropriate modes subject to the
process umask. The manifest makes retention explicit. Cleanup considers only validated content-addressed directories in
the product's private generation store. It keeps the current and preceding generation so a reader that observed the
former reference can finish validation. Stable public names come from those manifests, and replacement removes names
absent from the new generation. Cleanup never infers ownership from unrelated files in public output directories.

A separately requested inspection output that is explicitly marked independent of product success may use per-artifact
atomic publication. Transactional streams and memory collectors expose the complete product generation only when their
single host commit succeeds. Neither path can publish a partial product.

---

## Link-Plan Construction

The emitter constructs the typed `LinkPlan` owned by `bray-linker` after every required link input has been emitted or
staged.

The plan is assembled from already resolved compilation and target properties:

- emitted object or bitcode inputs,
- product kind and staged linked-output destination,
- entry point,
- startup and termination objects,
- runtime libraries,
- selected async runtime identity, artifact, ABI versions, root entry stub, execution-lane requirements, and capability
  metadata when applicable,
- product-host cleanup-report sink ABI identity when reachable cleanup can produce suppressed entries, independently of
  async runtime selection,
- native and system libraries,
- exported symbols and visibility requirements,
- search paths supplied by package and target configuration,
- platform linker options represented through typed policy,
- debug and companion output requirements.

The emitter does not rediscover these requirements from source directives. It maps validated emission inputs to emitted
paths and validates that every plan input exists in the emission plan.

Async runtime requirements arrive as already resolved product emission inputs defined by `docs/design/async-runtime.md`.
The emitter does not select a runtime, infer requirements from MIR, or locate runtime components by source-level names.

`.brayi` is excluded from the native link plan.

---

## Parallelism And Determinism

Package-interface construction may compute in parallel with the target and backend emission inputs needed for planning,
but the completed `InterfaceArtifact` must exist before the immutable plan is published. Independent codegen
contributions can compute in parallel after plan publication.

Each worker owns its mutable backend or encoding construction state. Artifact contributions are immutable before they
enter the emitter merge.

The emitter validates, stages, records, and diagnoses artifacts in deterministic plan order rather than worker
completion order. Independent byte writes may execute concurrently when their sinks and publication operations do not
conflict.

Linking begins only after all required link inputs and the complete link plan are available.

Serial and parallel emission of equivalent inputs must produce equivalent output bytes, names, link plans, artifact
records, and diagnostic order.

---

## Diagnostics

Emitter diagnostics describe artifact requests, output layout, sinks, staging, publication, and artifact consistency.

They include typed arguments such as:

- product and artifact identity,
- artifact kind,
- target identity,
- output path or sink identity,
- expected and actual length or digest,
- replacement policy,
- operating-system error category.

Emitter logic does not construct user-facing English. Messages are rendered through `bray-messages`.

Backend generation diagnostics remain owned by codegen. Interface encoding diagnostics remain owned by
`bray-package-interface`. Link invocation diagnostics remain owned by `bray-linker`. The product emission operation
merges those bags deterministically without relabeling them as emitter diagnostics.

External paths and tools usually have no Bray source span. Diagnostics must not invent one.

---

## Cancellation And Recovery

Cancellation is observed before planning publication, before requesting expensive contributions, between independent
staging operations, before linking, and before final publication.

Cancellation publishes no new product success. Already completed pure compiler emission inputs remain reusable.

Ordinary source errors prevent successful product emission after their owning diagnostics are collected. The emitter
does not emit recovery objects or malformed package interfaces as successful products.

Filesystem and linker failures are recoverable compiler-operation failures. They produce structured diagnostics and
leave the compilation usable for another request.

---

## Testing Strategy

Emitter tests should cover:

- deterministic plans for equivalent requests,
- backend capability and artifact-kind validation,
- per-unit artifact request derivation,
- deterministic contribution merging under reversed and parallel completion,
- `.brayi` inclusion without backend involvement,
- deterministic names and collision rejection,
- stream and filesystem sinks,
- atomic replacement and staging cleanup,
- cancellation before and during publication,
- digest and length mismatch rejection,
- complete link-plan construction,
- structured diagnostics without invented source spans,
- repeated emission using reused pure compilation emission inputs.

Tests should use injected sinks and temporary destinations. They must not require a production linker merely to validate
emitter planning and publication.

---

## Dependency And Conformance Order

Delivery follows this dependency order. Every completed step must use the final contracts and ownership boundaries
defined above:

1. Define artifact identities, kinds, requests, plans, contributions, records, and outcomes.
2. Implement deterministic planning and destination collision validation.
3. Implement byte and stream publication with structured diagnostics.
4. Implement managed generation staging, manifest validation, and atomic product publication.
5. Integrate completed `.brayi` and `.brayimpl` artifacts without moving encoding policy.
6. Derive backend artifact requests and merge immutable codegen contributions.
7. Construct typed link plans from emitted and stable compilation inputs.
8. Integrate linked staging results and complete product emission outcomes.
9. Add cancellation, parallelism, determinism, hostile-path, and failure tests.
