# Standard-library declarations

A **standard-library declaration** is an ordinary declaration supplied by a standard-library package.

The standard-library root package is `std`.

The `std` package namespace is reserved exclusively for standard-library packages. Compiler-known declarations, target
properties, and private runtime ABI symbols do not occupy `std` paths.

User and vendored packages cannot declare the package identity `std` or an identity beneath the reserved `std` package
namespace.

The compiler host must explicitly classify source as toolchain-owned standard-library source before it can use a
reserved package identity. That classification controls package identity only. It does not grant trusted implementation
capabilities or change any module, declaration, contract, ownership, capability, or checking rule.

The selected toolchain supplies `std` as an explicit package input. This does not make any declaration ambient and does
not permit the compiler to download or search for a standard library implicitly.

Standard-library declarations are not automatically visible.

Source code can use a standard-library declaration only when the declaration is reachable through the `std` package root
and visible through normal import or path rules.

When this specification writes a standard-library declaration without a body, that bodyless form describes the
declaration surface and contract.

It is not source syntax that standard-library packages can write.

A conforming standard-library package provides the declaration through ordinary Bray source, trusted Bray source, or
another declared dependency mechanism allowed by the dependency and trust rules.

For concurrency and parallelism, public policy and owner behavior are ordinary Bray source. Trusted Bray or private
linked dependencies can supply raw representation and platform mechanisms, but they do not replace the public Bray
implementation with a foreign-language semantic subsystem.

A private linked declaration does not imply that its implementation is foreign code. It can resolve to a separately
compiled Bray runtime artifact. Only an actual crossing into a foreign ABI follows the foreign-call rules.

If a declaration has no ordinary standard-library implementation because the compiler provides it, it is a
compiler-provided compiler-known declaration, not a standard-library declaration.

Standard-library declarations do not create ambient behavior.

Importing a standard-library declaration follows the same rules as importing any other declaration.

It does not execute code, initialize modules, or silently extend overload sets, implementation overload families,
operators, conversions, or behavioral contracts.

A `using std.some.path` declaration makes that qualified path available according to normal path rules.

It does not make `some`, `path`, or the final declaration name available as an unqualified name.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Compiler-known trait implementations](compiler-known-trait-implementations.md)
- Next: [Standard-library recognition](standard-library-recognition.md)
