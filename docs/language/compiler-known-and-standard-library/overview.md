# Overview

Bray distinguishes compiler-known declarations from standard-library declarations.

The compiler can reason about both categories, but they enter name resolution differently.

Compiler-known names supplied directly in the ambient scope are available without `using`, subject to target
availability rules. References into named compiler-known modules, such as `core.memory`, require a matching `using`
under the ordinary cross-module dependency rules.

Standard-library declarations are ordinary declarations supplied by standard-library packages and must be visible
through normal import and path rules before source code can use them.

This split keeps the language core small while still allowing the compiler to understand selected library contracts
precisely.

Compiler-known means the compiler has a language-defined contract for the declaration.

Standard-library recognition means the compiler recognizes a visible standard-library declaration by stable declaration
identity.

Recognition does not make a standard-library declaration ambient.

The compiler recognizes identity, not spelling.

The same name in another package does not imply the same contract.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Next: [Compiler-known declarations](compiler-known-declarations.md)
