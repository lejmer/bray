# Compiler architecture

This document defines the goal-state architecture for the Bray compiler implementation.

The language design documents define Bray semantics.

The compiler architecture defines how the implementation is organized so those semantics remain maintainable, testable, and
extendable.

Implementation coding rules live in `docs/contributing/coding-conventions.md`.

Crate ownership rules live in `docs/contributing/crates.md`.

Parser implementation rules live in `docs/design/parser.md`.

Declaration discovery implementation rules live in `docs/design/declaration-discovery.md`.

Compiler-known catalog implementation rules live in `docs/design/compiler-known-catalog.md`.

Compiled package interface implementation rules live in `docs/design/compiled-package-interfaces.md`.

Project manifest and deterministic package-graph rules live in `docs/design/project-manifests.md`.

Symbol and symbol-construction implementation rules live in `docs/design/symbols.md`.

Binder and bound-tree implementation rules live in `docs/design/binder.md`.

Semantic checker domain and analysis rules live in `docs/design/checker.md`.

Lowering and MIR implementation rules live in `docs/design/lowering.md`.

Code generation and backend implementation rules live in `docs/design/codegen.md`.

Artifact planning, serialization, and publication rules live in `docs/design/emitter.md`.

Native linker integration rules live in `docs/design/linker.md`.

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

Project-oriented tools first load explicit workspace and package manifests through `bray-project`. That boundary validates
project-owned paths, enumerates declared source roots, resolves exact package-product dependency edges against the workspace
inventory, and publishes one immutable dependency-first graph. Compilation, inspection, and language tooling consume that graph;
they do not search for packages or consult ambient dependency state.

The compiler pipeline is a logical dependency order:

```text
source text
-> lexing
-> parsing
-> declaration discovery
-> symbol construction
-> binding and semantic analysis
-> lowering to backend-independent MIR
-> MIR validation
-> backend IR generation
-> artifact emission
-> linking when required
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
- a canonical source-shaped bound high-level IR with independently published semantic facts,
- a backend-independent mid-level IR.

No universal checked-program object records which analyses have run. Each consumer observes the source-shaped bound HIR through a
validated view containing only the immutable semantic facts required by that consumer.

Lowering receives one such view over a canonical bound unit and its required typed side facts.
It publishes one execution-shaped MIR owned by `bray-ir`, not another durable family of lowered bound nodes.

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

`Compilation` also exposes package semantic diagnostics as a cached fact. That fact discovers the source package's declared
semantic units, requests their bound-unit and required checker facts, follows published nested-unit keys, and merges the diagnostics
owned by those facts deterministically. A check-diagnostics query requests this package fact beside source, syntax, and declaration
diagnostics. Neither the command driver nor the check-diagnostics query enumerates semantic units or invokes binder entry points
directly.

One compilation request carries the source package identity as an explicit semantic input. `Compilation` owns that identity and
lazily derives the matching symbol graph and canonical semantic value store. Binder-facing query APIs construct their read-only fact
context internally from compilation-owned inputs. They must not accept arbitrary caller contexts that could populate one cache from
another syntax, symbol, target, or semantic-value universe.

Dependency interfaces and the current library product's encoded package interface are also lazy facts. An imported identity-skeleton
query requests structural interface validation and deterministic external-key mapping. An imported symbol fact requests only its
length-delimited semantic payload. An interface-artifact query requests the reachable completed public surface and deterministic
encoding without requiring callers to sequence those phases manually.

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
- bound units and typed semantic facts,
- MIR units,
- backend codegen units.

A lazy fact must be complete within the boundary promised by its API. An expression-type accessor returns the complete immutable
expression-type fact for its key. A selected-call accessor returns the complete immutable call-selection fact for its key. Neither
accessor implies that unrelated storage, effect, contract, or lowering facts were evaluated.

A declaration-surface type accessor can promise a complete source type-expression template rather than a checked `TypeId`. Such a
template preserves embedded constant-expression occurrences and their expected-type sources without demanding expression typing,
selection, target validation, or constant evaluation. A separate checked type or signature accessor resolves only the occurrences
required by that request through the cooperating semantic fixed point.

Binding publishes one canonical immutable `BoundUnit`. Each semantic analysis publishes only its typed side facts keyed to that
unit. The compiler must not copy the bound tree into stage-specific wrapper families as analyses complete. A consumer that needs a
set of facts requests that exact set through typed accessors. There is no universal whole-unit completion query.

Typed fact keys and accessors are the public compiler model. The same keys identify cached evaluation internally. Dependency
recording, single-flight evaluation, scheduling, waiting, cancellation, and invalidation are private mechanics behind those
accessors. The compiler must not mirror facts with public query objects, generic fact wrappers, dynamic registries, duplicate query
identities, or progress-wrapper representation families.

Smaller operations should use smaller APIs with smaller contracts. For example, a language-server hover implementation can ask for
a declaration surface or a type signature. A completion implementation can ask for the local facts needed at a source position.
These are separate contracts, not partial executions of a broader diagnostic or lowering request.

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

## Selected Target Facts

One product compilation selects one immutable target context before requesting target-dependent semantic facts. The context owns
the canonical `bray_target::TargetProfile`, the applicable runtime compatibility facts, and the capabilities used to derive the
target-available compiler-known declaration view. Product formation selects a runtime only after reachable requirements are known.
None of these values may be inferred from the compiler host.

The target profile contains the complete typed language-defined fact surface. Pointer, endian, architecture, and target-name facts
are derived from the profile's identity and machine properties so independently supplied values cannot contradict them. Profile
construction rejects incomplete fact groups, values outside the target `usize` range, scalar dependencies that contradict each
other, ABI contracts that accept unavailable representations, and cross-group alignment contradictions before source checking
begins. Compiler-known declaration availability is derived from this validated profile rather than supplied as a second capability
model.

The selected target is a typed compilation fact. Binder and checker requests borrow the same target profile instead of copying its
identity, machine properties, or widths into phase-specific models. Target-sized literals and constants always use the profile's
pointer width. Post-selection layout and ABI checks consume the same context after type and operation selection. Lowering and
output contracts derive their target-facing inputs from those selected facts rather than rediscovering target properties.

Post-selection target validity is keyed by the complete source-correlated representation, callable ABI, or layout requirement.
Foreign ABI requirements include every by-value parameter and result representation plus the selected aggregate layout contract.
The semantic fact that establishes such a requirement requests its validity and retains its diagnostics, preserving ordinary
diagnostic projection without introducing a mutable global registry of previously requested checks.

Target-independent facts do not depend on the selected-target fact. A target change invalidates declaration availability and the
semantic, lowering, and output facts that requested target data while allowing source, syntax, declaration discovery, and other
target-independent facts to remain reusable.

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
- bound units and independently keyed semantic facts,
- MIR units,
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

MIR construction can run independently for bound units whose exact lowering facts are available. Code generation can run
independently for validated `bray-ir` units once their immutable emission-plan artifact requests are available.

Parallel execution must be deterministic:

- diagnostics are ordered by package, source, declaration, and span order, not worker completion order,
- emitted artifacts use stable names and stable ordering,
- caches and interning tables expose deterministic IDs or deterministic remapping at phase boundaries,
- compiler-owned published state is immutable.

Scheduler queues and internal caches can use synchronization, but they must not affect observable compiler behavior.

Hidden mutable global state is not allowed in compiler logic.

Request priority is scheduling policy, not semantic identity. Interactive tooling requests may receive bounded preference over
ordinary and background work, but ordinary work must continue to make progress under sustained interactive demand. Priority,
waiter count, worker assignment, and completion order must not enter fact keys or change published results.

Cancellation belongs to an interested request. Cancelling a waiter stops that waiter without cancelling a shared computation still
needed by another request. Work with no remaining interested request may be abandoned, but abandoned work publishes no partial fact
or partial diagnostic bag.

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

Compiler-known and compiler-provided declaration surfaces come from the immutable catalog defined in
`docs/design/compiler-known-catalog.md`. Checked-in `.braydef` sources are parsed and validated by development tooling, which emits
deterministic checked-in Rust tables. Production compiler processes consume those static tables without parsing catalog sources or
embedded Bray fragments. The catalog supplies stable language identities, prevalidated declaration surfaces, typed representation
roles, compiler-provided implementation hooks, and target-availability rules. It does not construct symbols itself.

Imported declaration surfaces come from immutable compiled package interfaces defined in
`docs/design/compiled-package-interfaces.md`. Imported symbols use the same kind-specific symbol records as source symbols. The
interface codec remains outside `bray-symbols`, and compilation maps stable external keys to deterministic compilation-local symbol
IDs before lazy imported facts are requested.

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

The immutable symbol graph is a forest rooted in package symbols and one dedicated compiler-known environment symbol. There is no
compilation-root symbol. A closed root-ID family supports traversal and completion while package and compiler-known roots retain
kind-specific APIs. Ambient compiler-known visibility is a lookup relationship and does not reparent source modules away from their
packages.

Local symbols use region-scoped typed IDs and immutable local symbol snapshots rather than consuming compilation-wide declaration
symbol IDs. A body or declaration-owned expression publishes its bound representation, local snapshot, and binding diagnostics as
one immutable bound-unit fact. Checker services publish separate immutable typed facts. This keeps lazy and parallel semantic work
from mutating the global symbol graph.

Force completion requests all declaration-surface facts for a symbol and its semantically contained children in deterministic order.
It does not bind or check executable bodies, which remain separate lazy bound-body facts.

Declaration-owned expressions such as runtime defaults, constant definition templates, predicate definitions, generic constraints,
and contract clauses are declaration-surface facts. Their bound units and semantic side facts are requested independently and
summarized through typed symbol APIs. Runtime-default providers are synthesized semantic symbols and are lowered only when
reachable. The exact fact contracts and provider APIs are defined in `docs/design/symbols.md`.

### Binding

Binding resolves names, paths, member references, local bindings, declarations, and reference targets.

Binding consumes syntax plus symbol tables and produces the canonical bound HIR while using focused checker services for decisions
required during binding.

The bound tree is the compiler's source-shaped high-level intermediate representation.

The binder owns bound-tree construction. Compilation-owned semantic queries call focused checker services after the canonical bound
unit and their other declared inputs are available.

The binder can use mutable builders internally, but the published bound representation is immutable. The compiler should not
recreate equivalent bound nodes only to add semantic information later.

The binder can construct a mutable lexical scope graph and local symbol tables while binding one semantic region. The published
scope graph and local symbols are immutable, region-owned data attached to that bound unit. Lexical scopes are not symbols, and
semantic symbol containment does not imply lexical lookup ancestry.

Bound nodes preserve source correlation and carry decisions established during binding. Later semantic analyses publish typed side
facts keyed to the same bound unit rather than recreating its nodes.

Binding can report unresolved names, ambiguous names, invalid lexical scopes, invalid shadowing, and reference-form errors.

Compilation owns semantic-analysis orchestration. Binding does not define every semantic rule itself.

Type checking, ownership checking, borrowing, aliasing, effect checking, contract solving, and target-availability checking live
in focused semantic checker services.

Those services return diagnostics and typed semantic facts for compilation to publish beside the canonical bound unit.

Some semantic facts require data-flow over an already published bound unit. Their queries depend on that unit and publish only their
own durable results.

### Semantic Checker Services

Semantic checker services determine whether bound declarations, bodies, and semantic facts are valid Bray.

The focused domain taxonomy, dependency graph, convergence contracts, recovery policy, and durable checker outputs are defined in
`docs/design/checker.md`.

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
conversion choices, contract facts, and capability facts belong to typed side facts associated with the bound HIR.

The bound HIR uses Bray's storage terminology rather than a separate compiler-theory "place" model. Unit-local storage identities
represent exact or symbolic storage origins. Storage-access identities represent evaluated access-path occurrences and retain ordered
projections. They are distinct because ID equality between access occurrences cannot establish storage equality or disjointness.

Portable dependency-contract templates belong to `bray-symbols` and use formal receiver, parameter, result, capability, and witness
subjects. The binder instantiates them into unit-local bound contracts that can reference exact storage, access, borrow-capability,
and obligation identities. Compiled package interfaces encode template structure rather than compilation-local IDs.

Initialization, movement, active borrows, alias relationships, and other facts that vary by program point remain checker-local
analysis state. The checker publishes the immutable storage, access, borrow, dependency-contract, and operation facts promised by
its typed query contract, not its complete transfer state or work lists.

Each semantic unit that requires unit-scoped flow analysis has one immutable checker-internal control-flow graph constructed from
its committed read-only bound unit view. Reachability, storage flow, ownership, borrowing, lifecycle, refinement, liveness, and
dependency-contract propagation share that graph while retaining focused typed analysis states. Mutually dependent storage,
ownership, movement, borrowing, mutation-authority, and lifecycle facts use one composite storage-flow domain rather than circular
independent passes.

The control-flow graph is task-local checker infrastructure. It is neither canonical bound HIR nor published Bray MIR, and
its block, edge, operation, and program-point IDs do not enter symbols, package interfaces, or published checked nodes. A separately
requested tooling view can later project source-correlated control flow without exposing checker-private identity.

Independent semantic units can build and analyze their control-flow graphs in parallel. Independent domains over one graph can run
in parallel when their explicit input facts are available and doing so is profitable. Deterministic fixed points, diagnostics, and
published facts must not depend on worker scheduling.

There is no single checked-program or checked-unit representation. A diagnostic, tooling, lowering, or emission request observes
the canonical bound HIR together with the exact immutable semantic facts required by that consumer.

Checker services should publish facts precise enough that lowering can consume its declared inputs without re-checking source
semantics. Lowering does not use the existence of unrelated cached facts as evidence that its requirements are satisfied.

Checker services should not lower control flow merely to make checking convenient unless that lowered form is an explicit
checker-local representation.

### Lowering

Lowering converts the completed source-shaped bound HIR view into backend-independent mid-level IR owned by `bray-ir`.

The complete lowering and MIR representation contract is defined in `docs/design/lowering.md`.

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
- async frame creation, direct await, task boundaries, and checked cleanup plans,
- trait dispatch selection,
- selected overload arms.

Lowering should not make new semantic decisions.

If lowering discovers that it needs a semantic fact absent from `LoweringInput`, the lowering-input contract is incomplete.

Async lowering consumes hidden frame identities, suspension liveness, invocation and deferred execution contracts, state-indexed
affinity facts, postcondition templates, and two-phase scope cleanup plans with distinct descriptor visitors. It emits typed MIR
operations rather than calls selected by source-level runtime or standard-library names. `docs/design/async-runtime.md` defines the
phase ownership and runtime boundary.

Task-local lowering builders may use private intermediate forms while constructing MIR. Those forms are not separately published,
cached, or exposed as another durable compiler representation.

### IR

`bray-ir` is the compiler's backend-independent mid-level IR, abbreviated MIR.

It represents explicit control flow, storage, operations, calls, cleanup behavior, concrete semantic instances, and the typed target
facts required by code generation.

It should not contain parser-only syntax details.

`bray-ir` owns generic MIR builders and validates representation invariants as construction commits nodes and control-flow blocks.
Lowering may retain additional private task-local state while driving those builders.

MIR validation checks compiler invariants after lowering.

MIR validation failures indicate compiler bugs.

### Code Generation

Code generation converts validated Bray MIR into semantically complete backend-specific low-level IR through a coarse typed backend
contract.

LLVM is Bray's first production backend. LLVM bindings, types, modules, target machines, optimization pipelines, and diagnostics are
isolated in `bray-codegen-llvm`. They must not appear in `bray-ir`, backend-neutral codegen contracts, compilation facts, or
emission APIs.

`bray-codegen` owns backend selection, codegen-unit partitioning, reachable concrete monomorphized-instance collection, backend
identity, capabilities, requests, and outcomes. It packages canonical layout, ABI, symbol, target, runtime, and linkage facts that
earlier phases already resolved. It does not reinterpret source directives or rediscover language semantics.

The compiler composition root supplies the selected backend through that backend-neutral contract. `bray-compilation` coordinates
its lazy facts without depending on `bray-codegen-llvm` or inspecting backend-private state.

The first product model uses native ahead-of-time generation. `bray-codegen-llvm` constructs LLVM IR in task-local modules. An
emitter-owned artifact request determines whether the backend serializes LLVM IR, bitcode, assembly, relocatable objects, or debug
artifacts from those modules.

Code generation does not own language semantics.

Code generation does not own CLI policy, package policy, source discovery, artifact layout, or linking policy.

Target, ABI, layout, symbol, runtime, reachability, and generic-instantiation decisions must be explicit before code generation.
Backend-specific legalization preserves those decisions rather than replacing them.

Stable target identities, machine-model contracts, and target-output naming facts belong to `bray-target`. Code generation,
emission, and linking consume that shared lower boundary without depending on one another for target vocabulary.

Codegen units are lazy compilation facts with stable structural keys. Independent units can be generated in parallel, while mutable
backend module construction remains task-local. The immutable emission plan and per-unit artifact request participate in the exact
fact key beside backend identity, target configuration, options, and backend-library revision.

The complete backend contract is defined in `docs/design/codegen.md`.

### Emission

Emission is the lifecycle that turns completed backend artifact contributions and independently constructed package-interface
artifacts into named external compilation artifacts.

`bray-emitter` owns the emission request, immutable emission plan, requested artifact kinds, output names and sinks, deterministic
serialization coordination, staging, atomic publication, artifact bookkeeping, emission diagnostics, and construction of the
typed link plan.

The emission plan can logically precede code generation. Each codegen task receives an immutable derived artifact request. Its
backend builds, finalizes, and serializes the task-local module, then returns immutable contributions. The emitter merges and
publishes those contributions in deterministic plan order.

Package-interface bytes and hashes are produced by `bray-package-interface`, not a codegen backend. The emitter includes a completed
interface artifact when the product requests `.brayi`, assigns its output, and publishes it through the same deterministic atomic
artifact policy.

Emission does not inspect syntax trees, bound trees, or MIR to decide language behavior. It does not implement backend-specific
serialization or perform the final native link.

### Linking

`bray-linker` consumes emitted objects or bitcode together with an immutable typed link plan. It owns linker selection, embedded or
system linker adapters, process invocation, argument construction, linker diagnostics, and production of the final linked artifact.

The link plan contains already selected entry-point, startup, runtime, distinguished main-thread-lane, structured-shutdown,
task/thread/process hard-limit, native-library, export, search-path, and platform-option requirements. The linker does not discover
semantic dependencies, inspect MIR, or choose product policy.

The linker writes to an emitter-owned staging destination. A successful linked artifact is atomically published through the
emitter's artifact policy.

---

## Data Ownership

Every durable compiler concept should have one owning crate or module.

Shared data should be shared through typed IDs, typed references, immutable tables, or explicit query handles.

Durable compiler representations are immutable after publication.

This includes source inputs, syntax trees, syntax nodes, syntax tokens, symbol tables, source-shaped bound nodes, checked semantic
facts, MIR nodes, emission plans, emitted artifact descriptors, link plans, and diagnostic records.

Mutable construction belongs inside local builders, task-local work state, or explicitly internal caches. Mutable construction
state must not be exposed as shared compiler data.

Do not pass loosely typed strings, raw indexes, or ad hoc maps across phase boundaries.

Source-correlated nodes should carry spans or source references until diagnostics no longer need them.

Syntax nodes belong to syntax and parser layers.

Typed syntax nodes are structured records of named token and child components, not untyped bags of children.

Syntax tokens retain trivia as syntax-owned data. Later phases can refer to syntax spans, nodes, and tokens, but semantic facts
should not duplicate trivia.

Compiler-known declaration descriptors and typed compiler-known behavior roles belong to `bray-compiler-known`. The generated
catalog is an immutable language-definition input to symbol construction and later semantic facts, not source syntax or a source
package. Runtime compiler work does not parse or structurally validate the checked-in catalog definitions.

Compiled package interface bytes, validated section directories, artifact hashes, and lazy wire decoders belong to
`bray-package-interface`. Imported semantic identities and normalized symbol-facing facts still belong to `bray-symbols`, while
serializable checked-template value contracts belong to their bound-representation owner.

Symbols belong to symbol construction and semantic reference layers.

Local symbol snapshots belong to their checked semantic regions and are published with the corresponding bound representation.

Source-shaped bound nodes must belong to `bray-bound-tree`.

Resolved references on bound nodes belong to binding.

Semantic facts associated with source-shaped bound nodes belong to semantic checker services.

Backend-independent MIR nodes belong to `bray-ir`. Lowering produces them and code generation consumes them.

Private runtime ABI contracts belong to `bray-runtime-interface`. Libraries publish requirements without selecting a runtime.
Executable and test products select one target-specific runtime before code generation and linking.

Stable target identities, architectures, object formats, byte order, relocation models, code models, and validated machine
properties belong to `bray-target`.

Backend-neutral codegen-unit, backend identity, capability, outcome, and artifact contracts belong to `bray-codegen`.

LLVM contexts, modules, builders, target machines, verification, optimization, and artifact construction belong exclusively to
`bray-codegen-llvm`.

Emission plans, output policies, and emitted artifact records belong to `bray-emitter`.

Link plans, linker drivers, and linked artifact records belong to `bray-linker`.

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

Walkers must belong with the representation they walk. Syntax walkers belong in the syntax layer, source-shaped bound walkers belong
in `bray-bound-tree`, and MIR walkers belong in `bray-ir`.

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

"Query" names this private evaluation behavior. It does not require a public query object, a `Query<T>` wrapper, a second identity
beside the fact key, or a dynamic registry. Public APIs expose typed facts directly.

A task is schedulable compiler work with explicit dependencies.

Queries and tasks should use immutable inputs and publish immutable outputs.

Fact contracts must be complete within their promised boundary. Do not model one fact as evidence that unrelated semantic facts
were evaluated.

Do not turn ordinary helper functions into queries merely because they are reusable.

### Context Handles

A context handle provides read-only access to shared compiler state needed by a task or query.

Context handles should expose typed APIs, not raw maps or global mutable state.

Phase-specific context handles should stay narrow. A parser context should not expose semantic checking APIs, and a checker service
context should not expose emission policy.

### Interners And Canonical Tables

Interners and canonical tables deduplicate stable compiler values.

They should expose typed handles and deterministic behavior.

Canonical semantic types, closed constant values, open constant terms, generic substitutions, trait applications, and callable or
implementation instances use a semantic value store whose value types and APIs are owned by `bray-symbols`. The compilation or
immutable symbol snapshot owns the store instance because its entries reference compilation-local symbol IDs.

Inference variables, unification state, evaluation stacks, and solver traces are not interned semantic values. They remain local to
the checker operation that owns them.

Semantic value IDs are opaque store-local handles. Numeric assignment can vary with lazy demand without affecting semantics because
serialization, diagnostics, sorting, and incremental reuse use canonical structural keys rather than numeric ID order.

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

Language-tooling accessors should accept a source position or source-versioned syntax identity and return explicit available,
recovered, or unavailable states. Position-based accessors should resolve a syntax identity and delegate to the same semantic path.
Diagnostics should be requestable at source, semantic-unit, and package scope without introducing protocol or rendering concerns.

Query inputs and outputs should be suitable for parallel scheduling.

A query should not rely on worker-local mutable state unless that state is an implementation cache that cannot affect observable
compiler behavior.

Variable-size fact caches must have an explicit finite retention policy. Retention order and eviction are performance choices, not
semantic state. Completed immutable facts may be reclaimed and recomputed, while in-flight single-flight entries must remain
coordinated until they publish or are abandoned. Snapshot invalidation removes obsolete entries through the ordinary dependency
graph.

Resource limits that can change a semantic answer or diagnostic are part of the corresponding request and fact identity.
Cancellation, priority, worker count, and cache retention are not result-affecting resource limits and must not enter that
identity.

Semantic-analysis requests carry deterministic limits for active recursive analysis and package-level pair comparisons. Facts
whose answers depend on those limits must be invalidated when a revised snapshot changes them. Unrelated syntax and semantic facts
remain reusable through their ordinary dependency identities.

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

Backend support should be selected after the feature is represented in the bound HIR, its required semantic facts, and the
backend-independent MIR owned by `bray-ir`.

A new backend implements the coarse codegen-unit contract. It must not require LLVM types in backend-neutral crates, reinterpret
source semantics, or extend Bray MIR with backend-owned values.

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

Do not place emission policy, output layout, publication, or linking behavior in codegen interfaces. Backend-specific serialization
remains a codegen capability selected through an emitter-owned artifact request.

---

## Testing Strategy

Each phase should have focused tests for its own contract.

Lexer tests should validate token streams, trivia, invalid tokens, and source ranges.

Parser tests should validate syntax trees, named token slots, named child slots, lossless source reconstruction, and syntax
diagnostics.

Declaration tests should validate discovered declaration surfaces.

Binding tests should validate symbol resolution and scope behavior.

Checker tests should validate language semantics, diagnostics, and recovery behavior.

Lowering and MIR tests should validate checked-HIR-to-`bray-ir` translation, explicit control flow and cleanup behavior, and MIR
invariants.

Codegen tests should validate MIR-to-backend-IR translation and backend conformance independently of artifact naming and sinks.

Emitter tests should validate artifact requests, deterministic naming, serialization coordination, atomic publication, package
interface inclusion, and link-plan construction.

Linker tests should validate typed-plan translation, target driver selection, invocation diagnostics, and staged linked outputs.

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
