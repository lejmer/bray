# Project Manifests And Package Build Graphs

This document defines Bray's project-owned workspace and package contracts. It applies core axiom 10 directly: dependencies are
part of the program, and the build, source, and dependency graphs are visible and reproducible.

## Ownership

`bray-project` owns:

- the workspace and package manifest contracts,
- normalized portable project paths,
- validation of package, product, source-root, feature, target, dependency, and output selections,
- deterministic discovery of Bray source files beneath declared source roots,
- generated-source plans, output validation, and immutable generated source nodes,
- and the immutable package graph consumed by compilation and tooling.

`bray-project` composes semantic package and product identities from `bray-symbols` and target identities and output categories from
`bray-target`. It does not own compilation, target realization, artifact naming, emission, linking, command-line policy, or
user-facing orchestration.

Bray Tack is the user-facing orchestrator over this contract. It is not the owner of project semantics. Compiler,
inspection, and language-tooling entry points consume an already loaded explicit graph and must not perform dependency discovery.

## Manifest Serialization

The normalized manifest serialization is strict UTF-8 JSON:

- `bray-workspace.json` is the single manifest at a workspace root.
- `bray-package.json` is the single manifest in every directory listed by the workspace.
- `format` is the exact serialization revision. Revision `1` defines the schema in this document.
- Unknown fields are rejected.

Each compiler release has an explicit dispatch table of supported revisions. A revision defines the complete schema, validation,
defaults, and stable semantic projection, and a loader rejects unsupported revisions and unknown fields. The reference writer
emits no byte-order mark, uses LF line endings, two-space indentation, schema property order, deterministic JSON escaping, normalized
set-like array order, and one final newline. Input formatting and semantically irrelevant array order do not affect the immutable
project graph or its content identity.

JSON is a serialization choice, not a package-management model. It provides explicit arrays and objects and strict unknown-field
handling without introducing a manifest language, configuration evaluator, or second build DSL. The semantic contract is
Bray-owned and independent of Rust or Cargo. An incompatible JSON schema uses a new exact revision. A replacement serialization
uses new stable workspace and package filenames, forbids both serialization families at one manifest boundary, and provides an
explicit converter rather than silently reinterpreting existing files.

Package manifests declare semantic package versions. They do not contain version ranges, registries, repository URLs, lockfile
references, resolution strategies, or acquisition instructions. There is no automatic dependency acquisition.

## Portable Paths

Every serialized path is relative and uses `/`. A path:

- is `.` only where the workspace package directory itself is allowed,
- otherwise contains one or more non-empty components,
- contains no `.`, `..`, backslash, null, leading separator, or trailing separator,
- and is interpreted beneath the workspace or package boundary named by its field.

This representation is stable before any host path is constructed. Manifest loading never accepts an absolute path or a path
that can escape the workspace. Manifest files and package inventory path components may not be symbolic links. Every inventory
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
  "package": {
    "version": "1.4.0"
  },
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
entries map a workspace-local defined name to an exact compiler-facing target identity. No target is inferred from the host.

The optional workspace `package` object supplies metadata that package manifests may explicitly inherit. Its `version` is a
Semantic Versioning value. The workspace itself is not a package and does not acquire a package identity or version from this
object.

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
  "version": {
    "workspace": true
  },
  "features": ["logging"],
  "source_roots": [
    {
      "name": "main",
      "path": "src"
    }
  ],
  "products": [
    {
      "name": "application",
      "kind": "executable",
      "source_roots": ["main"],
      "targets": ["native"],
      "dependencies": [
        {
          "package": "example.math",
          "product": "math",
          "when": {
            "property": "target.identity.SYSTEM",
            "equals": "linux"
          }
        }
      ],
      "outputs": ["dependency_metadata", "executable"]
    }
  ]
}
```

Package identities contain one or more lowercase ASCII dot-separated segments. Each segment starts with a letter and continues
with lowercase letters, digits, `_`, or `-`. Package identity is independent of directory placement. The toolchain-owned `std`
namespace is reserved and is supplied outside an ordinary workspace inventory.

Every package declares a [Semantic Versioning](https://semver.org/) version. A package may provide the version directly as a string,
or explicitly inherit the workspace package version with `{"workspace": true}`. Inheritance is invalid when the workspace has no
package version. The resolved version is retained in the project graph and compiled package interface.

Package-local names start with a lowercase ASCII letter and continue with lowercase letters, digits, `_`, or `-`. This rule covers
features, source roots, products, and workspace target names. Every stable selection is unique.

### Source Roots

Source roots are package-relative named directories. Products select one or more roots by name. During graph loading, every regular
file with the exact `.bray` extension beneath a selected declared root becomes a source node. Directory traversal and resulting
source paths are sorted deterministically. Other regular files are ignored.

The graph stores workspace-relative source paths, not host-absolute paths. A product selecting multiple roots receives the sorted
deduplicated union of those roots' source nodes. Source-root declaration order and filesystem enumeration order have no semantic
effect.

Generated source is represented by explicit generated-source graph nodes. Each node declares a package-local stable generator
name, the exact tool artifact and toolchain identity, ordered source, data, and configuration inputs, target dependencies, declared
output identities, and a closed permission set. Generated outputs use the exact `.bray` extension and cannot overlap another output
from the same generated-source graph.

Generator execution receives only its declared inputs, exact manifest-declared environment values, and declared capabilities. It
runs in a sandbox without ambient filesystem or network discovery. Source generators cannot request network access. Their cache
key covers the generator identity, tool artifact digest, toolchain identity, input content digests, environment values, selected
target properties, permissions, and generator schema revision. Successful output bytes are hashed and become immutable source
snapshots before the product source graph is frozen. Producing different bytes for the same key is a generator invariant failure.
Missing, extra, colliding, or path-escaping outputs are errors. Failure or cancellation publishes no graph node or cache entry.

`bray-project` owns generator semantics, cache identities, output validation, and graph publication. The compiler host supplies an
injected sandbox executor and the exact resolved tool artifacts. That executor runs a closed request and returns bytes and typed
host failure categories. It does not interpret manifests, discover inputs, choose permissions, or publish source nodes.

Package manifests declare generators and products select them explicitly:

```json
{
  "format": 1,
  "identity": "example.application",
  "version": "1.0.0",
  "features": [],
  "source_roots": [
    {
      "name": "main",
      "path": "src"
    }
  ],
  "generators": [
    {
      "name": "bindings",
      "tool": {
        "package": "example.binding-generator",
        "product": "generator",
        "target": "build-host"
      },
      "inputs": ["schema/service.json"],
      "generated_inputs": [],
      "outputs": ["generated/bindings.bray"],
      "environment": {},
      "permissions": {
        "network": false
      }
    }
  ],
  "products": [
    {
      "name": "application",
      "kind": "executable",
      "source_roots": ["main"],
      "generated_sources": ["bindings"],
      "targets": ["native"],
      "dependencies": [],
      "outputs": ["executable"]
    }
  ]
}
```

The tool names one exact workspace executable product built for a workspace target that is compatible with the compiler host and
contributes a typed host-tool dependency to the graph. Tool dependencies and generated-source dependencies are acyclic. `inputs`
are exact package-relative portable file identities. Each `generated_inputs` entry has the exact shape
`{"generator": name, "output": identity}` and names another generator and one of its declared outputs. The resulting dependency
graph determines execution order. `outputs` are exact generator-relative portable identities
rather than globs, directories, or workspace paths. The graph identifies an output by package, generator, selected target, and
output identity, and execution writes it only beneath a
private managed generator root. Generated source never mutates or impersonates a source-tree file. `environment` maps names to exact
UTF-8 values rather than reading the host environment. The revision-1 permission object has the single required field `network`,
whose only valid value for a source generator is `false`. A product cannot select a generator whose tool or execution contract is
unavailable on the compiler host.

### Dependencies

Each product dependency edge names one exact package identity and one exact library product. The selected package must appear in the
workspace inventory, and the product must appear in that package's manifest with kind `library`.

Dependency edges contain no location, version requirement, range, registry, URL, or fallback. Location and the exact selected
package version are supplied by the workspace's project-owned package inventory. A missing package or product is an error. It
never starts a search.

For each selected target, active product edges induce both an acyclic package dependency graph and an acyclic product dependency
graph. A package cannot depend transitively on itself through different products. A dependency product is built before its
dependents. Independent products are ordered by stable package and product identity, so manifest ordering and parallel
scheduling cannot affect the published build order.

A test product may select one sibling library product through `tested_library`. The selected library is built first and its emitted
public interface and implementation are supplied to the test compilation as an external dependency. The test product does not
compile the library's source roots into its own source graph. This keeps integration tests inside the package they test while
ensuring they exercise the same public contract consumed by dependent packages.

The toolchain-selected `std` package is not acquired or located through a package manifest. The package layer supplies its exact
configured standard-library root as a separate immutable build input under the
[standard-library artifact contract](standard-library.md). When selected, `std` still enters the ordinary dependency graph and
follows ordinary visibility rules. Workspace packages cannot claim the reserved `std` or `std.*` identities.

Dependencies are product-scoped. An edge may carry an explicit target predicate expressed only over language-defined target
properties. The predicate grammar is a closed normalized tree of `all`, `any`, `not`, equality, inequality, and membership tests
over literal property values. Loading validates every referenced property and value against each selected target, evaluates the
predicate deterministically, and retains both the normalized predicate and its evaluated property dependencies in graph identity.
An omitted predicate means unconditional. Features, host properties, environment variables, filesystem presence, and dependency
availability never activate an edge implicitly.

The serialized shapes are `{"all": [predicate, ...]}`, `{"any": [predicate, ...]}`, `{"not": predicate}`,
`{"property": name, "equals": value}`, `{"property": name, "not_equals": value}`, and
`{"property": name, "in": [value, ...]}`. Empty `all` is true, empty `any` is false, membership values are sorted and unique in
the defined projection, and every other object shape or combination is invalid.

Package-wide dependency syntax is not part of the manifest schema. The published graph and every downstream compiler contract
contain only product-scoped edges.

### Products

Product `kind` is one of:

- `executable`,
- `library`,
- or `test`.

A product selects one or more declared source roots, one or more workspace target configurations, and one or more external output
categories. Output category names correspond to `bray-emitter`'s stable external artifact taxonomy:

- `assembly`,
- `backend_ir`,
- `backend_opaque`,
- `relocatable_object`,
- `executable_module`,
- `debug_companion`,
- `package_interface`,
- `package_implementation`,
- `dependency_metadata`,
- `executable`,
- `static_library`,
- `shared_library`,
- and `linked_companion`.

The manifest chooses required output categories. `bray-target` supplies target-specific availability, names, prefixes, and suffixes,
while emission owns artifact planning and publication beneath `output_root`.

## Immutable Graph Contract

A successful load publishes a `ProjectGraph` containing:

- the configured output root,
- target configurations sorted by workspace-local name,
- package inventory nodes in stable package identity order,
- each package's identity, resolved semantic version, role, portable directory, declared and enabled features, source roots, and
  products,
- each source root's exact sorted source files,
- each generated-source node's identity, declared inputs, permissions, cache identity, outputs, and output digests,
- each product's exact source, target, dependency, and output selections,
- active package nodes in dependency-first order for each target,
- and selected product nodes in dependency-first order for each target.

The graph has no mutable caches or ambient lookup hooks. Shared readers may safely use it from parallel compilation, inspection,
and language-tooling work. Demand-driven compiler queries may retain or index graph values, but may not mutate project semantics.

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
- invalid target predicates,
- invalid generator tools, inputs, outputs, environment, or permissions,
- generated-source execution and reproducibility failures,
- and product, host-tool, or generated-source dependency cycles.

The loader retains exact validation categories in `ProjectLoadError`. Localized prose is rendered only through `bray-messages`.
Parser-library prose is not forwarded as a compiler diagnostic.

## Explicit Non-Goals

This contract does not define or imply:

- a package manager,
- a registry,
- remote or ambient package discovery,
- dependency downloads,
- transitive acquisition,
- version requirement solving,
- lockfile generation,
- repository synchronization,
- unconstrained build scripts or arbitrary build-time commands,
- compiler plugins,
- or network behavior.

A project may add vendored package inputs by editing project-owned files and directories. Compilation only consumes the resulting
explicit graph.
