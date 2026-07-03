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
4. [Modules and packages](modules-and-packages.md)
5. Declarations
6. [Types](types.md)
7. [Patterns](patterns.md)
8. [Callables](callables.md)
9. [Expressions](expressions.md)
10. Ownership and borrowing
11. Contracts and trust
12. Lifecycle
13. Async and concurrency
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
