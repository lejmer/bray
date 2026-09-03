# Lexical Grammar

Consult the lexical grammar for spelling, trivia, token boundaries, and lexer failures, not type or value semantics.

**Authorities:** [Lexical grammar](https://github.com/lejmer/bray/blob/develop/docs/language/lexical-grammar.md) and [plain lexical EBNF](https://github.com/lejmer/bray/blob/develop/docs/language/lexical-grammar.ebnf)

## Decisive rules

- Source text is UTF-8 without Unicode normalization. An optional byte order mark is accepted only at the beginning.
- Only ASCII space, horizontal tab, LF, and CRLF are whitespace outside comments and literals. A lone CR is not a line ending.
- Tokenization uses longest matching. Keywords win over identifiers at equal length, while a longer spelling such as `ifx` remains an identifier.
- `matches` is reserved for structural pattern tests. Longer identifiers such as `matches_value` remain identifiers.
- Bray has line comments, nested block comments, and structured line or block documentation comments.
- Identifiers are ASCII and case-sensitive, and they start with a letter. `_` alone is a discard token, and directive names are ordinary identifier spellings after `@`.
- Integer literals can be decimal, binary, or hexadecimal. Real literals are decimal, digit separators must occur between digits, and type suffixes are invalid. Only trailing `i` forms an imaginary literal.
- Character literals contain exactly one scalar after escape processing. Strings do not interpolate, cannot contain unescaped line breaks, and accept only the specified escapes.
- A decimal spelling after member access `.` is scanned as a tuple element index. Leading zeroes are invalid except for index `0`.
- Lexically invalid text is rejected before parsing, including malformed literals, unknown escapes, non-ASCII identifiers, and unterminated strings or comments.

**Remember:** This index defines tokens. Feature references define their meaning.
