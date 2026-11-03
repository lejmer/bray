# Overview

Bray distinguishes compiler-known declarations from ordinary standard-library declarations by how they enter name
resolution and how the compiler identifies their contracts.

A compiler-known declaration has a language-defined contract. Compiler-known names supplied in the ambient scope are
available without `using`, subject to target availability. References into named compiler-known modules, such as
`core.memory`, require a matching `using` under the ordinary cross-module dependency rules.

Standard-library declarations are ordinary declarations supplied by standard-library packages. They must be visible
through normal import and path rules before source code can use them. The compiler recognizes selected declarations
by stable identity. Recognition neither makes them ambient nor gives a declaration with the same spelling in another
package the same contract.

This distinction keeps the language core small while allowing the compiler to reason precisely about selected library
contracts. The [recognition rules](standard-library-recognition.md) define how identity establishes those contracts.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Next: [Compiler-known declarations](compiler-known-declarations.md)
