# Summary

Compiler-known declarations have language-defined identity and contracts.

Compiler-provided declarations are compiler-known declarations whose implementation is supplied by the compiler while preserving ordinary declaration surfaces.

Compiler-known declarations are available without imports when target-available.

`std.target` exposes compiler-known target facts for the selected target profile.

Standard-library declarations are ordinary declarations supplied by standard-library packages.

Recognized standard-library declarations are recognized by stable declaration identity, not spelling.

Recognition does not make a standard-library declaration ambient and does not bypass ordinary import and path rules.

Compiler-provided behavior, target facts, and recognized standard-library behavior are conforming only when their observable semantics match this specification.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Conformance catalog](conformance-catalog.md)
