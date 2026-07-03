# Module paths and package identity

## Module paths

A module path is one or more identifiers separated by `.`.

```bray
net
net.http
net.http.tests
```

A module path declared in source is relative to the current package.

It does not include package identity.

When an external path begins with a package identity, that leading component is resolved as the package, not as part of the source-declared module path inside that package.

For example, if another package is known as `geometry`, source in that package can declare:

```bray
module shapes;
```

Other packages can refer to that module through the package path:

```bray
geometry.shapes.Circle
```

## Package identity

A package has a package identity supplied outside Bray source.

The compiler receives package identity from the package and build layer.

Package identity determines how other packages refer to the compiled package.

Package identity is not a module declaration.

Package identity is not introduced by writing a source declaration with the same name.

Package dependencies are declared by the package and build layer, not by module declarations.

The package dependency graph is acyclic.

The module graph inside a package is a declaration graph and can be cyclic.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Overview](overview.md)
- Next: [Package products and source graphs](package-products-and-source-graphs.md)
