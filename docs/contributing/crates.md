# Crate Responsibilities

Use this page to find the crate that owns a concept. The design documents explain the intended relationships and guiding choices.

## `crates/`

Compiler libraries and installed Bray tools.

- `bray-base`
    - Small foundational types and helpers shared across otherwise unrelated compiler crates,
      including private same-filesystem staging for atomic file publication.

- `bray-source`
    - Source identities, snapshots, text ranges, spans, and source-location utilities.

- `bray-diagnostics`
    - Locale-neutral diagnostics, typed diagnostic arguments, and diagnostic-bearing result infrastructure.

- `bray-messages`
    - Locale-aware rendering of structured compiler diagnostics and messages.

- `bray-syntax`
    - Tokens, immutable syntax trees, typed syntax nodes, and reusable syntax traversal APIs.

- `bray-parser`
    - Demand-driven lexical analysis and parsing from source text into syntax trees and syntax diagnostics.

- `bray-declarations`
    - Syntax-based declaration discovery and deterministic aggregation of declarations across source units.

- `bray-compiler-known`
    - The checked-in compiler-known catalog format, validation, generation, and immutable descriptors.

- `bray-symbols`
    - Program-wide semantic identities, symbol relationships, semantic values, and generic lazy symbol-query contracts.

- `bray-package-interface`
    - Deterministic encoding, bounded decoding, validation, and semantic access for compiled package interfaces.

- `bray-package-interface-model`
    - Dependency-free package-interface record categories shared by semantic records and diagnostic arguments.

- `bray-project`
    - Bray workspace and package manifests, project-owned source discovery, and immutable deterministic package build
      graphs.

- `bray-profile`
    - Versioned compiler profile reports, descriptor catalogs, aggregate analysis, and report comparison.

- `bray-standard-library`
    - Standard library identities, source authority, bundle manifests, and artifact validation.

- `bray-binder`
    - Name resolution, semantic reference binding, local symbol construction, and immutable bound-unit construction.

- `bray-bound-tree`
    - The source-correlated high-level intermediate representation and durable typed semantic analyses.

- `bray-checker`
    - Focused type, ownership, control-flow, effect, and related semantic checker services.

- `bray-lowering`
    - Transformation of bound units and their required semantic inputs into explicit backend-independent mid-level IR.
    - MIR construction for compiler-generated bodies, including executable hosts, lifecycle helpers, and
      compiler-provided callables.

- `bray-ir`
    - Backend-independent mid-level IR, its validation, construction, and traversal APIs.

- `bray-target`
    - Target-machine identities and backend-neutral machine and output contracts.

- `bray-test-protocol`
    - Stable test identities, discovery metadata, selection filters, and runner protocol records shared across compiler
      and tooling.

- `bray-codegen`
    - Backend-independent code generation contracts, requests, artifact contributions, and backend orchestration.

- `bray-codegen-llvm`
    - LLVM-specific target realization, MIR translation, optimization, verification, and artifact serialization.

- `bray-emitter`
    - Artifact planning, serialization coordination, staging, publication, bookkeeping, and link-plan construction.

- `bray-formatter`
    - Deterministic Bray source formatting, comment and recovery preservation, and reusable check/write file operations.

- `bray-runtime-abi`
    - Dependency-free native symbol names, status values, fixed-layout records, handles, and callback signatures shared
      by generated code and linked runtime components.

- `bray-runtime-adapter`
    - Link-isolated native ABI adapters for host, foreign callback, scheduler, cancellation, event, and test-host runtime
      components. Each selected adapter publishes only the stable symbols for its semantic component and delegates its
      implementation to `bray-runtime`.

- `bray-runtime-model`
    - Dependency-light protected-frame, execution-lane, capability, identity, and ABI-version semantics shared by
      compiler contracts and runtime mechanisms.

- `bray-runtime-interface`
    - Compiler-facing runtime artifact metadata, product requirements, private execution and platform-service role
      contracts, compatibility validation, and executable-host integration.

- `bray-platform`
    - Safe typed ownership of native threads, waits, clocks, memory, processes, sockets, and other mechanisms used by
      compiler-host tooling and trusted runtime layers. Resource features let runtime archives compile only the
      mechanisms they own. Ordinary compiler-host consumers retain the complete default mechanism set.

- `bray-platform-abi`
    - Link integration that independently packages the temporal and dynamic-library providers for standard-library
      products. The temporal provider uses pinned third-party code.

- `bray-platform-abi-support`
    - Dependency-light native export declarations, raw-memory validation, and portable I/O error mapping shared by the
      temporal provider and runtime adapters.

- `bray-platform-abi-temporal`
    - Temporal platform ABI exports kept link-isolated from unrelated platform mechanisms and backed by the pinned
      third-party provider.

- `bray-runtime`
    - Target-independent protected-frame execution, task storage, scheduling, cancellation, observation, and
      executable-root services over `bray-platform` mechanisms. Its implementation symbols remain internal. Stable
      native symbols and test-runner protocol integration are present only in explicitly selected `bray-runtime-adapter`
      artifacts.

- `bray-linker`
    - Typed native link plans, linker and archiver adapters, invocation, and linked artifact results.

- `bray-compilation`
    - The demand-driven compilation context, query coordination, phase orchestration, profile collection, and top-level
      product emission.
    - Supply resolved semantic inputs to lowering services. Do not construct MIR bodies in compilation queries.

- `bray-tooling`
    - Shared diagnostic presentation, compiler inspection, source-request, and product-request tooling for command
      drivers.

- `bray-lsp`
    - The Bray language-server executable and incremental editor protocol handling over immutable compilation snapshots.

- `bray-driver`
    - Loose-file compiler command orchestration used by `brayc`.

- `bray`
    - Bray Tack project and workspace command orchestration across independently installed toolchain executables.

- `brayc`
    - The minimal Bray compiler executable entry point.

- `brayfmt`
    - The Bray formatter executable and its command boundary over `bray-formatter`.

- `bray-testing`
    - Shared test fixtures, builders, harnesses, and assertions used across compiler crates.

## `tools/`

Contributor-only development tools.

- `rust-style`
    - Project-independent Rust source-style enforcement used by `xtask`. It must remain independent and must therefore
      never depend on any repo-specific crates.

- `bray-llvm-toolchain`
    - Dependency-light provisioning and validation of the pinned LLVM development toolchain.

## `xtask/`

Repository automation.

- `xtask`
    - Repository automation invoked through Cargo, including generated-source and conformance tasks.
