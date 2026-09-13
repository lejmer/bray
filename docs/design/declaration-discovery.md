# Declaration discovery design

`bray-declarations` records the source declaration surface between parsing and symbol construction. It consumes lossless
syntax and publishes immutable declarations, containers, module contributions, and diagnostics. The [symbol
layer](symbols.md) gives those declarations semantic identity, while the [binder](binder.md) owns body-local
declarations and lexical scopes.

## Source identity before semantics

A declaration and a symbol have different identities. A malformed or conflicting declaration still needs a stable source
identity for diagnostics even when it cannot become an ordinary symbol.

Discovery records declaration structure and source correlation without resolving names, binding types, or evaluating
expressions. Compact syntax anchors retain access to source-owned syntax without embedding public green-tree internals
in the declaration table. Surface information needed by later phases is captured once rather than repeatedly
rediscovered.

## Containers and module contributions

Containers organize declaration spaces before semantic symbols exist. A declaration belongs to an owning container and
can introduce a child container for members, signature parameters, or payload fields.

Logical modules can receive contributions from several source units or block module declarations. The logical container
provides the common declaration space. Individual module parts preserve each contribution's source context and location.

Discovery stops at declaration surfaces. Callable bodies and expression-local declarations remain demand-driven binder
work. This boundary gives signatures stable identity without forcing body analysis.

## Independent discovery and deterministic merge

Each source-unit task produces an immutable declaration chunk using local construction state. A deterministic merge
borrows those chunks and publishes the declaration table with final identities and indexes.

Identity and iteration order follow package, source-unit, and source order. Worker completion order has no effect on the
result. Cached chunks remain immutable and reusable, rather than becoming fragments of a shared mutable table.

The syntax walker supplies traversal mechanics. Declaration discovery decides which syntax introduces a declaration or
container and which children need further traversal.

## Selection and diagnostics

Contribution gates select the declaration surface for a product and target before conflicts in that surface are
diagnosed. This keeps source discovery reusable while allowing the semantic environment to reflect the selected build.

Discovery owns diagnostics that can be decided from declaration structure. Errors requiring name resolution, type
information, or body analysis remain with their semantic owner. Recovered source retains its identity without creating
cascades of duplicate diagnostics.

Chunk diagnostics travel with their source results. Merge adds table-wide diagnostics in deterministic source order.
Both use the shared structured diagnostic model.

## Related documents

- [Compiler architecture](compiler-architecture.md)
- [Symbols](symbols.md)
- [Compiler diagnostics](compiler-diagnostics.md)
