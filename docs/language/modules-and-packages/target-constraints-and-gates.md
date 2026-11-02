# Target constraints and gates

## Target constraints

[Product constraints](../targets-layout-abi-and-raw-memory/target-constraints-and-gates.md#product-constraints) defines
the conditions a target must satisfy before module bodies are checked.

## Target-gated module contributions

[Module contribution gates](../targets-layout-abi-and-raw-memory/target-constraints-and-gates.md#module-contribution-gates)
defines `@target(...)` selection. For example:

```bray
@target(target.atomic.U64)
module counters;
```

Module path, module identity, module visibility, trusted-module state, declaration visibility, and path resolution
remain module rules.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Test products and entries](test-products-and-entries.md)
- Next: [Module declarations](module-declarations.md)
