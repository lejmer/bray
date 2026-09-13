# Code generation and backend design

Code generation translates validated Bray MIR into backend-specific low-level IR and requested artifact content. LLVM is
the primary conforming native backend. Other backends consume the same execution model through a coarse typed contract.

## Ownership and composition

`bray-codegen` owns backend selection, capabilities, concrete instance collection, unit partitioning, prepared mappings,
and artifact contracts. `bray-codegen-llvm` owns LLVM bindings and mutable LLVM state. `bray-target` supplies the shared
target vocabulary.

Compilation receives backend services through the compiler composition root. It coordinates lazy requests without
depending on LLVM or downcasting backend state. Bray Tack invokes the compiler as a separate tool.

A backend generates a complete unit rather than implementing a universal instruction-builder interface. It creates
private low-level IR, finalizes metadata, verifies, optimizes, verifies again, and serializes the requested artifacts.
Mutable modules never cross the service boundary.

## Prepared semantics and mappings

Backend requests contain validated MIR and complete target-facing mappings. Semantic types, concrete definitions,
runtime roles, layouts, callable signatures, binary symbols, linkage, and source provenance are resolved before
translation.

Target ABI classification is shared preparation work. Backends realize its passing modes and attributes without
repeating language or target policy. Backend-private CPU features and machine controls must agree with the selected
language-visible target profile.

Capability selection covers the requested product, target, artifacts, runtime, optimization, debug, and reproducibility
needs. An unsupported requirement produces a typed failure rather than a silent substitution.

## Concrete instances and units

Reachability starts from concrete product roots and follows direct dependencies through bounded query work. Exact
substitutions remain available beside structural instance keys, since fingerprints cannot reconstruct semantics.

Partitioning belongs to backend-neutral codegen policy. Units balance optimization scope against independent scheduling
and reuse. Stable co-placement groups preserve required relationships, and stable MIR-based cost estimates avoid
timing-dependent partition decisions.

Partition policy and its bounds participate in artifact identity. Mutable backend residency and worker concurrency are
bounded separately from unit size. Local changes should invalidate a bounded set of units without changing external
semantic identity.

## Runtime execution

MIR carries explicit runtime-facing operations and checked frame, task, cleanup, and failure behavior. Direct await and
independent task start remain different execution mechanisms. The backend does not rediscover runtime declarations by
name or inject new language semantics.

Earlier phases select compatible protected-frame and runtime contracts. Code generation realizes their typed operations
and preserves the separate cancellation and lifecycle phases described in [async runtime](async-runtime.md).

## Artifacts and emission

An emitter-owned immutable plan supplies each unit's artifact request before serialization. Backend contributions have
logical identities and immutable content, without final paths or overwrite policy.

Objects needed for native linking and optional inspection outputs share this boundary. Content can be memory-backed or
spooled so large artifacts do not require duplicate in-memory copies.

The emitter combines backend contributions with independently encoded package interfaces and resolved link inputs. It
owns staging and publication, while the linker owns native invocation. Neither needs access to mutable backend IR.

## LLVM translation and optimization

LLVM state remains local to one generation task. Shared target initialization or configuration requires an explicit
thread-safe ownership contract.

Translation uses conservative valid LLVM operations unless prepared semantics prove a stronger assumption. LLVM
aliasing, provenance, initialization, arithmetic, and control-flow guarantees must not exceed the Bray operation's
guarantees. Optimizer metadata is evidence-backed, not inferred from convenient source shape.

Backend-neutral options express optimization and debug intent. LLVM pass selection stays within the backend, and
byte-affecting configuration participates in cache identity. The compatible LLVM revision is pinned by toolchain policy,
documented in [LLVM development setup](../contributing/llvm.md).

Debug generation consumes stable source mappings and explicit generated provenance. Richer variable or scope inspection
requires corresponding typed inputs rather than reconstruction from backend state.

## Determinism, cancellation and failure

Independent units use the compilation scheduler rather than a second backend scheduler. Stable identities and ordering
govern entities, constants, metadata, and artifact output. Host properties, timestamps, temporary paths, and worker
order do not define generated content.

Complete immutable outcomes are cacheable against MIR, target, backend revision, options, artifact requests, and runtime
dependencies. Cancellation or failure publishes no successful partial artifact set.

MIR and backend verification failures identify compiler boundaries. Unsupported capabilities, configuration errors,
library failures, and artifact failures retain their specific typed causes. External backend text can accompany a
structured diagnostic as tool detail.

## Related documents

- [Lowering and MIR](lowering.md)
- [Emitter](emitter.md)
- [Linker](linker.md)
- [Compiled package interfaces](compiled-package-interfaces.md)
