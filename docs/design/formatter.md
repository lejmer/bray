# Source formatter design

`bray-formatter` owns deterministic source layout over the parser's lossless syntax. The [Bray style
guide](../contributing/bray-style-guide.md) owns the formatting conventions. Project selection and command-line policy
belong to the tools that call the formatter.

## Syntax-preserving layout

Ordinary formatting preserves the ordered non-trivia token stream, literal spellings, and comments. Syntax context
determines whitespace and legal breakpoints. Source relationships around comments remain part of the layout constraints.

Width-aware layout uses groups, indentation, required breaks, and optional breaks. A shared layout engine chooses among
legal breaks instead of scattering column checks through grammar-specific code. An indivisible token or preserved
comment can exceed the width target.

The same source and resolved configuration produce the same output. Formatting that output again makes no changes.

## Configuration and rule composition

The formatter receives immutable, resolved configuration. Independently configurable policies have stable rule
identities and explicit defaults. The rule model describes owned decisions, parameters, dependencies, and conflicts, so
configuration does not depend on incidental registry order.

Recovery preservation takes precedence over formatting. Optional syntax rewrites precede layout. Comment ownership and
structural breaks constrain groups, spacing operates within those groups, and wrapping chooses among legal breaks.
Output encoding and newline policy serialize the result.

Workspace configuration discovery belongs to Bray Tack. `brayfmt` decodes explicit formatter configuration and invokes
the reusable formatter. The formatter library does not search for project files.

## Optional rewrites and recovery

Syntax-changing rewrites are separate, explicitly enabled operations. They require a syntax-local proof that evaluation,
scope, lifecycle, control flow, and comment ownership are preserved. An unproven rewrite leaves the original form
intact, without invoking semantic analysis to justify a formatting change.

Recovered source remains lossless. The formatter preserves a recovered source unit rather than letting whitespace edits
change its recovery boundaries. Parser diagnostics stay parser-owned.

## Library and tool boundaries

Parsed-source, text, and file entry points share the same formatter. File operations add typed I/O and write/check
outcomes around that core.

The standalone `brayfmt` executable owns arguments, terminal I/O, and exit status. Bray Tack selects manifest-owned
files and forwards the selected configuration. Typed failures cross these boundaries as structured diagnostics rendered
through `bray-messages`.

## Related documents

- [Parser](parser.md)
- [Bray Tack](bray-tack.md)
- [Bray style guide](../contributing/bray-style-guide.md)
- [Formatter configuration](../tools/formatter.md)
