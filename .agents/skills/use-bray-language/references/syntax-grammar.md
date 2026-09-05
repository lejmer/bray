# Syntax Grammar

Consult the syntax grammar for parsing, formatting, syntax recovery, and proposed source forms.

**Authorities:** [Syntax grammar](https://github.com/lejmer/bray/blob/develop/docs/language/syntax-grammar.md) and [plain syntax EBNF](https://github.com/lejmer/bray/blob/develop/docs/language/syntax-grammar.ebnf)

## Decisive rules

- The grammar uses EBNF. Double quotes mark terminals, lowercase hyphenated names mark non-terminals, `|` selects alternatives, brackets make syntax optional, braces repeat syntax, and parentheses group productions.
- Lexical spelling is defined separately. Read the lexical grammar when the question concerns token boundaries, identifiers, literals, comments, or invalid characters.
- Grammar acceptance establishes source shape, not semantic validity. Type rules, ownership, contracts, modifier compatibility, duplicate restrictions, and availability are checked separately.
- Package products, selected source and dependency graphs, package identity, and target profiles are semantic inputs rather than source syntax.
- A source-unit module declaration can be followed by unbraced module items and then braced module declarations. Once the braced suffix begins, every remaining top-level declaration is another braced module declaration.
- `expression` accepts every expression form, while restricted expression roots deliberately accept less in particular contexts so the parser can produce a precise tree.
- Precedence and associativity follow the grammar structure. Binary levels use repetition for left association, right-associative and prefix forms use right recursion, and postfix forms use repetition.
- `subject matches case-pattern` has non-associative comparison precedence. Its pattern owns `|` alternatives.
- `if` and `while` accept chains of ordinary boolean operands and `let case-pattern = comparison-expression` operands separated by `&&`. Write `if let ?value = input && value > 0`. A `let` operand is not parenthesized and cannot occur under `||` or negation. Group an initializer or ordinary operand when it contains boolean operators.
- A range expression supplies both bounds around one top-level `..`. Inside square brackets, a top-level `..` selects slicing and a grouped range is an ordinary element selector.

**Remember:** Use the formal grammar for exhaustive parser or formatter work, and use feature references for ordinary authoring, semantics, and validity.
