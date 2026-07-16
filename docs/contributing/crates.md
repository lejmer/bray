# Crate Responsibilities

Use this page to find the crate that owns a concept. Detailed phase contracts and implementation guidance belong in the design
documents.

- `bray-base`
    - Small, dependency-light foundational types and helpers shared across otherwise unrelated compiler crates.

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

- `bray-binder`
    - Name resolution, semantic reference binding, local symbol construction, and checked bound-unit construction.

- `bray-bound-tree`
    - The checked, source-correlated high-level intermediate representation produced by binding and semantic analysis.

- `bray-checker`
    - Focused type, ownership, control-flow, effect, and related semantic checker services used during binding.

- `bray-lowering`
    - Transformation of checked bound units into explicit backend-independent mid-level IR.

- `bray-ir`
    - Backend-independent mid-level IR, its validation, construction, and traversal APIs.

- `bray-target`
    - Target-machine identities and backend-neutral machine and output contracts.

- `bray-codegen`
    - Backend-independent code generation contracts, requests, artifact contributions, and backend orchestration.

- `bray-emitter`
    - Artifact planning, serialization coordination, staging, publication, bookkeeping, and link-plan construction.

- `bray-linker`
    - Typed native link plans, linker and archiver adapters, invocation, and linked artifact results.

- `bray-compilation`
    - The lazy compilation context, compiler fact coordination, phase orchestration, and top-level product emission.

- `bray-driver`
    - User-facing command orchestration that translates tool requests into compilation operations and results.

- `brayc`
    - The minimal Bray compiler executable entry point.

- `bray-testing`
    - Shared test fixtures, builders, harnesses, and assertions used across compiler crates.

- `xtask`
    - Repository automation invoked through Cargo, including generated-source and conformance tasks.
