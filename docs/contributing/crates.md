## Crate responsibilities

- `bray-base`
    - Shared foundational helpers used across compiler crates.
    - Keep this crate small and dependency-light.
    - Do not use it as a junk drawer. Prefer a more specific owner crate when one exists.

- `bray-source`
    - Source files, source IDs, spans, text ranges, source maps, and source-location utilities.
    - Owns the compiler's model of "where in the input this came from."
- 
- `bray-diagnostics`
    - Diagnostics infrastructure: errors, warnings, notes, labels, suggestions, diagnostic codes, and reporting structures.
    - Should not own compiler logic; it only represents and renders diagnostics.

- `bray-syntax`
    - Syntax data structures: tokens, token kinds, syntax node kinds, syntax trees, trivia, and syntax-level representations.
    - Defines the shape of parsed source, but should not perform parsing itself.

- `bray-parser`
    - Lexer and parser.
    - Converts source text into `bray-syntax` trees.
    - Should remain syntax-only: no name resolution, type checking, or semantic validation.

- `bray-declarations`
    - Discovers declared program items from syntax: modules, imports, types, functions, traits, impls, fields, parameters, etc.
    - Builds the declaration surface needed before full body binding.

- `bray-symbols`
    - Semantic identities for declared things.
    - Owns symbols and symbol-related structures: modules, types, functions, fields, locals, parameters, traits, impls, and associated items.
    - Symbols answer "what declared thing is this?"

- `bray-binder`
    - Name binding and semantic reference resolution.
    - Converts syntax references into bound references to symbols.
    - Produces `bray-bound-tree` structures where names, members, calls, fields, and places are resolved.

- `bray-bound-tree`
    - Semantic tree after binding.
    - Represents bound expressions, statements, items, places, projections, calls, locals, temporaries, and other source-correlated semantic nodes.
    - This is still high-level enough to produce good user diagnostics.

- `bray-checker`
    - Semantic checking after binding.
    - Owns type checking, ownership checking, borrow checking, alias checking, mutation authority, move/drop legality, initialization tracking, and effect/capability contract validation.
    - Answers "is this bound program valid Bray?"

- `bray-lowering`
    - Lowers checked bound trees into a more explicit compiler IR.
    - Makes implicit semantics explicit: temporaries, drops, moves, control-flow normalization, pattern lowering, short-circuiting, and other desugaring.

- `bray-ir`
    - Backend-independent intermediate representation.
    - Represents lowered control flow, locals, places, explicit moves/drops, calls, branches, and other operations used by codegen.

- `bray-codegen`
    - Backend-independent code generation interface and codegen orchestration.
    - Converts `bray-ir` into backend-specific representations.
    - Should not own linking, artifact layout, or CLI policy.

- `bray-emitter`
    - Artifact emission.
    - Owns object/executable/library output, output paths, linking handoff, and final emitted build products.

- `bray-compilation`
    - Main compiler entry point and compilation context.
    - Owns compile requests, options, package/file inputs, target settings, session-like state, and pipeline orchestration.
    - Coordinates parsing, declaration discovery, binding, checking, lowering, codegen, and emission.

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
