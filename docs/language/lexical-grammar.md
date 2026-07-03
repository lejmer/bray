# Lexical grammar

This chapter explains Bray source text, lexical tokens, trivia, comments, identifiers, keywords, and literal spellings.

The companion plain EBNF reference is `lexical-grammar.ebnf`.

The source grammar is defined separately in `syntax-grammar.md` and `syntax-grammar.ebnf`.

Lexical grammar defines token spelling. Literal typing, literal adaptation, scalar operations, and string value semantics belong to
the scalar and literal rules.

---

## Source text

Bray source text is UTF-8.

An optional UTF-8 byte order mark is accepted only at the beginning of a source file and is ignored.

Source text is not Unicode-normalized by the compiler.

Line endings accepted by the lexer are LF and CRLF. The compiler treats both as line breaks for source locations.

A lone CR is not a line ending. It is valid only as ordinary text inside block comments.

```ebnf
source-text =
    [ utf8-bom ] { lexical-item } end-of-file ;

lexical-item =
      token
    | trivia ;

utf8-bom =
    ? U+FEFF at the beginning of the source text ? ;

end-of-file =
    ? end of source text ? ;
```

---

## Tokenization

The lexer uses longest-token matching.

Tuple element indices use a context-specific scan after member-access `.` as described below.

When two token kinds have the same spelling length, the more specific token kind wins:

- keywords win over identifiers,
- documentation comments win over ordinary comments,
- imaginary literals win over the numeric literal they contain.

Whitespace and comments separate tokens and are otherwise trivia.

Trivia is preserved by the syntax layer as leading and trailing trivia on syntax tokens so source text can be recreated from syntax
trees.

Trivia is not represented as ordinary syntax nodes.

Adjacent tokens are valid only when the resulting character sequence cannot be read as a longer token or invalid token.

For example, `1 i` is an integer literal followed by identifier `i`, while `1i` is an imaginary literal.

`1i32`, `1u8`, and `1.0r64` are invalid numeric-literal spellings.

A keyword spelling is a keyword only when the full token exactly matches the keyword. For example, `if` is a keyword and `ifx`
is an identifier.

```ebnf
token =
      keyword
    | identifier
    | literal-token
    | tuple-element-index
    | operator-or-punctuation-token ;
```

---

## Whitespace

```ebnf
trivia =
      whitespace
    | documentation-comment
    | comment ;

whitespace =
      " "
    | "\t"
    | line-break ;

line-break =
      "\n"
    | "\r\n" ;
```

Only ASCII space, horizontal tab, LF, and CRLF are whitespace outside comments and literals.

Other Unicode space characters are invalid outside comments and literals.

---

## Comments

Bray has line comments, nested block comments, line documentation comments, and block documentation comments.

```ebnf
comment =
      line-comment
    | block-comment ;

documentation-comment =
      documentation-line-comment
    | documentation-block-comment ;

line-comment =
    "//" { not-line-break } ;

documentation-line-comment =
    "///" { not-line-break } ;

block-comment =
    "/*" { block-comment-item } "*/" ;

documentation-block-comment =
    "/**" { block-comment-item } "*/" ;

block-comment-item =
      block-comment
    | not-block-comment-delimiter ;
```

Line comments and line documentation comments end before the line break or at end of file.

Block comments can nest.

Documentation comments are retained as structured trivia for documentation tooling.

A contiguous documentation-comment group immediately before a declaration, separated from it only by whitespace and ordinary
comments, documents that declaration.

Documentation comments are valid before the file-scoped module declaration.

---

## Identifiers

Identifiers are ASCII and case-sensitive.

```ebnf
identifier =
    ascii-letter { ascii-letter | decimal-digit | "_" } ;
```

An identifier starts with an ASCII letter and can contain ASCII letters, decimal digits, and underscores after the first character.

The single underscore token `_` is a discard pattern token, not an identifier.

Keywords are not identifiers.

Directive names are not keywords. They are identifier spellings used after `@` in directive syntax.

Examples:

```bray
parse_int
BufferReader
value2
```

---

## Keywords

The following words are reserved keywords:

```ebnf
keyword =
      "all"
    | "any"
    | "as"
    | "assert"
    | "async"
    | "await"
    | "box"
    | "break"
    | "callable"
    | "case"
    | "catch"
    | "const"
    | "construct"
    | "consume"
    | "continue"
    | "destruct"
    | "detached"
    | "each"
    | "else"
    | "ensures"
    | "enter"
    | "exit"
    | "export"
    | "extern"
    | "false"
    | "finalize"
    | "for"
    | "func"
    | "if"
    | "impl"
    | "in"
    | "internal"
    | "lambda"
    | "let"
    | "loop"
    | "match"
    | "module"
    | "move"
    | "mut"
    | "none"
    | "overload"
    | "panic"
    | "pos"
    | "predicate"
    | "public"
    | "requires"
    | "return"
    | "self"
    | "Self"
    | "spawn"
    | "static"
    | "struct"
    | "thread"
    | "trait"
    | "trusted"
    | "true"
    | "try"
    | "type"
    | "union"
    | "unit"
    | "using"
    | "uses"
    | "view"
    | "when"
    | "while"
    | "with"
    | "yield" ;
```

`self` and `Self` are distinct keywords.

Built-in scalar and compiler-known type names such as `i32`, `bool`, `char`, `string`, and `never` are compiler-known
declarations, not lexical keywords.

---

## Numeric literals

Numeric literal spelling is lexical.

Numeric literal typing and adaptation belong to the scalar and literal rules.

```ebnf
literal-token =
      numeric-literal
    | character-literal
    | string-literal ;

numeric-literal =
      imaginary-literal
    | real-literal
    | integer-literal ;

integer-literal =
      decimal-integer-literal
    | binary-integer-literal
    | hexadecimal-integer-literal ;

decimal-integer-literal =
    decimal-digit-sequence ;

binary-integer-literal =
    "0" ( "b" | "B" ) binary-digit-sequence ;

hexadecimal-integer-literal =
    "0" ( "x" | "X" ) hexadecimal-digit-sequence ;

real-literal =
      decimal-digit-sequence "." decimal-digit-sequence [ exponent-part ]
    | decimal-digit-sequence exponent-part ;

imaginary-literal =
    ( real-literal | integer-literal ) "i" ;

exponent-part =
    ( "e" | "E" ) [ "+" | "-" ] decimal-digit-sequence ;
```

Decimal integer literals use decimal digits.

Binary integer literals use `0b` or `0B`.

Hexadecimal integer literals use `0x` or `0X`.

Real literals are decimal. A real literal either has digits on both sides of `.` or has an exponent part.

`.5` and `1.` are not real literals.

Digit separators use `_` between digits.

Separators cannot appear at the start or end of a digit sequence and cannot appear next to another separator.

Numeric literal suffixes are not part of Bray syntax, except for the `i` suffix that forms an imaginary literal.

---

## Character literals

```ebnf
character-literal =
    "'" character-literal-body "'" ;

character-literal-body =
      character-literal-scalar
    | character-escape-sequence ;

character-literal-scalar =
    ? any Unicode scalar value except single quote, backslash, U+000A, or U+000D ? ;
```

A character literal is enclosed in single quotes.

After escape processing, a character literal must contain exactly one Unicode scalar value.

---

## String literals

```ebnf
string-literal =
    "\"" { string-literal-item } "\"" ;

string-literal-item =
      string-literal-scalar
    | string-escape-sequence ;

string-literal-scalar =
    ? any Unicode scalar value except double quote, backslash, U+000A, or U+000D ? ;
```

A string literal is enclosed in double quotes.

A line break cannot appear unescaped inside a string literal.

String literals perform no interpolation.

---

## Escapes

```ebnf
character-escape-sequence =
      string-escape-sequence
    | "\\'" ;

string-escape-sequence =
      "\\\""
    | "\\\\"
    | "\\n"
    | "\\r"
    | "\\t"
    | "\\0"
    | unicode-escape ;

unicode-escape =
    "\\u{" unicode-escape-digits "}" ;

unicode-escape-digits =
    hexadecimal-digit { hexadecimal-digit } ;
```

The valid string-literal escape sequences are:

- `\"` for a double quote,
- `\\` for a backslash,
- `\n` for newline,
- `\r` for carriage return,
- `\t` for tab,
- `\0` for Unicode scalar value U+0000,
- `\u{H...}` for a Unicode scalar value written with hexadecimal digits.

The valid character-literal escape sequences are the string-literal escape sequences plus `\'`.

A Unicode escape has one to six hexadecimal digits and must denote a valid Unicode scalar value.

Unknown escape sequences are invalid.

---

## Tuple Element Indices

Tuple element access uses a decimal tuple element index after `.`.

```ebnf
tuple-element-index =
      "0"
    | nonzero-decimal-digit { decimal-digit } ;
```

Tuple element indices do not use separators.

Leading zeroes are invalid except for the single index `0`.

A decimal spelling is treated as a tuple element index only in the member-selector position after `.`.

In ordinary expression positions, the same character shape is an integer literal.

After a member-access `.`, the parser performs the tuple-index scan before ordinary numeric-literal scanning.

This makes `entry.1.0` parse as `entry` followed by tuple element `1` and then tuple element `0`.

---

## Operators And Punctuation

```ebnf
operator-or-punctuation-token =
      "->"
    | "=="
    | "!="
    | "<="
    | ">="
    | "&&"
    | "||"
    | "<<"
    | ">>"
    | "**"
    | ".."
    | "("
    | ")"
    | "{"
    | "}"
    | "["
    | "]"
    | ","
    | ";"
    | ":"
    | "."
    | "?"
    | "="
    | "+"
    | "-"
    | "*"
    | "/"
    | "%"
    | "@"
    | "&"
    | "|"
    | "^"
    | "~"
    | "!"
    | "<"
    | ">"
    | "_" ;
```

The `@` token introduces directives in directive contexts and is the matrix/dot-product binary token in expression contexts.

The parser decides the role from grammar context.

---

## Invalid Tokens

The lexer rejects invalid source text before parsing.

Invalid lexical forms include:

- invalid UTF-8,
- `U+FEFF` outside the beginning of the source text,
- a lone CR outside block comments,
- unrecognized characters outside comments and literals,
- unterminated block comments,
- unterminated character literals,
- unterminated string literals,
- unknown escape sequences,
- Unicode escapes that do not denote Unicode scalar values,
- malformed numeric literals,
- numeric suffixes other than the imaginary `i` suffix,
- non-ASCII identifier characters.

Invalid tokens are rejected at the source range that forms or attempts to form the token.

---

## Character Classes

```ebnf
ascii-letter =
      "A" | "B" | "C" | "D" | "E" | "F" | "G" | "H" | "I" | "J"
    | "K" | "L" | "M" | "N" | "O" | "P" | "Q" | "R" | "S" | "T"
    | "U" | "V" | "W" | "X" | "Y" | "Z"
    | "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j"
    | "k" | "l" | "m" | "n" | "o" | "p" | "q" | "r" | "s" | "t"
    | "u" | "v" | "w" | "x" | "y" | "z" ;

decimal-digit =
      "0" | "1" | "2" | "3" | "4"
    | "5" | "6" | "7" | "8" | "9" ;

nonzero-decimal-digit =
      "1" | "2" | "3" | "4"
    | "5" | "6" | "7" | "8" | "9" ;

binary-digit =
      "0" | "1" ;

hexadecimal-digit =
      decimal-digit
    | "A" | "B" | "C" | "D" | "E" | "F"
    | "a" | "b" | "c" | "d" | "e" | "f" ;
```
