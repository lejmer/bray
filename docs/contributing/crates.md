# Crate Responsibilities

Use this page to find the crate that owns a concept. Detailed phase contracts and implementation guidance belong in the design
documents.

## `crates/`

Compiler libraries and installed Bray tools.

- `bray-base`
    - Small foundational types and helpers shared across otherwise unrelated compiler crates,
      including private same-directory staging for atomic file publication.

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
    - Program-wide semantic identities, symbol relationships, semantic values, and lazy symbol-fact contracts.

- `bray-package-interface`
    - Deterministic encoding, bounded decoding, validation, and semantic access for compiled package interfaces.

- `bray-project`
    - Bray workspace and package manifests, project-owned source discovery, and immutable deterministic package build graphs.

- `bray-standard-library`
    - Standard library identities, source authority, bundle manifests, and artifact validation.

- `bray-binder`
    - Name resolution, semantic reference binding, local symbol construction, and immutable bound-unit construction.

- `bray-bound-tree`
    - The source-correlated high-level intermediate representation and durable typed semantic side-fact representations.

- `bray-checker`
    - Focused type, ownership, control-flow, effect, and related semantic checker services.

- `bray-lowering`
    - Transformation of bound units and their required semantic facts into explicit backend-independent mid-level IR.

- `bray-ir`
    - Backend-independent mid-level IR, its validation, construction, and traversal APIs.

- `bray-target`
    - Target-machine identities and backend-neutral machine and output contracts.

- `bray-test-protocol`
    - Stable test identities, discovery metadata, selection filters, and runner protocol records shared across compiler and tooling.

- `bray-codegen`
    - Backend-independent code generation contracts, requests, artifact contributions, and backend orchestration.

- `bray-codegen-llvm`
    - LLVM-specific target realization, MIR translation, optimization, verification, and artifact serialization.

- `bray-emitter`
    - Artifact planning, serialization coordination, staging, publication, bookkeeping, and link-plan construction.

- `bray-formatter`
    - Deterministic Bray source formatting, comment and recovery preservation, and reusable check/write file operations.

- `bray-runtime-interface`
    - Backend-neutral contracts for protected-frame identities, private execution and platform-service ABI roles, runtime
      requirements, and executable-host integration.

- `bray-platform`
    - Safe typed ownership of native threads, waits, clocks, memory, processes, sockets, and other mechanisms used by compiler-host
      tooling and trusted runtime or standard-library layers.

- `bray-platform-abi`
    - Native adapters that implement the platform-service ABI over `bray-platform` without requiring the concurrency runtime.

- `bray-runtime`
    - Target-independent protected-frame execution, task storage, scheduling, cancellation, observation, and executable-root
      services over `bray-platform` mechanisms.

- `bray-linker`
    - Typed native link plans, linker and archiver adapters, invocation, and linked artifact results.

- `bray-compilation`
    - The lazy compilation context, compiler fact coordination, phase orchestration, and top-level product emission.

- `bray-tooling`
    - Shared diagnostic presentation, compiler inspection, source-request, and product-request tooling for command drivers.

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
    - Reusable Rust source-style enforcement used by `xtask`, with no dependencies on Bray compiler crates.

- `bray-llvm-toolchain`
    - Dependency-light provisioning and validation of the pinned LLVM development toolchain.

## `xtask/`

Repository automation.

- `xtask`
    - Repository automation invoked through Cargo, including generated-source and conformance tasks.
