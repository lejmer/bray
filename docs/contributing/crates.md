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
    - Diagnostics must be structured so other locales can be added without changing compiler logic.
    - Should not own compiler logic; it only represents diagnostics and rendering data.

- `bray-messages`
    - Locale-aware diagnostic rendering.
    - Owns localized message catalogs, argument formatting, and rendered diagnostic values.
    - Keeps diagnostic message catalogs split by human language so locale additions do not grow one shared catalog file.
    - Consumes structured `bray-diagnostics` records and must not own compiler logic.

- `bray-syntax`
    - Syntax data structures: tokens, token kinds, syntax node kinds, syntax trees, trivia, and syntax-level representations.
    - Owns reusable syntax walkers, visitors, and cursors.
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
    - Name binding and semantic-analysis orchestration.
    - Converts syntax references into bound references to symbols.
    - Calls focused checker services while constructing checked bound units.
    - Produces immutable `bray-bound-tree` structures where names, members, calls, fields, storages, and required semantic facts are resolved.

- `bray-bound-tree`
    - Immutable source-shaped semantic representation after binding and semantic analysis.
    - Represents bound expressions, statements, items, storages, projections, calls, locals, temporaries, resolved references, selected semantic facts, and other source-correlated semantic nodes.
    - Owns reusable bound-representation walkers and visitors.
    - This is still high-level enough to produce good user diagnostics.

- `bray-checker`
    - Focused semantic checker services used by `bray-binder`.
    - Owns type checking, ownership checking, borrow checking, alias checking, mutation authority, move/drop legality, initialization tracking, and effect/capability contract validation.
    - Returns structured diagnostics and semantic facts for the binder to place on bound nodes before publication.

- `bray-lowering`
    - Lowers checked bound trees into a more explicit compiler IR.
    - Makes implicit semantics explicit: temporaries, drops, moves, control-flow normalization, pattern lowering, short-circuiting, and other desugaring.

- `bray-ir`
    - Backend-independent intermediate representation.
    - Represents lowered control flow, locals, storages, explicit moves/drops, calls, branches, and other operations used by codegen.
    - Owns reusable IR walkers and visitors.

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
    - Coordinates parsing, declaration discovery, binding and semantic analysis, lowering, codegen, and emission.

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
