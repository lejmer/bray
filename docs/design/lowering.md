# Lowering and MIR design

`bray-lowering` translates source-shaped bound HIR and established semantic results into execution-shaped MIR. `bray-ir`
owns MIR representation, builders, traversal, and validation.

## A change of representation

MIR makes control flow, evaluation order, values, addressable storage, calls, cleanup, and exceptional exits explicit.
It exists because execution has a different structure from source, rather than because another phase needs another tree.

A MIR unit is an immutable execution graph with typed identities and explicit external references. Blocks contain
ordered operations and one terminator defining their successor edges. Values and block parameters represent computed
results, while storage represents addressable state whose lifetime or ABI role matters.

Source branches, loops, matching, short-circuiting, and propagation become graph structure. Backends do not receive
high-level nodes that still require source interpretation.

## Reuse and execution identity

MIR reuses semantic types, constants, concrete callable and implementation identities, ABI decisions, compiler-known
roles, and target information when their meaning is unchanged.

Lowering introduces identities for execution-level blocks, operations, values, storage, frames, and continuations. A
temporary does not become a symbol, and a source expression does not automatically require addressable storage.

Backend machine types and instruction details stay behind the backend boundary. Prefixing an existing semantic type with
`Mir` is not a reason to duplicate it.

## Checked input

A source-backed lowering request borrows one bound unit and the exact checker-published results it needs. These include
selected operations, storage and dependency decisions, control completion, lifecycle and async plans, and the selected
target and compiler-known view.

The checker establishes their identity, category, completeness, and recovery contracts before publishing them for
lowering. Lowering consumes the published results directly instead of reconstructing a second admission plan. Recovery
remains useful during checking while recovered semantic values continue to provide the inputs required for executable
semantics. Published cleanup decisions distinguish absent work from missing information.

Compiler-generated hosts have a separate typed input because they have no source bound unit. Compile-time-only units
have an explicit classification and do not demand execution analyses.

## Construction and naming

Transformation methods use `lower_*`. Helpers that append decided operations, validate, or inspect inputs use names for
those narrower tasks.

A task owns its mutable builder and temporary construction state. It translates the unit, validates the graph, and
publishes one immutable result. Private intermediate forms are discarded rather than becoming another cached
representation.

Merge values use one MIR-wide mechanism. Evaluation order and exits are explicit, including cleanup on early, abnormal,
and normal paths. Source provenance supports diagnostics and inspection without deferring semantic decisions to the
backend.

## Calls and cleanup

Calls retain the selected target, argument mapping, substitutions, defaults, conversions, ownership modes, and dispatch
contract. Lowering chooses an execution sequence for those established decisions without repeating selection or proof.

Cleanup follows checked lifecycle and dependency plans. [Cleanup storage and reports](cleanup-storage-and-reports.md)
describes the shared realization used by source lowering and specialization. The graph makes cleanup and report
ownership explicit enough for MIR validation and code generation.

Runtime and helper requirements derive from typed MIR operations. Product formation selects compatible runtime artifacts
after merging reachable requirements.

## Async execution

Async lowering consumes checked frame identity, suspension liveness, affinity, deferred contracts, and cleanup plans. It
emits explicit suspension, continuation, cancellation, task, and completion operations.

Frame retention and initialization remain distinct. Specialization preserves the execution context at each operation,
including context needed by cleanup that introduces suspension. Typed runtime roles connect execution to the runtime
without source-name recognition.

[Async runtime](async-runtime.md) describes frame and task ownership across the semantic, lowering, and runtime
boundaries.

## Demand, validation and backend boundary

Compilation lowers only units needed by a product or an explicit MIR request. Semantic checking alone does not require
lowering. Independent units can lower concurrently, while repeated requests share a completed result rather than a
mutable builder.

MIR validation enforces graph, identity, typing, storage, call, and cleanup invariants independently of a backend.
Invalid compiler-produced MIR is a compiler failure, distinct from source diagnostics and cancellation.

Code generation consumes validated MIR and resolved target-facing inputs. Backend legalization preserves those
decisions. A backend needing to inspect syntax, select an overload, or infer cleanup indicates a missing earlier
contract.

## Related documents

- [Binder and bound tree](binder.md)
- [Checker](checker.md)
- [Code generation](codegen.md)
- [Compiler architecture](compiler-architecture.md)
