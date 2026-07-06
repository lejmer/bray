# Parser design

This document defines how the Bray parser should be implemented.

The source language grammar is defined by `docs/language/syntax-grammar.md` and `docs/language/syntax-grammar.ebnf`.
This document is not a second grammar.
It is the implementation contract for turning that grammar into parser code.

The parser belongs to `bray-parser`.
It produces lossless syntax trees owned by `bray-syntax` and syntax diagnostics owned by the diagnostics model.
It must not perform semantic validation.

---

## Goals

The parser should be:

- hand-written and easy to follow,
- recursive descent for grammar structure,
- precedence climbing for expressions,
- faithful to the syntax grammar,
- lossless for source reconstruction,
- tolerant of malformed user source,
- deterministic under lazy evaluation,
- explicit about recovery and ambiguity.

Malformed user input must produce syntax diagnostics and recovered syntax trees, not compiler panics.

---

## Grammar Contract

`docs/language/syntax-grammar.md` is the readable source grammar.
`docs/language/syntax-grammar.ebnf` is the plain EBNF reference.

Parser changes must follow the grammar exactly.
If the implementation needs to accept a new source shape, update the grammar documents in the same change.
If the grammar changes, update syntax kinds, typed syntax APIs, parser code, diagnostics, and tests together.

Semantic restrictions must not be encoded as parser restrictions unless the grammar explicitly encodes them.
Examples of semantic restrictions include duplicate modifiers, incompatible modifier combinations, unknown names,
invalid types, invalid traits, and context-specific callable body requirements.

The parser may use syntax context to choose between grammar alternatives.
It must not use name resolution, type information, target facts, package facts, or later compiler phase state.

---

## Parser Shape

The parser is a recursive descent parser.

As a default rule, each grammar nonterminal has one private `parse_` method on `Parser`.
A grammar name like `source-module-declaration` maps to a method like `parse_source_module_declaration`.

Shared grammar shapes may use one shared implementation when the behavior is genuinely the same.
In that case, prefer either:

- a small named wrapper for each grammar root that calls the shared implementation, when the root is important to
  readability or typed syntax shape,
- one shared method with a typed context argument, when the grammar roots are the same parser operation with different
  accepted child roots or terminators.

Do not copy a large parser body only to change one token kind, node kind, or terminator set.
Use shared helpers, typed context objects, or local macros when they prevent real duplication while preserving clear
grammar ownership.

`parse_` methods should focus on the production they implement.
They should not inline generic token skipping, list handling, delimiter recovery, or ambiguity scanning logic.

---

## Expressions

Expression parsing uses precedence climbing.

The expression grammar in `syntax-grammar.md` remains the contract.
The precedence-climbing implementation is an implementation strategy for the expression ladder, not permission to accept
operators or groupings outside the grammar.

Expression roots such as `expression`, `condition-expression`, `constant-expression`, `predicate-expression`,
`type-expression`, and restricted expression roots must keep distinct parser entry points when the grammar gives them
distinct meanings.
Those entry points may share a precedence-climbing engine when their accepted operators and operands are the same.

The precedence table must be derived from the grammar and from the compiler-known operator set.
Associativity must be represented explicitly.
Right-associative forms such as assignment and exponentiation must not accidentally become left-associative because of a
generic loop.

Postfix parsing should be modeled as repeated postfix operations after a primary or access root, matching the grammar.
Prefix parsing should parse the operand at the correct binding strength.

Ambiguous expression starts must be resolved with syntax lookahead only.
For example, an `identifier "="` prefix in an argument list is a named argument, while a positional assignment
expression with that shape must be grouped as the grammar requires.

---

## Cursor And Tokens

The parser must consume tokens only through the parser cursor.

Parser code should use cursor-backed helpers such as:

- `peek`,
- `lookahead`,
- `at`,
- `consume`,
- `consume_if`,
- `expect`,
- recovery helpers built on the cursor.

No parser API should require eager tokenization of a whole source unit.
Lexing stays demand-driven.
Repeated lookahead must be stable.
EOF behavior must be repeatable and panic-free.

Lexical diagnostics produced while demanding tokens remain attached to the parser result.
Speculative scans may demand lexical tokens, but lexical diagnostics from those demanded tokens must be deterministic
and deduplicated.

At parser level, terminator and delimiter policy should be expressed in `SyntaxKind` sets, not raw characters.

---

## Syntax Output

The parser produces immutable green syntax through builders.

Syntax trees must preserve exact source reconstruction for present source text.
Whitespace and comments remain token trivia.
Skipped tokens remain present source text attached through skipped syntax.
Missing tokens are zero-width syntax tokens with the expected `SyntaxKind`.

Typed syntax APIs should expose recovery state.
Later phases must be able to detect missing tokens and skipped syntax without depending on public green internals.

Parser code should build syntax in source order.
Do not create public generic child-list APIs just to make parser construction convenient.
Use concrete typed nodes and shared internal helpers or macros where repeated grammar shapes need reuse.

---

## Recovery

Recovery is grammar-neutral at the helper layer and grammar-specific at the call site.

Use shared helpers for common operations such as:

- expecting a token and inserting a missing token when it is absent,
- recovering until one of several caller-provided terminators,
- recovering until a closing brace,
- recovering until a closing parenthesis,
- recovering until a closing bracket,
- recovering until comma, semicolon, closing delimiter, or EOF,
- attaching skipped tokens as skipped syntax at the recovery point.

Do not inline recovery loops in individual `parse_` methods.
If a `parse_` method needs a new recovery pattern, first decide whether it is a reusable parser helper.

Recovery loops must make progress.
Every loop that can see malformed input must either consume a token, insert a missing token and return, or stop at a
deterministic terminator.
EOF recovery must be deterministic.

Terminator sets should include EOF when the caller can finish at end of source.
Delimiter-specific recovery should stop before the delimiter so the caller can consume or expect it in the normal
grammar position.

Skipped-syntax diagnostics cover the skipped source range.
Missing-token diagnostics use an insertion-point span.

---

## Lists

Separated lists should use shared parser infrastructure.

The caller supplies:

- the item parser,
- the separator kind,
- the terminator set,
- the recovery set,
- whether a trailing separator is allowed,
- the concrete typed node or builder shape.

The implementation must preserve:

- item nodes,
- present separator tokens,
- missing separator tokens,
- trailing separators where the grammar allows them,
- skipped syntax,
- exact reconstruction for present source.

Do not duplicate list parser logic across every grammar list.
Use macros or typed internal helpers when most of the code would otherwise be identical.

---

## Ambiguity And Speculation

Use parser checkpoints or scan forks for syntax lookahead decisions.

Speculative scans are for syntax decisions only.
They must not perform semantic decisions.

Abandoned scans must not commit:

- parser cursor position,
- parser diagnostics,
- recovery nodes,
- syntax nodes,
- builder state.

Committed parsing must still flow through the normal cursor and builder path.

If a speculative scan demands lexical tokens, lexical diagnostics for those demanded tokens remain deterministic and
deduplicated in the final parser result.

---

## Diagnostics

The parser owns syntax diagnostics while parsing.
Lexical diagnostics and syntax diagnostics are merged deterministically when a parse result is materialized.
Compilation-level diagnostic bags should merge parser diagnostics lazily when a compiler API requests diagnostics.

Parser logic must emit structured diagnostics.
It must not construct user-facing English text.

Use standard cursor helper diagnostics whenever possible.
In ordinary parser code, prefer `expect` and shared recovery helpers instead of manually constructing diagnostics.

Add a custom syntax diagnostic only for a non-standard situation where the existing helper diagnostics would be unclear
or structurally incomplete.
Custom diagnostics must use the diagnostics framework for structured data and `bray-messages` for locale-aware
rendering.

Diagnostics should carry typed arguments such as expected syntax kinds, actual syntax kinds, token spellings, counts,
and source spans.
Do not pre-render or quote diagnostic arguments for English inside parser code.

---

## Comments And Naming

Method bodies should be clear from structure and names.
Add comments only for unclear, complex, or ambiguous code.

Comments in parser implementation should use ordinary keyboard characters only.
Use ASCII double quotes.
Do not use curly quotation marks.
Do not use em dashes.
Do not use semicolons in comments.

Names should match grammar concepts unless a shared helper names a lower-level parser operation.
Avoid abbreviations in parser APIs that are visible beyond a small local scope.

---

## Ownership

Avoid cloning in parser code.

Prefer borrowing syntax data, token data, snapshots, context objects, and terminator sets.
If cloning is truly necessary, keep the clone narrow and add a short comment explaining why that ownership boundary is
needed.

Do not hide expensive ownership movement behind helper names that sound like cheap observation.
Parser helpers that can demand tokens may take `&mut self` and should not use `is_` names.

---

## Tests

Parser changes should include focused tests near the parser code.

For each meaningful grammar feature, cover:

- valid source reconstruction,
- missing expected tokens,
- skipped-token recovery,
- list separators and trailing separators when relevant,
- ambiguous syntax decisions when relevant,
- EOF behavior when recovery can reach EOF,
- structured diagnostic kind, span, severity, and typed arguments when diagnostics change.

Rendered diagnostic text belongs at the terminal rendering or `bray-messages` boundary.
Parser tests should usually assert structured diagnostics.

Tests for malformed input should verify that ordinary malformed source does not panic.

---

## Implementation Checklist

When adding parser support for a grammar production:

1. Confirm the production in `syntax-grammar.md` and `syntax-grammar.ebnf`.
2. Add or update syntax kinds and typed syntax APIs before parser code depends on them.
3. Add the `parse_` method or a clearly named shared helper.
4. Consume tokens only through the parser cursor.
5. Use `expect` for required tokens.
6. Use shared recovery helpers for malformed input.
7. Use checkpoints for ambiguity.
8. Preserve exact reconstruction.
9. Preserve missing and skipped syntax in the tree.
10. Keep diagnostics structured and locale-neutral.
11. Add tests for valid input and recovery behavior.
12. Re-run the relevant formatter, compile check, and tests.
