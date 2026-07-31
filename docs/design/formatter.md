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

The formatter uses syntax node context and source-order token traversal.

- Indentation is four spaces.
- Braces use the language documentation's block layout.
- Module declarations are separated by one empty line.
- Struct construction and overload-arm bodies place comma-separated entries on
  separate lines.
- Parenthesized and bracketed lists remain inline unless comments require a
  line break.
- Token spellings, literal spellings, and comment text are copied exactly from
  the source snapshot.
- Ordinary whitespace is reconstructed from syntax context. Source whitespace
  still controls blank-line placement around comments.

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
