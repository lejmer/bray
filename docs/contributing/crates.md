## Crate responsibilities

- `bray-base`
    - Shared foundational helpers used across compiler crates.
    - Keep this crate small and dependency-light.
    - Do not use it as a junk drawer. Prefer a more specific owner crate when one exists.

- `bray-source`
    - Source files, source IDs, spans, text ranges, source maps, and source-location utilities.
    - Owns the compiler's model of "where in the input this came from."

- `bray-diagnostics`
    - Locale-neutral diagnostics infrastructure: errors, warnings, notes, labels, suggestions, diagnostic codes, message IDs, typed message arguments, and reporting structures.
    - Owns diagnostic rendering contracts consumed by the locale-aware `bray-messages` infrastructure.
    - Owns the neutral immutable `DiagnosticResult<T>` value-plus-diagnostics wrapper used by lazy semantic facts.
    - Diagnostics must be structured so other locales can be added without changing compiler logic.
    - Should not own compiler logic; it only represents diagnostics and rendering data.

- `bray-messages`
    - Locale-aware diagnostic rendering.
    - Owns localized message catalogs, argument formatting, and rendered diagnostic values.
    - Keeps diagnostic message catalogs split by natural language so locale additions do not grow one shared catalog file.
    - Consumes structured `bray-diagnostics` records and must not own compiler logic.

- `bray-syntax`
    - Syntax data structures: token kinds, syntax node kinds, green syntax storage, typed syntax nodes, syntax trees, tokens,
      trivia, and syntax-level representations.
    - Owns reusable syntax walkers, visitors, and cursors.
    - Defines the shape of parsed source, but should not perform parsing itself.

- `bray-parser`
    - Lexer and parser.
    - Converts source text into `bray-syntax` trees.
    - Should remain syntax-only: no name resolution, type checking, or semantic validation.

- `bray-declarations`
    - Discovers declared program items from syntax: modules, imports, types, functions, traits, impls, fields, parameters, etc.
    - Builds the declaration surface needed before full body binding.

- `bray-compiler-known`
    - Owns the checked-in compiler-known and recognized standard-library catalog format, parser, validation, and immutable
      descriptors.
    - Owns deterministic generation of checked-in Rust descriptor and surface tables from `.braydef` sources.
    - Owns stable catalog key types and closed representation, compiler-provided implementation, and target-availability roles.
    - Owns generated typed role-to-descriptor indexes without owning checker or lowering behavior.
    - Reuses `bray-parser` for embedded Bray declaration and type-expression surfaces.
    - Publishes one static immutable target-independent catalog without runtime catalog parsing or structural validation.
      Compilation-owned facts derive target-specific available views.
    - Must not construct symbols, bind surfaces, implement checker rules, or lower compiler-provided behavior.

- `bray-symbols`
    - Semantic identities for declared things.
    - Owns typed symbol IDs, kind-specific symbol models, semantic containment, typed member relationships, lookup contracts, and
      lazy symbol-fact contracts.
    - Owns canonical semantic types, closed constant values, open constant terms, generic substitutions, portable dependency-contract
      templates, trait applications, callable and implementation instances, and their typed interner and view APIs.
    - Owns typed synthesized runtime-default-provider identities and symbol-facing declaration-owned-expression summary contracts,
      but not checked bound expression storage.
    - Owns region-scoped local symbol IDs, immutable local symbol snapshot contracts, lexical-scope records, and typed local symbol
      access without placing locals in the compilation-wide declaration symbol graph.
    - Covers modules, types, functions, fields, locals, parameters, traits, implementations, overload families, and associated
      items without using a generic child-symbol model.
    - Symbols answer "what declared thing is this?"
    - Publishes compilation-local typed compiler-known role registries with forward and reverse identity lookup.
    - Owns semantic audits of materialized compiler-known identities, ownership, typed roles, target views, and completion coverage.
    - Binding-dependent symbol facts are computed by the owning binder or checker service and coordinated through compilation
      queries.

- `bray-binder`
    - Name binding and semantic-analysis orchestration.
    - Uses an injected read-only fact context and must not depend on `bray-compilation`.
    - Converts syntax references into bound references to symbols.
    - Binds declaration-owned expressions and computes their binder-owned checked representations through compilation queries.
    - Builds local symbol snapshots and lexical scope graphs together with each checked semantic region.
    - Calls focused checker services while constructing checked bound units.
    - Produces immutable `bray-bound-tree` structures where names, members, calls, fields, storage accesses, and required semantic
      facts are resolved.

- `bray-bound-tree`
    - Owns the checked source-shaped high-level IR produced by binding and semantic analysis.
    - Represents bound expressions, statements, items, storage identities, storage accesses, projections, calls, locals,
      temporaries, resolved references, selected semantic facts, and other source-correlated semantic nodes.
    - Owns unit-local borrow-capability identities and instantiated dependency contracts without making those IDs symbol facts.
    - Owns checked declaration-owned-expression nodes without making bound-node IDs part of `bray-symbols` records.
    - Owns category-specific checked-region value types that retain their immutable local symbol snapshots.
    - Owns durable checker result types stored on bound nodes so it does not depend on checker algorithms.
    - Owns reusable bound-representation walkers and visitors.
    - This is still high-level enough to produce good user diagnostics.

- `bray-package-interface`
    - Owns deterministic encoding, bounded decoding, validation, content hashing, and lazy fact access for compiled `.brayi`
      package interfaces.
    - Converts wire records into imported identity surfaces and fact values owned by `bray-symbols` and `bray-bound-tree`.
    - Must not define metadata-specific symbol kinds, expose codec internals through semantic APIs, or depend on compilation,
      binder, checker, lowering, code generation, or emission orchestration.
    - Treats dependency interfaces as untrusted external input and reports structured diagnostics rather than panicking.

- `bray-checker`
    - Focused semantic checker services used by `bray-binder`.
    - Depends on lower semantic representations and must not depend back on binder orchestration.
    - Owns type checking, ownership checking, borrow checking, alias checking, mutation authority, move/drop legality,
      initialization tracking, and effect/capability contract validation.
    - Owns one immutable checker-internal control-flow graph per analyzed semantic unit and focused forward or backward data-flow
      domains over that shared control-flow graph.
    - Keeps graph IDs, fixed-point state, work lists, and intermediate flow facts task-local rather than publishing them as bound,
      symbol, package-interface, or lowering identities.
    - Returns structured diagnostics and semantic facts for the binder to place on bound nodes before publication.
    - Receives target-available typed compiler-known role views through checker requests rather than resolving roles by name.

- `bray-lowering`
    - Owns the validated borrowing boundary over canonical bound units and their required durable semantic facts.
    - Lowers checked source-shaped bound HIR directly into the backend-independent MIR owned by `bray-ir`.
    - May use task-local construction forms but does not publish a second durable lowered representation.
    - Makes implicit semantics explicit: temporaries, drops, moves, control-flow normalization, pattern lowering, short-circuiting,
      and other desugaring.
    - Materializes reachable runtime-default providers from their checked declaration-owned expressions.
    - Receives target-available typed compiler-known role views through validated lowering inputs rather than resolving hooks by name.

- `bray-ir`
    - Backend-independent mid-level intermediate representation used as Bray's MIR.
    - Represents lowered control flow, locals, storages, explicit moves/drops, calls, branches, and other operations used by codegen.
    - Owns reusable IR walkers and visitors.

- `bray-codegen`
    - Backend-independent code generation interface and codegen orchestration.
    - Owns backend selection, codegen-unit partitioning, concrete monomorphized-instance collection, backend identity, capabilities,
      requests, and outcomes.
    - Supplies canonical layout, ABI, symbol, target, runtime, and linkage facts while translating validated `bray-ir` MIR into
      backend-specific low-level IR.
    - Allows compilation to use an injected backend without depending on a concrete backend crate.
    - Should not own linking, artifact layout, or CLI policy.

- `bray-codegen-llvm`
    - First production code generation backend.
    - Owns every LLVM dependency and LLVM-specific context, module, translation, verification, optimization, target-machine, and
      artifact-generation detail.
    - Constructs semantically complete LLVM IR and serializes emitter-requested LLVM IR, bitcode, assembly, object, and debug
      artifacts.
    - Must not make language-semantic, reachability, layout, ABI, artifact-policy, or linking-policy decisions.
    - Must not depend on compilation, emission, driver, or command-line orchestration.

- `bray-emitter`
    - Artifact emission lifecycle and orchestration.
    - Owns emission requests and plans, artifact policy, output names and sinks, backend serialization requests, staging, atomic
      publication, artifact bookkeeping, diagnostics, and link-plan construction.
    - Publishes completed package-interface artifacts without selecting their semantic surface or encoding their records.
    - Does not construct backend IR, perform backend-specific serialization, or invoke the final native link.

- `bray-linker`
    - Final native linking.
    - Owns typed link plans, embedded and system linker adapters, argument construction, invocation, linker diagnostics, and linked
      artifact results.
    - Consumes emitter-produced objects or bitcode and writes the linked result to an emitter-owned staging destination.

- `bray-compilation`
    - Main compiler entry point and compilation context.
    - Owns compile requests, options, package/file inputs, target settings, session-like state, and lazy compiler fact coordination.
    - Owns exact fact-key composition, caches, dependency scheduling, cancellation, and immutable fact publication around typed
      domain keys supplied by lower compiler representations.
    - Caches checked-region results without maintaining a mutable compilation-wide local symbol registry.
    - Coordinates lazy package-interface loading, imported symbol facts, export-bundle construction, and interface encoding.
    - Coordinates parsing, declaration discovery, binding and semantic analysis, lowering, and codegen through explicit fact APIs.
    - Owns the top-level effectful product-emission operation that obtains lazy facts, delegates artifact policy and publication to
      `bray-emitter`, delegates final native linking to `bray-linker`, and merges all diagnostic bags deterministically.

- `bray-driver`
    - User-facing compiler command orchestration.
    - Bridges CLI/tool input into `bray-compilation`.
    - Should stay thin: parse command intent, construct compilation inputs, call the compilation API, and return exit status.

- `brayc`
    - Compiler binary crate.
    - Contains the actual executable entry point.
    - Should be minimal and delegate almost everything to `bray-driver`.

- `bray-testing`
    - Shared compiler test infrastructure.
    - Test harnesses, snapshot helpers, fixture loading, compile-fail/run-pass support, and common assertions used by integration/UI tests.
