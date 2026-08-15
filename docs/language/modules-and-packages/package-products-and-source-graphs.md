# Package products and source graphs

## Package products

A package can define one or more products.

A product is a selected compilation surface over the package's source graph and dependency graph.

The language-defined product kinds are:

- library,
- executable,
- test.

Package identity is shared by all products of the package.

Product identity, selected source inputs, selected dependencies, and target constraints are supplied by the package and build layer.

A product is not a module and does not create a declaration container or lookup scope.

A runtime **product instance** is one activation of a formed executable, test, or loadable library product. Product-static storage
identity and cleanup ownership use that activation identity, not package identity alone.

A source declaration can contribute to more than one product when it is present in each product's selected source graph and is valid
under each product's product kind and target constraints.

## Source graphs

A package product is compiled from an explicit source graph.

The source graph is the selected set of source inputs for that product.

The compiler checks only source inputs that are in the selected source graph.

Every source input in the source graph must provide Bray source text containing module declarations.

Module declarations define module identity.

Source origins and source graph order do not define module identity.

Split module declarations are valid when all contributing source inputs are part of the same selected source graph and satisfy the
split module rules.

The same source input cannot appear more than once in the same product source graph.

Source graph construction is deterministic. The same package identity, product kind, selected source graph, selected dependency
graph, and target profile produce the same compiler input.

Closed static instance identity is independent of source graph ordering. The selected source and dependency graphs determine which
declarations and imported templates can be demanded, while the final product instance owns the realized storage.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Package versions](package-versions.md)
- Next: [Conditional module contributions](conditional-module-contributions.md)
