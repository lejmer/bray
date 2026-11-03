# Summary

This summary is informative. The detailed rules include:

- [package products and source graphs](package-products-and-source-graphs.md).
- [module declarations](module-declarations.md).
- [using declarations](using-declarations.md).

Source declarations give modules their identity, while the package and build layer supplies package identity and
selects each product's source and dependency graphs. Modules are declaration containers rather than runtime objects,
and using declarations and exports do not execute code. Re-exports preserve declaration identity, and internal access
requires explicit lexical acknowledgement.

Each runtime product instance owns its realized product statics. Exact native-thread attachments within that product
own their thread-local statics until deterministic detach cleanup completes.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Internal re-exports](internal-re-exports.md)
