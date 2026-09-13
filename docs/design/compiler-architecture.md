# Compiler architecture

Bray's compiler is organized around immutable representations and typed, demand-driven results. Each phase owns a
distinct kind of information. The [language documents](../language/index.md) define the language, while these documents
explain the design that implements it.

## Pipeline and representations

The logical dependency order is:

```text
source -> lexing -> parsing -> declaration discovery -> symbols
       -> binding and semantic analysis -> lowering -> validated MIR
       -> backend generation -> emission -> linking
```

These are dependencies between results, not whole-program scheduling barriers. Independent source units, declarations,
bodies, semantic instances, and backend units can advance as soon as their inputs are available.

The durable program representations are lossless syntax, source-shaped bound HIR with immutable semantic results, and
execution-shaped MIR. Each has one owner. Analyses extend knowledge about a bound unit through typed results rather than
copying its nodes into successive checked-tree families.

| Component                                         | Responsibility                                                            |
|---------------------------------------------------|---------------------------------------------------------------------------|
| Source                                            | Immutable inputs, source identity, ranges and location mapping            |
| Lexer and [parser](parser.md)                     | Demand-driven tokens, lossless syntax and recovery                        |
| [Declaration discovery](declaration-discovery.md) | Source declaration surfaces and deterministic merging                     |
| [Symbols](symbols.md)                             | Semantic declaration identity, containment and lazy declaration semantics |
| [Binder](binder.md)                               | Resolved references, lexical scopes and source-shaped bound units         |
| [Checker](checker.md)                             | Semantic selection, proof, storage and flow analysis                      |
| [Lowering and MIR](lowering.md)                   | Explicit execution from established semantics                             |
| [Code generation](codegen.md)                     | Concrete instance collection and backend translation                      |
| [Emitter](emitter.md)                             | Artifact planning, serialization coordination and atomic publication      |
| [Linker](linker.md)                               | Native linking from a resolved typed plan                                 |

Project tools load one explicit immutable package-product graph through `bray-project`. [Bray Tack](bray-tack.md)
orchestrates independently installable compiler, formatter, and language-server executables. It does not embed their
implementations.

## Demand-driven results

Public compiler APIs expose the result a consumer needs: a declaration surface, expression type, selected call, body
diagnostics, or product artifacts. The owning query obtains its prerequisites internally. A requested result is complete
within its promised boundary, without forcing unrelated later analyses.

Compilation owns the query graph and the context that makes its results meaningful. Syntax, symbols, semantic values,
target inputs, and package identity come from that context. A caller cannot fill a cache using an unrelated semantic
universe.

Correlated results can share one immutable owning stage when they share a traversal or fixed point. Focused accessors
borrow views from that stage. There is no universal completed-program object, and cache presence is not evidence that an
unrelated semantic requirement has been checked.

Scheduling, single-flight evaluation, dependency recording, waiting, cancellation, and invalidation are private query
mechanics. Ordinary helpers remain ordinary helpers. Query boundaries correspond to meaningful results with an explicit
dependency and reuse story.

## Immutability and identity

Construction state stays local to builders and analysis tasks. Published source snapshots, syntax, declaration tables,
symbols, bound units, semantic results, MIR, plans, and diagnostics are immutable.

Syntax uses shareable parentless green storage with relative widths. Typed views add source context and named components
without duplicating child storage. Token trivia and explicit missing or skipped syntax retain source reconstruction and
recovery information.

Typed IDs distinguish identities owned by different representations. Kinds classify objects, and source spans locate
them. Neither substitutes for semantic identity. Persistent interfaces use structural keys and source anchors rather
than arena positions or process-local handles.

One compilation-owned semantic value store interns types, constants, open terms, substitutions, and semantic
applications. Solver variables and inference state remain local to their analysis. Structural identity, rather than
interning order, governs serialization and deterministic comparison.

## Parallelism and resource ownership

A bounded scheduler runs independent work at stable semantic boundaries. Serial execution uses the same dependency graph
and result contracts. Compiler CPU work respects the invocation's worker budget, while external-tool and I/O waits
remain separately observable.

Broad requests discover and complete dependency closures, while interactive requests ask for narrow results directly.
Both use the same queries and caches. Publication order follows stable source and semantic order rather than worker
completion.

Priority affects scheduling, not semantic identity. Interactive work can receive preference without starving ordinary
work. Cancellation belongs to an interested request: abandoning one waiter does not cancel shared work still needed by
another. Incomplete or cancelled work publishes no reusable result.

Mutable backend modules, solver work lists, and builders remain task-local. Synchronization in caches and scheduling
does not become observable compiler state.

## Snapshots and reuse

An immutable compilation snapshot identifies the project graph, source inputs, selected product and target, compiler
semantics, and toolchain. Results record their exact dependencies so replacing an input invalidates its transitive
consumers.

Reuse requires matching structural identity and dependency fingerprints. Reused values are remapped into snapshot-local
identities. Cache eviction and retention are performance choices, and discarding caches cannot change compilation
semantics.

Result-affecting semantic limits participate in the relevant identity. Worker count, request priority, cancellation, and
cache retention do not. Variable-size caches have bounded retention without disrupting coordination of in-flight work.

Language tooling requests the narrowest relevant result through the same compiler APIs. Source-position access resolves
to source-versioned identities and exposes available, recovered, or unavailable results without adding protocol concerns
to compiler representations.

## Target and runtime inputs

One selected target context supplies the validated machine model, layout and ABI capabilities, and compiler-known
availability. All target-dependent phases consume that shared context rather than inferring properties from the host or
creating competing target models. Target-independent results remain reusable across target changes.

Libraries publish runtime requirements. Executable and test products select a compatible runtime after reachability is
known. Runtime ABI vocabulary, portable runtime semantics, and artifact compatibility have separate owners, described in
[async runtime](async-runtime.md).

[Compiled package interfaces](compiled-package-interfaces.md) supply dependency semantics without dependency source. The
[compiler-known catalog](compiler-known-catalog.md) supplies stable language identities from prevalidated generated
data. Both enter the ordinary typed symbol and semantic model.

## Shared compiler infrastructure

Representation owners provide reusable walkers, typed visitors, cursors, and builders. Walkers make traversal and
descent explicit, while visitors provide kind-specific dispatch. Parallel work schedules independent roots and uses the
same traversal machinery within each task.

Typed sinks collect diagnostics and artifacts for deterministic publication. Narrow read-only contexts expose the owning
phase's capabilities without becoming access to every compiler subsystem.

Backend-specific state stays behind the backend contract. LLVM objects do not enter bound HIR, Bray MIR, compilation
query results, or emitter APIs. Emission plans coordinate backend serialization and package-interface artifacts, and the
linker consumes a resolved plan without discovering language semantics.

## Diagnostics, recovery and observation

Each phase owns errors it has enough information to diagnose. Structured diagnostics preserve source correlation and
specific causes until rendering in `bray-messages`. Parent requests merge immutable diagnostic collections
deterministically.

Recovery is explicit in the representations. Missing or erroneous user input remains distinguishable from valid data and
compiler invariant failures. Later phases can continue useful analysis without treating placeholders as ordinary valid
values.

Profiling observes the same query graph without changing its results or identities. Shared instrumentation separates
active compiler work, scheduling, dependency waits, and external tools. Bounded tracing and a cheap disabled path keep
observation from becoming a new compiler bottleneck. [Profiling guidance](../contributing/profiling.md) covers commands
and reports.

## Further design

- [Compiler diagnostics](compiler-diagnostics.md)
- [Standard library](standard-library.md) and [core data](core-data-standard-library.md)
- [I/O and platform services](io-and-platform-services.md)
- [Foreign interoperability](foreign-and-platform-interoperability.md)
- [Testing](testing.md)
- [Contributor conventions](../contributing/coding-conventions.md) and [crate ownership](../contributing/crates.md)
