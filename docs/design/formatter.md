# Source formatter

## Ownership

`bray-formatter` owns deterministic Bray source layout. It depends on the parser
and lossless syntax tree, but it does not change parser recovery, perform
semantic validation, or own project and command-line policy.

The reusable entry points are:

- `format_source_unit` for callers that already own parsed syntax,
- `format_text` for editor and standard-input text,
- `format_file` for typed check and write operations over one UTF-8 source file.

The project CLI can aggregate those operations across manifest-owned source
files without moving formatter policy into the command layer.

## Layout

The formatter implements the canonical policies in the
[Bray source style guide](../contributing/bray-style-guide.md). The style guide owns source conventions, while this document owns
the formatter architecture and behavior required to apply them.

The formatter uses syntax node context and source-order token traversal. Ordinary formatting preserves the ordered non-trivia
token stream. Token spellings, literal spellings, and comment text are copied exactly from the source snapshot. Ordinary
whitespace is reconstructed from syntax context, while source whitespace still controls blank-line placement around comments.
Syntax-changing transformations are separate, explicitly enabled rewrite rules with stronger correctness requirements.

The maximum width is a layout target rather than permission to rewrite source tokens. An indivisible token, preserved comment,
or other source text without a legal breakpoint may exceed it. Width-aware layout uses groups, indentation, required breaks, and
optional breaks so wrapping remains deterministic and idempotent instead of relying on local column checks scattered throughout
syntax formatting code.

## Formatting rules and configuration

Every independently enforceable formatting behavior has a stable rule name. This includes indentation, brace placement, spacing,
blank-line placement, list layout, wrapping, comments, final newlines, line endings, and optional syntax rewrites.

The formatter provides a default configuration and accepts an immutable caller-provided configuration containing:

- the maximum line width, which defaults to 120 display columns,
- explicit enabled or disabled overrides keyed by formatting rule name,
- parameters owned by individual rules when a Boolean setting is insufficient.

A caller can disable any rule enabled by default or enable a rule disabled by default. Unknown rule names and invalid rule
parameters are configuration errors and are not silently ignored.

Disabling a layout rule means that the formatter does not enforce that policy. It does not mean that the formatter enforces the
opposite policy. Where the relevant trivia can be retained independently of enabled rules, the formatter preserves the source
layout.

Rule interaction and precedence must be deterministic and documented. Formatting the same source with the same configuration
always produces the same result, and formatting that result again makes no changes.

Initial rule names include:

- `indentation`,
- `block-braces`,
- `module-item-spacing`,
- `callable-member-spacing`,
- `directive-line-breaks`,
- `block-paragraph-spacing`,
- `match-case-spacing`,
- `struct-construction-layout`,
- `overload-arm-layout`,
- `parenthesized-list-layout`,
- `bracketed-list-layout`,
- `trailing-comma-layout`,
- `comma-spacing`,
- `colon-spacing`,
- `operator-spacing`,
- `generic-delimiter-spacing`,
- `member-access-spacing`,
- `range-spacing`,
- `prefix-operator-spacing`,
- `semicolon-layout`,
- `comment-placement`,
- `line-wrapping`,
- `line-ending-style`,
- `final-newline`,
- `simplify-nested-if`.

This registry grows when another independently configurable behavior is introduced. A broad rule must not hide unrelated style
decisions merely to avoid assigning them stable names.

An explicitly selected formatter configuration is a JSON object with an optional positive `maximum_line_width` and an optional
`rules` object whose keys are stable rule names and whose values are Booleans:

```json
{
  "maximum_line_width": 100,
  "rules": {
    "line-wrapping": true,
    "simplify-nested-if": false
  }
}
```

Omitted values use formatter defaults. Unknown top-level properties, unknown rule names, non-Boolean rule values, and maximum
line widths outside the range 1 through 65535 are configuration errors.

Configuration-file discovery and workspace policy do not belong in `bray-formatter`. Its APIs receive resolved configuration.
`brayfmt` accepts explicit formatter configuration, while Bray Tack can resolve workspace-owned configuration before invoking the
formatter executable.

## Optional syntax rewrites

An optional syntax rewrite may change non-trivia tokens only when its rule is explicitly enabled and the formatter can prove that
the replacement preserves program behavior. Failure to prove equivalence leaves the original syntax unchanged. A rewrite must
also preserve comments without changing which construct they document.

`simplify-nested-if` is disabled by default. It can combine nested conditional expressions only when all of the following are
preserved:

- condition evaluation order and short-circuit behavior,
- the result expected from the conditional expression,
- lexical scope and the lifetime of values and temporaries,
- constructor, destructor, finalizer, and scope lifecycle behavior,
- control-flow behavior,
- comment ownership and placement.

The rule may retain explicit inner blocks when those blocks are necessary to preserve scope and lifecycle behavior. It must leave
the nested form unchanged when an available syntax-local proof is insufficient. Formatting must not trigger binding or whole
program semantic analysis merely to make an optional rewrite apply.

The first LF or CRLF line ending in a source selects the output line ending.
Sources without a line ending use LF. A leading UTF-8 byte order mark is not
part of syntax text, but `format_file` retains it when writing a changed file.

## Recovery

A source unit containing a missing token, invalid token, or skipped-syntax node
is returned unchanged. This makes malformed and unsupported recovery regions
strictly lossless and guarantees idempotence even when changing nearby
whitespace would otherwise alter parser recovery boundaries.

Parser diagnostics remain parser-owned structured diagnostics. The formatter
does not render or add user-facing English diagnostics.

## Command integration

The standalone `brayfmt` executable exposes the reusable formatter operations:

- Standard input calls `format_text` and writes formatted text to standard
  output in write mode.
- File write mode calls `format_file` with `FormatMode::Write`.
- Check mode calls `format_text` or `format_file` without publishing changes and
  fails when any result reports changed source.
- Typed formatter failures are converted to path, I/O category, size, and
  formatting-status diagnostic arguments rendered through `bray-messages`.

`brayfmt` owns formatter argument selection, terminal I/O, and process exit
status. Bray Tack owns manifest source discovery and invokes `brayfmt` with the
explicit selected files or standard-input stream. The formatter command
converts typed failures into locale-neutral diagnostics, and `bray-messages`
renders them.
