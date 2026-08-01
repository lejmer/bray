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
5. [Declarations](declarations.md)
6. [Types](types.md)
7. [Patterns](patterns.md)
8. [Callables](callables.md)
9. [Expressions](expressions.md)
10. [Ownership and borrowing](ownership-and-borrowing.md)
11. [Contracts and trust](contracts-and-trust.md)
12. [Lifecycle](lifecycle.md)
13. [Async and concurrency](async-and-concurrency.md)
14. [Compiler-known declarations and standard library recognition](compiler-known-and-standard-library.md)
15. [I/O and platform services](io-and-platform-services.md)
16. [Targets, layout, ABI, and raw memory](targets-layout-abi-and-raw-memory.md)

The lexical and syntax grammar chapters define how source text is tokenized and parsed.

The semantic chapters define what parsed programs mean and which parsed forms are valid Bray.

---

## Grammar References

The Markdown grammar chapters explain the grammar and edge cases for readers.

The `.ebnf` files provide the same grammar in plain EBNF form:

- [lexical-grammar.ebnf](lexical-grammar.ebnf)
- [syntax-grammar.ebnf](syntax-grammar.ebnf)
