# Lowering and MIR design

This document defines the goal-state architecture for translating checked, source-shaped Bray HIR into backend-independent Bray
MIR.

The language documents define Bray semantics and observable evaluation behavior.

`docs/design/compiler-architecture.md` defines the compiler-wide phase, query, ownership, and publication model.

`docs/design/binder.md` defines bound units and the source-shaped HIR.

`docs/design/checker.md` defines the durable semantic facts consumed by lowering.

`docs/design/async-runtime.md` defines async frames, suspension, cancellation, cleanup, and the compiler/runtime boundary.

`docs/design/codegen.md` defines the backend boundary that consumes validated Bray MIR.

This document is authoritative for the lowering boundary, MIR representation, semantic primitive reuse, construction, validation,
and lazy compilation integration.

---

## Goals

The lowering architecture should:

- convert checked source structure into an explicit execution graph,
- make evaluation order, control flow, storage, cleanup, and exceptional behavior explicit,
- preserve every semantic decision already established by binding and checking,
- reuse canonical semantic identities and values when their meaning is unchanged,
- introduce MIR-owned types only for genuinely execution-level concepts and invariants,
- provide a backend-independent input that code generation can consume without interpreting source constructs,
- publish one immutable validated MIR unit for each demanded concrete bound unit or compiler-generated host,
- support lazy demand, independent parallel lowering, deterministic construction, cancellation, and incremental reuse,
- retain enough source correlation for diagnostics and inspection without retaining source structure as execution policy,
- reject invalid compiler-produced MIR before it reaches a backend.

---

## Non-Goals

Lowering does not:

- parse syntax, discover declarations, construct symbols, bind names, or check source semantics,
- infer types, select overloads, select implementations, or prove generic constraints,
- decide ownership, borrowing, effects, capabilities, contracts, layout, or source-level ABI rules,
- create another source-shaped tree with a reduced set of bound-node variants,
- copy canonical semantic primitives into MIR-prefixed equivalents,
- expose task-local lowering state as a durable compiler representation,
- introduce backend-specific types, instructions, target machines, or optimization policy,
- repair missing semantic facts by reinterpreting syntax or bound nodes,
- lower every unit eagerly merely because one product or tooling request needs one unit,
- publish partially constructed MIR after cancellation or failure.

---

## Terminology

### Bound HIR

The bound high-level intermediate representation, abbreviated HIR, is the immutable source-shaped semantic structure owned by
`bray-bound-tree`.

Bound HIR retains recognizable language constructs and close source correlation. Its semantic decisions are completed through
independently demandable durable facts rather than by progressively replacing the bound tree.

### MIR

Bray mid-level intermediate representation, abbreviated MIR, is the immutable execution-shaped representation owned by `bray-ir`.

MIR represents explicit basic blocks, operations, values, storage, control-flow edges, cleanup behavior, concrete semantic
references, and target facts required by code generation.

MIR is backend-independent. LLVM IR and another backend's internal representation are lower-level representations owned by their
respective code generation implementations.

### Lowering Input

`LoweringInput` is a validated borrowing view over one canonical bound unit and the exact durable semantic facts required to lower
that unit.

It is not a copied checked tree, a generic fact map, a completion marker, or a progressively enriched wrapper around the HIR.

### Lowering Task

A lowering task constructs one MIR unit from one validated lowering input or one compiler-generated host input.

Its mutable builder state is private to one worker. Only the completed immutable MIR unit may be shared or cached.

---

## The Representation Boundary

The phase boundary is:

```text
canonical bound HIR
    + exact durable semantic facts
    + selected compiler-known and target facts
    -> task-local lowering
    -> validated immutable Bray MIR
```

The reason for a separate MIR is a change in representation shape and invariants, not the existence of another compiler phase.

Bound HIR is organized around source constructs. MIR is organized around execution. A second hierarchy that mirrors bound
expressions, patterns, blocks, and declarations with fewer variants would not satisfy this boundary.

MIR must not preserve a source construct merely so code generation can interpret it later. Lowering must translate source-level
control and implicit behavior into the graph, operations, and references that define its execution.

Examples include:

- an `if` expression becoming conditional edges and merge values,
- a loop becoming explicit blocks, back edges, exits, and cleanup edges,
- short-circuit boolean operators becoming conditional control flow,
- pattern matching becoming projections, tests, bindings, and branch structure,
- propagation becoming explicit success and early-exit paths,
- implicit temporaries becoming MIR values or storage,
- moves, drops, finalization, and destruction becoming explicit operations and exits,
- a selected call becoming an operation with its concrete callable, argument mapping, ABI, and dependency behavior,
- async constructs becoming frame, suspension, resume, cancellation, and task operations.

MIR validity is therefore not defined only by excluding high-level variants. It is defined by graph, ownership, typing, control,
and operation invariants that do not apply to source-shaped HIR.

---

## Semantic Primitive Reuse

Lowering must introduce a new type only when it represents a genuinely different execution-level concept or enforces an invariant
that does not belong to an existing semantic type.

Canonical semantic meaning must retain its canonical type across the lowering boundary. MIR should directly reuse:

- semantic type and constant values,
- concrete symbol, callable, implementation, declaration, and member identities,
- compiler-known declaration and behavior-role identities,
- callable ABI and execution-mode decisions,
- target, layout, and representation facts whose meaning is already fixed,
- effect, capability, contract, ownership, and lifecycle decisions required by an operation,
- package and external-reference identities,
- source anchors and spans where downstream correlation is required.

MIR owns concepts created by lowering:

- MIR unit, block, operation, value, storage, frame, and continuation identities,
- block parameters and merge values,
- lowering-generated temporaries and storage allocations,
- explicit control-flow and cleanup edges,
- normalized operation and terminator categories,
- execution-level frame states and resume points,
- MIR provenance for compiler-generated operations.

MIR must not introduce types such as `MirType`, `MirConstantValue`, or `MirCallableIdentity` merely to indicate that an existing
semantic value appears in MIR. A MIR-specific projection is justified only when lowering adds a distinct invariant, such as an
assigned storage location, an explicit value definition, or a target-selected calling sequence.

Backend-specific machine types, registers, instruction values, and legalization details do not justify parallel MIR semantic
types. They belong to the backend's private low-level representation.

Shared semantic primitives remain immutable. Lowering references or borrows them through their stable identities and values rather
than cloning semantic graphs into MIR.

---

## MIR Shape

One MIR unit is a closed execution graph with explicit external references.

The durable unit should use compact typed identities and immutable indexed storage rather than a public generic child-tree API. At
minimum it contains:

- a stable unit identity and semantic origin,
- its selected MIR unit category and target facts,
- an exact entry block,
- immutable block, operation, value, storage, and frame tables,
- source provenance required for diagnostics and inspection,
- explicit references to other concrete units and external declarations.

Each basic block contains an ordered sequence of operations followed by exactly one terminator.

A terminator defines every successor edge of the block. Fallthrough is not implicit.

Values identify typed computed results, block parameters, constants, or other explicit MIR definitions. Operations consume values
or storage through typed references rather than nesting executable subtrees.

Storage identifies addressable state whose lifetime, initialization, movement, borrowing, destruction, or ABI role matters after
lowering. A pure temporary value should not become storage merely because the source expression had a node.

MIR tables may retain deterministic source order where it helps diagnostics, but table order must not substitute for explicit
control-flow or dependency relationships.

---

## Lowering Input Contract

Lowering may begin only with a validated `LoweringInput`.

For a source-backed unit, the input must borrow:

- the canonical bound unit,
- control-flow facts,
- final expression types,
- pattern and match facts,
- semantic selections,
- final literal values,
- the canonical semantic-value store owning referenced types, constants, and callable instances,
- storage identities and access plans,
- liveness and refinement facts,
- ownership, movement, and borrowing decisions,
- dependency contracts,
- async frame, suspension, task, and cleanup facts,
- body behavior and lifecycle obligations,
- target-available compiler-known identities,
- selected MIR unit and target facts.

Every fact must belong to the exact bound unit and semantic unit category being lowered. The constructor validates those ownership
relationships and any cross-fact completeness required by lowering.

The input contract should remain explicit. It must not be replaced with a generic fact map, an untyped bag, or a universal checked
unit object.

If lowering needs a semantic decision absent from `LoweringInput`, the input contract or an earlier semantic phase is incomplete.
Lowering must not perform the missing analysis itself.

Compiler-generated executable hosts use a distinct typed input because they have no source-backed bound unit. They still consume
the same canonical target, runtime, callable, and product facts where those meanings are shared.

---

## Lowering Construction

`bray-lowering` owns transformation policy and task-local construction state.

`bray-ir` owns MIR data types, generic builders, and representation validation.

Production transformation functions should use the `lower_` prefix. A function that only appends a previously lowered operation,
validates MIR, or observes semantic facts should use a name describing that narrower behavior.

The top-level source-backed entry point should conceptually have this contract:

```rust
pub fn lower_unit(input: LoweringInput<'_>) -> Result<MirUnit, LoweringError>;
```

The concrete result may carry structured diagnostics or cancellation according to the compiler-wide outcome contracts, but it must
not expose a partially built MIR unit.

A lowering task may recursively traverse source-shaped HIR as an implementation technique. Its published result must be the
execution graph, not a durable tree of intermediate lowered expressions.

The task-local lowerer should:

1. create a MIR builder from the validated input,
2. establish the entry block and unit-level storage,
3. lower bound control and operations in specified evaluation order,
4. create blocks, values, storage, and explicit exits as needed,
5. apply checked cleanup and async plans without rediscovering them,
6. commit every block with exactly one terminator,
7. validate and freeze the complete MIR unit.

Private helper states may track the current block, lexical exits, value results, cleanup targets, frame states, or source
provenance. They are implementation details and must not become another cached representation.

---

## Control Flow And Evaluation Order

Lowering must preserve the evaluation order defined by the language and established semantic facts.

Nested source control becomes explicit block structure. MIR should not contain general `IfExpression`, `MatchExpression`,
`LoopExpression`, or short-circuit expression nodes for a backend to interpret.

Expression lowering produces an explicit result appropriate to the expression:

- a value,
- storage,
- no value for diverging or statement-like behavior,
- a terminated control path.

Merge points use explicit block parameters, merge values, or another single MIR-wide mechanism. Different source constructs should
not invent unrelated merge representations.

Return, break, continue, propagation, panic, cancellation, and normal scope exit must route through the exact checked cleanup paths
that apply to that edge.

Unreachable blocks may exist only when required by construction or diagnostics and must be marked through explicit MIR control.
Code generation must not infer unreachability from missing source structure.

---

## Values, Storage, And Ownership

MIR separates computed values from addressable storage.

Lowering consumes checked storage and ownership facts. It does not rerun borrow analysis or decide whether an access moves, copies,
borrows, initializes, finalizes, or destroys a value.

For each evaluated access, lowering emits the operation selected by the checked storage plan and storage-flow decision. The MIR
operation must retain enough typed information for validation and code generation without retaining the checker algorithm that
produced the decision.

Lowering-generated temporaries receive MIR identities, not local symbols. They do not enter source lookup or the symbol graph.

Lexical storage, captured storage, async frame storage, ABI storage, and compiler-generated temporaries may use distinct MIR roles
when those roles enforce different lifetime or code generation contracts. They should not use separate storage types when one
typed role on the canonical MIR storage representation is sufficient.

---

## Calls, Dispatch, And Generics

Every lowered call must identify the concrete semantic target selected before lowering.

Lowering consumes:

- the selected callable or dispatch target,
- the argument-to-parameter mapping,
- instantiated type and constant arguments,
- selected runtime default providers,
- conversions and ownership modes,
- callable ABI and execution mode,
- required dependency and contract behavior.

Lowering may choose an execution-level sequence that implements those fixed decisions. It may not rerun overload resolution,
implementation selection, generic inference, accessibility, or contract checking.

Static, virtual, witness-based, foreign, compiler-known, and indirect calls may lower to distinct MIR operations when their
execution and validation contracts differ. They should share one operation representation when the difference is only an existing
semantic identity or typed role.

Generic specialization and reachable concrete-instance selection are compilation facts. MIR references concrete instances selected
for the requested product rather than carrying unresolved generic applications for the backend.

---

## Cleanup, Failure, And Lifecycle

Cleanup behavior must be explicit before code generation.

Lowering consumes checked lifecycle, liveness, storage, dependency, panic, cancellation, and body-behavior facts to construct:

- normal scope exits,
- early return and propagation exits,
- panic and abnormal exits,
- cancellation paths,
- finalization and destruction order,
- exactly-once cleanup behavior,
- cleanup required around calls, suspension, and task boundaries.

MIR validation must be able to reject missing, duplicated, or incompatible cleanup edges using MIR and its typed semantic
references. It must not need to invoke the checker again.

Lowering does not invent recovery semantics for invalid source. Product emission should not request MIR for a unit whose required
semantic facts are unavailable because of source errors.

---

## Async And Concurrency

Async lowering follows `docs/design/async-runtime.md`.

It consumes checked frame identities, suspension liveness, direct-await composition, task operation selections, cancellation
behavior, result propagation, affinity requirements, dependency contracts, and two-phase cleanup plans.

It emits explicit MIR frame, state, suspend, resume, task, cancellation, completion, and destruction operations. These operations
use typed runtime roles rather than source-level runtime or standard-library names.

Lowering derives portable runtime requirements and protected-frame contracts from checked facts. MIR refers to closed runtime roles
without selecting a runtime artifact or target-specific binary symbol. Product formation resolves those choices after reachable
requirements merge.

Async lowering must not rediscover live-across-suspension storage, choose task semantics, infer affinity, or derive cleanup by
walking types. Those are semantic decisions supplied by its input.

Compiler-generated host units establish the product-specific root frame and startup or shutdown sequence without pretending that
the host is a source declaration.

---

## Compiler-Known Behavior

Lowering receives target-available compiler-known identities and typed behavior roles.

It may map a recognized semantic operation to:

- a dedicated MIR operation,
- a typed runtime requirement,
- a normal concrete call,
- a target-selected execution sequence.

The mapping belongs to `bray-lowering`. It must be exhaustive over the compiler-known roles that lowering supports.

Lowering must not recover compiler-known behavior from source spelling, catalog keys, module paths, or symbol names.

---

## Source Correlation

MIR retains source anchors only where they support diagnostics, inspection, debugging, or stable provenance.

Lowering-generated operations should use the most specific meaningful source anchor from the construct or semantic fact that caused
them. Compiler-generated operations without a direct source occurrence use explicit generated provenance associated with the
owning unit or product.

MIR must not retain bound node IDs as deferred execution policy. A bound identity may appear only when it is itself the canonical
semantic origin required for correlation, not because code generation still needs to interpret the bound node.

Source correlation must not affect MIR identity assignment, control-flow meaning, or deterministic output.

---

## Lazy Demand, Parallelism, And Caching

Compilation exposes a typed lowering result as a lazy fact keyed by the canonical concrete unit identity and every input whose
change can alter that result. Executable units produce validated MIR. Units whose meaning is consumed entirely before runtime
produce an explicit compile-time-only classification instead of an absent MIR value.

A request for one lowering result:

1. requests that unit's canonical bound HIR,
2. classifies compile-time-only units without requesting execution facts,
3. requests only the durable semantic facts named by `LoweringInput` for executable units,
4. validates the lowering input,
5. lowers and validates the MIR in task-local state,
6. publishes the complete immutable result once.

It does not force unrelated units or unrelated semantic facts.

Independent MIR units may lower in parallel when their exact dependencies are available. Construction order and worker scheduling
must not affect identities, diagnostics, references, or serialized output.

Concurrent requests for the same fact should share one completed immutable result through the compilation fact machinery. They
must not share a mutable MIR builder.

Cancellation discards task-local construction. It does not publish a partial unit or poison a future request.

The `check` command requests semantic diagnostics and does not lower merely to prove semantic readiness. Product construction,
code generation, emission, or an explicit MIR inspection request demands MIR for the concrete reachable units it needs.

---

## Validation And Failures

`bray-ir` validates representation invariants independently of a backend.

Validation should cover at least:

- exact unit ownership of all MIR identities,
- one valid entry block,
- exactly one terminator per committed block,
- valid successor blocks and block argument arity,
- valid operation operands and result definitions,
- type consistency of values, storage, operations, and terminators,
- dominance or definition-before-use requirements selected by the MIR value model,
- valid storage lifetime and cleanup references,
- valid call, ABI, frame, and target contracts,
- complete control-flow references,
- deterministic table and external-reference structure.

Ordinary source errors belong to binding or checking and should prevent a lowering request from acquiring complete input.

An invalid `LoweringInput`, impossible checked-fact combination, or invalid MIR produced from validated input indicates a compiler
contract failure. Target or backend availability failures that can arise from a valid user request use structured diagnostics at
their owning boundary.

Compiler logic must not build user-facing English. Any lowering diagnostic uses `bray-diagnostics` with typed arguments and is
rendered through `bray-messages`.

---

## Code Generation Boundary

Code generation receives validated MIR and must not depend on bound HIR, syntax, checker stores, or task-local lowering state.

MIR must provide code generation with:

- explicit control and evaluation order,
- typed operations, values, and storage,
- concrete callable and external references,
- explicit cleanup and exceptional behavior,
- async frame and task operations,
- selected target, layout, ABI, and runtime requirements,
- sufficient source provenance for backend diagnostics.

If code generation needs to resolve a source name, inspect a bound expression, infer a semantic type, choose cleanup behavior, or
reconstruct source control flow, the lowering contract is incomplete.

Backend legalization may translate valid MIR into target-specific instruction sequences. It must not change language-semantic
decisions encoded by MIR.

---

## Testing

Lowering tests should cover:

- exact lowering of representative source-backed bound units,
- source constructs becoming execution-shaped blocks and operations rather than mirrored MIR nodes,
- preservation of evaluation order,
- block merges and diverging control,
- calls, argument mapping, conversions, and concrete dispatch,
- storage access, moves, borrows, initialization, drops, finalization, and destruction,
- normal, early, panic, propagation, and cancellation cleanup paths,
- patterns, match coverage decisions, loops, and short-circuit control,
- async frames, suspension, direct await, tasks, cancellation, and completion,
- compiler-known operations and unavailable target behavior,
- generated executable hosts,
- MIR validation failures for each representation invariant,
- deterministic output across demand order and worker counts,
- cancellation without partial publication,
- concurrent requests sharing one immutable result,
- narrow demand that does not lower unrelated units or request unrelated semantic facts.

Tests should assert complete MIR structures where practical. A coverage fixture should map every lowerable bound construct and
required semantic fact to a production lowering entry point and executable test.

---

## Implementation Direction

Lowering should be implemented as vertical execution-shaped slices:

1. establish the lazy per-unit compilation fact and production `lower_unit` entry point,
2. lower simple synchronous blocks, values, storage, calls, branches, and returns,
3. lower structured control, patterns, propagation, construction, and conversions,
4. lower checked ownership operations and complete cleanup paths,
5. lower lifecycle and interprocedural behavior,
6. lower async frames, suspension, tasks, cancellation, and generated hosts,
7. complete MIR validation and mechanical language-to-MIR coverage.

Each slice should extend MIR only with concepts needed by a current lowering contract. It should reuse existing semantic primitives
and delete redundant MIR wrappers discovered while implementing that slice.
