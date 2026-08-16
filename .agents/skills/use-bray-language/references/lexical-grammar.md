# Lexical Grammar

The lexical grammar decides how Bray source text becomes tokens before parsing, so consult it for spelling, trivia, token boundaries, and lexer failures rather than type or value semantics.

**Authorities:** [Lexical grammar](https://github.com/lejmer/bray/blob/develop/docs/language/lexical-grammar.md) and [plain lexical EBNF](https://github.com/lejmer/bray/blob/develop/docs/language/lexical-grammar.ebnf)

## Decisive rules

- Source text is UTF-8 and is not Unicode-normalized. An optional byte order mark is accepted only at the beginning.
- Only ASCII space, horizontal tab, LF, and CRLF are whitespace outside comments and literals. A lone CR is not a line ending.
- Tokenization uses longest-token matching. Keywords win over identifiers at equal length, while a longer identifier such as `ifx` remains an identifier.
- Bray has line comments, nested block comments, and structured line or block documentation comments.
- Identifiers are ASCII and case-sensitive. They start with a letter, `_` alone is a discard token, and directive names are ordinary identifier spellings after `@`.
- Integer literals can be decimal, binary, or hexadecimal. Real literals are decimal, digit separators must occur between digits, and type suffixes are invalid. Only trailing `i` forms an imaginary literal.
- Character literals contain exactly one scalar after escape processing. Strings do not interpolate, cannot contain unescaped line breaks, and accept only the specified escapes.
- A decimal spelling after member access `.` is scanned as a tuple element index. Leading zeroes are invalid except for index `0`.
- Lexically invalid text is rejected before parsing, including malformed literals, unknown escapes, non-ASCII identifiers, and unterminated strings or comments.

**Remember:** Use this index to settle how characters form tokens. Use the feature references to determine what those tokens mean.
