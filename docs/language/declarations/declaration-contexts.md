# Declaration contexts

A **declaration context** is a source location that accepts declarations.

The declaration contexts are:

- source-unit module declarations,
- block module declarations,
- module bodies,
- type bodies,
- trait bodies,
- implementation bodies,
- block expressions.

A source unit begins with either one source-unit module declaration or one or more block module declarations.

Block module declarations contribute declarations to an explicitly named module.

Module declaration rules are defined in [Modules and packages](../modules-and-packages.md).

The unbraced body after a source-unit module declaration accepts using declarations, export declarations, and module-level
declarations.

A braced module body accepts using declarations, export declarations, and module-level declarations.

A type body accepts representation declarations and type member declarations.

A trait body accepts trait member declarations.

An implementation body accepts inherent implementation members or trait implementation members according to the implementation header.

A block expression accepts local binding declarations and constant declarations.

Declarations that are not accepted by the current declaration context are rejected.

Named function declarations, type declarations, trait declarations, implementation declarations, module declarations, and package declarations do not appear inside ordinary block expressions.

Local callable behavior inside a block expression is expressed with a lambda value bound to a local binding.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Overview](overview.md)
- Next: [Module body declarations](module-body-declarations.md)
