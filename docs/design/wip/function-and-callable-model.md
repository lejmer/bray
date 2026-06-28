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
9. optional contract clauses.

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

## Omitted result type

If the result type is omitted, the function returns `unit`.

```bray
func log(message: String)
{
    print(message = message);
}
```

This means:

```bray
func log(message: String) -> unit
{
    print(message = message);
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
func log(message: String)
{
    print(message = message);
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
    log(message = "hello");
    log(message = "world");
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
5. a lifetime and capability contract.

The parameter binding exists inside the function body.

---

## Owned parameters

A parameter of plain type receives an owned value.

```bray
func consume(buffer: Buffer)
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
func inspect(buffer: Buffer)
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
func normalize(mut buffer: Buffer) -> Buffer
{
    ...
    return buffer;
}
```

This means the function receives ownership of the argument and the local parameter binding has mutation authority over that owned
value, subject to the type’s field and representation rules.

The `mut` before the parameter name applies to the local owned binding.

The caller’s binding keeps its own declared capability rules.

The grammar accepts `mut <name>: T` for owned parameter types.

---

## Shared borrow parameters

A shared borrow parameter uses `&T`.

```bray
func read(buffer: &Buffer)
{
    ...
}
```

A shared borrow allows observation through the borrow.

Multiple compatible shared borrows can exist at the same time.

---

## Mutable borrow parameters

A mutable borrow parameter uses `&mut T`.

```bray
func fill(buffer: &mut Buffer)
{
    ...
}
```

A mutable borrow grants temporary exclusive mutation authority over the storage reached by that borrow layer.

While the mutable borrow is active, incompatible access paths are suspended.

---

## `mut` placement in parameters

`mut` before a parameter name marks the local owned parameter binding as mutable.

```bray
func normalize(mut buffer: Buffer) -> Buffer
{
    ...
}
```

`mut` after `&` marks mutation authority over the storage reached by that borrow layer.

```bray
func fill(buffer: &mut Buffer)
{
    ...
}
```

These are different syntactic positions with different meanings.

The grammar accepts `mut <name>: T` for owned parameter types.

The grammar represents borrowed parameter capability through the borrow type itself:

```bray
func read(buffer: &Buffer)
{
    ...
}

func fill(buffer: &mut Buffer)
{
    ...
}
```

---

## Nested borrow parameter types

Nested borrow types are allowed.

Each borrow layer has its own capability.

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

The reachable operation depends on the whole access path, including every borrow layer.

An outer shared borrow provides shared access to the next layer.

An outer mutable borrow provides mutation authority over the next layer.

The innermost type alone does not determine the available operation.

---

## Function calls

A function call evaluates the callee and arguments according to Bray evaluation order.

```bray
let result = add(left = 1, right = 2);
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

`return` always targets the nearest enclosing callable execution scope.

---

## Callable execution scopes

A callable execution scope is created by the block expression body of a callable program element.

Functions create callable execution scopes.

Later callable forms such as local functions, anonymous functions, closures, and async functions also create callable
execution scopes.

`return` exits the nearest callable execution scope.

```bray
func outer() -> i32
{
    func inner() -> i32
    {
        return 1;
    }

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
func log(message: String)
{
    print(message = message);
}
```

TODO: Define whether functions that return `unit` may use explicit unit-return syntax.

```bray
func log(message: String)
{
    print(message = message);
    return;
}
```

TODO: Define syntax for explicitly returning `unit`.

---

## Return and `never`

A `never` expression satisfies any required result type at a control-flow merge because it has no normal continuation.

```bray
func fail(message: String) -> never
{
    abort(message = message);
}
```

A function declared to return `never` has no normal completion path.

TODO: Define the exact set of `never`-producing constructs.

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
let op: func(i32, i32) -> i32 = add;
```

Tuple types use tuple syntax:

```bray
(T1, T2)
```

Function types use callable syntax:

```bray
func(T1, T2) -> R
```

Example:

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}

let op: func(i32, i32) -> i32 = add;
```

---

## Function values

A function can be used as a value when its callable contract matches the expected function type.

```bray
let op: func(i32, i32) -> i32 = add;
```

A function value carries its full callable contract.

The callable contract includes:

1. parameter types,
2. result type,
3. execution mode,
4. ownership behavior,
5. borrowing behavior,
6. mutation requirements,
7. lifetime requirements,
8. capability requirements,
9. effects,
10. trusted caller obligations,
11. finalization behavior.

A function assignment succeeds when the target callable type preserves the callable contract required by the function value.

---

## Higher-order functions

A function can accept another function as a parameter.

```bray
func apply(value: i32, op: func(value: i32) -> i32) -> i32
{
    return op(value = value);
}
```

The function parameter type describes the callable contract required by `apply`.

The body of `apply` can use the capabilities guaranteed by that callable type.

A callable with trusted caller obligations, async execution mode, mutating requirements, or additional effects can be passed
when the parameter type includes those obligations.

---

## Callable contract preservation

Callable contracts are preserved through assignment, aliases, wrappers, generic parameters, dynamic dispatch, exports,
and re-exports.

A callable assignment succeeds when the target callable type preserves every caller-visible obligation of the source callable.

Example ordinary callable type:

```bray
let f: func(&Buffer, usize) -> u8 = get_checked;
```

A callable with trusted caller obligations requires a callable type that carries those obligations.

The exact syntax for function types with contract clauses is part of the Contract and Trust Model.

---

## Function overloading

Function overloading is explicit.

Same-name function declarations do not automatically form an overload set.

An overload declaration introduces a shared call name over separately named callable declarations.

```bray
func parse_int(text: String, radix: i32 = 10) -> i64
{
    ...
}

func parse_float(text: String) -> r64
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
let value = parse_int(text = input);
```

Direct calls to an arm use that arm’s ordinary callable rules, including default arguments.

Calls through the overload name use overload selection.

```bray
let value = parse(text = input);
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
parse(text = input)
```

selects `parse_float`.

```bray
parse(text = input, radix = 10)
```

selects `parse_int`.

The direct call remains valid:

```bray
parse_int(text = input)
```

An overload arm matches a call only when:

- every supplied argument name exists as a parameter of the arm,
- every parameter of the arm is supplied by the call,
- each supplied argument expression is compatible with the corresponding parameter type,
- the callable’s ownership, borrowing, capability, effect, trusted obligation, and contract requirements can be satisfied.

The receiver of a method is supplied by method-call syntax and is not a named argument.

For method overloads, receiver mode and receiver compatibility participate in overload selection.

Imported declarations participate in overload resolution only through visible overload declarations and deterministic lookup.

Ambiguous polymorphism is rejected.

---

## Generic functions

Generic functions are parameterized by types, constants, capabilities, effects, lifetimes, or other generic parameters.

TODO: Define generic function syntax.

A generic function body is checked against its declared constraints.

Generic code can use the operations, ownership behavior, effects, capabilities, and contracts guaranteed by its constraints.

Generic instantiation must satisfy the generic function’s full callable contract.

---

## Contract clauses on functions

Functions can have contract clauses.

```bray
func clamp(value: i32, min: i32, max: i32) -> i32
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

Contract clauses are parenthesized comma-separated lists.

The detailed rules for predicates, predicate expressions, fact contexts, trusted obligations, and contract clauses are part
of the Contract and Trust Model.

---

## Trusted functions

Trusted functions use the `trusted` modifier and a `uses(...)` clause.

```bray
trusted func copy_bytes(
    destination: &mut RawBuffer,
    source: &RawBuffer,
    count: usize,
) uses(raw_memory)
{
    ...
}
```

`uses(...)` declares trusted implementation capabilities used by the body.

A trusted implementation can expose an ordinary safe API.

A trusted caller obligation appears in the public function contract.

The detailed rules are part of the Contract and Trust Model.

---

## Async functions

A function can be asynchronous.

The exact placement of the `async` keyword is settled for lifecycle declarations as:

```bray
async finalize File() -> Result<unit, FileError>
{
    ...
}
```

For ordinary functions, the parallel syntax is:

```bray
async func fetch(url: Url) -> Result<Response, FetchError>
{
    ...
}
```

Calling an async function creates an async computation.

The async computation is an owned value representing suspendable execution.

Awaiting the computation drives it to completion.

---

## Async computation ownership

An async computation is an owned value.

Calling an async function creates an async computation value.

That value owns or borrows the state captured by the async function according to the function’s signature, body, suspension
points, and lifetime contract.

Suspension captures live values, borrows, capabilities, effects, and finalization obligations into the async computation’s
contract.

---

## Await

`await` drives an async computation to completion.

TODO: Define await syntax.

Semantic form:

```text
await <async-computation>
```

Awaiting produces the async function’s declared result when the computation completes successfully.

Awaiting must satisfy the computation’s ownership, borrowing, capability, effect, cancellation, and finalization obligations.

---

## Async cancellation

Destroying an incomplete async computation cancels it.

Cancellation destroys owned state and releases capabilities according to the async computation’s contract.

Ordinary destruction remains synchronous.

Async finalization obligations must be completed through asynchronous execution, transferred, or converted into an explicit
fallback ownership form before the owning scope exits.

---

## Function effects and capabilities

Function signatures include effects and capability contracts.

A function can use only the capabilities available through its parameters, local bindings, including pattern-introduced bindings,
generic constraints, execution mode, lifecycle state, trusted declarations, and surrounding context.

Effects and capability contracts are part of:

- function signatures,
- function types,
- overload resolution,
- behavioral contract satisfaction,
- generic constraints,
- dynamic dispatch,
- public API compatibility.

TODO: Define syntax for general effect and capability annotations beyond currently discussed clauses.

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

`using internal` acknowledges specific internal declarations or declaration paths.

`using internal` applies to declarations or declaration paths rather than entire modules or packages.

---

## Module paths and function calls

Bray uses `.` for module paths, package paths, type paths, member access, and nested access.

```bray
math.sin(angle = x);
pkg.module.function(value = x);
pkg.module.Type;
```

The binder resolves whether the left side is a module, package, type, value, or access path.

A referenced external path must either be declared by a `using` declaration or be reachable through the current package or
module context.

---

## Local functions

TODO: Define whether function declarations can exist inside callable bodies and whether local declarations are supported in block expressions.

```bray
func outer() -> i32
{
    func inner() -> i32
    {
        return 1;
    }

    return inner();
}
```

A local function introduces its own callable execution scope.

A `return` inside the local function exits the local function.

TODO: Define capture rules for local functions and their relationship to closure rules.

---

## Closures and anonymous functions

TODO: Define anonymous callable syntax.

TODO: Define closure capture rules.

Anonymous callable values obey the same callable model:

- parameters have declared types and capabilities,
- result type is explicit or `unit` by omission when the chosen syntax permits omission,
- captures are explicit or governed by a clear capture rule,
- captured ownership, borrows, mutation authority, effects, and finalization obligations are part of the callable value’s
  contract,
- `return` exits the anonymous callable’s own callable execution scope.

---

## Methods

TODO: Define method declaration syntax.

A method is a callable associated with a type or behavioral contract.

Method calls use `.`:

```bray
value.method(argument = argument);
```

A method has a function-like callable contract.

The receiver is a parameter with ownership, borrowing, mutation, capability, and lifetime behavior.

TODO: Define receiver syntax.

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

`mut` before a parameter name applies to an owned local parameter binding.

`mut` after `&` applies to the storage reached by that borrow layer.

Function types use `func(...) -> ...`.

Function values carry their full callable contract.

Higher-order functions preserve caller obligations.

Async functions produce owned async computations.

Trusted implementation power and trusted caller obligations are distinct.

Function contracts integrate with the Contract and Trust Model.
