# Module bodies

A module body contains declarations.

Declaration contexts and module-level declaration forms are defined in [Declarations](../declarations.md).

Module bodies do not evaluate at runtime.

Using, referencing, or contributing to a module does not execute code.

A module has no runtime initialization phase.

A static declaration's required constant initializer is materialized by its owning product. It does not execute module code.
Explicit runtime initialization remains an ordinary source operation performed after product entry.

Top-level executable statements are not module declarations.

Runtime work belongs in functions, constructors, lifecycle declarations, async tasks, tests, or other explicit executable constructs.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Trusted modules](trusted-modules.md)
- Next: [Using declarations](using-declarations.md)
