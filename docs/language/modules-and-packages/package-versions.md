# Package versions

Every package has a semantic version with three decimal components, `major.minor.patch`. Each component is a nonnegative
integer with no leading zero unless the component is zero.

An optional prerelease suffix begins with `-` and contains one or more nonempty dot-separated identifiers. Each
identifier contains only ASCII letters, decimal digits, and hyphens. A wholly numeric prerelease identifier has no
leading zero unless it is zero. An optional build suffix begins with `+` and uses the same identifier characters, but
permits leading zeros in numeric identifiers. Build metadata follows the prerelease suffix when both are present.

Version precedence compares major, minor, and patch numerically, then prerelease identifiers from left to right. A
release has higher precedence than a prerelease with the same three components. Numeric prerelease identifiers compare
numerically and precede nonnumeric identifiers. Nonnumeric identifiers compare in ASCII order. If all compared
identifiers are equal, the longer prerelease sequence has higher precedence. Build metadata does not affect precedence.

A release identifies a public API. After release, its contents do not change under the same version. Major version zero
denotes initial development with no stable API. Version `1.0.0` establishes the public API. For a stable API, a
backward-compatible bug fix increments patch, backward-compatible functionality or deprecation increments minor, and an
incompatible API change increments major. Incrementing minor resets patch to zero. Incrementing major resets minor and
patch to zero. A prerelease may be unstable and need not preserve the compatibility of its corresponding release.

The package and build layer supplies the version outside Bray source. Package versions do not participate in module
paths or source-level names.

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

Version inheritance applies only when requested by the package manifest. A workspace is a build container, not a
package, and the workspace package metadata does not give the workspace its own package identity or version.

Bray package versions identify selected and compiled package releases. They do not imply a registry, automatic
dependency downloads, or version solving. A workspace selects exact project-owned package inputs.

## Navigation

- [Language index](../index.md)
- [Modules and packages index](../modules-and-packages.md)
- Previous: [Module paths and package identity](module-paths-and-package-identity.md)
- Next: [Package products and source graphs](package-products-and-source-graphs.md)
