# Summary

Module identity is explicit in source.

Package identity belongs to the package and build layer.

Package products select explicit source and dependency graphs.

Each runtime product instance owns its realized product statics. Exact native-thread attachments within that product own their
thread-local statics until deterministic detach cleanup completes.

Modules are named declaration containers, not runtime objects.

Using declarations and exports do not execute code.

Internal access is explicit and lexical.

Re-exports preserve declaration identity.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Internal re-exports](internal-re-exports.md)
