# Compiler architecture

This document defines the goal-state architecture for the Bray compiler implementation.

The language design documents define Bray semantics.

The compiler architecture defines how the implementation is organized so those semantics remain maintainable, testable, and
extendable.

Implementation coding rules live in `docs/contributing/coding-conventions.md`.

Crate ownership rules live in `docs/contributing/crates.md`.

Parser implementation rules live in `docs/design/parser.md`.

Declaration discovery implementation rules live in `docs/design/declaration-discovery.md`.

Symbol and symbol-construction implementation rules live in `docs/design/symbols.md`.

This document is the design-level contract those implementation documents should follow.

---

## Goals

The compiler should be easy to extend without weakening phase boundaries.

Each compiler phase should own one kind of information and produce one clear output contract.

Language features should be added by extending the relevant phase models, not by adding cross-phase shortcuts.

Diagnostics should stay source-correlated and structured until the boundary where user-facing messages are rendered.

Syntax representation should be lossless. Whitespace and comments are preserved as token trivia so compiler tools can recreate
source text from syntax trees.

Published compiler representations should be immutable. Mutation is allowed inside local builders while constructing a value, but
the value becomes immutable before it is shared through the compiler graph.

Compiler facts should be evaluated on demand through explicit queries. Laziness applies to when a compiler fact is requested, not
to whether a requested fact is allowed to be partially completed.

Compiler behavior should be deterministic.

The compiler should be designed for parallel execution from the start.

Serial execution should be supported and should use the same phase contracts, dependency graph, and diagnostics behavior as
parallel execution.

Compilation speed is a product requirement. Quick single-threaded shortcuts should not become the default architecture when they
would make later parallelization invasive.

Malformed user input should produce diagnostics, not compiler panics.

Compiler panics are for violated compiler invariants.

---

## Pipeline

The compiler pipeline is a logical dependency order:

```text
source text
-> lexing
-> parsing
-> declaration discovery
-> symbol construction
-> binding and semantic analysis
-> lowering
-> IR validation
-> code generation
-> emission
```

The arrows show fact dependencies, not mandatory whole-program scheduling barriers.

A source unit, module, declaration, function body, type body, predicate body, implementation body, or backend unit can move to the
next relevant phase when its required inputs are available.

The compiler should represent work as dependency-tracked tasks or queries. A task can run when its explicit inputs are available.
The scheduler can run independent tasks concurrently.

Each phase consumes earlier phases through public output contracts.

A phase can keep internal helper structures, but downstream phases must not depend on those internals.

Each phase can emit diagnostics for violations it owns.

No phase should rely on a later phase to repair invalid data.

The main durable representations are:

- a lossless syntax tree,
- a bound representation,
- IR.

The checked program state is the bound representation after the binder has completed semantic analysis and all required semantic
facts have been populated.

---

## Demand-Driven Evaluation

Compiler work should be lazy by default. Code should expose typed APIs for compiler facts and compute those facts when they are
requested. A caller should ask for the fact it needs, such as a syntax tree, a declaration surface, an expression type, body
diagnostics, or an emitted artifact. It should not have to request intermediate phase work unless it needs that intermediate fact
directly.

Demand-driven evaluation must not change which diagnostics or semantic facts a fully checked program produces.

Lazy evaluation should use ordinary compiler APIs. If computing the type of an expression needs parsed syntax, declaration
surfaces, symbols, binding, and constraint solving, the type API obtains those dependencies internally through the owning phase
APIs.

`Compilation` exposes declaration chunks and the merged declaration table as cached facts. A source-unit chunk query requests that
source unit's syntax. A declaration-table query requests all source-unit chunks and performs one deterministic merge. Declaration
diagnostics are a projection of the merged result, so a check-diagnostics query materializes declaration discovery through that
fact dependency rather than through a phase-execution command.

Compiler facts should generally be lazy across stable compiler boundaries:

- source units,
- modules,
- declaration surfaces,
- type bodies,
- callable bodies,
- predicate bodies,
- implementation bodies,
- overload families,
- trait applications,
- generic instantiations,
- checked bound units,
- lowered IR units,
- backend codegen units.

A lazy fact must be complete within the boundary promised by its API. If an API returns a checked callable body, the whole callable
body is checked and the published result contains the required expression types, selected overloads, selected trait
implementations, move states, borrow states, contract facts, capability facts, and diagnostics for that body.

Smaller operations should use smaller APIs with smaller contracts. For example, a language-server hover implementation can ask for
a declaration surface or a type signature. A completion implementation can ask for the local facts needed at a source position.
These are separate contracts, not partial executions of a larger checked-body API.

Compiler commands and language-server entry points request the result they need:

- an outline request asks for source outlines,
- a go-to-definition request asks for the declaration target of a reference,
- a body diagnostic request asks for diagnostics for a body,
- a package check asks for package validation,
- emission asks for product artifacts.

The implementation computes whatever intermediate facts are needed to answer each request. Evaluation order, cache hits, worker
count, and language-server request order must not affect the semantic facts or diagnostics produced for the same requested result.
Diagnostics from lazily evaluated facts must be merged and ordered deterministically when a diagnostic result is materialized.

---

## Parallel Execution Model

Compiler work should be split at stable semantic boundaries:

- source units,
- modules,
- declarations,
- callable bodies,
- type bodies,
- predicate bodies,
- implementation bodies,
- generic instantiations,
- checked bound units,
- lowered IR units,
- backend codegen units.

The scheduler should run independent work in parallel whenever the dependency graph allows it.

Lexing and parsing should be demand-driven. A parser asks a token source for the next token, a lookahead token, or a recoverable
token window. The lexer produces tokens as needed and can cache produced tokens for repeated parser access, diagnostics, and
incremental reuse.

Whole-source-unit tokenization is allowed as an implementation strategy when it is beneficial, but it is not a semantic phase
boundary. Later compiler architecture must not require all source units to be fully lexed before any parser work can start.

Declaration discovery can run independently for syntax trees whose module context is known.

Binding and checking can run independently for declarations and bodies once their required symbols, imported surfaces, target
facts, and contract dependencies are available.

Lowering and code generation can run independently for checked bound units whose semantic facts are complete.

Parallel execution must be deterministic:

- diagnostics are ordered by package, source, declaration, and span order, not worker completion order,
- emitted artifacts use stable names and stable ordering,
- caches and interning tables expose deterministic IDs or deterministic remapping at phase boundaries,
- compiler-owned published state is immutable.

Scheduler queues and internal caches can use synchronization, but they must not affect observable compiler behavior.

Hidden mutable global state is not allowed in compiler logic.

---

## Worker Budget

The compiler invocation includes a CPU worker budget.

The CLI exposes this budget as `--cpu-count <N>`.

`N` must be a positive integer.

If `--cpu-count` is omitted, the compiler chooses the available host parallelism as the default worker budget, subject to host or
embedding constraints.

`--cpu-count 1` means serial execution. Serial execution uses the same task graph and query contracts as parallel execution, but
the scheduler runs at most one compiler-owned CPU task at a time.

The compiler must not intentionally run more compiler-owned CPU workers than the worker budget allows.

I/O waits, child tool execution, and linker execution can be represented separately, but any compiler-owned CPU-heavy work must
respect the worker budget.

---

## Phase Contracts

### Source

The source layer owns source inputs, source IDs, source text, source ranges, spans, line mapping, and source-map utilities.

The source layer does not know Bray syntax.

It only knows stable identity and location information for compiler inputs.

### Lexer

The lexer consumes source text through the source layer and produces tokens, trivia, and lexical diagnostics.

The lexer follows `lexical-grammar.md` and `lexical-grammar.ebnf`.

The lexer supports lazy token production for parser peek and consume operations.

The lexer can cache produced tokens, trivia, and lexical diagnostics for repeated access.

The lexer can also eagerly tokenize a source unit when the implementation chooses to, but eager tokenization is not required by
later phases.

The lexer does not perform parsing, name resolution, type checking, or semantic validation.

The lexer can classify keywords, identifiers, literals, comments, documentation comments, punctuation, and invalid tokens.

### Parser

The parser consumes a token source and produces syntax trees and syntax diagnostics.

The parser follows `syntax-grammar.md` and `syntax-grammar.ebnf`.

The parser owns token lookahead, token consumption, parser recovery, and syntax-tree construction.

The parser does not own the policy for scanning all source text up front.

Syntax trees are lossless. They preserve token width, token order, trivia width, trivia order, and recovered syntax markers needed
to reconstruct the original source text.

Syntax trees, syntax nodes, syntax tokens, and syntax trivia are immutable after construction.

The parser can use mutable builders internally, but the published syntax tree is immutable.

The canonical syntax-tree storage is green-style storage:

- green nodes are parentless immutable nodes with a `SyntaxKind`, full source width, and source-order child elements,
- green child elements are either green nodes or green tokens,
- green tokens and green trivia store kind plus width, not absolute source offsets,
- green subtrees can be shared across syntax trees and source snapshots when their text shape is identical.

Typed syntax nodes are red-style wrappers over green storage. A typed node provides source context, absolute ranges, parent/path
context where needed, and named component accessors. Typed nodes must not duplicate child storage that already exists in green
storage.

Trivia is attached to syntax tokens as ordered leading and trailing trivia. Trivia is not represented as ordinary syntax nodes.

Each trivia segment is attached exactly once.

Every syntax token in a source unit is reachable by walking the green tree in source order and synthesizing range-bearing syntax
tokens from the source-unit start offset. Source text is recreated by walking green tokens in source order and slicing the source
text by each token's synthesized leading trivia range, token range, and trailing trivia range.

The end-of-file token is part of the syntax token sequence and can carry final trivia when trivia appears after the last ordinary
token.

Typed syntax nodes expose named components.

A typed syntax node component can be:

- a named token slot,
- an optional named token slot,
- a named child-node slot,
- an optional named child-node slot,
- a named child-node list.

Keywords, punctuation, delimiters, and operators that belong to a grammar production should be exposed through named token slots
on the owning typed syntax node. They should not be hidden in anonymous side tables or represented only by source spans.

For example, an `if` expression node should expose token slots for its `if` keyword and any present `else` keyword, along with
named child slots for the condition and branch bodies.

The parser does not perform semantic validation.

Examples of checks the parser does not own:

- whether a declaration is visible,
- whether a type exists,
- whether a trait implementation is coherent,
- whether an expression has the expected type,
- whether a borrow is legal,
- whether a trusted obligation is discharged.

The parser should preserve enough syntax structure for later phases to produce precise diagnostics.

### Declaration Discovery

Declaration discovery walks syntax trees and records declared surfaces.

The implementation contract is defined in `docs/design/declaration-discovery.md`.

It owns the early catalog of modules, imports, exports, functions, constants, predicates, callable contracts, types, traits,
implementations, overloads, fields, variants, parameters, and member declarations.

Declaration discovery does not bind expression bodies.

Declaration discovery produces immutable declaration tables from immutable per-source-unit discovery chunks.
Source-unit discovery can run in parallel with task-local builders. The deterministic merge step borrows the chunks, assigns
declaration and container IDs, aggregates partial modules, and publishes the final immutable table.

Declarations are not symbols. `DeclarationId` identifies discovered syntax. Symbol construction later decides which declarations
create semantic symbols.

Declaration records carry stable syntax anchors and syntax-backed surface facts such as visibility, modifiers, directives,
constraints, and callable contract clauses. Those facts are still syntax-level data, not bound semantics.

Partial modules are represented as logical module containers with one or more source module parts.
Other declaration spaces, such as type, trait, and implementation bodies, are represented as containers before symbols exist.

Declaration discovery reports duplicate names within explicit declaration domains and conflicting visibility or trust state across
split module parts. Recovered declarations are excluded from these checks to avoid cascading diagnostics.

### Symbols

Symbol construction creates stable semantic identities for declarations.

The implementation contract is defined in `docs/design/symbols.md`.

A symbol answers "which declared thing is this?".

Symbols are not source strings.

Symbols should be typed IDs with explicit entity kinds.

Different concepts need different ID types.

For example, module symbols, type symbols, function symbols, trait symbols, implementation symbols, field symbols, local symbols,
and overload symbols should not be interchangeable raw integers.

Bray uses kind-specific symbol records and typed relationships rather than an inheritance hierarchy or one generic child-symbol
list. Modules, types, traits, implementations, callables, variants, overload families, and parameters expose the children and facts
meaningful to their exact semantic category.

A deterministic eager identity skeleton makes symbol IDs independent of lazy request order and worker scheduling. Expensive symbol
facts are evaluated on demand through compilation-owned queries and publish immutable values with fact-owned diagnostics.

Source, imported, compiler-known, compiler-provided, synthesized, and body-local symbols follow the same typed identity contracts.
Constructed types, trait applications, callable instances, and selected implementation witnesses use separate semantic identities
and do not pretend to be declaration symbols.

Force completion requests all declaration-surface facts for a symbol and its semantically contained children in deterministic order.
It does not bind or check executable bodies, which remain separate lazy bound-body facts.

Declaration-owned expressions such as runtime defaults, constant definition templates, predicate definitions, generic constraints,
and contract clauses are declaration-surface facts. Their full checked representations are binder-owned, requested through
compilation queries, and summarized through typed symbol APIs. Runtime-default providers are synthesized semantic symbols and are
lowered only when reachable. The exact fact contracts and provider APIs are defined in `docs/design/symbols.md`.

### Binding

Binding resolves names, paths, member references, local bindings, declarations, and reference targets.

Binding consumes syntax plus symbol tables and orchestrates semantic analysis to produce a bound representation.

The bound representation is the compiler's source-shaped semantic representation.

The binder owns bound-tree construction.

The binder calls semantic checker services during bound-tree construction whenever the relevant inputs are available.

The binder can use mutable builders internally, but the published bound representation is immutable. The compiler should not
recreate equivalent bound nodes only to add semantic information later.

Bound nodes preserve source correlation and carry resolved references plus completed semantic facts for their checked unit.

Binding can report unresolved names, ambiguous names, invalid lexical scopes, invalid shadowing, and reference-form errors.

Binding owns semantic-analysis orchestration. It does not define every semantic rule itself.

Type checking, ownership checking, borrowing, aliasing, effect checking, contract solving, and target-availability checking live
in focused semantic checker services.

Those services return diagnostics and semantic facts for the binder to place on the bound representation before the checked unit is
published.

Some semantic facts require data-flow over an already constructed portion of a body. The binder still orchestrates that analysis as
part of completing the bound unit before publication.

### Semantic Checker Services

Semantic checker services determine whether bound declarations, bodies, and semantic facts are valid Bray.

Semantic checker services are responsibility modules, not a second durable tree-producing phase.

Checker services own:

- type checking,
- trait satisfaction checking,
- overload resolution,
- conversion checking,
- ownership checking,
- borrowing and aliasing rules,
- mutation authority,
- initialization tracking,
- move and partial-move legality,
- lifecycle checking,
- effect and capability checking,
- trusted obligation checking,
- contract checking,
- const-evaluation validity,
- target-availability checking.

Semantic facts such as expression types, selected overloads, selected trait implementations, move states, borrow states,
conversion choices, contract facts, and capability facts belong to the bound representation.

The checked program state is the bound representation with all required semantic facts completed.

Checker services should make the bound representation complete enough that lowering can consume it without re-checking source
semantics.

Checker services should not lower control flow merely to make checking convenient unless that lowered form is an explicit
checker-local representation.

### Lowering

Lowering converts the checked bound representation into explicit compiler IR.

Lowering owns desugaring and normalization after semantic validity is established.

Lowering makes implicit behavior explicit:

- temporaries,
- moves,
- drops,
- finalization paths,
- panic paths,
- result propagation,
- nullable propagation,
- pattern matching decisions,
- loop control flow,
- short-circuit boolean flow,
- async task boundaries,
- trait dispatch selection,
- selected overload arms.

Lowering should not make new semantic decisions.

If lowering discovers that it needs a semantic fact that the checked bound representation did not provide, the checker service
contract is incomplete.

### IR

The IR is backend-independent.

IR should represent explicit control flow, explicit storage, explicit operations, explicit calls, and explicit cleanup behavior.

IR should not contain parser-only syntax details.

IR validation checks compiler invariants after lowering.

IR validation failures indicate compiler bugs.

### Code Generation

Code generation converts IR into backend-specific representations.

Code generation does not own language semantics.

Code generation does not own CLI policy, package policy, source discovery, artifact layout, or linking policy.

Backend-specific choices should be expressed behind typed backend contracts.

### Emission

Emission owns final artifacts, output paths, object files, libraries, executables, debug data, and linking handoff.

Emission should consume typed compilation outputs.

Emission should not inspect syntax trees or bound trees to decide language behavior.

---

## Data Ownership

Every durable compiler concept should have one owning crate or module.

Shared data should be shared through typed IDs, typed references, immutable tables, or explicit query handles.

Durable compiler representations are immutable after publication.

This includes source inputs, syntax trees, syntax nodes, syntax tokens, symbol tables, bound nodes, checked semantic facts, IR
nodes, emitted artifact descriptors, and diagnostic records.

Mutable construction belongs inside local builders, task-local work state, or explicitly internal caches. Mutable construction
state must not be exposed as shared compiler data.

Do not pass loosely typed strings, raw indexes, or ad hoc maps across phase boundaries.

Source-correlated nodes should carry spans or source references until diagnostics no longer need them.

Syntax nodes belong to syntax and parser layers.

Typed syntax nodes are structured records of named token and child components, not untyped bags of children.

Syntax tokens retain trivia as syntax-owned data. Later phases can refer to syntax spans, nodes, and tokens, but semantic facts
should not duplicate trivia.

Symbols belong to symbol construction and semantic reference layers.

Bound nodes belong to the bound representation layer.

Resolved references on bound nodes belong to binding.

Semantic facts on bound nodes belong to semantic checker services.

IR nodes belong to lowering and backend-independent codegen.

Emitted artifacts belong to emission.

---

## Reusable Compiler Primitives

Compiler primitives are implementation patterns and data types reused throughout the compiler. They are not Bray language
primitives.

Reusable compiler primitives should have one owning crate or module. New features should use these primitives instead of creating
local variants.

### Kinds

A kind is a classification enum.

Kinds answer "what category of thing is this?".

Examples include syntax kinds, symbol kinds, declaration kinds, bound kinds, location kinds, operator kinds, diagnostic kinds, and
IR operation kinds.

Kinds are useful for typed dispatch, pattern matching, diagnostics, debugging, snapshots, and exhaustive handling in visitors.

Kinds are not identities. Two different syntax nodes can have the same `SyntaxKind`; two different symbols can have the same
`SymbolKind`.

### Typed IDs

Cross-phase identity should use typed IDs.

An ID answers "which exact compiler object is this?".

Examples include source IDs, syntax node IDs, syntax token IDs, symbol IDs, declaration IDs, bound node IDs, IR IDs, and
diagnostic IDs.

Typed IDs should not be interchangeable raw integers.

Tables keyed by typed IDs should live in the crate that owns the identified concept.

Kinds and IDs should both be explicit in APIs when both are relevant. A `SyntaxNodeId` identifies a specific syntax node; a
`SyntaxKind` classifies that node.

### Spans And Ranges

Source ranges, spans, and source locations belong to the source layer.

A span identifies where compiler data came from. It is not a semantic identity.

A semantic identity should use a symbol, declaration ID, bound node ID, or another typed semantic ID.

### Walkers

A walker performs structured traversal over an immutable compiler representation.

Walkers should be used for mechanical traversal where most nodes follow the default traversal order.

A walker can maintain task-local state while walking, but it must not mutate the representation it walks.

Walker traversal order must be deterministic.

Walker APIs should make descent behavior explicit. A walker can visit all children by default, skip a subtree deliberately, or stop
early with an explicit result.

Walkers belong with the representation they walk. Syntax walkers belong in the syntax layer, bound walkers belong in the bound
representation layer, and IR walkers belong in the IR layer.

Whole-tree walkers can exist as serial convenience APIs. Parallel phases should schedule independent traversal roots, use the
representation-owned per-root walker inside each task, keep walker state task-local, and merge phase outputs through deterministic
sinks.

### Visitors

A visitor performs typed dispatch over nodes, tokens, or IR operations.

Visitors should be used when each node kind has meaning-specific handling or when the caller needs a typed result.

Visitors should not hide traversal policy. If a visitor descends into children, the API should make that behavior clear.

Visitors should not mutate immutable compiler representations.

Shared visitors belong with the representation they dispatch over. Feature-specific visitors can live in the owning feature module
when they are not broadly reusable.

### Builders

A builder is local mutable construction state for an immutable compiler representation.

Builders can allocate nodes, collect fields, attach diagnostics, attach facts, and validate construction invariants.

Builders must publish immutable output.

Published output should not expose builder internals or mutable collections.

### Cursors

A cursor is a lightweight position inside a token stream, node list, child list, or IR block.

Cursors should be used when traversal needs stable local movement, lookahead, or peeking without exposing mutable collections.

Parser token cursors and syntax cursors should preserve lossless token and trivia access.

### Sinks

A sink receives structured output during analysis.

Examples include diagnostic sinks, fact sinks, event sinks, and artifact sinks.

Sinks should accept typed data, not user-facing English strings.

Sinks used by parallel tasks must preserve deterministic final ordering.

### Queries And Tasks

A query computes a meaningful compiler fact with explicit inputs and a clear invalidation story.

A query can be evaluated lazily when its fact is requested.

Queries should feel like ordinary typed compiler APIs. A caller asks for the fact it needs and the query implementation obtains its
own dependencies internally.

A task is schedulable compiler work with explicit dependencies.

Queries and tasks should use immutable inputs and publish immutable outputs.

Query contracts must be complete within their promised boundary. Do not model a partially checked callable body, type body, or
implementation body as though it were a fully checked result.

Do not turn ordinary helper functions into queries merely because they are reusable.

### Context Handles

A context handle provides read-only access to shared compiler state needed by a task or query.

Context handles should expose typed APIs, not raw maps or global mutable state.

Phase-specific context handles should stay narrow. A parser context should not expose semantic checking APIs, and a checker service
context should not expose emission policy.

### Interners And Canonical Tables

Interners and canonical tables deduplicate stable compiler values.

They should expose typed handles and deterministic behavior.

Interning should not be used to hide ownership boundaries or to avoid defining a real semantic identity.

---

## Diagnostics

Compiler logic emits structured diagnostics.

Diagnostic records are immutable compiler data until they are rendered at the reporting boundary.

Each phase owns diagnostics for violations it has enough information to report accurately.

Diagnostics must remain deterministic under parallel execution.

Detailed diagnostic data, localization, suggestion, recovery, ordering, and testing rules are defined in
[Compiler diagnostics](compiler-diagnostics.md).

---

## Recovery

The compiler should recover from malformed user input when recovery improves diagnostics.

Recovery data must be explicit.

Do not represent recovered or erroneous state with ordinary valid nodes unless the node carries an explicit error marker.

Downstream phases should be able to distinguish:

- valid data,
- missing data caused by user errors,
- placeholder data introduced for recovery,
- compiler invariant violations.

User errors should not become compiler panics.

Compiler invariant violations should fail loudly during development and tests.

---

## Incrementality And Caching

Incremental compilation should be designed around stable identities and explicit dependencies.

Cache keys should be typed.

Cache entries should record the inputs and target profile facts they depend on.

The compiler should avoid hidden global mutable caches.

If a query-style system is used, query boundaries should align with phase ownership.

Do not make every helper a query.

A query should represent a meaningful compiler fact with a clear invalidation story.

Language-server entry points should request the narrow result they need. Intermediate compiler facts should be computed internally
by the lazy APIs that own those facts.

Query inputs and outputs should be suitable for parallel scheduling.

A query should not rely on worker-local mutable state unless that state is an implementation cache that cannot affect observable
compiler behavior.

---

## Extensibility Rules

A new language feature must identify:

- its syntax ownership,
- its declaration ownership,
- its binding rules,
- its type and contract rules,
- its ownership and borrowing rules,
- its lowering behavior,
- its diagnostic responsibilities,
- its dependency and invalidation shape,
- its parallel scheduling constraints,
- its test surface.

Adding a language feature should usually change several phase contracts deliberately.

If a feature can be added by changing only parser code and codegen, that is a warning sign.

Core language rules belong in the relevant model and checker contracts, not in backend-specific code.

Backend support should be selected after the feature is represented in the checked bound representation and IR.

---

## Module And Crate Rules

Crate boundaries should match durable compiler concepts.

Do not add a shared crate merely because two crates need one helper.

Put reusable behavior in the crate that owns the concept.

Keep `lib.rs` files thin.

Keep module roots thin when modules are split.

Prefer focused modules named after the local concept they implement.

Do not place parser behavior in syntax data structures.

Do not place checking behavior in bound-tree definitions.

Do not place lowering behavior in checker types.

Do not place emission behavior in codegen interfaces.

---

## Testing Strategy

Each phase should have focused tests for its own contract.

Lexer tests should validate token streams, trivia, invalid tokens, and source ranges.

Parser tests should validate syntax trees, named token slots, named child slots, lossless source reconstruction, and syntax
diagnostics.

Declaration tests should validate discovered declaration surfaces.

Binding tests should validate symbol resolution and scope behavior.

Checker tests should validate language semantics, diagnostics, and recovery behavior.

Lowering tests should validate explicit control flow and cleanup behavior.

IR tests should validate IR invariants.

End-to-end tests should validate compiler behavior across phases.

Diagnostic tests should check diagnostic identity, spans, labels, notes, suggestions, and rendered output where rendering matters.

Fuzz tests should be part of the regular compiler test strategy.

Fuzzing is a separate test surface from ordinary unit tests, integration tests, and fixtures.

Fuzz harnesses, corpora, minimized reproducers, and tool-specific fuzz configuration belong under the top-level `fuzz/`
directory, not inside ordinary crate test modules.

Fuzzing should cover lexer, parser, syntax recovery, lossless source reconstruction, diagnostic production, and binder entry
points.

Fuzz-generated input can be valid or invalid. Invalid input should produce diagnostics or recovery data, not compiler panics.

Fuzz tests should include deterministic replay artifacts for discovered failures.

When a fuzz failure exposes a stable language or compiler behavior, a small focused regression test should be added to the
ordinary test suite as well.

Fixtures should be small and focused.
