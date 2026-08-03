# Package versions

Every package has a version following [Semantic Versioning](https://semver.org/).

The package and build layer supplies the version outside Bray source. Package versions do not participate in module paths or
source-level names.

A package manifest may declare its version directly:

```json
{
  "format": 1,
  "identity": "example.geometry",
  "version": "2.1.0"
}
```

A workspace may provide a package version for packages that explicitly inherit it:

```json
{
  "format": 1,
  "package": {
    "version": "2.1.0"
  }
}
```

```json
{
  "format": 1,
  "identity": "example.geometry",
  "version": {
    "workspace": true
  }
}
```

Version inheritance applies only when requested by the package manifest. A workspace is a build container, not a package, and the
workspace package metadata does not give the workspace its own package identity or version.

Bray package versions identify selected and compiled package releases. They do not imply a registry, automatic dependency
downloads, or version solving. A workspace selects exact project-owned package inputs.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module paths and package identity](module-paths-and-package-identity.md)
- Next: [Package products and source graphs](package-products-and-source-graphs.md)
