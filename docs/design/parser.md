# Parser design

The parser turns the [source grammar](../language/syntax-grammar.md) into lossless syntax. `bray-parser` owns parsing
and syntax recovery. `bray-syntax` owns the immutable syntax representation and its typed views. Name resolution and
semantic validation belong to later phases.

## Grammar-shaped parsing

Bray uses handwritten recursive descent for grammar structure and precedence climbing for expressions. Grammar roots
have private `parse_` methods named after their nonterminals, such as `parse_source_unit_module_declaration`. Distinct
expression roots retain distinct entry points even when they share an expression engine.

Shared grammar shapes use shared list, delimiter, recovery, and lookahead machinery. Grammar methods select the relevant
context and boundaries. This keeps the production visible without repeating infrastructure in every parser method.
Associativity and precedence are explicit properties of the expression engine.

Parser decisions depend only on syntax. Ambiguity is resolved through lookahead or speculative cursor forks, without
querying names, types, targets, or package semantics.

## Demand-driven token access

Lexer operations use the `scan_*` prefix, complementing `parse_*` for parsing operations.

A parser cursor mediates token consumption and stable lookahead. It demands lexical tokens as needed, so parsing does
not require eager tokenization of the source. Helpers that can demand tokens expose that work through their names and
mutable access rather than looking like passive predicates.

Speculation isolates parser position, diagnostics, recovery, and builder state until a branch is selected. Lexical
tokens already demanded by a scan remain reusable, with deterministic diagnostic ownership.

## Lossless syntax

Parser builders publish immutable green syntax in source order. Typed syntax views expose concrete grammar shapes and
recovery state without exposing green storage internals.

Present tokens retain their spelling and trivia. Skipped input remains in explicit recovery nodes, and missing syntax
has zero-width tokens. These representations let editors, formatters, and later compiler phases work with incomplete
source while preserving exact reconstruction.

Concrete grammar nodes belong together in the syntax representation. Reusable node traits, list storage, builders, and
definition macros belong in shared syntax infrastructure. Macros can remove typed-node boilerplate while grammar
decisions remain in handwritten parser methods.

## Recovery and bounded work

Recovery helpers are grammar-neutral. Each grammar entry point supplies the boundaries at which normal parsing can
resume. Shared list machinery preserves items, separators, missing tokens, and skipped input through the same lossless
representation.

Recovery always makes progress or returns to its caller. A common nesting budget bounds recursion across expression,
type, and pattern parsing, including speculative paths. Exhaustion follows ordinary structured recovery rather than
relying on the host stack.

## Diagnostics and ownership

Parsing owns syntax diagnostics and incorporates demanded lexical diagnostics into its result. Compilation collects
those immutable results lazily and merges them deterministically. Diagnostic records carry typed arguments and source
locations, with rendering in `bray-messages`.

The parser borrows source snapshots, tokens, and syntax data where their ownership allows it. Mutable cursor and builder
state stays local to a parse. Published syntax and diagnostics can be shared across independent consumers.

## Related documents

- [Compiler architecture](compiler-architecture.md)
- [Declaration discovery](declaration-discovery.md)
- [Source formatter](formatter.md)
- [Parser contribution guidance](../contributing/testing.md#parser-coverage)
