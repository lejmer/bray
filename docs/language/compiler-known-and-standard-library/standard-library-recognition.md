# Standard-library recognition

The compiler may recognize selected standard-library declarations by stable declaration identity.

Recognized standard-library declarations can have compiler-defined:

- checking,
- lowering,
- optimization,
- const eligibility,
- contract behavior.

Recognition is based on the declaration's identity, not on accidental spelling.

A user-defined declaration named like a standard-library declaration is just a user-defined declaration.

It does not gain standard-library recognition.

A standard-library declaration that wraps a compiler-provided declaration remains an ordinary standard-library declaration unless an owning language rule explicitly makes it compiler-known.

Compiler recognition of a standard-library declaration must preserve that declaration's specified contract and observable semantics.

Recognition does not let different compilers define different standard-library behavior.

Recognized standard-library declarations are ordinary declarations for name resolution, imports, visibility, overload declarations, implementation participation, and public API compatibility.

If the relevant standard-library declaration is not visible, a source reference to it is rejected by ordinary name resolution.

If a visible declaration is not the recognized standard-library declaration, it is checked as an ordinary declaration.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Standard-library declarations](standard-library-declarations.md)
- Next: [Recognized standard-library operations](recognized-standard-library-operations.md)
