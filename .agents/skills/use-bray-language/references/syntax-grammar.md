# Syntax Grammar

The syntax grammar defines which token sequences form Bray source trees, so consult it when implementing or reviewing parsing, formatting, syntax recovery, or a proposed source form.

**Authorities:** [Syntax grammar](https://github.com/lejmer/bray/blob/develop/docs/language/syntax-grammar.md) and [plain syntax EBNF](https://github.com/lejmer/bray/blob/develop/docs/language/syntax-grammar.ebnf)

## Decisive rules

- The grammar uses EBNF. Double quotes mark terminals, lowercase hyphenated names mark non-terminals, `|` selects alternatives, brackets make syntax optional, braces repeat syntax, and parentheses group productions.
- Lexical spelling is defined separately. Read the lexical grammar when the question concerns token boundaries, identifiers, literals, comments, or invalid characters.
- Grammar acceptance establishes source shape, not semantic validity. Type rules, ownership, contracts, modifier compatibility, duplicate restrictions, and availability are checked separately.
- Package products, selected source and dependency graphs, package identity, and target profiles are semantic inputs rather than source syntax.
- A source-unit module declaration can be followed by unbraced module items and then braced module declarations. Once the braced
  suffix begins, every remaining top-level declaration is another braced module declaration.
- `expression` accepts every expression form, while restricted expression roots deliberately accept less in particular contexts so the parser can produce a precise tree.
- Precedence and associativity follow the grammar structure. Binary levels use repetition for left association, right-associative and prefix forms use right recursion, and postfix forms use repetition.
- Use the formal grammar for exhaustive parser or formatter work. Use the routed feature reference for ordinary Bray authoring and for the semantics of a parsed form.

**Remember:** The grammar answers what parses. The feature specifications answer what a parsed form means and whether it is valid.
