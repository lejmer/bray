# Linker design

This document defines the goal-state architecture for producing final native products from emitted Bray link inputs.

`docs/design/compiler-architecture.md` defines the compiler-wide phase and diagnostic model.

`docs/design/codegen.md` defines generation of backend-specific low-level IR and linkable object or bitcode artifacts.

`docs/design/emitter.md` defines artifact policy, staging, publication, and construction of typed link plans.

This document defines link-plan ownership, linker-driver selection, invocation, diagnostics, cancellation, and linked artifact
results.

---

## Goals

The linker architecture should:

- consume one complete immutable typed link plan,
- support an embedded linker such as LLD and configured system linkers through focused drivers,
- keep command construction and platform quirks out of codegen and the emitter,
- write final linked content only to emitter-owned staging destinations,
- produce structured diagnostics for tool discovery, invocation, and linker failures,
- preserve deterministic plan and argument ordering,
- support executables, shared libraries, static libraries, and target-specific companion outputs,
- observe cancellation without publishing a partial final artifact,
- remain independent of Bray syntax, bound nodes, MIR, and backend-private modules.

---

## Non-Goals

The linker does not:

- discover reachable code or generic instances,
- select runtime behavior or native dependencies from source directives,
- generate LLVM IR, bitcode, assembly, or object contents,
- encode package interfaces,
- choose product or artifact policy,
- assign user-facing output names,
- publish final destinations directly,
- reinterpret ABI, symbol, layout, or linkage semantics,
- parse linker text into invented Bray source diagnostics,
- act as a package manager or fetch external libraries.

---

## Terminology

### Link Plan

A `LinkPlan` is the complete immutable typed description of one native link or archive operation. `bray-linker` owns its contracts
and validation builder. `bray-emitter` constructs it from emitted artifacts and canonical compilation facts.

### Link Input

A link input is an emitted or staged object, bitcode module, archive, startup object, runtime library, native library, or another
target-supported input with explicit identity, kind, order, and provenance.

### Linker Driver

A linker driver translates a validated target-specific link plan into one embedded linker call or external tool invocation.

Drivers own tool-specific flags, response-file syntax, quoting, environment requirements, and output interpretation.

### Linked Artifact

A linked artifact is the validated staging result produced by a successful linker or archiver invocation. It is not a published
final output until `bray-emitter` includes it in an atomically published product generation.

---

## Link Plan

The link plan contains typed values for:

- product identity and linked product kind,
- selected target and object format,
- ordered object and bitcode inputs,
- ordered archives and native libraries,
- entry point,
- startup and termination objects,
- Bray runtime components,
- exported and retained symbols,
- library and framework search paths,
- target-defined subsystem and platform options,
- relocation, code, and link model,
- dead-stripping, section-garbage-collection, and whole-archive policy,
- debug and companion output requirements,
- emitter-owned staging output destinations,
- selected linker-driver identity and revision.

The plan does not contain raw command fragments supplied by source code. Source directives are resolved and validated into typed
link requirements before emission.

Plan construction rejects missing inputs, duplicate incompatible inputs, unsupported combinations, output collisions, invalid
target options, and a driver that cannot satisfy the selected target contract.

Input order is canonical where the platform permits it and language-defined where order affects linker semantics.

---

## Driver Selection

The selected target profile and compiler host provide the available linker drivers. `bray-linker` validates the requested or default
driver against the plan's target and product kind.

The driver categories are:

- embedded LLD,
- external LLD,
- configured platform system linker,
- static-library archiver,
- target-specific linker driver when a platform requires one.

The architecture does not require every compiler distribution to expose every driver. Missing required support is a structured
target-toolchain diagnostic.

Every driver publishes a typed immutable capability record covering target identities and object formats, product kinds, accepted
input kinds, produced artifacts and companions, startup and runtime contract ownership, export and retention policy, response-file
support, environment requirements, cancellation behavior, and determinism guarantees. Driver selection compares the complete link
plan with that record before staging or invocation. A category name, executable spelling, or successful probe cannot imply an
undeclared capability. Target-specific extensions use namespaced typed capability keys and cannot replace a required common
capability.

Driver identity and revision participate in linked-artifact cache or reproducibility metadata whenever they can affect output.

Arbitrary linker executables found on `PATH` are not silently treated as compatible. Tool discovery and compatibility policy are
explicit compiler-host inputs.

Native executable and shared-library drivers may invoke a platform compiler driver so the platform startup and C runtime contract
is supplied without hardcoding SDK objects or libraries into Bray. Linux targets use the GNU compiler-driver contract, Windows
targets use the Microsoft-compatible compiler-driver contract, and macOS targets use the Apple compiler-driver contract. Raw GNU,
Microsoft, and Apple linker command languages remain separate driver families for plans that explicitly provide their complete
startup contract.

Native runtime artifact components declare their ordered system-library and framework requirements as typed metadata. Link planning
preserves that order and any intentional duplicates while keeping each selected component distinct from its platform dependencies.
The runtime artifact builder derives these requirements from the selected Rust target so compiler logic does not rediscover
platform libraries or encode host-specific guesses.

Runnable native conformance uses a compiler host matching the selected target platform and architecture. Other supported targets
remain valid codegen, artifact, runtime-metadata, and link-plan targets and are checked structurally without pretending their
products can execute on the current host.

---

## Invocation Boundary

The conceptual linker service is:

```rust
pub trait LinkerDriver: Send + Sync {
    fn identity(&self) -> LinkerIdentity;

    fn supports(&self, target: &LinkTarget) -> bool;

    fn link(&self, plan: &LinkPlan, cancellation: &CancellationToken) -> LinkOutcome;
}
```

This is a design shape rather than a requirement to preserve the exact signature.

The driver receives a complete plan and writes only to its emitter-owned staging destinations. It returns linked artifact metadata,
structured diagnostics, cancellation, or failure. It does not publish final paths.

External process execution belongs behind an injectable process boundary so tests can inspect exact invocations without launching a
real linker.

The linker must not invoke a shell to interpret constructed command text. External tools receive an executable path and an explicit
argument vector. Response files use driver-owned deterministic encoding when command length or platform rules require them.

Each external-tool invocation also provides its complete child environment, optional working directory, and exact response-file
paths and bytes. The process boundary clears the inherited environment before applying the invocation environment, captures stdout
and stderr as uninterpreted tool-authored bytes, and returns stable host failure categories rather than host-authored prose.

Response files are created without replacing existing files and are removed after process completion, failure, or cancellation.
Their paths and contents are fixed before invocation and do not depend on process identity, thread scheduling, or environment
enumeration.

This boundary serves compiler-host tools only. It is separate from Bray source-level process declarations, task cancellation, and
process result contracts.

---

## Static Libraries

Static-library creation is part of the linker domain even when the platform tool is called an archiver rather than a linker.

The static-library driver consumes ordered object inputs and produces one archive staging artifact. It owns archive indexing,
deterministic member metadata, target archive format, and tool invocation.

Archive member timestamps, user IDs, group IDs, permissions, and names must be normalized where the format permits so equivalent
inputs produce deterministic output.

---

## Shared Libraries And Executables

Executable and shared-library plans explicitly identify their product kind, entry point, exports, runtime components, startup
objects, and platform options.

An executable or test plan that requires runtime services also identifies the selected runtime artifact, its exact selected
components, runtime ABI version, root entry stub, required lane facts, reactor and event features, and target/panic compatibility.
The linker validates those typed inputs against runtime artifact metadata. It does not choose a runtime or infer requirements from
unresolved symbols.

Every selected runtime component occurs once as a typed runtime input. All selected components must belong to the same runtime
artifact named by the executable-host contract, and the host contract's target must match the link target before invocation.

A synchronous-only product omits the async runtime unless another selected dependency explicitly requires it. The full selection and
ABI contract is defined in `docs/design/async-runtime.md`. The product-host cleanup-report sink is a separate typed startup and
termination service and remains linkable without an async scheduler when synchronous lifecycle or native-thread cleanup requires it.

An async executable link plan names the distinguished main-thread-lane entry and drive roles, the internal root
terminal-observation role, and structured shutdown ordering. Native-thread and child-process platform services used by ordinary
standard-library declarations are linked through their selected trusted product dependencies rather than becoming compiler-known
runtime symbols.

The linker does not infer an entry point from source names or object inspection. The semantic and product layers select it before
the plan is constructed.

Shared-library import libraries, export definition files, debug companions, and similar target outputs are separate typed staging
artifacts recorded in the link outcome and later published by the emitter.

---

## Package Interfaces

Compiled package interfaces are not linker inputs.

`.brayi` publication can accompany a library product, but the interface artifact is constructed independently by
`bray-package-interface` and staged by `bray-emitter` in the managed product generation.

The linker never inspects, rewrites, embeds, hashes, or validates `.brayi` content. A target-defined product container that carries
an interface is a separate typed linked companion produced from an emitter-supplied opaque artifact reference, not an ordinary
native link input.

---

## Staging And Publication

The emitter creates every linked output staging destination before invocation and records it in the plan.

Each destination carries both its exact writable path and an emitter-validated normalized path identity. The linker compares the
normalized identities when rejecting output collisions, including aliases that use different textual paths for the same host
filesystem destination.

The linker writes only to those staging destinations. It cannot choose or replace the final user-visible path.

After successful invocation, `bray-linker` validates that every required output exists and returns `LinkedArtifact` records with
kind, staging identity, observed length, and available tool metadata.

`bray-emitter` validates expected output relationships and includes the linked artifacts in the product generation transaction. If
invocation or validation fails, the emitter removes private generation state and preserves the preceding published generation.

---

## Diagnostics

Linker diagnostics describe:

- unavailable or incompatible linker drivers,
- missing planned inputs,
- unsupported product and target combinations,
- process creation and termination failures,
- response-file failures,
- linker exit status,
- missing or malformed linked outputs,
- cancellation and resource limits.

Diagnostics use stable message IDs and typed arguments such as target, driver, product, input artifact, native library, symbol,
output kind, and exit status. User-facing English is rendered through `bray-messages`.

External linker stdout and stderr are retained as explicitly labeled external-tool detail. They are not treated as localized Bray
diagnostic text and do not receive invented source spans.

When canonical symbol provenance is available, a linker diagnostic can relate an unresolved or duplicate external symbol to a Bray
declaration through typed related information. The linker does not reverse-engineer source locations from mangled names.

---

## Cancellation And Failure

Cancellation is checked before process creation, while waiting for an external tool, before validating outputs, and before returning
a successful result.

An external process boundary should terminate a cancellable child process when the platform contract permits it. If termination
cannot be guaranteed, a completed result is discarded after cancellation and never published.

Link failure leaves no published new final artifact. Staging cleanup is coordinated with the emitter.

Failure does not invalidate completed MIR, codegen artifacts, package-interface artifacts, or emission plans. A later request can
reuse those pure facts with corrected toolchain or destination state.

---

## Determinism

The same link plan, driver identity, tool revision, and input bytes should produce byte-equivalent linked artifacts where the target
toolchain supports deterministic output.

The driver must suppress or normalize timestamps, random identifiers, host paths, process identifiers, and environment-derived
metadata when supported.

Argument order, response-file contents, environment construction, and diagnostic ordering follow the immutable plan rather than
hash-map or filesystem enumeration order.

When a toolchain cannot provide deterministic output, the driver reports that capability explicitly. It must not claim reusable
deterministic artifacts.

---

## Parallelism

Independent products can link concurrently subject to compiler process and I/O budgets.

One link plan is executed once. Drivers must not create hidden compiler-owned worker pools that violate configured resource policy.

External linker and archiver invocations acquire a permit from one explicit nonzero compiler-host process budget. Permit acquisition
observes compiler-operation cancellation, and permit release follows child-process termination and reaping.

Linking begins only after all required inputs are complete. It does not block unrelated MIR, codegen, interface, or emission tasks
whose fact dependencies are ready.

---

## Testing Strategy

Linker tests should cover:

- exact typed plan validation,
- deterministic input and argument ordering,
- embedded and external driver selection,
- response-file encoding and quoting,
- executable, shared-library, and static-library plans,
- target-specific companion outputs,
- missing tool and process failures,
- external stdout and stderr attachment,
- cancellation and child-process termination behavior,
- missing linked output rejection,
- emitter-owned staging and publication boundaries,
- deterministic repeated invocations with a controlled test driver,
- exclusion of `.brayi` from native link inputs.

Most tests should use recording drivers and process doubles. A smaller target-gated integration suite can invoke supported production
linkers.

---

## Dependency And Conformance Order

Delivery follows this dependency order. Every completed step must use the final contracts and ownership boundaries defined above:

1. Define typed plan, input, product, driver identity, outcome, and linked artifact contracts.
2. Implement deterministic plan validation and ordering.
3. Add an injectable external process boundary and recording test driver.
4. Implement each supported native linker driver against the complete capability contract.
5. Implement deterministic static-library archiving.
6. Integrate emitter-owned staging and linked artifact publication.
7. Add structured diagnostics, cancellation, response files, and companion outputs.
8. Add target-gated end-to-end executable and library tests.
