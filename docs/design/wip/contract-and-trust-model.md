# Contract and trust model

## Overview

Bray separates five related concepts:

1. **Trusted implementation capability:** authority used inside a trusted declaration body.
2. **Ordinary contract requirement:** a checkable condition expressed in Bray's contract expression language.
3. **Static constraint:** a compile-time fact expressed in Bray's predicate expression language.
4. **Trusted obligation:** an externally trusted fact that Bray cannot prove through ordinary semantics.
5. **Predicate:** a named contract-level relation used in requirements, guarantees, constraints, and trusted obligations.

Trusted implementation power and trusted caller obligations are independent. A function can use trusted implementation capabilities
internally while exposing an ordinary safe API. A function exposes trusted caller obligations only when its public contract says so.

---

## Trusted modules

Trusted declarations are permitted only inside trusted modules.

A trusted module declaration says that the module contains trusted declarations. It does not grant trusted capabilities to every
declaration in the module.

Module declaration syntax, block module declarations, module visibility, and split module rules are defined by the Module and
Package Model.

```bray
trusted module runtime.memory;
```

Block-style module declarations are also supported:

```bray
trusted module runtime.memory
{
    ...
}
```

A trusted module declaration is an error if the module contains no trusted declarations.

A non-trusted module cannot contain trusted declarations:

```bray
module app;

trusted func f() uses(raw_memory)
{
    ...
}
```

This is invalid because `app` did not opt in to trusted declarations.

---

## Trusted functions

A trusted function declaration uses the `trusted` modifier.

A trusted function can use trusted implementation capabilities, expose trusted caller obligations, or both.

A trusted function that uses trusted implementation capabilities declares the exact capabilities used by its body with `uses(...)`.

```bray
trusted func copy_bytes(
    destination: &mut ByteBuffer,
    source: &ByteBuffer,
    count: usize,
) uses(raw_memory)
{
    ...
}
```

Multiple trusted capabilities are comma-separated:

```bray
trusted func call_os_read(
    handle: OsHandle,
    buffer: &mut ByteBuffer,
) -> Result<usize, OsError>
    uses(foreign_call, unchecked_init)
{
    ...
}
```

The `uses(...)` clause is implementation-side. It says what trusted capabilities the declaration body uses. It does not by itself
impose any trusted obligation on callers.

A trusted declaration must use exactly the capabilities it declares. Declaring an unused trusted capability is an error.

If a trusted declaration uses no trusted implementation capabilities, it has no `uses(...)` clause.

```bray
trusted module runtime.io;

trusted func suspicious(...) uses(foreign_call, raw_memory)
{
    // Uses only foreign_call.
    // Error: raw_memory declared but unused.
}
```

A non-`trusted` function cannot have a `uses(...)` clause with trusted capabilities.

---

## Trusted implementation vs trusted caller obligation

A trusted implementation can expose an ordinary safe API.

```bray
trusted func copy_bytes_checked(
    destination: &mut ByteBuffer,
    source: &ByteBuffer,
    count: usize,
) -> Result<unit, CopyError>
    uses(raw_memory)
{
    if count > destination.length {
        return CopyError.destination_too_small;
    }

    if count > source.length {
        return CopyError.source_too_small;
    }

    ...
}
```

Calling this function is ordinary because the function checks or establishes the necessary invariants itself.

By contrast, a function exposes a trusted caller obligation when its contract requires a trusted fact:

```bray
trusted func copy_bytes_unchecked(
    destination: RawPointer<u8>,
    source: RawPointer<u8>,
    count: usize,
) -> unit
    requires(
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.valid_read(pointer = source, count = count),
        trusted core.memory.non_overlapping(left = destination, left_count = count, right = source, right_count = count),
    )
    uses(raw_memory)
{
    ...
}
```

Here the caller must provide or acknowledge facts Bray cannot prove ordinarily.

A trusted implementation capability is declared with `uses(...)`.

A trusted caller obligation is declared inside `requires(...)` or another contract clause using `trusted <predicate-call>`.

---

## Contract clauses

Contract clauses are always parenthesized comma-separated lists.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

The same rule applies to `ensures(...)`, `with(...)`, and `uses(...)`.

```bray
ensures(
    result.length == length,
    result.capacity == capacity,
)

uses(raw_memory, manual_alloc)
```

This avoids ambiguous or hard-to-read clause chains.

Invalid style:

```bray
requires length <= capacity requires trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align)
```

Valid style:

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

A `requires(...)` clause lists facts that must hold before the declaration body executes.

An `ensures(...)` clause lists facts established after the declaration completes normally.

Inside an `ensures(...)` clause for a declaration that completes with a value, `result` is the compiler-introduced postcondition
binding for that normal completion value.

`result` is available only in postcondition predicate contexts.

`result` is not available in `requires(...)`, `with(...)`, predicate declaration bodies, guard expressions, ordinary expression
contexts, or declarations whose normal completion does not produce a value.

Bray does not support named result bindings.

A `with(...)` clause lists static constraint facts required by a generic declaration.

A `uses(...)` clause lists trusted implementation capabilities used by the declaration body.

---

## Ordinary requirements

An ordinary requirement is a contract expression.

```bray
requires(
    length <= capacity,
)
```

Ordinary requirements are evaluated in contract-expression context. They must be pure, deterministic, total, terminating, and
observational.

Calls inside ordinary requirements must resolve to predicates, compiler-known predicate-valid operations, or const callables valid
in contract-expression context.

Ordinary requirements can be proven statically, established by previous facts, or checked through runtime assertion mechanisms where
appropriate.

A failed runtime check of an ordinary requirement panics.

---

## Trusted requirements

A trusted requirement is a trusted predicate call inside a contract clause.

```bray
requires(
    trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
)
```

A trusted requirement represents a fact Bray cannot prove or check through ordinary semantics.

Trusted requirements must come from one of these sources:

- a caller acknowledgement,
- an enclosing trusted obligation,
- a witness value,
- a trusted declaration that establishes the fact,
- or another compiler-recognized trusted source.

Trusted requirements must not appear from nowhere.

---

## Ensured facts

A declaration can establish ordinary facts:

```bray
func clamp(pos value: i32, min: i32, max: i32) -> i32
    requires(
        min <= max,
    )
    ensures(
        result >= min,
        result <= max,
    )
{
    ...
}
```

A trusted declaration can establish trusted facts:

```bray
trusted func allocate(bytes: usize, align: usize) -> RawPointer<u8>
    ensures(
        trusted core.memory.owned_allocation(pointer = result, bytes = bytes, align = align),
        trusted core.memory.valid_write(pointer = result, count = bytes),
    )
    uses(manual_alloc)
{
    ...
}
```

Trusted ensured facts become available to the caller after successful completion, subject to their lifetime, ownership, capability,
and invalidation rules.

---

## Trusted witness values

A trusted witness value is an ordinary value whose live value contract carries one or more trusted predicate facts.

Witness values are a semantic category of values, not a separate source construct.

There is no dedicated witness expression syntax.

Witness values are introduced by ordinary declaration mechanisms:

- trusted functions,
- trusted constructors,
- trusted lifecycle declarations,
- compiler-known declarations,
- recognized standard-library declarations whose language-defined contracts establish trusted facts.

A declaration introduces a witness value when its successful result carries trusted facts in `ensures(...)` and those facts are tied
to the returned value, its fields, its owned storage, its borrows, or its dependency contract.

```bray
struct ReadableByte
{
    pointer: RawPointer<u8>;
}

trusted func assume_readable_byte(pos pointer: RawPointer<u8>) -> ReadableByte
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
    ensures(
        result.pointer == pointer,
        trusted core.memory.valid_read(pointer = result.pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = result.pointer),
        trusted core.memory.initialized_as<u8>(pointer = result.pointer),
    )
{
    return ReadableByte(pointer = pointer);
}
```

The returned `ReadableByte` value is a witness value.

While the witness value is live and valid, its carried trusted facts are available in the fact context.

Those facts can satisfy matching trusted requirements.

```bray
let readable = trusted assume_readable_byte(pointer);

let byte = core.memory.read<u8>(readable.pointer);
```

The trust boundary in the first statement acknowledges the trusted requirements declared by `assume_readable_byte`.

The second statement can use the trusted facts carried by `readable`.

A trust boundary expression does not create a witness value by itself.

If a trust boundary operand returns a witness value, the lasting facts come from the returned value's contract, not from the trust
boundary.

### Witness lifetime and movement

A witness value carries its trusted facts only while the value is fully initialized, live, and valid.

Moving a witness value moves the trusted facts carried by that value.

After the move, the old access path no longer carries those facts.

Returning a witness value returns its trusted facts as part of the result value's dependency contract.

Storing a witness value stores its trusted facts only when the destination can preserve every dependency required by the value.

Storing a witness value does not extend any allocation, borrow, scoped capability, resource, or lifetime that the witness depends
on.

Copying a witness value is rejected unless the value's copy contract explicitly states how the copied value preserves or splits the
trusted facts.

Observing or copying ordinary fields from a witness value does not copy the trusted facts carried by the witness value.

The trusted facts remain tied to the witness value and to the dependencies named by its contract.

Partially moving a witness value is rejected when the move would separate trusted facts from the value or dependency that carries
them.

### Witness invalidation

The trusted facts carried by a witness value are invalidated when the witness value is moved from, consumed, destroyed, finalized,
assigned `none`, replaced, or otherwise stops being live.

The trusted facts are also invalidated when a value, storage identity, allocation, borrow, scoped capability, resource, epoch, or
version named by the carried facts is invalidated.

Mutating a witness value, a referenced access path, or referenced storage invalidates every carried fact whose truth can depend on
the mutated state.

Destroying or finalizing an owner invalidates witness facts that depend on the owner's storage or resource state.

Leaving a `with` body invalidates witness facts that depend on the scoped capability produced by that `with` expression unless the
selected `exit` contract transfers the obligation to another live value.

Cancellation, panic propagation, `return`, `yield`, `break`, `continue`, nullable propagation, result propagation, and run-result
propagation preserve witness facts only along paths where the witness value and every dependency it names remain live and valid.

### Witness use in contracts

A trusted requirement can be satisfied by a live witness value when the fact carried by the witness value matches the required
trusted predicate after ordinary path, field, equality, and dependency reasoning.

Witness facts participate in the same fact context as other trusted facts.

They do not bypass ownership checking, borrowing rules, mutation authority, initialization checking, destruction checking,
finalization checking, capability checking, visibility, internal-access acknowledgement, or effect checking.

A callable or lifecycle declaration that receives, returns, stores, or captures a witness value must preserve the witness value's
dependency contract.

A public API that exposes a witness value exposes the trusted facts and obligations carried by that value as part of its public
contract.

---

## Predicates

A predicate is a contract-level relation. It is not an ordinary runtime function.

Predicates are used in contract clauses, constraint clauses, predicate bodies, and trusted obligations.

An ordinary predicate has a body:

```bray
predicate fits(length: usize, capacity: usize) =
    length <= capacity;
```

A predicate body is a single predicate expression.

A trusted predicate is opaque and has no ordinary body:

```bray
trusted predicate owned_allocation(
    pointer: RawPointer<u8>,
    bytes: usize,
    align: usize,
);
```

Trusted predicates name facts outside ordinary Bray proof power.

The Raw Memory Model defines the compiler-known trusted predicates under `core.memory`.

Specification notation for compiler-known trusted predicates:

```bray
trusted predicate owned_allocation(
    pointer: RawPointer<u8>,
    bytes: usize,
    align: usize,
);

trusted predicate valid_read<T>(
    pointer: RawPointer<T>,
    count: usize,
);

trusted predicate valid_write<T>(
    pointer: RawPointer<T>,
    count: usize,
);

trusted predicate non_overlapping<T>(
    left: RawPointer<T>,
    left_count: usize,
    right: RawPointer<T>,
    right_count: usize,
);
```

These are not keywords.

They are compiler-provided declarations accessed through normal Bray paths.

User packages and standard-library packages cannot declare or replace the compiler-known `core.memory` predicates.

---

## Predicate expressions

Predicate expressions are contract-level expressions.

They are parsed and checked in predicate-expression context. The parser may reuse ordinary expression grammar, but binding and
checking apply predicate-expression restrictions.

Predicate expressions are not ordinary runtime Bray expressions.

A predicate expression must be pure, deterministic, total, terminating, and observational.

Predicate expressions are checked in one of these predicate contexts:

- **value predicate context:** for value predicate bodies, `requires(...)`, `ensures(...)`, and trusted obligations.
- **static constraint context:** for `with(...)` clauses on generic declarations and static predicate bodies.

Both contexts use the same purity and determinism rules.

Each context decides which names, entities, and built-in predicate forms are available.

A predicate declaration is checked in the predicate context required by its parameters, body, and declared use.

Allowed in predicate expressions:

- literals,
- references to predicate parameters,
- `self` where applicable,
- result references in postconditions,
- constants and constant-valued members,
- field access through observable access paths,
- tuple, array, nullable, and union inspection by observation,
- arithmetic using contract arithmetic semantics,
- boolean operators,
- comparisons,
- calls to predicates,
- calls to functions and methods whose selected callable contract is valid in predicate-expression context,
- conditional expressions whose condition and branches are valid predicate expressions,
- `all(...)` and `any(...)` boolean fold expressions whose operands are predicate-valid, finite, bounded, and iterable as `bool`.

In static constraint context, a boolean fold operand must be statically enumerable or have a finite bound the compiler can reason
about.

In value predicate context, a runtime contract check may iterate a runtime-sized boolean fold operand only when its finite bound is
available from observable state in that predicate context.

If finiteness or boundedness cannot be proven in the required predicate context, the predicate expression is rejected.

Forbidden in predicate expressions:

- block expressions,
- `let` bindings,
- assignment,
- mutation,
- movement or consumption,
- destruction,
- finalization,
- allocation,
- I/O,
- function calls whose selected callable contract is not valid in predicate-expression context,
- method calls whose selected callable contract is not valid in predicate-expression context,
- async, await, spawn, `try`, or `catch`,
- `with` expressions and resource-scope behavior,
- loops with runtime control flow other than generator iteration expressions used to produce finite boolean operands for `all(...)`
  or `any(...)`,
- dynamic dispatch with effects,
- trusted capability use,
- reading mutable global state,
- depending on time, randomness, address layout, scheduler state, or implementation scheduling.

Because ordinary Bray blocks are expressions, block expressions must be explicitly forbidden in predicate expressions.

Invalid:

```bray
predicate valid(length: usize, capacity: usize) =
    {
        yield length <= capacity;
    };
```

Valid:

```bray
predicate valid(length: usize, capacity: usize) =
    length <= capacity;
```

---

## Static constraint predicate expressions

A `with(...)` clause is a comma-separated list of static predicate expressions.

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I: Iterator,
        I(Iterator).Element == Token,
    )
{
    ...
}
```

Static predicate expressions are checked in static constraint context.

They are compile-time facts.

They do not read runtime storage, execute runtime code, allocate, mutate, move, borrow, perform I/O, dispatch dynamically, or establish trusted runtime facts.

Allowed in static predicate expressions:

- references to generic parameters,
- type expressions,
- trait applications,
- type-valued member references,
- compile-time constants and constant generic values,
- capability, effect, lifetime, and execution-mode entities,
- boolean operators,
- equality and comparison operators whose operands are valid in static constraint context,
- calls to predicates, functions, and methods that are valid in static constraint context.

A predicate, function, or method is valid in static constraint context only when its parameters, body, selected callable contract,
and referenced declarations are valid in static constraint context.

For an ordinary function or method call to be valid in static constraint context, the callable must be a const callable or a private
or local helper inferred as const-eligible inside the same checking unit.

Calling a value predicate from static constraint context is rejected.

Built-in static predicate forms include trait satisfaction:

```bray
T: Comparable<T>
I: Iterator
&T: Iterable
&mut T: Iterable
```

The left side of a trait satisfaction predicate is an implementing subject.

The Type Model defines which type-form subjects are implementation-eligible.

Built-in static predicate forms include type equality:

```bray
I(Iterator).Element == Token
A(Iterator).Element == B(Iterator).Element
(&T)(Iterable).Element == &T
```

Type equality is a compile-time fact.

It does not introduce a type alias or alternate type name.

Constraint facts are unordered.

These two clauses have the same meaning:

```bray
with(
    I: Iterator,
    I(Iterator).Element == Token,
)

with(
    I(Iterator).Element == Token,
    I: Iterator,
)
```

A type-valued member reference in static constraint context is valid only when the exact implementation subject and exact trait application are established by the same constraint set or by an enclosing constraint context.

This is rejected because the selected `Iterator` application is not established:

```bray
func first_token<I>(iter: I) -> Token?
    with(
        I(Iterator).Element == Token,
    )
{
    ...
}
```

Type equality does not select an implementation, create a trait satisfaction fact, or choose an arm from an implementation overload family.

Result type and expected type do not infer missing static constraints.

Static constraint facts become available while checking the constrained declaration body, its signature, and its contract clauses.

They do not become runtime facts unless a separate value predicate or contract clause establishes a runtime fact.

---

## Predicate-expression function calls

A predicate expression can call a function or method when the selected callable contract is valid in predicate-expression context.

The callable can return any value type that is valid in predicate-expression context.

```bray
func remaining(capacity: usize, length: usize) -> usize
    requires(length <= capacity)
{
    return capacity - length;
}

predicate has_space(length: usize, capacity: usize) =
    remaining(capacity = capacity, length = length) > 0;
```

A callable contract is valid in predicate-expression context only when the callable is:

- pure,
- deterministic,
- total,
- terminating,
- observational,
- effect-free,
- allocation-free,
- async-free,
- free of trusted capability use,
- checked only through predicate-valid operations.

Public predicate-expression use of an ordinary function or method requires the selected declaration surface to expose `const`.

Private or local helper callables can be inferred as const-eligible within the same checking unit.

Inferred const eligibility does not become part of an exported declaration surface.

The selected callable contract is the full callable contract of the resolved function or method after overload selection and
generic substitution.

The callable body may use ordinary callable-body structure, including `return`, when every reachable result-producing path uses
only predicate-valid expressions and predicate-expression-valid calls.

The callable's parameters and result type must be valid predicate-expression values.

The call arguments must be predicate-valid expressions.

The callable's `requires(...)` obligations must be satisfied by the current predicate fact context.

The callable's `ensures(...)` facts become available after the call inside the predicate-expression check.

---

## Contract arithmetic

Integer-valued contract expressions use exact mathematical integer semantics, not machine overflow semantics.

For example:

```bray
predicate range_fits(offset: usize, count: usize, length: usize) =
    offset + count <= length;
```

The expression `offset + count` means exact integer addition over the values represented by `usize`.

Runtime checks generated from contract expressions must preserve contract semantics, for example through checked arithmetic or
equivalent rewriting.

Contract arithmetic must not silently wrap.

---

## Fact context

The fact context is the compiler's flow-sensitive set of known contract facts at a program point.

Facts can enter the fact context through:

- parameter contracts,
- constructor contracts,
- function contracts,
- finalizer contracts,
- destructor contracts,
- scope enter/exit contracts,
- branch conditions,
- successful pattern matches,
- runtime assertions of ordinary conditions,
- witness values,
- trust boundary expressions,
- trusted declarations that establish facts.

A fact is tied to the values, storage identities, lifetimes, capabilities, and versions it mentions.

A fact is invalidated when one of its referenced values or storage locations is mutated, moved, consumed, destroyed, reinitialized,
finalized, or otherwise changed in a way that can affect the truth of the fact.

A fact over immutable copied scalar values can survive independently.

A fact over mutable storage does not survive arbitrary mutation of that storage.

A fact over a borrow cannot outlive the borrow.

A fact over raw memory cannot outlive the allocation, lifetime, or epoch it refers to.

Example:

```bray
predicate can_index<T>(buffer: &Buffer<T>, index: usize) =
    index < buffer.length;
```

A fact of `can_index(buffer = buffer, index = index)` is valid only while the relevant buffer identity remains valid and the length state it depends
on remains unchanged.

If `buffer` is mutated in a way that can change `length`, the fact expires.

---

## Obligation propagation

Any trusted obligation used inside a callable or lifecycle declaration must be discharged before the declaration exits, or appear
in that declaration's public contract.

This prevents trusted obligations from being hidden by wrappers.

Invalid:

```bray
func bad(pointer: RawPointer<u8>) -> u8 {
    return trusted core.memory.read<u8>(pointer);
}
```

Valid if the obligation is exposed:

```bray
func read_wrapper(pointer: RawPointer<u8>) -> u8
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
{
    return trusted core.memory.read<u8>(pointer);
}
```

Valid if the wrapper proves or establishes the required facts before calling the trusted operation.

Trusted obligations must propagate through:

- wrappers,
- function values,
- generic parameters,
- dynamic dispatch,
- constructors,
- destructors,
- finalizers,
- scope enter/exit declarations,
- named callable contracts,
- exports,
- re-exports,
- internal declarations.

A function with trusted caller obligations cannot be used where an ordinary function is expected.

Invalid:

```bray
let f: func(buffer: &Buffer, index: usize) -> u8 = get_unchecked;
```

if `get_unchecked` has trusted caller obligations.

The function type must preserve the trusted obligation.

---

## Trust boundaries

Calling a function with trusted caller obligations requires a trust boundary at the call site or in an enclosing declaration.

The expression-level trust boundary syntax is:

```bray
trusted expression
```

A trust boundary expression is a trust boundary for its operand expression.

It visibly accepts the trusted caller obligations required by that operand.

The compiler records the exact trusted predicate requirements accepted at the boundary.

The semantic rule is fixed: the caller must visibly accept the trusted obligation unless it has already been established by the fact context.

A call to a safe wrapper around trusted implementation code does not require caller acknowledgement.

A call to a declaration with trusted caller obligations does require caller acknowledgement or proof.

The boundary scope is exactly the operand expression.

For a block operand, the scope is the block.

Trusted facts introduced solely by the boundary do not become facts after the operand completes.

A trust boundary inside a callable or lifecycle declaration does not hide the obligation from that declaration's callers.

A trusted obligation introduced by a trust boundary inside a callable or lifecycle declaration must be discharged before the
declaration exits, or it must appear in that declaration's public contract.

This keeps the invalid wrapper invalid:

```bray
func bad(pointer: RawPointer<u8>) -> u8
{
    return trusted core.memory.read<u8>(pointer);
}
```

The wrapper accepts a trusted obligation inside its body but does not expose that obligation to its caller.

This wrapper exposes the obligation:

```bray
func read_wrapper(pointer: RawPointer<u8>) -> u8
    requires(
        trusted core.memory.valid_read(pointer = pointer, count = 1),
        trusted core.memory.aligned_for<u8>(pointer = pointer),
        trusted core.memory.initialized_as<u8>(pointer = pointer),
    )
{
    return trusted core.memory.read<u8>(pointer);
}
```

`trusted expression` does not grant trusted implementation capabilities and does not satisfy ordinary requirements.

---

## Lifecycle declarations and trusted contracts

Trusted rules apply equally to lifecycle declarations.

Constructors can use trusted implementation capabilities:

```bray
trusted construct(length: usize) -> Self
    uses(manual_alloc)
{
    ...
}
```

This can still expose an ordinary safe construction API if the constructor establishes all invariants itself.

Constructors can expose trusted caller obligations:

```bray
trusted construct from_raw_parts(
    pointer: RawPointer<u8>,
    length: usize,
    capacity: usize,
    align: usize,
) -> Self
    requires(
        length <= capacity,
        trusted core.memory.owned_allocation(pointer = pointer, bytes = capacity, align = align),
    )
    uses(raw_memory, manual_alloc)
{
    ...
}
```

Finalizers, destructors, `enter`, and `exit` declarations follow the same distinction:

- `uses(...)` declares trusted implementation capability.
- `requires(...)` declares caller or lifecycle preconditions.
- `ensures(...)` declares established facts.
- trusted requirements must be propagated, discharged, or acknowledged.

Destructors remain synchronous and return `unit`. Fallible or asynchronous cleanup belongs to finalization, not destruction.

---

## FFI and foreign-call trust boundaries

An FFI boundary is any boundary where Bray code interacts with code, storage, callbacks, or runtime behavior outside Bray's ordinary
semantic model.

FFI boundaries include:

- calls from Bray into an extern callable,
- calls from foreign code into an exported ABI callable,
- foreign callbacks invoked through ABI-qualified callable values,
- foreign APIs that read, write, initialize, retain, release, or alias Bray-accessible storage,
- foreign APIs that create, consume, or transfer resource handles,
- foreign runtime behavior that can reenter Bray.

The Function and Callable Model defines `extern`, `@abi(...)`, `@link(...)`, and `@symbol(...)`.

The Type Model defines ABI-relevant data layout through `@layout(...)`.

The Raw Memory Model defines raw pointer facts, raw memory predicates, allocation facts, and ABI layout helpers.

### Imported foreign callables

An extern callable with a foreign ABI is a trusted declaration.

```bray
@link(name = "c")
@symbol(name = "getpid")
@abi(c)
extern trusted func get_process_id() -> i32
    uses(foreign_call);
```

The declaration is the complete Bray-visible contract for the foreign callable.

The compiler checks calls to the extern callable against that declaration exactly like calls to ordinary Bray callables.

The foreign implementation body is not Bray source and is not type checked as Bray source.

`uses(foreign_call)` is required for an extern callable with a foreign ABI.

The foreign-call capability is consumed by the call boundary itself.

A foreign callable that requires trusted facts must state them as trusted requirements.

```bray
@link(name = "runtime")
@symbol(name = "runtime_fill")
@abi(c)
extern trusted func fill_buffer(pos destination: RawPointer<u8>, count: usize) -> i32
    requires(
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.aligned_for<u8>(pointer = destination),
    )
    ensures(
        result < 0 || trusted core.memory.initialized_range_as<u8>(pointer = destination, count = count),
    )
    uses(foreign_call);
```

Calling `fill_buffer` requires those trusted facts to already be established or visibly acknowledged at a trust boundary.

The extern declaration must describe every caller-visible foreign obligation, including:

- pointer validity,
- alignment,
- initialization state,
- byte count and element count,
- lifetime and retention behavior,
- ownership transfer,
- aliasing permissions,
- thread-affinity requirements,
- synchronization requirements,
- callback reentrancy behavior,
- resource acquisition and release obligations,
- ordinary error return conventions,
- panic boundary behavior.

### Linkage and symbol identity

`@link(...)` selects an external artifact or system library dependency from the declared dependency graph.

`@symbol(...)` names the exact foreign symbol used by the selected ABI and target profile.

The dependency graph records the selected artifact identity, target constraints, ABI, architecture, linkage mode, version identity,
and integrity checks.

If the selected target profile cannot resolve the link dependency or symbol according to the dependency graph, the program is
rejected before code generation.

Link and symbol directives do not establish memory facts, ownership facts, resource facts, or safety facts.

Those facts belong to ordinary types, trusted predicates, and contract clauses.

### Ownership and borrowing across FFI

Foreign code cannot receive a hidden Bray ownership, borrow, lifetime, or finalization contract.

Every value that crosses a foreign ABI boundary is lowered according to the callable ABI and the value's data layout contract.

Raw pointers are plain pointer values at the ABI boundary.

A raw pointer crossing a foreign ABI boundary does not by itself carry ownership, validity, initialization, alignment, lifetime, or
aliasing facts.

Those facts must be established by the surrounding Bray context and named in the extern declaration's contract when the foreign
call depends on them.

Passing a raw pointer derived from a Bray borrow to a foreign call keeps the corresponding Bray borrow active for the full duration
of the call.

During that call, ordinary Bray code cannot use the borrowed storage in a way that conflicts with the borrow represented by the raw
pointer.

If foreign code stores a pointer, handle, callback, or context value beyond the dynamic extent of the call, the extern
declaration must represent that retention as an ownership transfer, resource obligation, or trusted lifetime obligation.

If the declaration does not state retention, the foreign call is checked as non-retaining.

A foreign API that initializes storage must establish initialization facts in `ensures(...)`.

A foreign API that consumes ownership must consume a linear owner, return a new linear owner, or state the ownership fact it
requires and invalidates.

A foreign API that releases a resource must consume the owner or scoped capability that carries the release obligation.

### ABI-safe values

ABI-safe values are values whose callable ABI representation and data layout are accepted by the selected ABI.

For foreign ABI boundaries, source-level convenience types such as slices, borrows, trait views, owned indirection, task handles,
thread handles, nullable types with protected representation, default-layout products, default-layout unions, and captured callable
values are not ABI contracts by themselves.

FFI surfaces use explicit ABI representations such as scalars, raw pointers, ABI-qualified callable values, and explicitly laid-out
products or unions.

```bray
@layout(c)
struct BufferView
{
    pointer: RawPointer<u8>;
    length: usize;
}
```

`@layout(c)` and compatible `@layout(transparent)` declarations can be used for C ABI aggregate values.

The Raw Memory Model's `std.memory.size_of<T>()`, `std.memory.align_of<T>()`, `std.memory.stride_of<T>()`, and
`std.memory.layout_of<T>(count = count)` helpers observe the effective layout contract used by these ABI representations.

### Error and panic boundaries

Recoverable foreign failures are modeled as ordinary ABI values.

An imported foreign status code, null pointer, handle value, or out-parameter convention is converted to Bray `Result<T, E>` by an
ordinary wrapper when the wrapper wants to expose recoverable failure as Bray error handling.

```bray
func fill_checked(pos destination: RawPointer<u8>, count: usize) -> Result<unit, IoError>
    requires(
        trusted core.memory.valid_write(pointer = destination, count = count),
        trusted core.memory.aligned_for<u8>(pointer = destination),
    )
{
    let status = fill_buffer(destination, count = count);

    if status < 0
    {
        return Result.Error(IoError.ReadFailed);
    }

    return Result.Ok(unit);
}
```

An uncaught Bray panic must not unwind through a foreign ABI frame.

An exported ABI callable catches any uncaught Bray panic at the foreign entry boundary and applies the program-root panic behavior
for that run.

The boundary does not return a success value after catching that panic.

A Bray callable that wants to report a panic-like failure to foreign code uses `catch` inside its body and returns an
ABI-representable error value.

If a foreign callable attempts to unwind through a foreign ABI boundary into Bray, it violates the extern declaration's ABI
contract.

Declarations for foreign APIs that can unwind must use a foreign wrapper that converts the unwind into an ordinary ABI result
before control reaches Bray.

### Foreign callbacks

A foreign callback accepted by a foreign API is represented by an ABI-qualified callable type.

```bray
callable VisitCallback =
    @abi(c) func(pos context: RawPointer<u8>, pos value: i32) -> i32;
```

Registering a Bray callable as a foreign callback is part of the trusted contract of the registration call.

The registration declaration must state whether the foreign API:

- calls the callback only during the registration call,
- stores the callback for later calls,
- stores the context pointer,
- calls from one thread or many threads,
- calls concurrently,
- calls after cancellation or shutdown,
- requires explicit unregistering.

Callbacks stored beyond the registration call require an owner, scoped capability, or resource obligation that keeps the callback
target and context valid until the foreign API releases them.

Captured Bray state does not cross a plain foreign callback ABI implicitly.

Foreign callback state is represented explicitly through context pointers, ABI-laid-out context values, handles, owners, or scoped
capabilities.

### Exported Bray callables

A Bray callable with `@abi(...)` and `@symbol(...)` can be exported as a native symbol.

The callable body is checked by Bray.

The foreign caller is not checked by Bray.

Every assumption about foreign-provided arguments must be represented by ABI-safe parameter types, ordinary runtime checks,
trusted requirements, or trusted wrappers.

An exported callable that dereferences a foreign pointer, accepts foreign-owned storage, or trusts foreign-provided lifetime facts
must use the same raw memory and trusted predicate rules as an imported foreign call.

---

## Design principles

No magic trusted-condition names exist.

No English-like pseudo-expressions are allowed in contracts.

No arbitrary runtime functions are allowed in predicate bodies.

Predicates are declared contract relations.

Trusted predicates are opaque facts with controlled introduction.

Contract clauses are parenthesized comma-separated lists.

Predicate expressions are a restricted expression language checked in contract context.

Trusted capabilities are implementation authority.

Trusted obligations are caller-supplied or externally established assumptions accepted at a trust boundary.

Facts are flow-sensitive and invalidated by ownership, mutation, lifetime, capability, and storage-state changes.
