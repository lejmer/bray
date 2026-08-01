# Project Manifests And Package Build Graphs

This document defines Bray's project-owned workspace and package contracts. It applies core axiom 10 directly: dependencies are
part of the program, and the build, source, and dependency graphs are visible and reproducible.

## Ownership

`bray-project` owns:

- the workspace and package manifest contracts,
- canonical portable project paths,
- validation of package, product, source-root, feature, target, dependency, and output selections,
- deterministic discovery of Bray source files beneath declared source roots,
- and the immutable package graph consumed by compilation and tooling.

`bray-project` composes semantic package and product identities from `bray-symbols` and target identities and output categories from
`bray-target`. It does not own compilation, target realization, artifact naming, emission, linking, command-line policy, or
user-facing orchestration.

Bray Tack is the user-facing orchestrator over this contract. It is not the owner of project semantics. Compiler,
inspection, and language-tooling entry points consume an already loaded explicit graph and must not perform dependency discovery.

## Bootstrap Serialization

The bootstrap manifest serialization is strict UTF-8 JSON:

- `bray-workspace.json` is the single manifest at a workspace root.
- `bray-package.json` is the single manifest in every directory listed by the workspace.
- `format` is the explicit serialization revision. The only current revision is `1`.
- Unknown fields are rejected.

JSON is a narrow serialization choice, not a package-management model. It gives the greenfield implementation a standard,
deterministic parser, explicit arrays and objects, and strict unknown-field handling without introducing a manifest language,
configuration evaluator, or second build DSL. The semantic contract is Bray-owned and independent of Rust or Cargo. A future
serialization change requires a deliberate format revision or a replacement design; it must not silently reinterpret existing
project files.

The manifests intentionally contain no package versions, version ranges, registries, repository URLs, lockfile references,
resolution strategies, or acquisition instructions. There is no automatic dependency acquisition.

## Portable Paths

Every serialized path is relative and uses `/`. A path:

- is `.` only where the workspace package directory itself is allowed,
- otherwise contains one or more non-empty components,
- contains no `.`, `..`, backslash, null, leading separator, or trailing separator,
- and is interpreted beneath the workspace or package boundary named by its field.

This representation is canonical before any host path is constructed. Manifest loading never accepts an absolute path or a path
that can escape the workspace. Manifest files and package inventory path components may not be symbolic links; every inventory
entry therefore resolves only through project-owned directories beneath the explicit workspace root.

Package source trees may not contain symbolic links. This keeps source ownership explicit and prevents a declared source root from
indirectly selecting ambient files outside its portable path. Non-UTF-8 source paths are rejected because they cannot have one
portable manifest-visible identity. No two declared source roots in the workspace may overlap, so every source file has one package
and source-root owner.

## Workspace Manifest

The workspace manifest records the complete package inventory and build-wide selections:

```json
{
  "format": 1,
  "output_root": "build",
  "targets": [
    {
      "name": "native",
      "identity": "x86_64-unknown-linux-gnu"
    }
  ],
  "packages": [
    {
      "path": "application",
      "role": "root",
      "features": ["logging"]
    },
    {
      "path": "vendor/math",
      "role": "vendored",
      "features": ["simd"]
    }
  ]
}
```

`output_root` is the sole workspace-relative root for project build outputs. It may not overlap any declared source root. Target
entries map a workspace-local canonical name to an exact compiler-facing target identity. No target is inferred from the host.

Every package is listed exactly once by directory. A `root` package is an independently selected workspace build root. A `vendored`
package is an exact project-owned dependency input. At least one root package is required.

The workspace records the complete enabled feature set for each package. Feature selection is not unified, inferred, propagated, or
solved from dependency edges. A selected feature must be declared by that package's manifest. The resulting graph therefore
contains one visible feature configuration for every package node.

Package inventory order, target order, and feature order have no semantic effect.

## Package Manifest

A package manifest records one package's semantic identity, declared feature surface, source roots, exact dependency products, and
build products:

```json
{
  "format": 1,
  "identity": "example.application",
  "features": ["logging"],
  "source_roots": [
    {
      "name": "main",
      "path": "src"
    }
  ],
  "dependencies": [
    {
      "package": "example.math",
      "product": "math"
    }
  ],
  "products": [
    {
      "name": "application",
      "kind": "executable",
      "source_roots": ["main"],
      "targets": ["native"],
      "outputs": ["dependency_metadata", "executable"]
    }
  ]
}
```

User and vendored package identities contain at least two lowercase ASCII dot-separated segments. Each segment starts with a letter
and continues with lowercase letters, digits, or `-`. Package identity is independent of directory placement. The toolchain-owned
`std` package is the sole one-segment exception and is supplied outside the workspace inventory.

Package-local names start with a lowercase ASCII letter and continue with lowercase letters, digits, `_`, or `-`. This rule covers
features, source roots, products, and workspace target names. Every canonical selection is unique.

### Source Roots

Source roots are package-relative named directories. Products select one or more roots by name. During graph loading, every regular
file with the exact `.bray` extension beneath a selected declared root becomes a source node. Directory traversal and resulting
source paths are sorted canonically. Other regular files are ignored.

The graph stores workspace-relative source paths, not host-absolute paths. A product selecting multiple roots receives the sorted
deduplicated union of those roots' source nodes. Source-root declaration order and filesystem enumeration order have no semantic
effect.

Generated source is not implicit. A future generated-source design must represent the generator, inputs, output identity,
permissions, and reproducibility contract as explicit graph nodes before generated files can participate.

### Dependencies

Each dependency edge names one exact package identity and one exact library product. The selected package must appear in the
workspace inventory, and the product must appear in that package's manifest with kind `library`.

Dependency edges contain no location, version, range, registry, URL, or fallback. Location is supplied exactly once by the
workspace's project-owned package inventory. A missing package or product is an error; it never starts a search.

Package dependencies must be acyclic. A dependency package is built before its dependents. Independent packages are ordered by
canonical package identity, so manifest ordering and parallel scheduling cannot affect the published build order.

The toolchain-selected `std` package is not acquired or located through a package manifest. The package layer supplies its exact
configured standard-library root as a separate immutable build input under the
[standard-library artifact contract](standard-library.md). When selected, `std` still enters the ordinary dependency graph and
follows ordinary visibility rules. Workspace packages cannot claim the reserved `std` or `std.*` identities.

The initial contract applies dependencies package-wide. It does not speculate about conditional, platform-specific, or
product-specific dependency activation. Such behavior would require an explicit graph contract rather than hidden selection
policy.

### Products

Product `kind` is one of:

- `executable`,
- `library`,
- or `test`.

A product selects one or more declared source roots, one or more workspace target configurations, and one or more target output
categories. Output category names correspond to `bray-target`'s backend-neutral output contract:

- `assembly`,
- `backend_ir`,
- `backend_bitcode`,
- `relocatable_object`,
- `executable_module`,
- `debug_companion`,
- `package_interface`,
- `dependency_metadata`,
- `executable`,
- `static_library`,
- `shared_library`,
- and `linked_companion`.

The manifest chooses required output categories; target policy later chooses external names, prefixes, and suffixes. Emission owns
artifact planning and publication beneath `output_root`.

## Immutable Graph Contract

A successful load publishes a `ProjectGraph` containing:

- the canonical output root,
- target configurations sorted by workspace-local name,
- package nodes in dependency-first build order,
- each package's role, portable directory, declared and enabled features, source roots, dependencies, and products,
- each source root's exact sorted source files,
- and each product's exact source, target, and output selections.

The graph has no mutable caches or ambient lookup hooks. Shared readers may safely use it from parallel compilation, inspection,
and language-tooling work. Demand-driven compiler facts may retain or index graph values, but may not mutate project semantics.

Two workspaces with the same manifest values and project-owned source paths produce equal graph values regardless of manifest array
ordering or host directory enumeration order.

## Diagnostics

Manifest loading emits locale-neutral structured diagnostics through `bray-diagnostics`. Diagnostic categories distinguish:

- manifest reads,
- schema parsing,
- invalid selections,
- duplicate selections,
- invalid source roots,
- missing dependency packages,
- invalid dependency products,
- and dependency cycles.

The loader retains exact validation categories in `ProjectLoadError`; localized prose is rendered only through `bray-messages`.
Parser-library prose is not forwarded as a compiler diagnostic.

## Explicit Non-Goals

This contract does not define or imply:

- a package manager,
- a registry,
- remote or ambient package discovery,
- dependency downloads,
- transitive acquisition,
- version solving,
- lockfile generation,
- repository synchronization,
- build scripts,
- compiler plugins,
- or network behavior.

A project may add vendored package inputs by editing project-owned files and directories. Compilation only consumes the resulting
explicit graph.
