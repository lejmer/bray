# Declaration order and checking

Declarations are collected from the selected source graph for a package product.

The selected source graph, package identity, product kind, dependency graph, target profile, test selection, and target gates determine which declarations participate in the product being checked.

Split module contributions are merged before module-level declaration checking finishes.

Source unit order does not define declaration identity.

Source graph order does not define declaration identity.

Order between source units does not affect name resolution.

Within a declaration body, source order matters where the containing grammar or semantic rule makes it matter.

Duplicate declarations in the same declaration namespace are rejected unless the declaration form explicitly defines merging or overload-family behavior.

Declarations are checked against:

- their declaration context,
- their directives,
- their modifiers,
- their visibility,
- their generic parameters and constraints,
- their type surfaces,
- their callable contracts,
- their trusted obligations,
- their body requirements,
- their coherence and overload participation rules.

Declarations in disabled module contributions are not semantically checked for that product.

They must still be lexically and syntactically valid Bray source.

A declaration that exports public interface metadata exposes its public declaration surface, including visibility, name, generic parameters, parameter surfaces, result types, constraints, contracts, trusted obligations, effects, layout contracts, ABI contracts, and lifecycle obligations.

## Navigation

- [Language index](../index.md)
- [Declarations index](../declarations.md)
- Previous: [Overload declarations](overload-declarations.md)
- Next: [Summary](summary.md)
