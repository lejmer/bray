# Emitter design

This document defines the goal-state architecture for planning, serializing, staging, and publishing Bray compilation artifacts.

`docs/design/compiler-architecture.md` defines the compiler-wide phase and lazy fact model.

`docs/design/codegen.md` defines backend selection, codegen units, backend IR construction, and backend-specific serialization.

`docs/design/compiled-package-interfaces.md` defines `.brayi` construction, encoding, validation, and hashes.

`docs/design/linker.md` defines final native linker and archiver integration.

This document defines the emission lifecycle that coordinates those completed compiler facts into deterministic external artifacts.

---

## Goals

The emitter architecture should:

- own one coherent lifecycle from an emission request through published artifact records,
- let artifact policy be established before backend serialization begins,
- keep emission planning immutable and safe to share across parallel codegen tasks,
- request compiler facts lazily rather than execute earlier phases through workflow commands,
- coordinate backend-specific serialization without inspecting backend modules,
- publish package-interface artifacts without moving their encoding policy out of `bray-package-interface`,
- own deterministic output names, sinks, staging, atomic publication, and artifact bookkeeping,
- construct complete typed link plans without performing the final native link,
- preserve complete diagnostics when artifact construction or publication cannot continue,
- publish no incomplete individual artifact and never report a partial product as complete.

---

## Non-Goals

The emitter does not:

- parse source, discover declarations, bind names, check semantics, lower HIR, or construct MIR,
- select reachable declarations, generic instances, implementations, or runtime behavior,
- translate Bray MIR into backend-specific instructions,
- inspect, mutate, or retain LLVM modules or another backend's private IR,
- define LLVM optimization pipelines or perform backend-specific serialization itself,
- encode `.brayi` records or decide which semantic facts belong in a package interface,
- discover native libraries by reading source directives,
- implement or invoke the final linker directly,
- make the command driver enumerate compiler phases,
- publish partial success as a completed product.

---

## Terminology

### Emission Request

An `EmissionRequest` describes the product and external artifacts requested by the compiler host. It contains typed product,
target identity, destination, artifact, and replacement intent.

Target-output facts and compilation diagnostic policy remain facts of their owning target and compilation contexts. Planning
requests those facts rather than duplicating them in the host's artifact request.

It does not contain mutable compiler state or backend-private values.

### Emission Plan

An `EmissionPlan` is the complete validated immutable plan derived from one request and the selected product, target, backend
capabilities, and package-interface policy.

It fixes artifact identities, required and optional artifact kinds, logical ordering, output names, sinks, per-unit backend artifact
requests, package-interface output, staging requirements, and prospective link outputs before serialization begins.

The planning boundary is an immutable `EmissionPlanner` composed from target-output facts, an optional selected backend with its
canonical codegen-unit keys and output policy, and package-interface availability. Planning consumes one `EmissionRequest` and
returns either the complete `EmissionPlan` or a typed planning error. Callers do not preassemble planned artifacts or backend
artifact requests.

### Artifact Contribution

An artifact contribution is immutable content produced for one planned artifact identity. Backend contributions come from codegen
units. Package-interface contributions come from `bray-package-interface`. Runtime or compiler-provided contributions come from
their owning compilation facts.

### Output Sink

An output sink is a typed destination selected by the plan. Initial sinks include a filesystem artifact path and an explicitly
supplied writable stream or in-memory collector for embedding and tests.

A sink defines where bytes go, not what those bytes mean.

Memory collectors and streams are resolved only at publication through a host-provided transactional write boundary. Bytes written
to that operation remain hidden until an explicit commit succeeds. Dropping an uncommitted operation discards its buffered bytes,
so an indirect sink cannot expose a partial artifact after a write, flush, cancellation, or commit failure.

### Emitted Artifact

An `EmittedArtifact` records one successfully completed external artifact, including its identity, kind, destination, byte length,
content digest, producer identity, and relationship to a product or link plan.

### Emission Outcome

An `EmissionOutcome` contains complete emitted artifact records, an optional link plan, and the emission-owned diagnostic bag. A
cancelled or failed outcome exposes no product-level success claim.

---

## Lifecycle

The logical lifecycle is:

```text
EmissionRequest
    -> validate product, target, backend capabilities, and destinations
    -> freeze immutable EmissionPlan
    -> request required package and semantic diagnostics
    -> request planned MIR and codegen-unit facts
    -> backend constructs task-local modules
    -> backend serializes requested artifact contributions
    -> request completed package-interface artifact when planned
    -> validate and merge contributions in plan order
    -> stage required contributions
    -> construct immutable LinkPlan when required
    -> invoke bray-linker through the product emission operation
    -> validate the complete product result
    -> publish planned external artifacts in deterministic order
    -> return EmissionOutcome
```

The emitter is a lifecycle orchestrator. Orchestration does not transfer ownership of earlier compiler semantics into the emitter.
It requests typed completed facts and applies emission-owned policy to them.

The lifecycle can logically bracket code generation without sharing a mutable emission session. `EmissionPlan` is immutable. Each
codegen task receives only its derived immutable request and publishes one immutable contribution.

---

## Compilation Integration

`bray-compilation` owns the top-level product emission operation because it owns lazy fact coordination, cancellation, and
deterministic diagnostic aggregation.

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

The operation asks the emitter to create the plan, requests the compilation facts named by that plan, returns immutable
contributions to the emitter, invokes `bray-linker` when a link plan exists, and asks the emitter to publish the linked staging
result.

The operation does not manually call parser, binder, checker, lowering, and codegen entry points. It asks for final typed facts, and
those facts request their own dependencies.

The command driver performs one product emission request and renders the resulting merged diagnostics. It does not implement the
lifecycle itself.

Emission is effectful and is not cached as if filesystem state were an immutable compiler fact. The MIR, codegen contributions,
package-interface artifact, and other pure inputs remain lazy cached facts. Repeating emission can reuse them while reattempting
publication against current external state.

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

The plan uses typed artifact identities rather than paths as cross-task keys. Paths and streams are sink properties resolved only by
the emitter.

An emission plan is `Send` and `Sync`. It contains no mutable output stream, backend module, temporary file handle, linker process,
or compilation cache reference.

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

It does not contain final paths or writable sinks. Backend workers produce immutable content contributions so serialization can
remain task-local and independent of filesystem publication.

`bray-codegen-llvm` physically serializes LLVM IR, bitcode, assembly, object, and LLVM-owned debug content because only it
understands LLVM modules and target machines. The emitter owns why those formats were requested and where the resulting
contributions are published.

Verification, optimization, target-machine setup, and backend metadata finalization remain backend responsibilities. The emitter
does not call instruction-level backend APIs.

---

## Package-Interface Integration

`.brayi` is an independently produced artifact contribution.

`bray-package-interface` owns:

- public semantic surface construction contracts,
- deterministic record and section encoding,
- format and semantic revision handling,
- semantic content and artifact hashes,
- the immutable completed `InterfaceArtifact`.

`bray-emitter` owns:

- whether the selected product request includes `.brayi`,
- its logical artifact identity and deterministic output name,
- its output sink,
- validation that the completed interface artifact matches the planned product,
- staging, atomic publication, emitted length and digest bookkeeping,
- publication diagnostics.

The interface artifact does not pass through `CodeGenerator`, does not participate in the native link plan, and is never rebuilt by
the emitter.

Final library emission publishes `.brayi` only after the complete library product check required by emission succeeds. A standalone
interface artifact request still obtains all compilation diagnostics required by the package-interface contract.

---

## Artifact Policy

Initial externally requestable artifact kinds include:

- LLVM IR inspection output,
- LLVM bitcode,
- assembly,
- relocatable native object,
- codegen-owned debug companion data,
- compiled package interface,
- compiler-owned dependency metadata,
- final executable,
- static library,
- shared library,
- target-required linked companion artifacts.

The emitter validates kinds against the selected backend, target, and product. An unavailable required format produces a
structured capability diagnostic. An unavailable optional format is omitted deterministically rather than turning the complete
request into a failure.

One emission plan contains at most one linked product. Its target-required linked companion, when requested, belongs to the same
typed linker operation. Staged link inputs preserve the linked product's required or optional completion contract.

Intermediate objects needed only for linking are planned staging artifacts. They are not published as user-visible outputs unless
the request explicitly asks for object artifacts.

Inspection artifacts do not become required inputs to later compiler phases merely because they can be emitted.

---

## Naming And Layout

The emitter owns deterministic external artifact naming and product layout.

Names derive from typed package, product, target, artifact-kind, and codegen-unit identities. They do not derive from worker order,
memory addresses, temporary names, or compilation-local numeric IDs whose assignment depends on lazy demand.

Target-specific extensions and naming conventions are typed target-output facts. They are not raw strings assembled by codegen.
`bray-target` represents these as a target identity, machine properties, and canonical per-artifact prefix and suffix rules. The
emitter validates product-derived filename stems and applies those rules while resolving final sinks.

Explicit user-selected output names and target-derived final filenames are validated against host filename rules before use. They
must not create path traversal, filesystem-equivalent sink collisions, ambiguous multi-artifact destinations, or incompatible
extensions without a deliberate target policy.

Temporary filenames are private implementation details and need not be stable, but they must not enter emitted content, content
digests, diagnostics ordering, or cache keys.

---

## Staging And Atomic Publication

Filesystem artifacts are written to private staging paths on the destination filesystem whenever atomic replacement requires the
same filesystem.

The publication sequence is:

1. Validate the planned destination and replacement policy.
2. Create a private staging artifact.
3. Write the complete contribution or let the linker write its result to that staging path.
4. Close and flush the producer boundary.
5. Validate expected length, digest, and artifact kind where available.
6. Atomically promote or replace the final destination.
7. Record the completed emitted artifact.

Cancellation or failure removes private staging state and does not publish the planned artifact.

Atomicity is guaranteed per artifact. Product-wide atomic publication across unrelated filesystem paths is not claimed by ordinary
file replacement. A future managed-generation layout can provide a stronger product transaction without weakening the per-artifact
contract.

The emitter never deletes unrelated files or scans output directories to infer ownership. Cleanup uses explicit staging and emitted
artifact records.

---

## Link-Plan Construction

The emitter constructs the typed `LinkPlan` owned by `bray-linker` after every required link input has been emitted or staged.

The plan is assembled from already resolved compilation and target facts:

- emitted object or bitcode inputs,
- product kind and staged linked-output destination,
- entry point,
- startup and termination objects,
- runtime libraries,
- native and system libraries,
- exported symbols and visibility requirements,
- search paths supplied by package and target configuration,
- platform linker options represented through typed policy,
- debug and companion output requirements.

The emitter does not rediscover these requirements from source directives. It maps canonical facts to emitted paths and validates
that every plan input exists in the emission plan.

`.brayi` is excluded from the native link plan.

---

## Parallelism And Determinism

Independent codegen and package-interface facts can compute in parallel after the immutable plan is published.

Each worker owns its mutable backend or encoding construction state. Artifact contributions are immutable before they enter the
emitter merge.

The emitter validates, stages, records, and diagnoses artifacts in deterministic plan order rather than worker completion order.
Independent byte writes may execute concurrently when their sinks and publication operations do not conflict.

Linking begins only after all required link inputs and the complete link plan are available.

Serial and parallel emission of equivalent inputs must produce equivalent output bytes, names, link plans, artifact records, and
diagnostic order.

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
`bray-package-interface`. Link invocation diagnostics remain owned by `bray-linker`. The product emission operation merges those
bags deterministically without relabeling them as emitter diagnostics.

External paths and tools usually have no Bray source span. Diagnostics must not invent one.

---

## Cancellation And Recovery

Cancellation is observed before planning publication, before requesting expensive contributions, between independent staging
operations, before linking, and before final publication.

Cancellation publishes no new product success. Already completed pure compiler facts remain reusable.

Ordinary source errors prevent successful product emission after their owning diagnostics are collected. The emitter does not emit
recovery objects or malformed package interfaces as successful products.

Filesystem and linker failures are recoverable compiler-operation failures. They produce structured diagnostics and leave the
compilation usable for another request.

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
- repeated emission using reused pure compilation facts.

Tests should use injected sinks and temporary destinations. They must not require a production linker merely to validate emitter
planning and publication.

---

## Initial Implementation Order

The emitter should be implemented in this order:

1. Define artifact identities, kinds, requests, plans, contributions, records, and outcomes.
2. Implement deterministic planning and destination collision validation.
3. Implement byte and stream publication with structured diagnostics.
4. Implement filesystem staging and per-artifact atomic replacement.
5. Integrate completed `.brayi` artifacts without moving encoding policy.
6. Derive backend artifact requests and merge immutable codegen contributions.
7. Construct typed link plans from emitted and canonical compilation inputs.
8. Integrate linked staging results and complete product emission outcomes.
9. Add cancellation, parallelism, determinism, hostile-path, and failure tests.
