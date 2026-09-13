# Emitter design

`bray-emitter` owns the lifecycle from an artifact request to a published product generation. It plans output,
coordinates immutable contributions, stages artifacts, and publishes them together. Compilation supplies lazy compiler
results and coordinates the overall request.

## Planning before effects

An immutable emission plan fixes artifact identities, required and optional contributions, destinations, naming, and
publication policy before output begins. It derives the artifact request for each codegen unit and validates product,
target, producer, and sink compatibility.

Artifact identity is distinct from its path. Backend workers receive logical requests without writable sinks, so
generation remains independent of filesystem publication.

Emission is effectful and must observe current external state. Pure inputs such as codegen contributions and encoded
interfaces remain reusable query results, while publication is attempted against the destination each time.

## Contribution ownership

Codegen owns physical serialization of its private backend IR. `bray-package-interface` independently constructs
semantic interfaces and implementation bundles. The emitter includes their completed artifacts without inspecting MIR,
rebuilding metadata, or retaining backend modules.

Contributions become immutable before entering the emitter. Their content can be held in memory or spooled, with typed
identity and integrity information.

Required link inputs become available before the emitter constructs a complete typed link plan. The linker owns
invocation and writes to the staging destination. The emitter owns publication of the linked result and its companions.

## Product generations

A managed product is published as one complete generation. Private staging gathers all required outputs and a manifest
recording their identities, digests, and relationships. A published-generation reference identifies the committed set.

Stable public filenames provide the user-facing artifact paths. A product publication lock coordinates their replacement
with generation-reference publication. Readers that can race a publisher use the same coordination so they observe one
complete set.

Rollback preserves the previous public artifacts until commit succeeds. Content-addressed generations can be reused
after identity validation. Bounded retention keeps the current and preceding generation, using manifest-owned paths
rather than inferring ownership from nearby files.

The filesystem must support the atomic operations required by this model. A destination without those capabilities fails
explicitly. Transactional streams and memory collectors use an equivalent host commit boundary that hides partial bytes.

Independent inspection outputs can have their own publication boundary when explicitly requested. They do not weaken the
completeness of a product generation.

## Names, targets and links

Output names derive from stable product and artifact identity together with target-owned naming conventions. Temporary
names and worker order do not enter artifact contents or cache identity.

The emitter validates destinations and collisions. It maps resolved runtime, native-library, entrypoint, export, and
debug requirements to staged artifact paths without rediscovering source semantics.

Semantic package interfaces and implementation bundles participate in product publication but do not become native
linker inputs.

## Parallelism and diagnostics

Independent contributions can be produced concurrently after planning. The emitter merges and diagnoses them in plan
order. Independent writes can run concurrently when their sinks and transactions do not conflict.

Diagnostics retain the failure's owner: backend generation, interface encoding, link invocation, or emission. Filesystem
failures carry paths and typed causes without invented source locations.

Cancellation before commit leaves the prior product visible. A committed generation remains committed even if
cancellation arrives afterward. Failed publication does not invalidate completed pure compiler inputs or make the
compilation unusable for a later request.

## Related documents

- [Compiler architecture](compiler-architecture.md)
- [Code generation](codegen.md)
- [Compiled package interfaces](compiled-package-interfaces.md)
- [Linker](linker.md)
