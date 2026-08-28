# Coding conventions

These rules apply across the Bray codebase unless a narrower rule is stated.

For crate layout, crate ownership, and workspace structure, see the crate responsibility documentation.

## Core principles

- Prefer clarity and maintainability over cleverness.
- Keep behavior easy to reason about at the contract boundary and in the owning crate or module.
- Follow the style and architectural conventions already present near the code you are editing.
- Prefer explicit structure over implicit behavior.
- Do not add abstractions, helpers, dependencies, or configuration points speculatively.
- When changing behavior, update the relevant tests, fixtures, examples, and docs in the same change.

## Compiler architecture

- Keep compiler phases separated by responsibility.
- Do not mix parsing, binding, checking, lowering, code generation, emission, and linking logic in the same module
  unless there is a narrow local reason.
- Syntax-level crates must not perform semantic validation.
- Parser code should produce syntax trees and syntax diagnostics only.
- Binder code should resolve names and semantic references, but should not perform full type, ownership, aliasing, or
  effect validation.
- Checker code owns type checking, ownership checking, borrow checking, alias checking, mutation authority,
  initialization tracking, move/drop legality, and effect/capability validation.
- Lowering code makes checked semantics explicit before IR/codegen.
- Codegen should not own CLI policy, package policy, output path policy, or linking policy.
- Emitter code owns artifact policy, serialization coordination, publication, and typed link-plan construction. Linker
  code owns
  final native linker and archiver invocation.
- Keep source-correlated semantic information available long enough to produce good diagnostics.
- Prefer structured compiler data over strings, flags, and ad hoc side channels.

## Crate ownership

- Every substantial concept should have one owning crate.
- Before adding a type, helper, module, or dependency, decide which crate owns the concept.
- Do not place code in `bray-base` just because multiple crates need it.
- `bray-base` is for small foundational helpers with no better owning crate.
- Prefer specific owner crates over broad shared crates.
- If a crate boundary becomes artificial or obstructs clear design, refactor the boundary rather than working around it.
- If a subsystem grows into a stable substantial concept, consider splitting it into its own crate.

## Public contracts

- Treat public APIs as deliberate contracts.
- Default visibility to `pub(crate)`.
- Only make items `pub` when they are part of a deliberate cross-crate API.
- Avoid changing public APIs unintentionally.
- Document public APIs with `///` doc comments.
- Include minimal examples in public docs when they clarify the contract.
- If a breaking public API change is intentional, update in-repo call sites, tests, examples, and docs in the same
  change.
- Boundary APIs should use typed inputs and typed outputs, not loosely structured strings or maps.
- If more than one optional override is needed, prefer a typed configuration or contract object instead of constructor
  proliferation.
- Avoid passthrough wrappers unless they add boundary value such as validation, policy injection, type conversion, error
  mapping, or telemetry.

## Do not repeat yourself

- Before adding a helper, function, type, or module, search for equivalent or near-equivalent behavior.
- If equivalent behavior exists, reuse it or move it to one shared implementation in the appropriate owning crate or
  module.
- If similar behavior exists, extend the existing implementation when that keeps the design clearer.
- If no equivalent helper exists but the behavior is reusable beyond the immediate local context, place it where future
  callers can find it.
- Keep behavior local only when it is tightly coupled to one function or module.
- Do not repeat chunks of code without a real reason.

## General code quality

- Never use unsafe operations, blocks, functions, traits, or implementations.
- An unsafe attribute is allowed only in a dedicated native ABI boundary when Rust requires it to publish a stable
  symbol. Keep the item body safe and add a narrow `#[expect(unsafe_code, reason = "...")]` explaining the boundary
  requirement.
- Do not use wildcard imports such as `::*`.
- Always import items explicitly.
- Keep invariants represented in types where practical.
- Prefer impossible states being unrepresentable over comments explaining possible invalid states.
- Avoid global mutable state.
- Avoid hidden behavior, hidden dependencies, and hidden policy decisions.
- Keep core logic operating on validated, well-typed structures.
- Validate external inputs at boundaries.
- `fetch_update` will be deprecated from Rust 1.99.0: use `try_update` instead.

## Modules and files

- `lib.rs` files must be thin crate roots and must not contain implementation code.
- Keep modules focused.
- Prefer adding a focused module over growing an already large module.
- Target production modules under 500 lines of code.
- If a file exceeds roughly 800 lines, add new functionality in a new module unless there is a strong reason not to.
- Prefer adding helpers over growing large functions.
- Target functions under 100 lines of code.
- If a function exceeds roughly 200 lines, add new behavior in a new helper or function unless there is a strong reason
  not to.
- Do not use the legacy `mod.rs` module layout.
- When a module needs to be split into smaller files, keep the parent module as a thin module root containing only
  submodule declarations, reexports, crate-local wiring, and minimal documentation.
- Store split implementation in aptly named files under a same-named subdirectory, using the modern layout: `foo.rs`
  plus `foo/bar.rs`, `foo/baz.rs`, etc., not `foo/mod.rs`.
- When splitting an existing large module, move implementation out of the old module file. The old file should become
  the thin module root, not remain a mixed root-and-implementation file.
- Avoid repeating the parent module name as a filename prefix.
- If a module grows large enough that related code would require prefixed sibling files, split the module using the
  modern layout instead.
- Prefer `foo.rs` plus `foo/bar.rs` over `foo.rs` plus `foo_bar.rs`.
- Files inside a module subdirectory should be named for the local concept they contain, not for the full module path.
  The directory already provides the parent context.

## Ownership and borrowing

- Prefer borrowing over owning when ownership is not needed.
- Accept `&str` instead of `String` when ownership is not needed.
- Accept slices such as `&[T]` instead of `Vec<T>` when ownership is not needed.
- Avoid unnecessary clones.
- Do not use `.clone()` just to make the borrow checker happy. Restructure the code first.
- Any `.clone()` added in compiler core code must be on a small Copy-like type or have a comment explaining why cloning
  is the best tradeoff.
- If cloning is needed to decouple a lifetime or avoid holding a lock, say so in a short comment.
- Prefer iterators over indexing.
- Avoid indexing patterns that can panic from out-of-bounds access.
- Keep ownership transfer explicit at API boundaries.
- Do not hide expensive ownership movement behind helper names that sound like cheap observation.

## Types and values

- Use `Option<T>` for absence.
- Use `Result<T, E>` for fallible behavior.
- Do not use sentinel values for absence or failure.
- Avoid unchecked conversions and casts unless they are validated and justified.
- Prefer newtypes for IDs, indexes, symbols, node IDs, file IDs, and other distinct compiler concepts.
- Do not use raw integers or strings for semantically distinct IDs when a newtype would prevent mistakes.
- Keep serialized and external contracts explicit and typed.
- Use typed configuration or contract objects when behavior has multiple options.

## Compiler diagnostics and user-facing text

See [Compiler diagnostics](diagnostics.md) for the structured producer, rendering, suggestion, and readiness contracts.

- Compiler logic must emit structured diagnostics, not hardcoded user-facing English strings.
- Use diagnostic codes, severities, spans, labels, notes, suggestions, related locations, message IDs, and typed message
  arguments.
- User-facing text must be rendered through the locale-aware `bray-messages` infrastructure.
- Do not use semicolons or em dash characters in user-facing prose, comments, or documentation.
  A semicolon is allowed only when it is part of literal code syntax.
- Prefer precise terms such as normalized, stable, shared, or authoritative. Use "canonical" only
  when it names a defined uniqueness or normalization property.
- CLI and LSP output must use localized messages.
- Diagnostic data must be designed so multiple locales can be added without changing compiler logic.
- Do not construct user-facing prose inside parser, binder, checker, lowering, codegen, emitter, or linker logic.
- Do not pre-render diagnostic arguments into any one language before passing them to message rendering.
- Pass typed arguments such as symbols, types, declaration kinds, operator kinds, spans, counts, and source snippets to
  the renderer.
- Let localization handle argument ordering, plural forms, list formatting, quotation style, and grammar-specific
  phrasing.
- Diagnostics should explain the cause, point at the relevant source locations, and prefer actionable suggestions when
  the suggestion is mechanically reliable.
- Do not emit vague diagnostics when the compiler has enough structure to be precise.
- Snapshot tests for diagnostics should validate diagnostic structure and rendered output where appropriate.

## Errors and panics

- Model errors explicitly.
- Use crate-local error enums for library layers when appropriate.
- Convert internal errors to user-facing diagnostics or boundary errors at the appropriate boundary.
- Do not use `unwrap()` or `expect()` in production paths.
- `unwrap()` and `expect()` are allowed only in tests, benchmarks, or when guarded by an invariant and accompanied by a
  comment explaining that invariant.
- Prefer clear validation errors or diagnostics over panics.
- Panics are for violated compiler invariants, not ordinary user input errors.
- User source code should not be able to crash the compiler.
- If malformed input reaches a later compiler phase, report a compiler bug or recover through a deliberate error path
  rather than panicking silently.

## Control flow

- Prefer `match` over deep `if` or `else` ladders when branching on enums, options, or results.
- Use early returns such as `return Err(...)` to keep the happy path left-aligned.
- Keep functions small enough that control flow remains obvious.
- Prefer explicit error handling over implicit fallthrough.
- Avoid boolean parameter combinations that create unclear control flow. Prefer enums or typed configuration objects.

## Serialization and persisted data

- Use `serde` derives where sensible.
- Avoid hand-rolled parsers unless they are truly necessary.
- Validate external input at serialization boundaries.
- Keep serialized contracts explicit and typed.
- Keep compiler logic separate from loosely typed payload handling.
- Introduce a serialized format, schema, protocol, cache, corpus, or ABI at version 1.
- Increment a version only when an earlier version remains a supported compatibility contract.
- Regenerate in-repository artifacts and disposable local caches in place when compatibility is not required.
- Treat every supported persisted format as a compatibility contract.

## Concurrency

- Do not introduce shared mutability casually.
- Prefer message passing, immutable sharing, or ownership transfer.
- If sharing is required, use synchronization deliberately.
- Minimize lock scope.
- Do not hold locks across await points.
- Do not introduce global mutable state.
- If a global is truly needed, use immutable data or one-time initialization and document why.
- Keep concurrent compiler work deterministic unless nondeterminism is explicitly part of the contract.
- Diagnostics, emitted artifacts, and test output must not depend on thread scheduling.

## Logging and telemetry

- Use structured logging where possible.
- Do not use `println!` in production paths.
- Log at boundaries and high-level operations.
- Avoid noisy per-element logs in hot loops.
- If hot-loop detail is useful, guard it behind a verbosity setting that defaults to off.
- Do not localize logs intended for debugging or machine processing unless they are user-facing.
- Do not put user-facing CLI or LSP text in logs as a substitute for proper messages.

## Dependencies

- Add dependencies deliberately.
- Avoid just-in-case crates.
- Prefer widely used crates with good maintenance signals.
- Keep feature flags minimal.
- Document why each non-default feature is enabled.
- Do not add dependencies for trivial helpers.
- Avoid dependencies that force broad transitive feature sets unless the tradeoff is justified.
- Keep compiler crates dependency-light where possible, especially foundational crates.

## Tests

- Prefer fast unit tests near the code they validate.
- Use integration tests for cross-crate compiler behavior.
- Use compile-pass tests for programs that should compile.
- Use run-pass tests for programs that should compile and execute.
- Use compile-fail/UI tests for invalid programs and diagnostics.
- Test diagnostic codes, spans, labels, notes, and suggestions, not only whether compilation failed.
- Use snapshot tests when output shape matters.
- Use `assert_eq!()` on entire objects instead of checking fields one by one when that is practical.
- Add regression tests for bug fixes.
- Update contract tests when intentional language behavior changes.
- Do not add regression tests for intentional feature changes as if the old behavior were still the contract.
- Keep fixtures small and focused.
- Prefer one test per semantic idea unless combining cases makes the contract clearer.

## Naming

- Use clear, descriptive names.
- Avoid cryptic abbreviations and shorthands outside genuinely local or mathematical contexts.
- Use names such as `index`, `symbol`, `source`, `span`, `node`, `place`, and `diagnostic`.
- Avoid abbreviations such as `idx`, `sym`, `src`, and `diag` in public or broadly used APIs.
- Short names are acceptable in tight local scopes when the meaning is obvious.
- Short math-oriented names are acceptable when expressing a mathematical formula directly.
- Name types after the concept they represent, not after the implementation detail that currently stores them.
- Name modules for the local concept they contain. Do not repeat the full parent path in the filename.
- Use `fact` only for generic demand-driven query machinery whose contract is independent of a particular compiler
  domain.
- Name domain modules, types, methods, variables, tests, and documentation after their semantic meaning, such as
  properties, analyses, records, contracts, or plans. Lazy evaluation alone does not make a domain value a fact.
- Do not replace an imprecise `fact` name with another vague suffix such as `data` or `info`.

## Comments and structure

- Comments should explain intent, structure, non-obvious constraints, invariants, or what the code does not clearly
  state.
- Comments must not restate code mechanically.
- Use comments sparingly.
- Use standard keyboard symbols only, except where mathematical notation is genuinely needed.
- Do not use emojis.

### Semantic paragraph structure

- Treat blank lines as paragraph boundaries between statements with different semantic purposes.
- Use one blank line between logical phases such as setup, validation, transformation, I/O, and publication or return.
- Start a new paragraph whenever the statement purpose changes, including transitions from deriving or assigning values
  to actions and transitions back to further derivation after actions.
- Separate consecutive guard clauses when each enforces a different invariant.
- Separate dependent construction layers when each introduces a distinct semantic level, such as package, module, and
  declaration identities.
- Keep homogeneous declarations or assertions together when they form one conceptual group. Separate groups that
  construct different cases or verify different properties.
- Do not add blank lines between near-identical statements that form one conceptual group.
- Do not compress unrelated statements together just to minimize vertical space.

`cargo xtask style` applies automatic fixes and then runs every structural check. Use `cargo xtask style check` to run
the same diagnostics without changing files.

### Mechanically enforced blank-line rules

- Isolate `let ... else` guard clauses from both the setup before them and the work after them, even when the guard
  directly validates the preceding value.
- Do not add blank lines inside argument lists, parameter lists, struct literals, enum variants, or match cases.
- Use one blank line as a separator. Do not add multiple consecutive blank lines for decoration.
- Separate multiline statements and expressions (like multiline `let` expressions or `assert*` macros) with blank lines.
- In blocks, always put a blank line above any comment unless the comment is the absolute first thing in that block.
- Tuple and bracket destructuring `let` expressions (`let (a, b) = ...` and `let [a, b] = ...`) should be separated with
  blank lines.
- Separate every returned expression from preceding statements or expressions in the same block. This applies both to
  explicit `return value;` expressions and to implicit tail expressions. When a comment documents the returned
  expression, place the boundary above the comment so the comment stays attached to the expression.

### Automated structural checks

A check-only rule runs in both modes, but the command reports the problem instead of trying to rewrite the source. Error
diagnostics fail the command, while warning diagnostics do not.

The following structural rules apply to Rust source:

- A production module over 800 physical source lines produces a warning at its 801st production line. Test-only items
  and dedicated test sources do not count.
- A production function over 250 physical source lines produces an error. Keep the design target at roughly 200 lines so
  functions do not routinely approach the enforced limit. Tests, dedicated test sources, and helpers inside test-only
  modules do not count.
- `lib.rs` and a module file paired with a same-named directory must be thin roots. They may contain documentation and
  attributes, external module declarations, and visible reexports, but no implementation, declarations, private imports,
  or inline modules.
- The legacy `mod.rs` layout is an error.
- A submodule filename that repeats its parent module name, such as `foo/foo_parser.rs`, produces a warning. Name it
  `foo/parser.rs`. The directory already supplies the parent context.
- Wildcard imports and reexports are errors regardless of visibility.

Where a rule is genuinely unreasonable for a specific source location, use a narrow source exemption with a nonempty
reason:

```rust
// rust-style: allow(module-too-large, reason = "locale argument catalog is intentionally a flat list")
```

File-level exemptions must appear before the first item and apply only to `module-too-large`, `legacy-mod-rs`, or
`repeated-module-prefix`. Item-level exemptions must appear immediately before the affected item and apply only to
`function-too-large`, `non-thin-lib-root`, `non-thin-module-root`, or `wildcard-import`. Unknown rules, malformed
directives, empty reasons, and invalid placement are errors. An exemption that does not suppress a diagnostic produces a
warning.

Do not use an exemption merely to avoid a reasonable cleanup. A wildcard exemption is appropriate only when naming the
symbols explicitly is unreasonable to maintain, such as a machine-generated file or an API whose names are defined by
macros. A module-size exemption is appropriate for a genuinely flat catalog or similar list whose cohesion would be
harmed by an arbitrary split.

## Documentation

- Keep documentation phrased as the current project shape, not as a migration note.
- Link to the owning reference or convention page instead of duplicating long rules across documents.
- Document public APIs with `///` doc comments.
- Describe public APIs in terms of their purpose, observable behavior, caller obligations, and
  relevant failure conditions.
- Do not expose internal storage, caching, construction, publication, or phase-coordination
  mechanics in public API documentation unless they materially affect callers.
- Document invariants that callers, implementers, or future compiler phases must preserve.
- Keep design documentation aligned with implementation when behavior changes.
- Do not document speculative future behavior as if it already exists.

## Agent-specific rules

- Prefer modifying the existing abstraction over adding a parallel one.
- Do not preserve backward compatibility unless explicitly requested.
- Prefer one coherent breaking change over compatibility clutter, unless explicitly requested to avoid breaking changes.
- Delete obsolete code paths in the same change.
- Do not add broad fallbacks unless the invariant is genuinely optional.
- Fail loudly on impossible states.
- Do not special-case current input data. Identify the general invariant first.
- Do not introduce a new abstraction merely because code could be shared. Introduce one when it clarifies a real concept
  or prevents a real duplication problem.
- Keep user-visible progress and failure information useful. Commands should not appear to hang silently during
  meaningful work.
- After changing behavior, run the relevant formatting, compile, and test commands.

## Review checklist

Before finishing a change:

- Verify crate placement and ownership.
- Verify public APIs were not changed unintentionally.
- Verify public APIs have appropriate `///` docs.
- Verify visibility is no broader than needed.
- Verify imports are explicit and no wildcard imports were added.
- Verify `lib.rs` files remain thin.
- Verify no unsafe operation was introduced and every unsafe attribute follows the native ABI exception.
- Verify equivalent behavior was not duplicated.
- Verify hardcoded user-facing English text was not introduced.
- Verify diagnostics use structured message IDs and typed arguments.
- Verify error handling uses explicit error types and boundary error conversion.
- Verify production paths do not use `unwrap()` or `expect()`.
- Verify clones are necessary and documented when required by policy.
- Verify conversions and casts are validated and justified.
- Verify concurrency code does not introduce accidental shared mutable state or hold locks too long.
- Verify logging does not use `println!` in production paths.
- Verify serialization uses structured contracts rather than ad hoc parsing.
- Verify docs and inline comments were updated where behavior changed.
- Verify tests were added or updated for behavior changes.
- Run formatting, compile, and test commands.
- Verify code compiles.
- Verify tests succeed.
- Do a final formatting pass to catch issues such as missing blank lines.
