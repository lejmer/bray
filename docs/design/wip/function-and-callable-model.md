# Function and callable model

## Overview

Bray functions are callable declarations.

A function defines:

1. a name,
2. parameters,
3. parameter capabilities,
4. an execution mode,
5. a result type,
6. a block expression body,
7. ownership and borrowing behavior,
8. effects and capability requirements,
9. constant-evaluation eligibility,
10. optional contract clauses.

Functions are named program entities. They can be referenced, called, passed as values when their type permits it, and used in
generic or higher-order contexts according to their full callable contract.

---

## Function declarations

A function is declared with `func`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The function name follows `func`.

Parameters are declared inside parentheses.

The result type follows `->`.

The function body is a block expression in callable-body context.

---

## Const functions

A function can be declared with the `const` modifier:

```bray
const func max(pos a: i32, pos b: i32) -> i32
{
    if a >= b
    {
        return a;
    }

    return b;
}
```

`const func` means the callable body is valid in constant-evaluation context.

A const function is valid in constant expressions, predicate expressions, contract expressions, and static constraint contexts when
its arguments are valid in that context and its ordinary callable contract is satisfied.

A const function must be pure, deterministic, total for valid inputs, terminating, observational, effect-free, and allocation-free.

A const function cannot read runtime storage, mutate storage, perform I/O, spawn work, await, suspend, catch or raise panics as
runtime behavior, use runtime dynamic dispatch, or depend on runtime identity.

The body of a const function is checked in constant-evaluation context.

Every reachable result-producing path must use only expressions, operations, and calls valid in constant-evaluation context.

Public use in predicates, contract expressions, static constraints, and constant expressions requires the declaration surface to
expose `const`.

Private or local helper callables can be inferred as const-eligible inside the checking unit.

Inferred const eligibility for a private or local helper does not become part of the exported declaration surface.

When a declaration is exported or otherwise used through compiled interface metadata, const-context callers can depend on that
declaration only when its interface exposes `const`.

Compiler-known declarations and recognized standard-library declarations can be `const` when their language-defined declaration
contract marks them as `const`.

`const` composes with ordinary function modifiers only where the combined contract is valid.

`trusted const func` is valid only when the trusted capability use is itself permitted by the constant-evaluation and trust models.

`async const func` is rejected.

Async evaluation implies suspension or runtime execution machinery, which is not valid in constant-evaluation context.

`const` is part of a callable's caller-visible contract.

Changing whether a public function exposes `const` is a public API change.

---

## Omitted result type

If the result type is omitted, the function returns `unit`.

```bray
func log(pos message: string)
{
    print(message);
}
```

This means:

```bray
func log(pos message: string) -> unit
{
    print(message);
}
```

An omitted result type means `unit`.

---

## Callable result syntax

Callable result types use `->`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The `:` syntax annotates the declared thing before it.

The `->` syntax declares the result type of a callable.

```text
:    type annotation for the declared thing before it
->   result type of a callable
```

---

## Function bodies

A function body is a block expression in callable-body context.

```bray
func f() -> i32
{
    return 1;
}
```

Callable-body context gives the block expression a callable execution scope.

The callable execution scope determines the target of `return`.

Callable result values are supplied through explicit callable exits.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

A function whose declared result type is `unit` can complete normally.

```bray
func log(pos message: string)
{
    print(message);
}
```

For a function with a declared result type other than `unit`, every reachable normal completion path supplies the callable result
through `return` or ends in a `never` expression.

---

## Semicolons

Semicolons are mandatory for sequenced expressions.

```bray
func f()
{
    log("hello");
    log("world");
}
```

A `return` expression is a sequenced expression and is terminated with a semicolon.

```bray
func f() -> i32
{
    return 1;
}
```

---

## Parameters

A parameter is a binding declared by a function signature.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

Here `left` and `right` are parameters.

A parameter has:

1. a name,
2. a type,
3. an ownership or borrowing mode,
4. a mutation capability,
5. a call-position permission,
6. a lifetime and capability contract.

The parameter binding exists inside the function body.

---

## Positional parameters

Parameters are named by default.

A parameter can use the `pos` modifier to permit positional arguments at call sites.

```bray
func print(pos message: string)
{
    ...
}

func fit(pos data: DataFrame, max_iterations: i32)
{
    ...
}
```

The `pos` modifier changes the call surface only.

The parameter still has a name, and the function body uses that name.

A `pos` parameter can be supplied positionally or by name.

```bray
print("hello");
fit(data_frame, max_iterations = 10);
fit(data = data_frame, max_iterations = 10);
```

A parameter without `pos` must be supplied by name unless it is omitted through a default.

```bray
fit(data_frame, 10); // invalid
```

A `pos` parameter may only appear before any non-`pos` parameter in the same parameter list.

```bray
func valid(pos first: A, pos second: B, third: C);

func invalid(first: A, pos second: B); // invalid
```

Parameter modifiers apply independently to the parameter.

---

## Owned parameters

A parameter of plain type receives an owned value.

```bray
func consume(pos buffer: Buffer)
{
    ...
}
```

The argument is moved into the function unless the argument type is copyable or another explicit passing rule applies.

Inside the function, the parameter binding owns the value.

---

## Immutable owned parameters

Owned parameters are immutable by default.

```bray
func inspect(pos buffer: Buffer)
{
    ...
}
```

The function owns `buffer`, and the local binding has immutable access authority.

Ownership and mutation authority are separate capabilities.

---

## Mutable owned parameters

A mutable owned parameter uses `mut` before the parameter name.

```bray
func normalize(pos mut buffer: Buffer) -> Buffer
{
    ...
    return buffer;
}
```

This means the function receives ownership of the argument and the local parameter binding has mutation authority over that owned
value, subject to the type’s field and representation rules.

The `mut` before the parameter name applies to the local owned binding.

The caller’s binding keeps its own declared capability rules.

The grammar accepts parameter modifiers before the parameter name.

---

## Shared borrow parameters

A shared borrow parameter uses `&T`.

```bray
func read(pos buffer: &Buffer)
{
    ...
}
```

The Borrow type forms section of the Type Model defines shared-borrow observation, aliasing, lifetime, and capability rules.

---

## Mutable borrow parameters

A mutable borrow parameter uses `&mut T`.

```bray
func fill(pos buffer: &mut Buffer)
{
    ...
}
```

The Borrow type forms section of the Type Model defines mutable-borrow exclusivity, lifetime, and capability rules.

---

## `mut` placement in parameters

`mut` before a parameter name marks the local owned parameter binding as mutable.

```bray
func normalize(pos mut buffer: Buffer) -> Buffer
{
    ...
}
```

`mut` after `&` marks mutation authority over the storage reached by that borrow layer.

```bray
func fill(pos buffer: &mut Buffer)
{
    ...
}
```

These are different syntactic positions with different meanings.

The grammar accepts parameter modifiers before the parameter name.

The grammar represents borrowed parameter capability through the borrow type itself:

```bray
func read(pos buffer: &Buffer)
{
    ...
}

func fill(pos buffer: &mut Buffer)
{
    ...
}
```

---

## Nested borrow parameter types

Nested borrow type forms can appear in parameter types.

```bray
func f(x: &T)
{
}

func f(x: &mut T)
{
}

func f(x: &&T)
{
}

func f(x: &&mut T)
{
}

func f(x: &mut &T)
{
}

func f(x: &mut &mut T)
{
}
```

The Borrow type forms section of the Type Model defines nested borrow capability and reachable-operation rules.

---

## Function calls

A function call evaluates the callee and arguments according to Bray evaluation order.

```bray
let result = add(left = 1, right = 2);
print("hello");
```

The final evaluation-order rules are part of the expression model.

A call is valid when every argument satisfies the corresponding parameter’s type, ownership, borrowing, mutation, lifetime,
capability, effect, and contract requirements.

---

## Return

`return` exits the current callable execution scope.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

The returned expression must be compatible with the callable’s declared result type.

The `return` expression itself has type `never` because control exits the callable execution scope.

`return` always targets the nearest enclosing callable execution scope.

---

## Callable execution scopes

A callable execution scope is created by the block expression body of a callable program element.

Functions create callable execution scopes.

Lambdas and async functions also create callable execution scopes.

`return` exits the nearest callable execution scope.

```bray
func outer() -> i32
{
    let inner = lambda () -> i32
    {
        return 1;
    };

    inner();

    return 2;
}
```

The first `return` exits `inner`.

The second `return` exits `outer`.

---

## Return and `unit`

A function returning `unit` can complete normally.

```bray
func log(pos message: string)
{
    print(message);
}
```

A function returning `unit` can also return explicitly with `return unit;`.

```bray
func log(pos message: string)
{
    print(message);
    return unit;
}
```

`return;` is shorthand for `return unit;`.

---

## Return and `never`

A `never` expression satisfies any required result type at a control-flow merge because it has no normal continuation.

At a control-flow merge, `never` contributes no value and does not determine the merged result type.

```bray
func fail(pos message: string) -> never
{
    panic(message);
}
```

A function declared to return `never` has no normal completion path.

Panic is outside the ordinary callable result contract.

If a callable panics, the panic propagates to the nearest panic-catching boundary instead of producing the callable's declared result.

A `catch` expression creates an expression-level panic-catching boundary.

The canonical never-producing expression forms are defined by the Expression Model.

---

## Callable result exits

Callable result values are supplied explicitly through `return`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

This keeps callable exits visible in the source.

---

## Yield and return

`yield` supplies a value to the nearest enclosing yield-capable region.

`yield;` is shorthand for `yield unit;`.

`return` exits the nearest callable execution scope.

```bray
func f() -> i32
{
    let x: i32 =
    {
        yield 1;
    };

    return x;
}
```

The `yield` supplies the value of the value-producing block expression.

The `return` exits the function.

---

## Block expression results and callable results

Block expression results and callable results are related but distinct semantic layers.

A value-producing block expression receives its value through `yield`.

```bray
let x: i32 =
{
    yield 1;
};
```

A callable receives its result through `return`.

```bray
func f() -> i32
{
    return 1;
}
```

`yield` targets the nearest yield-capable region.

`return` targets the nearest callable execution scope.

A function body is a block expression, and its callable result is governed by callable-body rules.

---

## Function types

Callable types use `func(...) -> ...`.

```bray
let op: func(left: i32, right: i32) -> i32 = add;
```

Tuple types use tuple syntax:

```bray
(T1, T2)
```

Function types use callable syntax:

```bray
func(left: T1, right: T2) -> R
```

Example:

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}

let op: func(left: i32, right: i32) -> i32 = add;
```

---

## Function values

A function can be used as a value when its callable contract matches the expected function type.

```bray
let op: func(left: i32, right: i32) -> i32 = add;
```

A function value carries its full callable contract.

The callable contract includes:

1. parameter names,
2. parameter call-position permissions,
3. parameter types,
4. result type,
5. execution mode,
6. ownership behavior,
7. borrowing behavior,
8. mutation requirements,
9. lifetime requirements,
10. capability requirements,
11. caller-visible effects,
12. trusted caller obligations,
13. finalization behavior.

A function assignment succeeds when the target callable type preserves the callable contract required by the function value.

---

## Higher-order functions

A function can accept another function as a parameter.

```bray
func apply(pos value: i32, op: func(pos value: i32) -> i32) -> i32
{
    return op(value);
}
```

The function parameter type describes the callable contract required by `apply`.

The body of `apply` can use the capabilities guaranteed by that callable type.

A callable with trusted caller obligations, async execution mode, mutating requirements, or additional caller-visible effects can be passed
when the parameter type includes those obligations.

---

## Callable contract preservation

Callable contracts are preserved through assignment, named callable contracts, wrappers, generic parameters, dynamic dispatch,
exports, and re-exports.

A callable assignment succeeds when the target callable type preserves every caller-visible obligation of the source callable.

Example ordinary callable type:

```bray
let f: func(buffer: &Buffer, index: usize) -> u8 = get_checked;
```

Callable types write parameter names.

Callable types that permit positional arguments use the same `pos` parameter modifier as callable declarations.

```bray
let op: func(pos value: i32) -> i32 = double;
```

A callable with trusted caller obligations requires a callable type that carries those obligations.

A callable type uses the callable signature grammar without a body.

```bray
func(left: i32, right: i32) -> i32
func(pos value: i32) -> i32
async func(pos request: Request) -> Response
trusted func(pos bytes: &mut [u8])
    uses(raw_memory)
```

The parameter list uses the same parameter grammar as callable declarations.

The result type can be omitted when the result is `unit`.

Callable modifiers that are visible in a callable contract are written before `func`.

Contract clauses attach after the callable signature.

```bray
func(pos value: i32) -> i32
    requires(value >= 0)
```

Contract clauses on callable types use the same predicate-expression syntax as contract clauses on callable declarations.

The callable type must preserve every caller-visible obligation of the callable value assigned to it.

### Named callable contracts

A named callable contract declaration gives a reusable name to a callable type form.

```bray
callable Transform =
    func(pos value: i32) -> i32;
```

A named callable contract declaration has no body and creates no callable value.

The declared name can be used in type positions that expect a callable type.

```bray
func double(pos value: i32) -> i32
{
    return value + value;
}

let transform: Transform = double;
```

A named callable contract can be generic.

```bray
callable Mapper<T, U> =
    func(pos value: T) -> U;

callable ArrayConsumer<T, const N: usize> =
    func(pos items: &[T; N]) -> unit;
```

A generic callable contract can have `with(...)` constraints.

```bray
callable OrderedPredicate<T>
    with(T: Comparable<T>) =
    func(pos left: &T, pos right: &T) -> bool;
```

The `with(...)` clause uses static predicate expressions.

The callable type on the right-hand side can reference the callable declaration's generic parameters and constraints.

A named callable contract is not a general type alias.

It can name only callable type forms.

It does not rename a type, function, method, lambda, implementation, module, or package.

It does not create a wrapper value or adapter.

A callable value satisfies a named callable contract when its visible callable contract satisfies the named contract.

Capture state is not written in the named callable contract.

Captured ownership, borrows, capabilities, effects, finalization obligations, and lifetimes must still be preserved by the callable
value that satisfies the contract.

Named callable contracts cannot be overloaded.

There can be at most one visible callable contract declaration for a given name in a name/coherence domain.

---

## Function overloading

Function overloading is explicit.

Same-name function declarations do not automatically form an overload set.

An overload declaration introduces a shared call name over separately named callable declarations.

```bray
func parse_int(pos text: string, radix: i32 = 10) -> i64
{
    ...
}

func parse_float(pos text: string) -> r64
{
    ...
}

overload parse =
{
    parse_int,
    parse_float,
}
```

The overload name is the shared call surface.

Each overload arm keeps its own declaration name.

The separately named arm can still be referenced and called directly.

```bray
let value = parse_int(input);
```

Direct calls to an arm use that arm’s ordinary callable rules, including default arguments.

Calls through the overload name use overload selection.

```bray
let value = parse(input);
```

Overload selection uses only arguments explicitly supplied by the caller.

Default arguments do not participate in overload selection.

A defaulted parameter does not make an overload arm selectable when that parameter is omitted.

After a single overload arm has been selected, the call is checked as a call to that selected arm.

Result type does not participate in overload selection.

Expected type does not participate in overload selection.

Overload resolution does not rank candidates.

A call through an overload name must resolve to exactly one overload arm.

If no overload arm matches, the call is rejected.

If more than one overload arm matches, the call is rejected as ambiguous.

For the overload above:

```bray
parse(input)
```

selects `parse_float`.

```bray
parse(input, radix = 10)
```

selects `parse_int`.

The direct call remains valid:

```bray
parse_int(input)
```

An overload arm matches a call only when:

- every supplied named argument exists as a parameter of the arm,
- every supplied positional argument corresponds to a `pos` parameter at the same position in the arm,
- no parameter is supplied more than once,
- every parameter of the arm is supplied explicitly by the call,
- each supplied argument expression is compatible with the corresponding parameter type,
- the callable’s ownership, borrowing, capability, effect, trusted obligation, and contract requirements can be satisfied.

The receiver of a method is supplied by method-call syntax and is not a named argument.

For method overloads, receiver mode and receiver compatibility participate in overload selection.

Imported declarations participate in overload resolution only through visible overload declarations and deterministic lookup.

Ambiguous polymorphism is rejected.

---

## Generic functions

Generic functions are parameterized by type parameters, const parameters, or both.

The generic parameter list is written after the function name and before the function parameter list.

```bray
func identity<T>(pos value: T) -> T
{
    return value;
}

func element_count<T, const N: usize>(pos items: &[T; N]) -> usize
{
    return N;
}
```

Bare generic parameter names declare type parameters.

Const parameters are declared with `const Name: Type`.

Generic function parameter lists contain type parameters and const parameters.

Capability requirements and caller-visible effects are represented by the ordinary callable surface and contract clauses.

Generic function calls supply generic arguments explicitly.

```bray
let value = identity<i32>(10);
let count = element_count<u8, 4>(&bytes);
```

Generic arguments are not inferred from ordinary arguments, expected result type, assignment target type, return type, or
constraints.

A generic function body is checked against its declared constraints.

Generic function constraints are written with `with(...)` clauses.

`with(...)` clauses contain static predicate expressions.

Generic code can use the operations, ownership behavior, effects, capabilities, and contracts guaranteed by its constraints.

Generic instantiation must satisfy the generic function’s full callable contract.

---

## Contract clauses on functions

Functions can have contract clauses.

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

`requires(...)` declares preconditions.

`ensures(...)` declares postconditions.

Postconditions can reference the compiler-introduced `result` binding for the declaration's normal completion value.

Bray does not support named result bindings.

Contract clauses are parenthesized comma-separated lists.

The detailed rules for predicates, predicate expressions, fact contexts, trusted obligations, and contract clauses are part
of the Contract and Trust Model.

---

## Trusted functions

Trusted functions use the `trusted` modifier and a `uses(...)` clause.

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

`uses(...)` declares trusted implementation capabilities used by the body.

A trusted implementation can expose an ordinary safe API.

A trusted caller obligation appears in the public function contract.

Trusted implementation capabilities are available only inside the trusted declaration body.

Trusted caller obligations must be visible in the callable contract and satisfied by the caller.

---

## Async functions

A function can be asynchronous.

The `async` keyword appears before function declarations and lifecycle declarations whose lifecycle kind permits asynchronous
execution.

For lifecycle declarations:

```bray
async finalize() -> Result<unit, FileError>
{
    ...
}
```

For ordinary functions:

```bray
async func fetch(pos url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Calling an async function creates an async computation.

The async computation is an owned value representing suspendable execution.

Awaiting the computation drives it to completion.

Async computation ownership, captured state, await behavior, async block expressions, spawning, detached spawning, task handles,
task-obligation checking, transfer, escape, and cancellation rules are defined by the Async Model.

---

## Async computation ownership

Async computations are owned values.

Calling an async function creates an async computation value.

That value owns or borrows the state captured by the async function according to the function's signature, body, suspension
points, and lifetime contract.

Suspension captures live values, borrows, capabilities, effects, and finalization obligations into the async computation's
contract.

The Async Model defines the full ownership, borrowing, movement, escape, and cancellation rules for async computations.

---

## Await

`await` drives an async computation to completion.

```bray
await expression
```

Awaiting produces the async function’s declared result when the computation completes successfully.

Awaiting an async computation directly is not a run-boundary observation. Panic from the async computation propagates to the awaiting
execution flow unless a panic-catching boundary handles it.

Applying `catch` to a spawned task join observes a run boundary and produces `RunResult<T>`, where `T` is the spawned
computation's declared result type.

Awaiting must satisfy the computation’s ownership, borrowing, capability, effect, cancellation, and finalization obligations.

Await expression syntax and semantics are defined by the Async Model.

---

## Async cancellation

Destroying an incomplete async computation cancels it.

Cancellation destroys owned state and releases capabilities according to the async computation’s contract.

Ordinary destruction remains synchronous.

Async finalization obligations must be completed through asynchronous execution, transferred, or converted into an explicit
fallback ownership form before the owning scope exits.

Async cancellation is defined by the Async Model.

---

## Function effects and capabilities

Function signatures include effects and capability contracts.

A function can use only the capabilities available through its parameters, local bindings, including pattern-introduced bindings,
generic constraints, execution mode, lifecycle state, trusted declarations, and surrounding context.

An effect is a semantic property of evaluating, driving, or resolving a callable that matters to call checking, context validity,
generic satisfaction, dynamic dispatch, or public API compatibility.

The caller-visible effect surface is:

- `const`,
- `async`,
- receiver, parameter, and capture modes,
- `requires(...)`,
- `ensures(...)`,
- `with(...)`,
- trusted `uses(...)`,
- lifecycle contracts,
- task and async-computation contracts,
- parameter, result, and captured-value type contracts.

Effects and capability contracts are part of:

- function signatures,
- function types,
- call checking,
- behavioral contract satisfaction,
- generic constraints,
- dynamic dispatch,
- public API compatibility.

The compiler computes a body effect summary for each callable body.

The body effect summary is derived from:

- selected callable contracts of calls and method calls,
- selected lifecycle declaration contracts,
- construction, destruction, finalization, and resource-scope behavior,
- assignments and mutation,
- allocation and deallocation,
- I/O,
- async computation creation, `await`, `spawn`, task joins, and task cancellation,
- panic-catching boundaries,
- trusted capability use.

The computed body effect summary must be valid for the callable's declaration surface and surrounding context.

Constant-evaluation eligibility is declared with the `const` function modifier.

`requires(...)` declares caller obligations and preconditions.

`ensures(...)` declares established facts after normal completion.

`uses(...)` declares trusted implementation capabilities used by a trusted callable body.

A trusted callable's `uses(...)` clause must exactly describe the trusted implementation capabilities used by its body.

Trusted implementation capabilities cover low-level operations named by the trust model.

Ordinary safe allocation, ordinary I/O, and ordinary mutation are checked as body effects and as caller-visible effects when they
cross the callable boundary or are constrained by the surrounding context.

### Mutation effects

Mutation authority is represented by receiver, parameter, and capture modes.

Caller-reachable mutation occurs when a callable can mutate storage reachable by the caller before or after the call.

Caller-reachable mutation must be visible through one of:

- a mutable receiver mode,
- a mutable borrow parameter,
- an owned parameter consumed by the callable,
- a mutable capture,
- a global or module storage contract,
- a trusted raw-memory contract,
- a type, lifecycle, or trait contract that exposes mutation authority.

Internal mutation occurs when a callable mutates storage that is created inside the callable or owned exclusively by the callable
and is not reachable by the caller except through the callable's returned value.

Internal mutation is still a runtime effect.

Constant-evaluation context, predicate-expression context, static constraint context, and other effect-free contexts require
callables without internal mutation.

Internal mutation is represented in callable types only when it creates a caller-visible requirement through the ordinary callable
surface.

### Allocation and I/O effects

Allocation creates runtime storage or asks a storage policy to create runtime storage.

Deallocation releases runtime storage.

Safe allocation and safe deallocation are ordinary runtime effects.

Low-level allocation, raw allocation facts, and allocator manipulation require trusted capabilities such as `manual_alloc` or
`raw_memory` when the selected operation's contract names those capabilities.

Constant-evaluation context, predicate-expression context, static constraint context, and other allocation-free contexts require
callables without allocation or deallocation.

Allocation and deallocation become caller-visible when the callable's signature, type contracts, lifecycle contracts, or result
obligations require the caller to preserve, destroy, finalize, join, cancel, or otherwise resolve storage produced by the call.

I/O is any interaction with external state outside the Bray abstract machine, including files, terminals, network connections,
devices, clocks, environment state, randomness, and foreign callbacks with externally visible behavior.

I/O enters Bray through compiler-known, standard-library, foreign, or user declarations whose contracts describe the resource,
capability, ownership, borrowing, finalization, and panic behavior involved.

Constant-evaluation context, predicate-expression context, static constraint context, and other effect-free contexts require
callables without I/O.

I/O becomes caller-visible when the callable's signature or contracts require an I/O resource, return an I/O resource, mutate an
I/O resource, transfer an I/O obligation, or state facts about external behavior.

### Cancellation effects

Cancellation is an async and run-boundary effect.

An `async` callable type carries suspendable execution and cancellation participation.

Calling an async function creates an async computation whose cancellation behavior is governed by the Async Model.

Awaiting an async computation, spawning it as a task, joining a task, cancelling a task, and observing a run boundary must satisfy
the async computation's ownership, borrowing, capability, effect, finalization, and cancellation obligations.

A synchronous callable cancels a task through ownership of a task handle or another value whose contract gives cancellation
authority.

That authority is represented by the parameter, receiver, capture, or field type that carries the task obligation.

Task cancellation is represented through `async`, task-handle ownership, and the contracts of values that carry cancellation
authority.

### Effects in callable types

A callable type includes every caller-visible contract clause needed to call a value of that type.

Two callable declarations with the same parameter and result shape but incompatible caller-visible contracts have different
callable types.

Callable types represent caller-visible effects through the ordinary callable type surface:

- `const` for constant-evaluation eligibility,
- `async` for suspendable execution and cancellation participation,
- receiver and parameter modes for ownership, borrowing, movement, and mutation requirements,
- trusted `uses(...)` for trusted implementation capability envelopes that must be preserved,
- contract clauses for preconditions, postconditions, static constraints, trusted caller obligations, facts, and resource obligations,
- parameter and result types for task handles, async computations, storage obligations, lifecycle obligations, and resource
  ownership.

Callable type assignment, trait implementation checking, dynamic dispatch, and public API compatibility preserve caller-visible
effects.

An ordinary runtime callable that allocates internally, mutates internal temporary storage, or performs internal safe bookkeeping can
match an ordinary runtime callable type when those effects remain internal and impose no caller-visible obligations.

A callable can match a callable type or context when its body effect summary is valid for every effect requirement of that type or
context.

After overload resolution selects exactly one overload arm by the supplied arguments, contract checking verifies that the
caller's context satisfies the selected arm's caller-visible obligations.

Overload resolution does not rank or distinguish overloads by effect or capability contracts.

If multiple overloads remain applicable after argument matching, the call is ambiguous.

---

## Parameter evaluation and ownership transfer

At a call site, each argument is checked against its corresponding parameter.

Depending on the parameter, the argument may be:

- observed,
- borrowed,
- mutably borrowed,
- moved,
- copied,
- consumed,
- used to establish a contract fact.

Owned parameters take ownership of the argument value unless the value is copyable or another explicit rule applies.

Borrow parameters create borrow access paths.

Mutable borrow parameters require mutation authority.

Consumed values become unavailable through their old access paths unless reinitialized.

---

## Function visibility

Function declarations can be public or internal.

Public is the default.

```bray
public func exported() -> i32
{
    return 1;
}

internal func helper() -> i32
{
    return 2;
}
```

Since `public` is the default, this is equivalent to `public func`:

```bray
func exported() -> i32
{
    return 1;
}
```

Use of internal functions outside their intended scope requires explicit acknowledgement.

```bray
let x = internal some.module.helper();
```

A module can acknowledge internal use through `using internal`.

```bray
using internal some.module.helper;
```

`using internal` acknowledges specific internal modules, declarations, or declaration paths.

`using internal` applies to specific modules, declarations, or declaration paths rather than entire packages.

---

## Module paths and function calls

Bray uses `.` for module paths, package paths, type paths, member access, and nested access.

```bray
math.sin(x);
pkg.module.function(value = x);
pkg.module.Type;
```

The binder resolves whether the left side is a module, package, type, value, or access path.

A referenced external path must either be declared by a `using` declaration or be reachable through the current package or
module context.

---

## Local callable values

Function declarations are declaration forms, not block-expression items.

They do not appear inside callable-body block expressions or ordinary block expressions.

Named reusable behavior belongs at module scope, type scope, implementation scope, or trait scope.

Local callable behavior is expressed with a lambda value bound to a local binding.

```bray
func outer() -> i32
{
    let inner = lambda () -> i32
    {
        return 1;
    };

    return inner();
}
```

A lambda introduces its own callable execution scope.

A `return` inside the lambda exits the lambda.

Block expressions support local binding declarations.

They do not support nested named function declarations, nested type declarations, nested trait declarations, nested implementation
declarations, nested module declarations, or nested package declarations.

Lambda capture uses ordinary lexical capture rules.

---

## Lambda expressions and anonymous callables

An anonymous callable expression uses `lambda`.

```bray
let increment = lambda (pos value: i32) -> i32
{
    return value + 1;
};
```

A lambda has the same callable shape as a function declaration, but has no binding name.

```bray
lambda (parameters) -> Result
{
    ...
}
```

Parameters use the same parameter grammar as functions.

The result type is optional. An omitted result type means `unit`.

A lambda body is a callable-body block and creates its own callable execution scope.

`return` exits the lambda's callable execution scope.

Lambdas have no receiver.

Inside a method body, `self` is available inside a lambda body only as captured state from the enclosing method body.

Receiver-mode modifiers such as `mut` and `consume` do not apply to `lambda`.

Trusted, asynchronous, and contract clauses compose with lambda syntax according to their ordinary callable rules.

```bray
async lambda (pos request: Request) -> Response
{
    return await handle(request);
}

trusted lambda (pos bytes: &mut [u8])
    uses(raw_memory)
{
    ...
}
```

Captures use ordinary lexical bindings.

```bray
let f =
{
    let source = &buffer;
    let captured_limit = limit;

    lambda () -> usize
    {
        return source.count() + captured_limit;
    }
};
```

A lambda body can reference:

- lambda parameters,
- local bindings introduced inside the lambda body,
- declarations visible from the declaration context,
- local bindings visible from enclosing lexical scopes.

An enclosing local binding referenced by the lambda body becomes captured state of the callable value.

A lambda that references no enclosing local bindings is capture-free.

A lambda that references one or more enclosing local bindings is capture-bearing.

Captured names are available inside the lambda body under the same name.

The captured binding must support the way it is captured.

An owned copyable binding is copied into the callable value.

An owned non-copyable binding is moved into the callable value.

A shared borrow binding is copied into the callable value.

A mutable borrow binding is moved into the callable value.

A captured shared borrow requires the reached access path to be observable for the lifetime of the lambda value.

A captured mutable borrow requires exclusive mutation authority for the lifetime of the lambda value.

After a moved capture, the old access path is unavailable until reinitialized.

Captured state is formed in the order the captured binding declarations were evaluated.

Captured ownership, borrows, mutation authority, effects, and finalization obligations are part of the callable value's contract.

To select a specific capture mode, introduce an ordinary local binding before the lambda expression.

```bray
let f =
{
    let target = &mut buffer;

    lambda ()
    {
        target.clear();
    }
};
```

Method paths do not implicitly produce bound-method values.

```bray
let f = buffer.clear; // invalid
```

Use a lambda with an ordinary lexical binding instead.

```bray
let f =
{
    let target = &mut buffer;

    lambda ()
    {
        target.clear();
    }
};
```

---

## Methods

A method is a callable associated with a type or behavioral contract.

Method calls use `.`:

```bray
value.method(argument);
```

A method has a function-like callable contract.

The receiver is supplied by method-call syntax.

The receiver is not written as an ordinary parameter.

Inside a method body, `self` is the compiler-introduced receiver binding.

`self` cannot be declared as an ordinary parameter, local binding, or pattern binding.

The type name `Self` can still be used in ordinary parameter, result, local binding, and field types wherever `Self` is in scope.

```bray
func distance_to(pos other: &Self) -> r64
{
    ...
}
```

Method declaration syntax is determined by receiver mode.

```bray
func length() -> usize;

mut func clear();

consume func into_bytes() -> Bytes;

consume mut func normalize() -> Self;
```

`func` declares a method with a shared receiver.

Inside a shared receiver method, `self` is an observable receiver access path.

`mut func` declares a method with a mutable receiver.

Inside a mutable receiver method, `self` is an exclusive mutable receiver access path.

`consume func` declares a method that consumes the receiver.

Inside a consuming receiver method, `self` is an owned receiver value.

`consume mut func` declares a method that consumes the receiver and gives the method body mutable local authority over `self`.

Receiver mode is part of the method's callable contract.

Receiver mode participates in method call checking and method overload selection.

Const, trusted, asynchronous, generic, and contract clauses compose with receiver-mode syntax according to their ordinary declaration
rules.

```bray
trusted mut func reserve(pos count: usize)
    uses(manual_alloc);
```

A static function has no receiver.

`self` is unavailable inside a static function body.

---

## Design principles

Functions use `func`.

Callable result types use `->`.

An omitted callable result type means `unit`.

Function bodies are block expressions in callable-body context.

Callable result values are supplied explicitly through `return`.

`return` exits the nearest callable execution scope.

`yield` supplies values to yield-capable regions.

Parameters are bindings declared by function signatures.

Owned parameters are immutable by default.

Parameters are named by default.

The `pos` parameter modifier permits positional arguments for that parameter.

`pos` parameters form an initial run in the parameter list.

`mut` before a parameter name applies to an owned local parameter binding.

`mut` after `&` applies to the storage reached by that borrow layer.

Function types use `func(...) -> ...`.

Function values carry their full callable contract.

Higher-order functions preserve caller obligations.

Async functions produce owned async computations.

Trusted implementation power and trusted caller obligations are distinct.

Function contracts integrate with the Contract and Trust Model.
