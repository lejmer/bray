# Code generation and backend design

This document defines the goal-state architecture for translating validated Bray MIR into backend-specific low-level IR and
emitter-requested backend artifacts.

The language documents define Bray semantics, target profiles, layout, ABI, and linkage behavior.

`docs/design/compiler-architecture.md` defines the compiler-wide phase, query, ownership, and publication model.

`docs/design/binder.md` and `docs/design/checker.md` define how source semantics become complete checked facts.

`bray-lowering` makes implicit execution behavior explicit and constructs the backend-independent representation owned by
`bray-ir`.

This document defines the backend boundary, the first LLVM implementation, codegen-unit contracts, artifact requests, backend IR
construction, serialization, determinism, diagnostics, and the boundary with emission.

---

## Goals

The code generation architecture should:

- use LLVM as Bray's first production code generation backend,
- produce native ahead-of-time artifacts as the first product model,
- keep LLVM types and policy isolated from backend-independent compiler crates,
- allow another backend to consume the same validated Bray MIR without reimplementing language semantics,
- expose coarse typed backend contracts instead of reproducing LLVM's instruction-building API,
- support lazy codegen-unit facts, independent parallel generation, cancellation, and deterministic reuse,
- preserve target, ABI, symbol, and layout decisions made by earlier compiler phases,
- construct semantically complete backend IR before serialization,
- serialize only the artifact kinds selected by an immutable emitter-owned plan,
- publish immutable backend artifact contributions without choosing final output paths or invoking the linker,
- report structured backend and target diagnostics without user-facing English in codegen logic,
- make backend failures distinguishable from invalid source and compiler invariant failures.

---

## Non-Goals

Code generation does not:

- parse syntax, discover declarations, construct symbols, bind names, or check source semantics,
- infer types, select overloads or implementations, or decide ownership and borrowing behavior,
- decide source-level layout, ABI, linkage, panic, async, or runtime semantics,
- own source graph selection, product policy, output directories, final artifact names, or overwrite policy,
- invoke linkers or publish files to final output locations,
- expose LLVM IR as Bray's durable backend-independent representation,
- promise a runtime-loadable plugin ABI for third-party backends,
- define a lowest-common-denominator instruction builder shared by every backend,
- preserve partially generated artifacts after cancellation or failure.

---

## Terminology

### Codegen Backend

A codegen backend translates one validated backend-independent Bray MIR unit into semantically complete backend-specific low-level
IR and serializes the artifact kinds requested for that unit.

The backend may use an internal representation such as LLVM IR. That representation remains private to the backend
implementation.

### Codegen Unit

A codegen unit is the smallest independently generated and cached backend work item. It contains a closed validated MIR surface,
its external references, selected target contract, and code generation options.

A codegen unit is not a source file, syntax tree, module declaration, or arbitrary collection selected by a backend.

### Backend Artifact

A backend artifact is immutable content produced by code generation, such as a relocatable object, assembly listing, backend IR
inspection output, or a directly executable target module.

A backend artifact has a logical identity and kind but no final filesystem path.

### Backend LIR

Backend-specific low-level IR, abbreviated LIR, is the private executable representation constructed from Bray MIR by one codegen
backend. LLVM IR is the first backend LIR.

Backend LIR is not a durable backend-independent compiler representation. Its types and mutable state remain inside the concrete
backend that understands and serializes it.

### Backend Module

A backend module is task-local mutable state used while translating one codegen unit into backend-specific low-level IR. LLVM
modules are the first implementation.

Backend modules are not durable compiler facts, do not cross the backend boundary, and are never shared between workers. The
backend consumes them internally while producing immutable artifact contributions.

### Backend Identity

A backend identity is the stable compiler-facing identity of one backend implementation and revision. It participates in codegen
fact keys and cache compatibility.

For the LLVM backend, it includes the Bray LLVM backend revision and the compatible LLVM revision used to produce artifacts.

---

## Pipeline Boundary

The code generation boundary is:

```text
checked bound HIR and durable semantic facts
    -> lowering
    -> validated Bray MIR owned by bray-ir
    -> backend codegen unit
    -> task-local backend-specific low-level IR
    -> emitter-requested backend serialization
    -> immutable backend artifact contributions
```

An immutable emission plan is created before backend serialization. It selects required and optional artifact kinds, derives a
typed request for each codegen unit, and later determines output names and sinks. It does not expose mutable backend state.

Every language-semantic decision required to generate code must be explicit before the backend receives a unit.

If a backend needs to resolve a name, infer a type, select an implementation, reconstruct cleanup behavior, or reinterpret a
source construct, the earlier compiler contract is incomplete.

Backend-specific legalization may transform operations to satisfy target instruction and object-format constraints. It must
preserve semantics already fixed by Bray MIR and the selected target contract.

---

## Crate Ownership

### `bray-target`

`bray-target` owns stable target identities and machine-model values shared by backend-neutral output phases. This includes
architecture, object format, byte order, relocation model, code model, and validated machine properties. It remains independent of
syntax, bound HIR, MIR, codegen policy, emission, and linking.

### `bray-codegen`

`bray-codegen` owns backend-independent code generation contracts:

- backend selection against implementations supplied by the compiler host,
- backend identity and capability types,
- codegen-unit keys and immutable requests,
- reachable concrete monomorphized-instance and unit-partitioning policy,
- packaging of canonical layout, ABI, symbol, target, runtime, and linkage facts,
- backend-neutral code generation options,
- backend artifact request and contribution types,
- backend artifact kinds and immutable artifact sets,
- codegen outcomes and structured diagnostic contracts,
- backend conformance contracts,
- codegen orchestration that does not depend on one backend implementation.

It depends on `bray-ir` and lower foundational contracts. It must not depend on LLVM.

### `bray-codegen-llvm`

`bray-codegen-llvm` owns the first production backend implementation:

- LLVM binding dependencies,
- LLVM contexts, modules, builders, types, values, metadata, and target machines,
- translation from validated Bray MIR to LLVM IR,
- LLVM-specific legalization and optimization pipelines,
- LLVM module verification,
- object, assembly, LLVM IR, and bitcode generation when requested,
- conversion of LLVM failures into backend-neutral structured outcomes.

No other Bray crate may import LLVM bindings or expose LLVM-owned types through a compiler phase contract.

### `bray-compilation`

`bray-compilation` owns lazy codegen fact coordination, cache keys, dependency scheduling, worker budgets, cancellation, and the
selected backend service for one compilation request.

It receives available backend implementations through immutable backend-neutral service contracts, asks `bray-codegen` to validate
the requested selection, supplies validated MIR units and emitter-derived artifact requests, and caches only complete immutable
outcomes. It must not depend on `bray-codegen-llvm`.

### `bray-emitter`

`bray-emitter` owns emission requests and plans, artifact policy, output names and sinks, deterministic publication, emitted-product
records, and link-plan construction.

It constructs backend artifact requests and consumes immutable backend artifact contributions. It must not inspect LLVM modules,
LLVM target machines, Bray syntax, bound nodes, MIR, or semantic stores.

### `bray-linker`

`bray-linker` owns typed link plans, target linker drivers, invocation, linker diagnostics, and linked artifact results. It consumes
emitted objects or bitcode and does not depend on LLVM module state or Bray MIR.

---

## Dependency Direction And Composition

The core dependency direction is shown below. Arrows point from a dependency to its consumer.

```text
bray-ir -------------------> bray-codegen
bray-target ---------------> bray-codegen
bray-target ---------------> bray-emitter
bray-target ---------------> bray-linker
bray-codegen --------------> bray-codegen-llvm
bray-codegen --------------> bray-emitter
bray-package-interface ----> bray-emitter
bray-linker ---------------> bray-emitter
bray-codegen --------------> bray-compilation
bray-emitter --------------> bray-compilation
bray-linker ---------------> bray-compilation
bray-compilation ----------> bray-driver
bray-codegen-llvm ---------> bray-driver
```

`bray-driver` or another compiler host is the composition root. It constructs the available backend implementations and supplies
backend-neutral service handles and the requested backend identity to `Compilation`. `bray-codegen` validates the selection.

`Compilation` treats available backend identities and capabilities as immutable request inputs. Its lazy codegen facts use the
selection made through `bray-codegen`, accept emitter-derived artifact requests, and do not downcast a service or inspect LLVM state.

This is dependency injection at a coarse compiler boundary, not a promise that arbitrary binary backend plugins can be loaded at
runtime. A new in-tree backend implements the same contract and is selected by the composition root without changing Bray MIR,
compilation queries, or emission APIs.

`bray-codegen-llvm` depends on `bray-codegen` and `bray-ir`. It must not depend on `bray-compilation`, `bray-emitter`, or command-line
or package orchestration crates.

---

## Backend Contract

The backend API should remain coarse and typed. Its conceptual shape is:

```rust
pub trait CodeGenerator: Send + Sync {
    fn identity(&self) -> BackendIdentity;

    fn capabilities(&self) -> BackendCapabilities;

    fn generate(&self, request: CodegenRequest<'_>) -> CodegenOutcome;
}
```

This is a design contract, not a requirement to preserve these exact method signatures if more precise typed boundaries emerge.

`CodegenRequest` contains only the inputs required to generate one unit:

- its stable codegen-unit key,
- a validated immutable Bray MIR view,
- a validated codegen target,
- backend-neutral generation options,
- an immutable backend artifact request derived from the emission plan,
- a read-only cancellation contract.

`CodegenOutcome` contains:

- a complete immutable artifact set when generation succeeds,
- a deterministic diagnostic bag,
- explicit cancellation or internal-failure state,
- no partially published artifact set after cancellation or failure.

The backend contract must not expose an instruction-level virtual interface such as generic `build_add`, `build_load`, or
`build_branch` methods. Each backend owns translation from Bray MIR into its own internal representation.

`generate` conceptually brackets the complete task-local backend lifecycle:

```text
create backend module
    -> generate backend IR
    -> finalize debug and runtime metadata
    -> verify
    -> optimize
    -> verify
    -> serialize requested artifacts
    -> discard mutable module state
```

The public contract does not return a mutable type-erased `BackendModule`. Keeping that module private avoids foreign-handle
lifetime leaks, cross-worker mutation, downcasting, and cache entries that cannot be safely shared. The emitter still controls the
artifact lifecycle through its immutable request.

Backend capabilities are typed declarations of supported artifact and target features. They do not silently change language
semantics. Unsupported required capabilities produce structured diagnostics before artifact publication.

---

## Codegen Units

Codegen units should be large enough to optimize coherent code and small enough to schedule, cache, and regenerate independently.

Unit partitioning is codegen policy owned by `bray-codegen`, not a concrete backend. The LLVM backend must not repartition the
source package based on LLVM implementation convenience.

`bray-codegen` requests canonical compilation facts to collect reachable concrete monomorphized instances and package them into
units. It does not rediscover reachability from syntax, reinterpret directives, or make semantic instance selections.

Each unit must have:

- a stable structural key independent of demand order and worker assignment,
- a closed set of definitions generated by that unit,
- an explicit set of external symbol references,
- concrete target and ABI facts,
- fully lowered control flow, storage, cleanup, panic, and call behavior,
- no unresolved generic, overload, trait, or runtime-default decisions.

The initial partitioning policy may use package and declaration ownership boundaries. The key contract is that equivalent compiler
inputs produce equivalent unit membership regardless of source discovery order or parallel scheduling.

Changing partitioning policy invalidates affected codegen facts. It does not change program semantics or external symbol identity.

---

## Target Contract

The selected language-level target profile remains the authority for target facts visible to Bray programs.

Stable target identity and machine-model values come from `bray-target`. `bray-codegen` composes those shared values with ABI,
layout, symbol, compatibility, CPU, feature, and backend-selection facts in `CodegenTarget` rather than owning parallel copies.

Code generation also needs compiler-private machine configuration that does not belong under `target`, including backend
feature strings, object-format controls, relocation model, code model, and toolchain details. These values belong in a validated
`CodegenTarget` supplied by compilation, not in the language-visible target fact namespace.

`CodegenTarget` should contain typed values for:

- target identity and canonical target triple,
- architecture and object format,
- pointer and scalar representation facts,
- endianness and alignment,
- data layout and address spaces,
- supported callable ABI mappings,
- relocation and code model,
- CPU and enabled target features,
- symbol and linkage encoding rules,
- backend-required compatibility information.

Construction validates agreement between the language-level target profile and backend-private target configuration before any
unit reaches code generation.

The LLVM backend translates this validated contract into an LLVM target machine and data layout. LLVM must not independently
override a language-visible target fact.

---

## Layout, ABI, And Symbols

Source-level and public ABI layout decisions belong to language semantics and checking. Lowering and MIR make the selected layouts
and calling contracts explicit.

The backend performs mechanical target realization only:

- map validated Bray scalar and aggregate layouts to backend types,
- map Bray and foreign calling conventions to supported backend conventions,
- preserve required alignment, address spaces, linkage, and visibility,
- emit exact external symbol names selected by semantic and target policy,
- lower internal symbol identities through Bray's stable mangling contract.

Bray symbol mangling must be specified independently of LLVM. LLVM receives completed symbol names and must not derive names from
source strings, numeric compilation-local IDs, or module insertion order.

An unsupported layout or ABI mapping is a target or backend capability failure. It is not permission for the backend to choose a
different representation.

---

## Generics And Reachability

Code generation consumes concrete reachable instances selected by compilation, binding, checking, and lowering.

The backend does not:

- discover generic instantiations,
- select trait implementations,
- decide whether a declaration is reachable,
- materialize runtime-default providers from unchecked expressions,
- merge semantically distinct instances because their machine representation happens to match.

Deduplication based on canonical semantic identity may occur before code generation. Backend-level identical-code folding is an
optimization and must preserve externally observable identity, linkage, debugging, and address semantics.

---

## Runtime Boundary

Runtime entry points, compiler-known behavior roles, panic behavior, allocation hooks, async support, and lifecycle helpers must be
resolved to explicit MIR references before code generation.

Async MIR carries typed frame identities, resume states, direct-await composition, task start, cancellation, terminal publication,
current-run forwarding of observed `RunResult<T>`, cleanup-incident transfer, and checked phase-one broadcast and phase-two
lifecycle plans. Concrete and erased descriptors retain separate entry points for those phases. The backend must not lower every
async call as a task or mandatory heap allocation. Direct await has no task-control-block or scheduler semantics;
`Future<T>.start()` is the independent task-storage boundary. Async entrypoint lowering pins the host-owned root frame to the
distinguished main-thread lane and preserves the internal terminal root outcome through product shutdown.

Frame descriptor and runtime ABI lowering follows `docs/design/async-runtime.md`.

The LLVM backend may lower a known MIR operation to an LLVM intrinsic or a declared runtime call. That mapping is typed backend
policy and must have a conformance test.

The backend must not locate runtime declarations by source-level names or silently inject semantically significant runtime calls
that are absent from MIR and its target contract.

---

## Backend Artifacts

`bray-codegen` should model artifact kinds explicitly. Initial kinds include:

- relocatable native object,
- assembly inspection output,
- backend IR inspection output,
- backend bitcode or equivalent opaque backend representation,
- directly executable target module when a target backend produces one without native linking,
- codegen-owned debug companion data when it is not embedded in another artifact.

The emitter selects required and optional kinds through `BackendArtifactRequest`. The first native LLVM product path requires
relocatable objects for linking. Assembly, LLVM IR, and LLVM bitcode are optional requested outputs rather than mandatory durable
intermediates.

An artifact record contains:

- its codegen-unit key,
- typed artifact kind,
- immutable content or immutable content source,
- byte length when known,
- backend identity and target identity,
- deterministic content digest when the producing boundary requires one.

It does not contain a final output path, user-selected filename, linker command, or overwrite policy. Those belong to the emission
plan and link plan.

Artifact content APIs should support memory-backed content and compiler-owned immutable spooled content. The contract must not
require large object files to remain duplicated in memory merely to cross the emission boundary.

---

## LLVM Backend

LLVM is Bray's first production backend and may remain the primary backend. The architecture must still treat it as one
implementation of the backend contract.

The LLVM backend pipeline is:

```text
validated Bray MIR
    -> LLVM context and module construction
    -> LLVM IR verification
    -> optimization pipeline
    -> final LLVM verification
    -> emitter-requested target-machine and textual serialization
    -> immutable backend artifact contributions
```

LLVM contexts and mutable module construction state remain task-local for the full generate-and-serialize operation. No mutable LLVM
module is published as a compilation fact or shared between independent codegen workers.

LLVM target initialization and immutable target-machine data may be shared only when the LLVM API and Bray wrapper contract make
thread safety explicit. Otherwise each worker owns the required state.

LLVM constructs with stronger semantics than the corresponding Bray operation may be used only when earlier facts prove their
preconditions. In particular:

- `undef` and poison values must not represent source recovery, ordinary uninitialized storage, or an unknown semantic value,
- `inbounds`, `noalias`, `nonnull`, `noundef`, exactness, and integer no-wrap flags require explicit supporting facts,
- LLVM `unreachable` requires a Bray MIR control-flow proof that execution cannot reach that point,
- host pointer size, host CPU features, and host data layout must never replace the selected codegen target,
- panic and foreign-unwind behavior must follow explicit MIR control and ABI contracts,
- pointer provenance and address-space distinctions must survive translation.

The absence of a proven optimization fact means the LLVM backend emits the conservative valid form. It does not infer a stronger
fact from source shape or LLVM convenience.

Bray pins a compatible LLVM revision for each compiler release. The backend identity and cache compatibility contract include that
revision. The compiler must not load an arbitrary incompatible system LLVM at runtime and treat its output as cache-compatible.

The choice between static and dynamic LLVM distribution is packaging policy. It must not affect codegen semantics or artifact
identity except through the explicit backend identity when generated bytes can differ.

---

## Verification And Failures

Bray MIR is validated before code generation. Invalid Bray MIR is a compiler invariant failure, not an ordinary source diagnostic.

LLVM module verification runs before optimization and before artifact generation. A verifier failure from validated Bray MIR is a
compiler bug attributed to the LLVM translation boundary.

The backend should distinguish:

- unsupported target or requested artifact capability,
- invalid backend configuration,
- cancellation,
- resource exhaustion or worker-budget failure,
- backend library failure,
- generated-module invariant failure,
- artifact construction failure.

Ordinary invalid source must be rejected or represented through explicit recovery before code generation. The backend must not
panic because source recovery reached a later phase.

No failed or cancelled unit publishes a successful artifact set. Internal temporary state is discarded with the task.

---

## Lazy Facts And Caching

One codegen-unit artifact contribution is a lazy compilation fact. Requesting emission first creates an immutable emission plan and
then asks compilation for the unit contributions selected by that plan without making the caller sequence lowering, MIR
construction, backend generation, and serialization manually.

A codegen fact key includes every input that can affect output, including:

- stable codegen-unit identity and MIR dependency keys,
- target profile and validated codegen target,
- backend identity and compatible backend-library revision,
- optimization level and code generation options,
- debug-information mode,
- panic, relocation, and code model where applicable,
- exact emitter-derived artifact request,
- relevant runtime and external ABI dependencies.

Cache keys must not use memory addresses, task-local numeric IDs, worker assignment, or lazy demand order.

A cached result is complete and immutable. Cancellation and failed generation publish no reusable artifact set.

---

## Parallelism And Cancellation

Compilation schedules independent codegen units in parallel within its worker budget. A backend generates one unit per task and does
not own a second competing global scheduler.

Backend implementations must be `Send` and `Sync` at their immutable service boundary. Mutable module, builder, pass, and diagnostic
state remains local to one generation task.

Cancellation is checked:

- before backend work begins,
- between major translation and optimization phases,
- before expensive optional artifact generation,
- before publishing the completed outcome.

An underlying backend operation that cannot be interrupted may complete internally, but its result is discarded if cancellation
was observed before publication.

Parallel and serial generation of equivalent units must produce semantically and byte-for-byte equivalent required artifacts when
the selected object format supports deterministic output.

---

## Determinism

Required emitted backend artifacts must not depend on:

- source discovery order,
- hash-map iteration order,
- worker scheduling,
- LLVM object insertion order that is not derived from stable Bray order,
- process IDs, timestamps, temporary paths, or memory addresses,
- random backend seeds,
- host CPU features not present in the selected codegen target.

The LLVM backend assigns module entities, symbols, constants, metadata, and debug records from stable Bray ordering and identities.

If an object format or backend feature embeds nondeterministic data, the backend must configure, normalize, or reject that feature
for deterministic compiler outputs.

Determinism tests compare artifacts produced under serial, parallel, reversed-demand, and repeated generation.

---

## Optimization

Backend-neutral options describe intent through typed policy such as optimization level, size preference, debug mode, and overflow or
panic behavior already selected by compilation.

They do not expose arbitrary LLVM pass strings as stable compiler APIs.

LLVM pass pipeline selection belongs to `bray-codegen-llvm`. The selected pipeline and every option affecting generated bytes
participate in backend identity or codegen fact keys.

LLVM optimizations must remain valid under Bray's aliasing, provenance, initialization, panic, concurrency, and foreign-boundary
semantics. The backend may attach optimization metadata only when earlier checked and lowered facts prove the required invariant.

Optimization must not repair invalid MIR or make new language-semantic decisions.

---

## Diagnostics

Code generation emits structured diagnostics for target, backend, resource, and artifact-construction failures.

Codegen logic must not construct user-facing English. Diagnostic records use stable message IDs and typed arguments and are
rendered through `bray-messages`.

Backend diagnostics may include:

- backend identity and stage,
- target identity,
- codegen-unit identity,
- requested artifact kind,
- relevant source or semantic provenance when available,
- external backend detail as explicitly labeled machine or tool data.

Raw LLVM messages are not Bray diagnostic prose. They may be retained as backend detail for compiler developers while the primary
diagnostic remains structured and localized.

Codegen must not duplicate diagnostics already owned by binding, checking, lowering, or MIR validation.

---

## Emission Boundary

The emitter owns artifact policy before code generation serializes a backend module. It derives one immutable
`BackendArtifactRequest` per codegen unit from the complete emission plan.

Code generation owns the task-local backend lifecycle from MIR translation through verification, optimization, and physical
serialization. It returns immutable backend artifact contributions and discards the mutable backend module.

The emitter then assigns planned sinks, stages and publishes artifacts, records emitted metadata, and constructs a typed link plan.
It may combine:

- backend-generated object and debug artifacts,
- package-interface artifacts produced by `bray-package-interface`,
- runtime and external link inputs selected by package and target policy.

`bray-package-interface` constructs and encodes `.brayi` independently of codegen. The emitter requests that immutable artifact as a
compilation fact, assigns its planned output, and publishes it without passing it through the selected codegen backend.

The emitter must not request or retain LLVM modules, reinterpret Bray MIR, or ask a backend to make semantic decisions during
serialization. It does not perform the final native link.

The emission lifecycle, output layout, atomic publication, package-interface integration, and emitted-product receipt are defined
in `docs/design/emitter.md`. The final link contract is defined in `docs/design/linker.md`.

---

## Testing Strategy

`bray-codegen` owns backend conformance tests that every backend implementation must satisfy.

The test surface should include:

- exact translation of representative validated MIR operations,
- layout and ABI preservation,
- stable symbol and linkage behavior,
- runtime and intrinsic mappings,
- unsupported capability diagnostics,
- verifier and backend failure conversion,
- cancellation without partial publication,
- deterministic serial, parallel, repeated, and reversed-demand outputs,
- codegen fact invalidation when relevant target, backend, or option inputs change,
- absence of LLVM types from backend-neutral public contracts.

LLVM-specific tests may inspect LLVM IR and verifier behavior inside `bray-codegen-llvm`. Cross-backend semantic tests should execute
or inspect backend artifacts through the backend-neutral conformance harness rather than making LLVM output the language contract.

End-to-end executable and library tests cross the emitter and runtime boundary and therefore belong in compiler integration tests.

---

## Initial Implementation Order

The first implementation should proceed in this order:

1. Define backend identity, target, request, outcome, and artifact contracts in `bray-codegen`.
2. Define stable codegen-unit partitioning and lazy compilation fact keys.
3. Add `bray-codegen-llvm` with isolated LLVM initialization and target-machine construction.
4. Translate and verify a minimal validated Bray MIR unit.
5. Accept immutable emitter-derived artifact requests and serialize requested backend contributions.
6. Emit deterministic relocatable native objects for the first native product path.
7. Add backend conformance, cancellation, parallelism, and determinism tests.
8. Integrate emitter publication and native linker handoff without exposing backend modules.

Package-interface artifact emission may use the generic emitter publication primitives before native code generation is complete,
but those primitives must follow the backend-neutral artifact and path-ownership boundaries defined here.
