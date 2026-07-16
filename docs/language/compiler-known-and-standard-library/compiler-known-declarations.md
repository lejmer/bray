# Compiler-known declarations

A **compiler-known declaration** is a language-defined declaration whose identity and contract are known to the compiler.

Compiler-known declarations are available in every module without an import, subject to target availability rules.

They participate in ordinary:

- type checking,
- path resolution,
- overload selection,
- implementation coherence,
- ownership checking,
- borrowing,
- effect checking,
- contract checking,
- code generation.

Compiler-known names occupy the ordinary lookup namespace before source declarations are checked.

User code cannot declare another entity with the same ordinary name in the same declaration scope as a compiler-known declaration.

Compiler-known declarations are not dependencies.

They are not imported, re-exported, versioned, or shadowed by package declarations.

No compiler-known declaration is owned by the `std` package namespace. The `std` root is reserved for ordinary standard-library
packages, all of which remain subject to ordinary path and `using` rules.

When this specification gives a compiler-known declaration a semantic declaration, that declaration describes its language contract.

It does not imply that user code can replace, redeclare, or emulate that declaration by spelling the same name.

Some compiler-known declarations have protected representation.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Overview](overview.md)
- Next: [Compiler-provided declarations](compiler-provided-declarations.md)
