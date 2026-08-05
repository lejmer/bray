# Target constraints and gates

## Target constraints

Target constraints are product constraints supplied by the package and build layer.

Target profiles, target facts, target constraints, target-conditional declarations, and `@target(...)` gates are defined in [Target constraints and gates](../targets-layout-abi-and-raw-memory/target-constraints-and-gates.md).

## Target-gated module contributions

A module contribution can be gated by the selected target profile with `@target(...)`.

```bray
@target(target.atomic.U64)
module counters;
```

Module path, module identity, module visibility, trusted-module state, declaration visibility, and path resolution remain module rules.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Test products and entries](test-products-and-entries.md)
- Next: [Module declarations](module-declarations.md)
