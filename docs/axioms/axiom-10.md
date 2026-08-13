# Axiom 10: Dependencies are part of the program

A program's dependency graph is part of the program's source reality.

The compiler, build system, and tooling operate on an explicit build graph, source graph, and dependency graph. The graph that determines a build is visible, reproducible,
and inspectable.

Dependencies are declared in project-owned files. Dependency resolution produces a concrete lockable result: selected versions, source identities, artifact identities,
checksums, feature selections, build settings, target constraints, and linkage requirements are recorded as part of the build contract.

Vendoring is the default dependency model. A vendored dependency may be source code, generated code, a compiled artifact, an interface description, a header/module description,
metadata, or another declared build input. Vendoring means the project owns the exact dependency input used by the build. It does not require every dependency to be available
in source form.

Compiled dependencies are first-class dependency graph nodes. Their ABI, target platform, architecture, calling convention, exported interface, linkage mode, version identity,
and integrity checks are part of their declared contract.

Foreign libraries and system libraries are explicit boundary dependencies. When a dependency must come from the host system, SDK, toolchain, or platform, the required identity,
version range, target constraints, discovery rules, and validation checks are declared in the graph.

Imports and modules resolve through the declared source and dependency graph. Adding a dependency changes available behavior only through explicit imports, declarations, or
linkage contracts according to the language rules.

Build scripts, generated code, compiler plugins, foreign artifacts, and tool-driven source transformations are explicit graph nodes with declared inputs, outputs, permissions,
and reproducibility contracts.

The same declared source graph and dependency graph produce the same compiler input on every machine with the same target configuration.
