# Compiler-known trait implementations

Compiler-known traits can be implemented by ordinary packages when the trait contract permits it.

An implementation of a compiler-known trait is still an ordinary implementation declaration.

It participates in a coherence domain only when it is declared in that domain or explicitly imported into it according to implementation coherence rules.

A package dependency does not silently activate dependency implementations for compiler-known traits.

The compiler recognizes the trait identity and member contracts.

It does not infer implementations from names, structure, or similar-looking members.

Implementations of compiler-known traits obey ordinary implementation visibility, coherence, overload-family, import, ownership, borrowing, contract, and target availability rules.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Compiler-known surface](compiler-known-surface.md)
- Next: [Standard-library declarations](standard-library-declarations.md)
