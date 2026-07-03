# Bray language specification

Bray programs are defined by their lexical grammar, syntax grammar, and semantic rules.

This reference describes the source language accepted by a conforming Bray compiler and the observable meaning of valid Bray
programs.

---

## Contents

The chapters are organized from source text to program meaning:

1. [Lexical grammar](lexical-grammar.md)
2. [Syntax grammar](syntax-grammar.md)
3. [Concepts](concepts.md)
4. Declarations
5. [Types](types.md)
6. [Patterns](patterns.md)
7. [Callables](callables.md)
8. [Expressions](expressions.md)
9. Ownership and borrowing
10. Contracts and trust
11. Lifecycle
12. Async and concurrency
13. Modules and packages
14. Compiler-known declarations and standard library recognition
15. Targets, layout, ABI, and raw memory
16. Diagnostics

The lexical and syntax grammar chapters define how source text is tokenized and parsed.

The semantic chapters define what parsed programs mean and which parsed forms are valid Bray.

---

## Grammar References

The Markdown grammar chapters explain the grammar and edge cases for readers.

The `.ebnf` files provide the same grammar in plain EBNF form:

- [lexical-grammar.ebnf](lexical-grammar.ebnf)
- [syntax-grammar.ebnf](syntax-grammar.ebnf)
