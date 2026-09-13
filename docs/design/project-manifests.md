# Project manifests and package graphs

`bray-project` turns explicit workspace inputs into one immutable graph for compilation, inspection, and tooling.
Dependencies are part of the program: package locations, features, source roots, products, and targets are declared
rather than discovered through ambient configuration or a resolver.

## Ownership

The project layer owns manifest meaning, portable paths, source discovery, generated-source plans, selection validation,
and graph publication. It reuses package and product identities from `bray-symbols` and target identities and output
categories from `bray-target`. Target realization, artifact naming, emission, and command policy stay with their owners.

Bray Tack loads and selects through this shared model. Compiler entry points consume the validated graph and do not
repeat project discovery. This separation lets editors and command-line tools interpret the same project without
implementing competing manifest semantics.

## Explicit, portable inputs

Strict versioned JSON provides arrays and objects without introducing an executable manifest language or another build
DSL. A normalized semantic projection removes irrelevant formatting and ordering from graph identity. Supported
revisions are explicit, and unknown fields cannot silently change meaning between tools.

Portable paths are identities within a declared workspace or package boundary. Host paths are constructed only after
validation. Source ownership is unambiguous, with non-overlapping roots and no symlink escape into ambient files.
Directory enumeration order does not affect source order. Output storage remains separate from source ownership.

The workspace inventories exact packages and feature selections. Package manifests choose source roots and products.
Dependencies belong to products and name exact inventory entries, not version ranges or acquisition requests. Target
predicates are a closed expression model over declared target properties, and their evaluated dependencies participate
in graph identity. Missing inputs produce diagnostics rather than starting a search.

Integration-test products consume their sibling library's published interface and implementation through a dependency
edge. They do not recreate the library by compiling its source roots again. A configured standard library likewise
enters the ordinary graph through an explicit toolchain-owned edge.

## Generated source

Generators are graph nodes with exact tool artifacts, declared inputs and outputs, selected target properties,
environment values, and closed permissions. Host-tool and generated-output dependencies participate in cycle detection
and ordering. Generated source has its own identity and cannot impersonate or mutate a source-tree file.

The host supplies an injected sandbox executor for a closed request. The project layer owns permissions, cache identity,
output validation, and publication. The executor provides bytes and typed failures, without interpreting manifests or
choosing additional inputs. Network discovery is not part of generator execution.

Successful output becomes an immutable source snapshot before the product graph is published. Cache identity includes
the tool, toolchain, inputs, environment, target observations, permissions, and schema. Different output for equal
inputs is an invariant failure. Cancellation or failure cannot publish a partial graph node.

## Publication and reuse

A loaded graph contains exact source and generated-source nodes, resolved package metadata, product selections, and
acyclic dependency order for each target. Stable identities determine the order of independent work. Readers can share
the graph across compiler queries and tools without mutable project caches or ambient lookup hooks.

Queries may index and retain graph values, but project meaning stays immutable. Manifest diagnostics preserve specific
read, validation, selection, and execution causes as structured data for the common message layer.

## Related documents

- [Manifest reference](../tools/project-manifests.md)
- [Bray Tack](bray-tack.md)
- [Standard library](standard-library.md)
- [Compiler architecture](compiler-architecture.md)
