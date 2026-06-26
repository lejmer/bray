# Core language primitives

## Values, storage, and access paths

A **value** is the semantic thing a program computes, owns, moves, copies, borrows, observes, mutates, or destroys.

**Storage** is where a value can live. Storage has identity for the purposes of ownership, borrowing, mutation, aliasing,
initialization, movement, and destruction.

An **access path** is a way to reach storage. A local binding, parameter, field projection, index projection, dereference,
temporary, view, or borrow may all be access paths when they reach storage.

Ownership, borrowing, mutation, movement, and destruction are checked over access paths and the storage they reach.
Two access paths may conflict when they can reach the same storage or overlapping storage with incompatible capabilities.

---

## Bindings

A **binding** gives a name to an access path, value, function, type, module, contract, or other declared program entity.

A local value binding introduces an access path to storage. The binding has a declared capability: by default it grants
read-only observation. When explicitly mutable, it grants local mutation authority.

A binding is introduced once in its scope. Rebinding and shadowing are not part of Bray's language model.

A binding may own the value it names, borrow storage owned elsewhere, or denote a non-value entity such as a type, function,
module, or contract. The kind of binding determines which operations are valid through that name.

Bindings are part of name resolution and capability checking. Resolving a name identifies the binding. Checking the operation
determines whether that binding grants the required ownership, borrowing, mutation, movement, or effect capability.

The difference between bindings and storages is that a binding is the *declared named handle* through which code refers to
something, while storage is the *semantic location* where a value lives.

Several bindings may refer to the same storage only when Bray's rules create an explicit non-owning alias or mediated
ownership relationship. A binding may also refer to no storage at all, such as when it names a type, function, module, or
behavioral contract.

A binding can become unusable for some operations while the storage it once reached still exists. For example, after ownership
moves away from a binding, the binding no longer owns the value, but the value continues through the new owner.

---

## Scopes and blocks

A **scope** is a region of program structure where bindings are visible and where lifetime, ownership, and capability rules
are evaluated.

A **block** is a scoped sequence of program elements. Blocks introduce scopes, delimit local bindings, and define where local
owned resources are destroyed.

A binding belongs to the scope that introduces it. A binding is visible according to Bray's lexical visibility rules and is
introduced once in that scope.

Leaving a block ends the lifetimes of local owned values whose ownership remains in the block. Destruction occurs in the
deterministic order defined by Bray.

Scoped capabilities end at their declared boundary. This includes temporary borrows, mutation authority, internal-effect
authority, and other scoped permissions. A capability may extend beyond its creating expression only when its contract ties
it to a valid enclosing scope.

Every exit from a block has a coherent ownership, initialization, and destruction state.

---

## Expressions and program elements

An **expression** is a program element that produces a value, access path, control-flow outcome, or compile-time entity.

Bray is expression-oriented. Blocks, conditionals, matches, and other control-flow forms produce values when their exits
have coherent type, ownership, initialization, and destruction state.

An expression carries a semantic contract: its type, ownership behavior, capability requirements, and effects.

A **program element** is an item that appears inside a block, module, or declaration body. Program elements include
declarations, expressions, and control-flow forms.

Operations such as observation, borrowing, mutation, movement, copying, consumption, construction, and destruction occur
through expressions whose required capabilities are available at that program point.

---

## Control flow

**Control flow** determines which program elements execute and how execution leaves expressions, blocks, functions,
and other control-flow regions.

A **control-flow region** has one or more exits. Each exit carries a type state, ownership state, initialization state,
destruction state, and capability state.

When control-flow paths merge, their states merge into one coherent program state. The merged state determines which bindings
remain usable, which values remain initialized, which resources remain owned, and which capabilities remain active.

Value-producing control flow has a resulting type and ownership story. Each value-producing exit contributes to the resulting
value according to the same type, ownership, initialization, and destruction rules.

Abrupt exits, such as returning from a function or leaving a loop, still preserve deterministic ownership and destruction
behavior. Values owned by scopes that are exited are destroyed according to Bray's destruction rules unless ownership has
moved elsewhere.

---

## Execution modes

An **execution mode** defines how a computation produces its result.

Bray has two execution modes: **synchronous** and **asynchronous**.

A **synchronous computation** completes at the point where it is evaluated. Its result, effects, ownership changes, capability
changes, and destruction behavior are resolved before control continues.

An **asynchronous computation** is an owned value representing suspended or suspendable execution. It contains the state required
to complete according to its type, ownership, lifetime, capability, and effect contract.

Awaiting an asynchronous computation drives it to completion and produces its declared result.

Suspension captures the values, borrows, capabilities, and effects that remain live across the suspension point. Captured state
becomes part of the asynchronous computation's ownership and lifetime contract.

A computation's execution mode is part of its semantic contract. Function signatures, behavioral contracts, and callable values
distinguish synchronous computation from asynchronous computation.

A value with an asynchronous finalization obligation carries that obligation as part of its type contract. The compiler tracks
the obligation across ownership transfer, movement, scope exit, cancellation, and destruction.

---

## Tasks and task scopes

A **task** is an owned asynchronous computation that is scheduled for execution.

A **task scope** is a structured ownership boundary for spawned asynchronous work. Tasks spawned inside a task scope belong to
that scope unless ownership is transferred to an explicit task handle.

A task scope owns the completion, cancellation, and destruction obligations of the tasks it contains. Leaving a task scope
requires every contained task to be completed, cancelled, or transferred according to its task contract.

**Spawning** creates a task from an asynchronous computation and places that task under a task scope or task handle.

A **task handle** is an ownership-extending value for a task. The handle carries responsibility for joining, cancelling, or
otherwise completing the task according to its contract.

**Awaiting** an asynchronous computation drives it to completion and produces its declared result.

**Cancelling** a task or incomplete asynchronous computation destroys its owned captured state and releases the capabilities it
holds according to Bray's destruction and finalization rules.

Destroying an incomplete asynchronous computation cancels it.

Detached asynchronous work exists only through explicit task handles. A detached task owns or otherwise validly extends the
lifetime of all state it uses.

Low-level async runtime machinery is part of the trusted substrate. Executors, reactors, wakers, completion queues, foreign async
callbacks, device async integration, and custom scheduling primitives are implemented through trusted capabilities and exposed
through safe async contracts.

---

## Functions

A **function** is a callable program element with parameters, a body, an execution mode, and a result contract.

A function signature declares the function's boundary contract: parameter types, parameter capabilities, result type, execution
mode, ownership transfer, mutation requirements, effects, trusted capabilities when present, and asynchronous finalization
behavior when relevant.

A function is either synchronous or asynchronous.

A synchronous function completes at the call site and produces its declared result before control continues.

An asynchronous function creates an asynchronous computation. The computation is an owned value that is awaited, spawned,
transferred, or destroyed according to its type and execution contract.

A **parameter** is a binding declared by a function. It exists inside the function body and receives its type, capability,
ownership behavior, and effect requirements from the function signature.

An **argument** is the expression supplied at a call site for a parameter. Calling a function evaluates each argument, checks
that it provides the capability required by the corresponding parameter, and transfers or lends values according to the function
signature.

Parameters and arguments are different semantic roles. Parameters define what the function requires. Arguments provide values,
access paths, or computations that satisfy those requirements at a specific call site.

A function body is a control-flow region. Every exit from the body satisfies the declared result type, ownership state,
initialization state, destruction state, effect contract, execution mode, and finalization obligations.

A function that consumes a value receives ownership of that value. A function that returns a value transfers ownership of the
returned value to the caller.

---

## Types

A **type** is a semantic contract for values, storage, access paths, and operations.

A type defines the structure, ownership behavior, initialization rules, destruction behavior, copy behavior, mutation behavior,
valid operations, and capability requirements for the values it describes.

A type has one primary representation declaration. The primary representation declaration defines the type's identity, structure,
layout-relevant shape, and representation-level ownership contract.

Methods, behavioral contract implementations, associated behavior, and other implementation blocks for a type can be declared
separately from the primary representation declaration. Separate implementation blocks add behavior, they do not add representation.

Every value has a type. Every expression has a type. Every access path reaches storage whose current value state is governed
by a type.

A **generic type** is a type parameterized by other types, constants, capabilities, effects, lifetimes, or other language-defined
generic parameters.

A type declaration introduces the type's own name. Bray does not support type aliases; a name that denotes a type denotes a
declared type, not an alternate name for another type.

A **generic parameter** is part of a declaration's contract. Its constraints define which operations, ownership behavior,
capabilities, effects, and behavioral contracts the generic declaration relies on.

Generic code is checked against its declared constraints. A generic body uses only the behavior guaranteed by those constraints.

Types participate in ownership, borrowing, aliasing, movement, destruction, effects, behavioral contracts, polymorphism, and code
generation.

A type can expose ordinary operations, behavioral contract implementations, associated types, constants, constructors, destructors,
and trusted contracts according to its declaration.

Type checking determines whether expressions, calls, bindings, control-flow exits, generic instantiations, and declarations satisfy
the type contracts they use.

---

## Built-in types

A **built-in type** is a type defined by the language or standard substrate rather than by user code.

Built-in types participate in the same ownership, borrowing, mutation, initialization, destruction, constraint, and effect
rules as user-defined types, unless their language-defined contract states otherwise.

The core built-in type categories are:

- **booleans:** represent truth values.
- **characters:** represent a Unicode scalar value.
- **signed/unsigned integers:** represent a whole-number value with a defined signedness and width.
- **floating-point numbers:** represent an approximate real-number value with a defined format.
- **machine-sized integers:** represent an integer whose width is defined by the target platform.
- **unit type:** the type with exactly one value. It represents completion without meaningful returned data.
- **never type:** the type with no values. It represents computation that does not produce a value because control flow does not
  continue normally from that point.
- **optional values:** represent either a present value of a contained type or absence of a value. Absence is a valid
  initialized state of the optional type.
- **tuples:** a fixed-size ordered product type. A tuple's element types are part of its type. Tuple ownership, borrowing,
  movement, copying, initialization, and destruction are derived from its elements.
- **arrays:** a fixed-size ordered sequence type. An array's element type and length are part of its type. Array ownership,
  borrowing, movement, copying, initialization, and destruction are derived from its elements.

Conversions are explicit operations, except for literals whose value is valid for the target type. Non-literal values do not
implicitly cast, widen, narrow, reinterpret, allocate, borrow, clone, move, or dispatch through conversion-like behavior.

Numeric operations are defined by the contracts of the participating types. Overflow, narrowing, rounding, division by zero,
NaN behavior, and platform-dependent behavior are part of those contracts.

Built-in types can have compiler-known layout, code generation, and intrinsic behavior. That knowledge does not exempt them from
the language's ordinary semantic rules.

---

## Constraints

A **constraint** is a compile-time requirement attached to a generic parameter, declaration, expression, or contract.

Constraints describe what the compiler knows about an otherwise generic entity. They define the operations, behavioral contracts,
ownership behavior, copy behavior, destruction behavior, capability requirements, effects, execution mode, and finalization
obligations that generic code can rely on.

A generic body is checked against its declared constraints. The body can use only behavior guaranteed by those constraints.

A generic instantiation satisfies a constraint when the supplied type, value, capability, effect, lifetime, or other generic argument
provides the required contract.

Constraints are part of the public semantic contract of a declaration. Changing constraints changes what callers may supply and what
the generic body may assume.

---

## Behavioral contracts

A **behavioral contract** is a named compile-time contract that describes behavior a type provides.

A behavioral contract defines required operations, associated types, constants, capability requirements, effect requirements,
execution-mode requirements, and semantic obligations.

A type satisfies a behavioral contract through an explicit implementation.

Generic code uses behavioral contracts through constraints. When a generic parameter is constrained by a behavioral contract, the
generic body can use the behavior declared by that contract.

A behavioral contract can describe synchronous behavior, asynchronous behavior, consuming behavior, mutating behavior, observing
behavior, internal-effect behavior, and trusted behavior when those are part of the contract.

A behavioral contract is part of the public semantic surface of a program. Changing a contract changes what implementers must
provide and what callers can rely on.

---

## Implementations

An **implementation** is an explicit declaration that makes a type satisfy a behavioral contract.

An implementation defines how the type provides the operations, associated types, constants, capability requirements, effect
requirements, execution modes, and semantic obligations required by the contract.

Implementations are nominal relationships between a type and a behavioral contract. A type satisfies a contract only through an
implementation visible to the compiler.

An implementation is part of the program's semantic surface. It participates in type checking, generic constraint satisfaction,
dispatch, documentation, and public API compatibility.

An implementation may provide additional behavior only when that behavior is declared by the implementation form or by the type's
own public contract.

---

## Visibility and protected representation

Visibility declares intended API audience and stability. It is part of the public contract surface, not a memory-safety mechanism.

Internal declarations are accessible only through explicit acknowledgement at the use site. This records that the caller is
depending on implementation detail.

Protected representation is reserved for compiler-known constructs whose invariants are required by Bray's ownership, aliasing,
mutation, initialization, async, or trusted-memory model. Protected representation is enforced by the compiler.

---

## Workspaces

A **workspace** is a local development collection of packages.

A workspace groups packages for editing, building, testing, vendoring, and tooling. It is not itself a package, versioned
dependency, or importable program unit.

Packages inside a workspace depend on each other through ordinary package dependencies. The package dependency graph remains
acyclic, including dependencies between packages in the same workspace.

A workspace owns workspace-level configuration, shared tool settings, local package discovery, and dependency override policy.

---

## Packages

A **package** is a build, versioning, distribution, and dependency unit.

A package owns a set of modules, declared dependencies, build settings, target constraints, and package-level metadata.

The package dependency graph is acyclic. A package can depend on another package, but two packages cannot depend on each other
directly or indirectly.

A package is compiled from its declared source graph and dependency graph. The compiler discovers package declarations before
binding and checking module bodies.

A package defines the boundary between its own declarations and declarations supplied by dependencies.

---

## Modules

A **module** is a namespace and source-organization unit inside a package.

A module contains declarations. Modules organize names, define declaration ownership, and participate in visibility and
import rules.

A module can be declared across multiple declaration blocks and source files. Each declaration block that contributes to a
module declares the same module identity.

Split module declarations contribute to one logical module. Declaration merging is deterministic, and duplicate declarations
are errors unless the declaration form explicitly defines merging behavior.

Modules are compile-time structure. A module has no runtime initialization phase and does not execute code when imported or
referenced.

Modules cannot alias other modules.

Modules inside the same package can refer to each other cyclically. The module graph is a declaration graph, not a
build-order graph.

---

## Imports

An **import** is an explicit declaration that makes declarations from another module or package available for name
resolution.

Imports resolve through the declared workspace, package, module, and dependency graph.

An import does not execute code, initialize a module, or change runtime behavior by itself.

An import does not silently extend overload sets, operators, conversions, behavioral contracts, or other polymorphic
behavior. Imported behavior participates in the program only through explicit imported declarations and Bray's deterministic
lookup rules.

Imports are part of the source graph and semantic context of the importing module.

---

## Ownership operations

An **ownership operation** changes or uses the ownership state of a value, storage, or access path.

- A **move** transfers ownership from one access path to another. The source access path no longer owns the moved value.
- A **copy** creates a separate value with its own ownership story. Copying exists only for types whose contracts support
  copy semantics.
- A **borrow** creates a temporary non-owning access path to storage owned elsewhere.
- A **mutable borrow** creates temporary exclusive mutation authority over storage owned elsewhere.
- A **consume** operation takes ownership of a value for the purpose of using, transforming, returning, transferring, or
  destroying it.
- A **destroy** operation ends ownership of a value and releases the resources governed by its destruction contract.

Ownership operations are checked against initialization state, active borrows, aliases, mutation authority, finalization
obligations, and the type's ownership contract.

---

## Borrowing

- A **borrow** is a temporary non-owning access path to storage owned elsewhere.
- A **shared borrow** grants observation. Multiple shared borrows to the same storage can exist at the same time when their
  capabilities are compatible.
- A **mutable borrow** grants temporary exclusive mutation authority over the storage it reaches.
- A **reborrow** creates a new borrow from an existing borrow. The new borrow is narrower than or equal to the authority of
  the borrow it comes from, and the original borrow is suspended for the reached storage while the reborrow is active.

A borrow has a lifetime. The borrow is valid only while the owner and the reached storage remain valid, and only while the
borrow's capability remains compatible with other active access paths.

While a borrow exists, the owner remains responsible for the borrowed storage, but owner capabilities that conflict with the
borrow are suspended.

Borrowing is checked over access paths and the storage they reach.

---

## Mutation authority

**Mutation authority** is the capability to change the value stored at an access path.

Mutation authority is unique by default. While an access path holds mutation authority over storage, incompatible shared
observation, mutation, movement, and destruction of the same storage are suspended.

A mutable binding is a local source of mutation authority. A mutable borrow is temporary mutation authority over storage owned
elsewhere.

Mutation authority applies to the storage reached by an access path. Mutation of substructure requires mutation authority over
the reached substructure.

Shared mutation exists only through explicit shared-mutation constructs. These constructs define the mutation discipline they
provide, such as synchronization, atomicity, interior mutability, runtime borrow checking, or single-assignment initialization.

Mutation authority ends at the boundary defined by the expression, borrow, block, guard, or shared-mutation construct that
created it.

---

## Initialization

**Initialization** is the process that gives storage a valid value governed by a type.

Storage has an initialization state. It can be uninitialized, partially initialized, fully initialized, moved from, or destroyed.

Only fully initialized values can be observed, borrowed, moved, copied, consumed, or destroyed as complete values.

Partially initialized storage is tracked by the compiler. Each initialized part follows its own ownership and destruction rules
until the whole value becomes fully initialized.

Initialization is not ordinary mutation. A value being initialized has no stable observable identity until initialization
is complete.

Reinitialization gives valid storage a new value after its previous value has moved or been destroyed, when the storage and type
contract permit it.

---

## Destruction and finalization

**Destruction** ends ownership of a value and releases the resources governed by its destruction contract.

Destruction is deterministic. A fully initialized owned value is destroyed exactly once unless ownership moves elsewhere or the
value enters an explicit ownership construct with a different lifetime contract.

Destruction is synchronous. Leaving a scope destroys local owned values whose ownership remains in that scope, in the order
defined by Bray.

A moved-from value is not destroyed by the old owner. Partially initialized storage destroys only the parts that were initialized.

**Finalization** is a required lifecycle obligation that must be completed before ownership ends.

A value can carry a synchronous or asynchronous finalization obligation as part of its type contract. The compiler tracks
finalization obligations across movement, scope exit, cancellation, and destruction.

A value with a finalization obligation must be finalized, transferred to another owner that assumes the obligation, or converted
into an explicit fallback ownership form before the owning scope exits.

Asynchronous finalization is completed through asynchronous execution. Ordinary destruction remains synchronous.

---

## Effects and capability contracts

An **effect** is an observable or declared consequence of evaluating a program element beyond producing a value.

A **capability contract** declares which capabilities an operation requires, holds, creates, transfers, or releases.

Effects and capabilities are part of function signatures, behavioral contracts, generic constraints, callable values, and
trusted declarations.

The core capability categories are:

* **Observe:** reads or inspects without visible mutation or internal mutation.
* **Observe with internal effects:** preserves the abstract value while performing declared internal effects such as caching,
  metrics, lazy initialization, locking, or reference-count updates.
* **Mutate:** changes the abstract value reached through an access path.
* **Consume:** takes ownership of a value.
* **Finalize:** completes a required lifecycle obligation before ownership ends.
* **Trusted:** uses declared trusted memory capabilities.

A program element can use only the effects and capabilities available through its bindings, parameters, constraints,
execution mode, and surrounding context.

Generic code is checked against declared effects and capability contracts. A generic body uses only the effects and capabilities
guaranteed by its constraints.

Effects and capability contracts are part of overload resolution, behavioral contract satisfaction, dynamic dispatch, and public
API compatibility.

---

## Trusted declarations

A **trusted declaration** is a declaration that uses bounded unchecked memory power through Bray's trusted capability model.

A module must opt in to trusted declarations before it can contain trusted functions. The module directive permits trusted
declarations. It does not make the module's ordinary declarations trusted.

A **trusted function** declares the exact trusted capabilities it uses. The set of trusted capabilities is closed:

* `raw_memory`
* `unchecked_alias`
* `unchecked_init`
* `foreign_call`
* `layout_reinterpret`
* `manual_alloc`
* `device_memory`
* `intrinsic`

A trusted function uses exactly the trusted capabilities it declares. Declaring an unused trusted capability is an error.

Trustedness is local to the trusted declaration. Calling a trusted function uses that function's contract, but does not
grant trusted capabilities to the caller.

A public trusted implementation exposes either a safe public wrapper or an explicitly trusted public contract.

### Trusted capabilities

- `raw_memory`
    - Grants direct access to memory storage outside ordinary typed access paths.
    - Used for operations that address, read, write, or copy raw memory regions.

- `unchecked_alias`
    - Grants the ability to create or use aliases whose compatibility cannot be proven by the ordinary aliasing rules.
    - Used when an implementation establishes aliasing invariants that the compiler cannot derive.

- `unchecked_init`
    - Grants the ability to work with storage whose initialization state is managed manually.
    - Used for deferred initialization, output buffers, placement construction, and foreign APIs that initialize memory.

- `foreign_call`
    - Grants the ability to call code outside Bray's ordinary semantic model.
    - Used for FFI, system calls, platform APIs, and foreign runtime integration.

- `layout_reinterpret`
    - Grants the ability to reinterpret storage through a different layout contract.
    - Used for ABI boundaries, serialization primitives, packed data, and representation-level transformations.

- `manual_alloc`
    - Grants the ability to allocate, deallocate, or manage memory outside ordinary ownership constructs.
    - Used for allocators, arenas, runtime internals, and low-level containers.

- `device_memory`
    - Grants access to memory or resources governed by an external device or accelerator.
    - Used for GPU memory, DMA buffers, mapped device regions, and completion queues.

- `intrinsic`
    - Grants access to compiler-recognized operations whose semantics are defined by the compiler rather than ordinary
      Bray code.
    - Used for target intrinsics, atomic lowering hooks, SIMD, runtime primitives, and operations that require special
      compiler knowledge.

---

## Dependency graph

A **dependency graph** is the declared graph of packages, modules, source inputs, generated inputs, external artifacts,
system libraries, tools, and build steps that determine a compilation.

The dependency graph is explicit, inspectable, and reproducible.

Package dependencies form an acyclic graph. A package can depend on another package, but packages cannot depend on each other
directly or indirectly.

Dependencies are declared in project-owned files. Dependency resolution produces a concrete lockable result that records
selected versions, source identities, artifact identities, checksums, feature selections, build settings, target constraints,
and linkage requirements.

Vendored dependencies are project-owned dependency inputs. A vendored dependency can be source code, generated code, a compiled
artifact, an interface description, metadata, or another declared build input.

Compiled dependencies are dependency graph nodes. Their ABI, target platform, architecture, calling convention, exported
interface, linkage mode, version identity, and integrity checks are part of their declared contract.

Foreign libraries and system libraries are explicit boundary dependencies. Their required identity, version range, target
constraints, discovery rules, and validation checks are declared in the graph.

Build scripts, generated code, compiler plugins, foreign artifacts, and tool-driven source transformations are dependency graph
nodes with declared inputs, outputs, permissions, and reproducibility contracts.

The same declared dependency graph and target configuration produce the same compiler input.
