# Split modules

The same module can be declared by multiple source units and block module declarations.

Every declaration block that contributes to a module declares the same explicit module identity.

Split module declarations contribute to one logical module.

Declaration merging is deterministic.

Duplicate declarations in the same module are rejected unless the declaration form explicitly defines merging behavior.

All declarations contributed to the same module share the module's ordinary declaration scope.

The enabled contributions also share their `using` declarations. References within that logical module need no
self-import. Independent modules have separate dependency declarations even when they share a dotted parent or appear
in the same source unit. See [Using declarations](using-declarations.md).

Order between source units does not affect name resolution.

Order within a single declaration block follows ordinary source-order rules where a declaration form depends on order.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module declarations](module-declarations.md)
- Next: [Module visibility](module-visibility.md)
