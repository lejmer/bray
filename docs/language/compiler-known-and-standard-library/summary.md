# Summary

This summary is informative. The detailed rules include:

- [compiler-known declarations](compiler-known-declarations.md).
- [standard-library recognition](standard-library-recognition.md).
- [conformance catalog](conformance-catalog.md).

Compiler-known declarations have language-defined identity and contracts. Compiler-provided declarations are
compiler-known declarations with compiler-supplied implementations that preserve ordinary declaration contracts.

Ambient compiler-known names are available without `using` when target-available. References into named compiler-known
modules require matching using declarations. The ambient `target` path exposes selected target properties without
occupying `std`.

Standard-library declarations are ordinary package declarations. Recognition uses their stable identity rather than
spelling and preserves ordinary import and path rules. Compiler-provided behavior, target properties, and recognized
standard-library behavior must preserve the observable semantics specified by their rules.

## Navigation

- [Language index](../index.md)
- [Compiler-known and standard library index](../compiler-known-and-standard-library.md)
- Previous: [Conformance catalog](conformance-catalog.md)
