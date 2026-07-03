# Standard-library declarations

A **standard-library declaration** is an ordinary declaration supplied by a standard-library package.

The standard-library root package is `std`.

Standard-library declarations are not automatically visible.

Source code can use a standard-library declaration only when the declaration is reachable through the `std` package root and visible through normal import or path rules.

When this specification writes a standard-library declaration without a body, that bodyless form describes the declaration surface and contract.

It is not source syntax that standard-library packages can write.

A conforming standard-library package provides the declaration through ordinary Bray source, trusted Bray source, or another declared dependency mechanism allowed by the dependency and trust rules.

If a declaration has no ordinary standard-library implementation because the compiler provides it, it is a compiler-provided compiler-known declaration, not a standard-library declaration.

Standard-library declarations do not create ambient behavior.

Importing a standard-library declaration follows the same rules as importing any other declaration.

It does not execute code, initialize modules, or silently extend overload sets, implementation overload families, operators, conversions, or behavioral contracts.

A `using std.some.path` declaration makes that qualified path available according to normal path rules.

It does not make `some`, `path`, or the final declaration name available as an unqualified name.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Compiler-known trait implementations](compiler-known-trait-implementations.md)
- Next: [Standard-library recognition](standard-library-recognition.md)
