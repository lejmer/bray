# Symbols design

`bray-symbols` gives declarations precise semantic identities and relationships. It owns the symbol model and the
semantic value contracts used by binding, checking, tooling, and output. Compilation coordinates queries whose
implementation belongs to the binder or checker.

## Identity and representation

A declaration is a source contribution. A symbol identifies the semantic entity that contribution defines. Several
module parts can contribute to one module symbol, while conflicting declarations remain distinguishable for diagnostics.

A cheap, deterministic identity skeleton precedes expensive declaration semantics. It establishes roots, owners,
origins, source anchors, and keys for synthesized surface entities without binding executable bodies. Lazy request order
and worker scheduling therefore do not determine symbol identity.

Each category has a typed ID and kind-specific record or view. Closed family IDs serve operations that intentionally
accept several categories. Erased identities support diagnostics and heterogeneous tooling, while semantic storage and
relationships retain their specific types.

Shared private components and generated plumbing reduce representation boilerplate. They do not create an inheritance
hierarchy or a universal record full of category-dependent optional fields.

Numeric IDs belong to one compilation snapshot. Stable structural keys support persistent references and deterministic
remapping across snapshots.

## Origins and roots

Source, imported, compiler-known, compiler-provided, and synthesized declarations use the same category-specific symbol
contracts. Origin identifies the provider of a symbol's data rather than creating another hierarchy.

The graph is a forest of package roots and one compiler-known environment root. Source and imported modules belong to
their packages. Compiler-known modules and ambient declarations belong to the compiler-known environment. There is no
synthetic compilation-root symbol.

Containment describes semantic ownership. Lookup, imports, exports, and references add graph edges without reparenting
symbols. Dotted module paths use path indexes rather than invented containing modules. Ambient compiler-known visibility
is a lookup relationship.

Compiler-known roles map to exact symbol identities through the generated catalog. Semantic phases use those roles
instead of recognizing names. Imported providers reconstruct ordinary symbol contracts through package interfaces,
without requiring dependency syntax.

## Relationships and lookup

Owners expose meaningful typed collections: modules expose declarations, types expose fields and associated members,
variants expose payloads, and callables expose parameters and contracts. A generic traversal can project those
relationships for tooling, but it is not their source of truth.

Type-associated surfaces aggregate direct and inherent members while retaining each member's declaring owner and
provenance. Aggregation stores existing identities. Constructed-type views apply substitution and applicability to the
definition-level surface. Trait fulfillment remains a separate relationship to an exact trait member.

Implementation candidates and selected implementations are distinct. Candidate discovery gathers identities and
evidence. The checker commits a selection after evaluating applicability and coherence.

Lookup indexes reflect the language's ordinary namespace and remain owner-specific. A context-specific request resolves
a name and checks its category without inventing another namespace. Results retain ambiguity, inaccessibility, recovery,
and wrong-category information needed for precise diagnostics.

Stable enumeration provides deterministic metadata and diagnostics. It does not give the first candidate semantic
precedence.

## Declaration semantics and completion

Kind-specific context-bound views expose lazy signatures, members, constraints, defaults, and other declaration
semantics. `bray-symbols` owns their contracts, binding and checking compute their meaning, and compilation owns caching
and publication. This keeps the crate graph acyclic.

Completion has separate boundaries:

- Identity completion establishes the symbol and its source or provider relationship.
- Declaration-surface completion obtains the semantics needed to describe and use that declaration.
- Executable body analysis belongs to bound-unit and checker queries, outside symbol completion.

Force completion fans out over ordinary declaration queries and owned relationships in deterministic order. It does not
recursively complete every referenced symbol or check every executable body. Narrow tooling requests can obtain a member
list or signature without forcing stronger results.

## Declaration-owned expressions

Defaults, constant definitions, predicates, and contract expressions have declaration ownership even though their full
checked representation belongs to binding and checking. Symbol APIs expose summaries and typed relationships, not a
duplicate executable tree.

Runtime defaults have synthesized provider identities derived from their exact owner. Providers remain outside ordinary
lookup and become reachable lowering work when needed. Their inputs, result dependencies, and behavior are explicit so
source and imported defaults share the same contract. Invalid defaults retain their identity and diagnostics without
becoming valid executable providers.

Constant definitions are checked templates. Concrete evaluation additionally depends on substitution, selected
implementation, and target. Definition diagnostics remain separate from instance-specific diagnostics.

Predicate definitions describe semantic propositions rather than one precomputed Boolean result. Applications and proof
requests use the checked definition and their exact arguments.

Signature-only dependencies allow declaration recursion without repeatedly forcing the owner's defaults or bodies.
Declaration completion can request a body when a particular semantic operation needs it, such as constant evaluation,
while preserving that body's separate ownership.

## Semantic values

Types, trait applications, substitutions, callable instances, selected witnesses, constants, and portable dependency
contracts are semantic values rather than new declaration symbols. A constructed value references its definition and
arguments.

The compilation-owned semantic store interns these immutable values through contracts owned by `bray-symbols`. Open
terms remain distinct from closed constants, and open substitutions from validated concrete substitutions. Inference
variables stay checker-local.

Portable dependency templates refer to formal subjects and structural projections. Unit-local storage, evaluated access
occurrences, and borrow capabilities belong to the bound representation. They do not enter persistent symbol templates.

## Local regions

Bodies and declaration-owned expressions own immutable local-symbol snapshots. Binding publishes the local snapshot,
bound unit, and diagnostics together rather than appending locals to the global symbol graph.

Local IDs include their semantic region and category. Region keys derive from semantic owners and source anchors, so
nested callable identity can be established without eagerly checking the nested body. Snapshots can be analyzed and
replaced independently.

Lexical scopes and semantic containment are separate. Scope graphs retain lookup ancestry and visibility boundaries,
while root scopes reference applicable declaration-surface parameters without copying them into local storage. Anonymous
callables own separate regions and callable boundaries.

Recovery retains useful local identities and source anchors without allowing an invalid declaration to overwrite a valid
lookup entry.

## Publication, cycles and recovery

Queries publish immutable values with their diagnostic ownership. Repeated requests and force completion observe the
same results. Cancellation and abandoned speculation publish neither partial semantics nor partial local snapshots.

Cycle handling belongs to the semantic query category. The query runtime detects recursive dependencies and cross-worker
waits, while the owner distinguishes legal recursive structure from erroneous semantic cycles. Cache locks do not remain
held while computing prerequisites.

Error-aware symbol results preserve the category and available context needed by later phases. Compiler invariant
failures remain distinct from malformed source. [Compiler diagnostics](compiler-diagnostics.md) and [compiler
architecture](compiler-architecture.md) describe the shared publication and reuse model.

## Related documents

- [Declaration discovery](declaration-discovery.md)
- [Binder and bound tree](binder.md)
- [Checker](checker.md)
- [Compiler-known catalog](compiler-known-catalog.md)
- [Compiled package interfaces](compiled-package-interfaces.md)
