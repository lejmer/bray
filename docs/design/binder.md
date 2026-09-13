# Binder and bound tree design

The binder resolves syntax references and constructs Bray's source-shaped high-level IR. `bray-bound-tree` owns that
immutable representation. Focused checker services establish semantic results beside it, and compilation owns their
demand and publication.

## Phase ownership

| Owner | Responsibility |
| --- | --- |
| `bray-symbols` | Declaration identities, semantic values and symbol-facing contracts |
| `bray-bound-tree` | Bound units, typed nodes, local storage identities and durable semantic result contracts |
| `bray-binder` | Reference resolution, lexical construction and bound-unit assembly |
| `bray-checker` | Semantic rules, inference and analysis state |
| `bray-compilation` | Query context, dependencies, caching and publication |
| `bray-lowering` | Execution-shaped MIR from established semantic results |

The binder receives a narrow read-only query context implemented by compilation. It does not depend on compilation
internals. A symbol-facing result may require binding or checking without moving those algorithms into the symbol crate.

Operations that bind syntax use the `bind_*` prefix at every level. Lookup, construction, checking, and publication
helpers use names for those narrower responsibilities. Public compiler APIs expose lazy results rather than
caller-managed binding workflows.

## Semantic units and immutable HIR

An independently requested body, anonymous callable, declaration-owned expression, or ordered contract expression
sequence forms a semantic unit. Ordinary blocks and patterns stay within their containing unit.

A bound unit owns one arena, an exact root, its local-symbol snapshot, and stable references to nested units. Typed node
IDs include unit identity. Published nodes retain source anchors and resolved references without copying source text,
trivia, or symbol records.

Construction is task-local. Publication freezes the tree and local snapshot together. Later analyses return typed
results associated with that same unit rather than mutating it or creating successive checked-tree wrappers.

Nested callables own separate units. Enclosing units retain their keys and request only the nested results needed for
the current semantic question. Broad diagnostic or emission requests follow the reachable unit graph through
compilation.

Representation-owned walkers and visitors provide deterministic traversal. Parent indexes, when useful for tooling, are
derived views rather than the ownership model.

## Binding and checker cooperation

Binding fixes source-semantic structure and reference identity. Checker services own type relations, selection, proof,
storage, effects, and other language policy.

Fine-grained cooperation uses explicit candidate and expected-context inputs. Contextual typing and selection can depend
on one another without publishing partially decided nodes. Unit-scoped analysis consumes committed read-only bound data,
not private binder builders.

A declaration-surface query that only resolves a type or relationship need not allocate a bound tree. Expression-backed
declarations share their bound representation with semantic and lowering consumers while exposing a smaller
symbol-facing summary.

Cheap body-presence queries remain separate from body analysis. Cancellation is a query outcome, not an absent body or a
fabricated semantic error.

## Semantic values and inference

The symbol-owned semantic store contains immutable types, closed constants, open constant terms, substitutions, and
applications. These values compose with symbol identity, so keeping their representation below the binder and checker
avoids a dependency cycle.

Structural construction and substitution operate on already established values. They do not perform lookup, candidate
selection, or proof. Inference variables, unification state, exact intermediate arithmetic, and evaluation stacks remain
checker-local.

Open constant terms preserve selected operations and evaluation order. They provide the restricted identity needed by
generic types without becoming another general bound tree. Contextual proof can relate distinct open terms without
globally merging their identities.

Declaration types can retain source type-expression templates before embedded constants are checked. Those occurrences
preserve their declaration context and expected-type source. The cooperating semantic analysis resolves only demanded
occurrences into checked terms and stable types. Target-dependent interpretation remains separate from source occurrence
identity.

Concrete substitutions distinguish fully resolved inputs from open generic contexts. Persistent interfaces encode
structural values and remap them into the consuming store.

## Storage and semantic dependencies

The representation follows the language's storage terminology. A binding symbol, a storage origin, an evaluated
storage-access occurrence, and a borrow capability are separate identities.

An access retains its root and ordered typed projections. Dynamic selectors remain semantic operands rather than source
strings. Two evaluations of similar syntax can reach different storage, and distinct accesses can reach the same
storage. Occurrence identity therefore does not replace overlap analysis.

Borrow records preserve origin and derivation. Their activity at a program point belongs to flow analysis. Moving an
owning value transfers its carried dependencies without creating a new identity for the allocation it owns.

Portable dependency templates use formal declaration subjects. Instantiated bound contracts refer to exact unit-local
storage, accesses, capabilities, witnesses, and obligations. Guarded dependencies retain the condition under which they
exist rather than becoming unconditional requirements.

A semantic `DependencyContract` is distinct from a compiler `QueryDependency` or `FactDependency`. The former describes
requirements on a program value or access. The latter controls evaluation and invalidation in the compiler.

## Local symbols and speculation

Local identity and lexical visibility are separate concerns. Identity can be assigned deterministically before binding,
while name activation follows source scope. The published local snapshot references applicable declaration parameters
and owns local bindings and scopes.

Speculation uses task-local checkpoints and rollback trails. Abandoned candidates leave no nodes, local symbols,
selected results, or diagnostics in the committed unit.

Observed query dependencies need separate treatment: an observation that influenced rejection, ambiguity, ordering, or
selection still affects the answer even when its candidate was abandoned. Only irrelevant observations can be omitted
from invalidation dependencies.

## Flow analysis boundary

Checker domains share one immutable, checker-private control-flow graph per unit when flow analysis is required. It
fixes semantic operation order and branch structure for both forward and backward analyses.

The graph is neither bound HIR nor lowered MIR. It does not introduce backend-oriented execution or become part of a
package interface. Analyses can derive traversal indexes from it without defining competing control-flow models.

Mutually dependent initialization, movement, borrowing, mutation authority, and lifecycle state form a composite
storage-flow domain. Independent analyses retain their own typed state. Shared fixed-point mechanics do not imply one
universal analysis record.

Only durable results required by consumers are published. Work lists, iteration state, and private graph IDs remain
checker-owned. [Checker design](checker.md) describes these domains and their convergence model.

## Recovery, diagnostics and reuse

Category-specific error nodes preserve useful source shape and semantic context. Recovery suppresses cascades while
retaining independent errors. Invalid source, cancellation, and compiler invariant failures remain distinct outcomes.

Binding diagnostics belong to the bound result, and checker diagnostics to the operation that establishes the relevant
semantics. Nested and declaration-owned expressions retain stable diagnostic ownership across summary, tooling, and
lowering views.

Independent units can be bound and analyzed concurrently. Stable keys, source-semantic construction order, and immutable
publication make their results independent of worker order. Query-level cycle handling and snapshot reuse follow
[compiler architecture](compiler-architecture.md).

## Lowering boundary

Lowering receives a borrowed bound unit and the exact typed semantic results required for that unit and target. Matching
unit identity and semantic category establish that these inputs belong together.

Resolved calls, conversions, storage plans, control completion, lifecycle behavior, and compiler-known roles are
explicit inputs. Lowering does not infer missing semantics from cached work, names, or source syntax.

## Related documents

- [Symbols](symbols.md)
- [Checker](checker.md)
- [Lowering](lowering.md)
- [Compiled package interfaces](compiled-package-interfaces.md)
- [Compiler diagnostics](compiler-diagnostics.md)
