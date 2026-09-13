# Semantic checker design

`bray-checker` owns semantic rule algorithms and analysis state. It consumes resolved bound structure and typed semantic
inputs, then returns immutable results for compilation to publish. The language documents define the rules being
checked.

## Services and naming

A focused rule check decides one semantic question from explicit inputs. A flow domain owns a coherent state, transfer
and merge operations, convergence, recovery, diagnostics, and durable results over a semantic unit.

Rule-enforcement methods use `check_*`. Operations that compute, select, infer, evaluate, or merge use names describing
that responsibility. The prefix distinguishes policy enforcement from other work inside the checker.

Services consume validated read-only unit views and narrow semantic contexts. They use typed symbol contracts across
source, imported, and compiler-known origins. Source anchors provide diagnostic correlation without exposing arbitrary
syntax traversal to checker policy.

The checker does not mutate bound nodes or publish another checked tree. Durable result types belong to the lower
representation that owns their meaning. Solver state and algorithms remain checker-owned.

## Demand and semantic cooperation

Typed accessors request expression types, selected operations, storage plans, body behavior, or another particular
result. Correlated results can share an owning stage when they require the same traversal or fixed point. Requesting one
stage does not imply a universal checker schedule.

Type inference and semantic selection cooperate in one task-local fixed point. Candidate results can supply type
evidence and expected contexts can constrain operands. Intermediate rounds do not publish final diagnostics or partially
resolved semantic values.

Source type-expression templates join this cooperation when embedded constants need typing or selection. Checked
occurrences become stable open terms, and their containing types or signatures are resolved without forcing unrelated
declarations.

Candidate discovery retains identities and evidence without claiming applicability. Selection commits after the relevant
constraints are stable. Post-selection ownership, behavior, and ABI checks validate that operation rather than using
their failures to choose another candidate.

## Domain relationships

The main dependency relationships are:

```text
selected target and declaration availability
                    |
bound structure and symbol semantics
                    |
        types and semantic selection
                    |
    patterns, representation and target validity
                    |
          storage/access planning
                    |
    shared control flow and reachability
               /             \
         refinement        liveness
               \             /
             composite storage flow
                    |
         dependency-contract propagation
                    |
      effects, contracts and boundary validation
```

This diagram describes dependencies where needed, not a mandatory pass list for every request. Independent domains can
run concurrently. Mutually dependent semantics belong in one explicit fixed point instead of forming circular requests
between supposedly complete results.

Constant, predicate, and declaration-owned expression analysis reuse ordinary selected semantics. They do not recreate
name resolution, type selection, or control-flow models.

## Types, selection and patterns

The type domain owns compatibility, inference, literal adaptation, and stable result types. Expected types constrain
expressions but do not replace independent type evidence. Generic definitions retain symbolic context, with concrete
requests checking substitution-dependent properties on demand.

The selection domain consumes typed candidate sets and compiler-known role bindings. Its durable results retain exact
targets, substitutions, argument mappings, conversions, and witnesses. Candidate evidence remains distinct from a
checked selection.

Pattern analysis establishes compatibility, bindings, coverage, required operations, and refinement seeds. Storage and
refinement domains determine whether those operations and facts remain valid at the relevant program point.

## Constants, representation and proof

Constant validity and concrete evaluation are separate operations. A checked generic definition can remain open.
Concrete evaluation consumes selected semantic operations and exact dependencies, with deterministic work limits and
target-aware numerical representations.

The evaluator uses the language's selected integer and floating-point semantics rather than host arithmetic.
Materialized values enter the semantic store, while evaluation intermediates remain local.

Declared representation analysis describes source-level type structure, copying and lifecycle properties. Physical
offsets, sizes, and ABI layout are separate target-dependent results. Public checked representation information can
cross package interfaces without rechecking dependency source.

Proof retains typed subjects, dependencies, and trusted provenance. Proven, disproven, unknown, and recovered outcomes
remain distinct. A Boolean answer alone cannot carry the evidence needed by later contract and boundary checks.

## Target availability and validity

Preselection availability determines which declarations can participate for the selected target. Its foundational
context depends on target properties rather than ordinary source selection, avoiding a cycle.

Post-selection validity checks the chosen representation, layout requirement, or callable ABI. It consumes the same
validated target profile and compiler-known view used throughout compilation. It can reject the selected operation
without reopening selection.

Results retain exact target dependencies for diagnostics, package interfaces, and incremental invalidation.

## Shared control flow

Flow-sensitive domains share one immutable checker-private graph per semantic unit. It establishes a common evaluation
order, branch structure, and typed exits. Forward and backward analyses use the same operations and edges.

Reachability provides control completion and the reachable mask. Refinement is a forward must-analysis that retains
propositions valid on every incoming path and invalidates them when their dependencies change. Liveness is a backward
may-analysis of subjects needed by later operations.

Only consumer-required projections become durable results. Full point states, graph IDs, work lists, and traversal
indexes remain private.

## Storage planning and flow

An immutable storage plan records origins and evaluated access paths from checked types, patterns, and selections.
Planning identifies the operations to analyze without deciding their flow legality.

Initialization, movement, borrowing, overlap, mutation authority, and lifecycle obligations share one composite forward
domain. These concerns are mutually dependent, so independent passes would duplicate state or validate against stale
results.

The domain consumes refinement and liveness and owns flow-sensitive run and cleanup obligations as well as ordinary
storage state. Other domains observe its finalized decisions instead of maintaining another obligation state.

Merges conservatively preserve requirements and exact partial-state identity. Access IDs are occurrence identities, so
their inequality is not a proof of disjoint storage.

## Dependencies, behavior and async plans

Dependency analysis infers the requirements carried by values and accesses from established storage, capability,
refinement, and witness results. Portable templates use formal subjects, while bound contracts refer to unit-local
subjects. Aggregate guards and transfer requirements remain explicit.

Effects and contract validation consume selected operations and finalized flow results to establish the body summary and
its relation to the declaration's surface. They do not rerun lifecycle state transfer.

Async analysis separates invocation semantics from deferred execution and publishes frame, suspension, affinity, and
cleanup information. Machine layout remains downstream. Cleanup plans preserve partial storage state and separate
cancellation broadcast from lifecycle resolution. [Async runtime](async-runtime.md) and [cleanup
storage](cleanup-storage-and-reports.md) describe those cross-component choices.

## Fixed points and bounded work

Shared worklist machinery owns scheduling. Each domain owns its state meaning, direction, boundaries, merge, convergence
argument, and durable projection.

Merges are deterministic and monotone. Finite domains converge by height, and richer domains require an explicit
widening or bound. A broken convergence invariant is distinct from language-defined resource exhaustion.

Result-affecting limits belong to the immutable semantic request and its dependencies. They use deterministic work
measures rather than elapsed time or available memory. Scheduling limits and cache retention remain performance policy.

## Recovery and publication

Each domain has a conservative recovery result that preserves useful context without establishing an unsupported proof.
Unknown, disproven, recovered, unavailable, cancelled, and infrastructure failure are not interchangeable.

Diagnostics remain source-correlated and owned by the deciding operation. Independent errors survive recovery, while
cascades of the same root cause are suppressed. Compilation merges diagnostic ownership deterministically.

Cancellation discards task-local state and publishes nothing. Successful publication validates unit identity and freezes
the result with its diagnostics. Lowering consumes explicit completed results rather than evidence that a pass happened
to run.

## Related documents

- [Compiler architecture](compiler-architecture.md)
- [Binder and bound tree](binder.md)
- [Symbols](symbols.md)
- [Lowering](lowering.md)
- [Compiler diagnostics](compiler-diagnostics.md)
