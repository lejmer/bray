# Conditional module contributions

A module contribution is enabled for a product unless a directive that controls module contribution disables it.

The language-defined module contribution gates are `@test` and `@target(...)`.

When a source-unit module declaration is disabled for a product, the entire source unit contributes no declarations to that
product.

When a block module declaration is disabled for a product, that block module declaration contributes no declarations to that
product.

A disabled module contribution must still be lexically and syntactically valid Bray source.

Declarations inside a disabled module contribution are not semantically checked for that product.

Only enabled module contributions participate in split module merging, module visibility agreement, trusted-module agreement, name
resolution, overload declarations, implementation coherence, entrypoint resolution, test entry formation, public API construction,
and compiled interface metadata for that product.

When multiple contribution gates apply to the same module declaration, the contribution is enabled only when every gate enables it.

Contribution gates do not change module identity, module visibility, trusted-module state, declaration visibility, path resolution,
internal access, trusted capability access, or runtime behavior.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Package products and source graphs](package-products-and-source-graphs.md)
- Next: [Library and executable products](library-and-executable-products.md)
