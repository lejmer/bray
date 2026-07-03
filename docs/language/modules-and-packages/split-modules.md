# Split modules

The same module can be declared by multiple source files and block module declarations.

Every declaration block that contributes to a module declares the same explicit module identity.

Split module declarations contribute to one logical module.

Declaration merging is deterministic.

Duplicate declarations in the same module are rejected unless the declaration form explicitly defines merging behavior.

All declarations contributed to the same module share the module's declaration namespace.

Order between source files does not affect name resolution.

Order within a single declaration block follows ordinary source-order rules where a declaration form depends on order.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module declarations](module-declarations.md)
- Next: [Module visibility](module-visibility.md)
