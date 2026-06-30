# Core language primitives

## Values, storage, and access paths

A **value** is the semantic thing a program computes, owns, moves, copies, borrows, observes, mutates, or destroys.

**Storage** is where a value can live. Storage has identity for the purposes of ownership, borrowing, mutation, aliasing,
initialization, movement, and destruction.

An **access path** is a way to reach storage. A local binding, parameter, field projection, tuple element projection, index
projection, dereference, temporary, view, or borrow may all be access paths when they reach storage.

Ownership, borrowing, mutation, movement, and destruction are checked over access paths and the storage they reach.
Two access paths may conflict when they can reach the same storage or overlapping storage with incompatible capabilities.

---

## Bindings

A **binding** gives a name to an access path, value, function, type, module, contract, or other declared program entity.

A local value binding introduces an access path to storage. The binding has a declared capability: by default it grants
read-only observation. When explicitly mutable, it grants local mutation authority.

A binding can be introduced directly by a declaration or indirectly by a pattern. A pattern-introduced binding receives its
kind, type, lifetime, capability, initialization state, and ownership story from the pattern operation that created it.

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

## Declarations, expressions, and directives

A **declaration** introduces a named program entity or semantic relationship.

An **expression** produces a value, access path, control-flow outcome, or compile-time entity.

A **directive** is a compile-time instruction that modifies compiler interpretation, checking, diagnostics, target selection,
trusted permissions, or build behavior for a declared scope or program element.

Bray is expression-oriented. Blocks, conditionals, matches, and other control-flow forms produce values when their exits have
coherent type, ownership, initialization, and destruction state.

An expression carries a semantic contract: its type, ownership behavior, capability requirements, and effects.

Directives are part of the source graph and semantic context. They do not execute at runtime and do not produce runtime values.

A block, module, or declaration body contains declarations, expressions, and directives according to Bray's grammar.

Operations such as observation, borrowing, mutation, movement, copying, consumption, construction, and destruction occur through
expressions whose required capabilities are available at that program point.

A nullable propagation expression `expression?` evaluates a nullable expression, produces the contained value on the present path,
and propagates `none` from the nearest nullable propagation boundary on the absent path.

A result propagation expression `try expression` evaluates a `Result<T, E>` or `RunResult<T>` expression, produces the contained
value on the success path, and propagates the non-success outcome to the nearest compatible propagation boundary on the
non-success path.

A panic-catching expression `catch expression` evaluates an expression inside a panic-catching boundary. For ordinary synchronous
code, `catch` produces `Result<T, PanicReport>`. For task or thread observation, `catch` produces `RunResult<T>`.

---

## Patterns

A **pattern** is a structural matching form that can refine a value, bind parts of it, and determine how ownership or access paths
flow into the matched region.

Patterns form their own grammar category. They are checked against a subject type and interpreted by the construct that uses them.

A pattern can introduce bindings, discard parts of a value, match literals, match union variants, decompose product values,
decompose tuples, decompose fixed-size arrays, match nullable state, or match through type forms such as `box`.

```bray
_
value
mut value
0
true
Error
Circle(center = c, radius = r)
{ x, y }
(x, y)
[first, second]
none
?inner
box(inner)
```

Pattern names resolve before they bind.

If a bare identifier in pattern context resolves to a pattern-capable declaration, the pattern uses that declaration.

If it does not resolve to a pattern-capable declaration, it introduces a new binding.

A leading-dot variant pattern remains available as an explicit subject-member shorthand.

```bray
.Empty
.Circle(center = c, radius = r)
```

Product and variant payload patterns match fields by name. Field order does not matter. Field shorthand binds a field to a binding
with the same name. The shorthand binding name is not resolved as a named constant or variant.

```bray
{ x, y }

Circle(center, radius)
```

The `..` pattern explicitly accounts for remaining fields or elements and introduces no bindings.

```bray
{ x, .. }

Circle(radius, ..)

[first, .., last]
```

A pattern is **irrefutable** when it matches every value of its subject type. A pattern is **refutable** when it matches only
some values of its subject type.

Local destructuring and iteration patterns require irrefutable patterns.

Union handling and match expressions can use refutable patterns and perform coverage checking according to the subject type.

For nullable subjects, `none` matches absent state and `?pattern` matches present state before applying `pattern` to the contained value.

Nullable patterns are the ordinary way to prove present state and bind the contained value.

A pattern operation has a mode supplied by the construct using the pattern. The core modes are observe, shared borrow, mutable
borrow, consume, and copy.

The same pattern syntax can bind owned values, copied values, observed access paths, shared borrowed access paths, or mutable
borrowed access paths depending on the operation mode.

A successful union variant pattern refines the subject to that active variant in the matched region. The selected payload exists
in that region, and payload fields are available according to the operation mode.

A consuming pattern can move fields or payloads out of a subject. Moving parts out requires ownership of the subject and no
conflicting active borrows. After a partial move, the subject is partially initialized. Destruction of a partially moved value
destroys only the still-initialized parts.

Successful pattern matching can add facts to the fact context, including active union variant, literal equality, field
availability, payload initialization, tuple or array shape, and narrowed control-flow state.

Pattern matching is structural, deterministic, and effect-free. Guards belong to the surrounding construct and provide additional
boolean checks after structural matching.

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

## Conditional expressions

An `if` expression selects between branches based on a boolean condition.

The condition expression must have boolean type.

Parentheses around the condition are ordinary expression grouping and are therefore not required.

The branch bodies are block expressions, and only the selected branch body is evaluated.

An `if` expression without an `else` branch produces `unit` on the false path.

When an `if` expression is used as a value-producing region, every normal completion path must yield a value compatible with the
expected result type.

An `else` branch is required when the `if` expression must produce a non-`unit` value and the condition can evaluate to false.

`else if` is syntactic nesting of another `if` expression in the `else` branch.

---

## While expressions

A `while` expression repeats a body while a boolean condition remains true.

The condition expression is evaluated before each attempted iteration and must have boolean type.

Parentheses around the condition are ordinary expression grouping and are therefore not required.

The body is evaluated only when the condition evaluates to `true`.

When the condition evaluates to `false`, the else body is evaluated if one is present.

When the condition evaluates to `false` and no else body is present, the `while` expression completes as `unit`.

Reaching the end of the body starts the next condition evaluation.

`break value` exits the `while` expression and supplies the `while` result.

`break;` is shorthand for `break unit;`.

`continue` skips the rest of the current body evaluation and starts the next condition evaluation.

The body belongs to the `while` expression's break-capable region.

The else body belongs to the same break-capable region.

The body and else body do not capture `yield`.

`continue` is not valid in the else body.

If a `while` expression must produce a non-`unit` value, every reachable normal exit path must break with a compatible value. A
reachable false-condition path requires an else body.

---

## For expressions

A `for` expression iterates over a source that provides an iteration contract.

The source expression is evaluated once before iteration begins.

The source expression must provide an iteration contract. The iteration contract defines the element type, element access mode,
iteration order, cardinality when known, whether iteration is finite, and ownership and borrowing behavior for each produced
element.

The pattern is checked against the source element type and must be irrefutable.

Each iteration creates fresh bindings from the pattern.

Iteration bindings are scoped to the for body.

Iteration bindings are not visible in the source expression or else body.

For each produced element, the pattern is applied and the body is evaluated once.

Reaching the end of the body starts the next iteration step.

The for body belongs to the `for` expression's break-capable region.

The else body belongs to the same break-capable region.

The body and else body do not capture `yield`.

`break value` exits the `for` expression and supplies the `for` result.

`break;` is shorthand for `break unit;`.

`continue` skips the rest of the current body evaluation and starts the next iteration step.

`continue` is not valid in the else body.

Natural exhaustion selects the else body when one is present.

Natural exhaustion without an else body completes as `unit`.

If a `for` expression must produce a non-`unit` value, every reachable normal exit path must break with a compatible value. A
reachable natural-exhaustion path requires an else body.

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

The Async Model defines async computations, `await`, `async` block expressions, `spawn`, `spawn detached`, task handles, task
obligations, and async cancellation.

---

## Tasks and async blocks

A **task** is an owned asynchronous computation that is scheduled for execution.

An **async block** is a structured ownership boundary for spawned asynchronous work. Tasks spawned inside an async block belong to
that block unless the task handle is transferred to another valid owner.

An async block owns the completion, cancellation, and destruction obligations of the tasks it contains. Non-panic exits from an
async block require every contained task to be completed, cancelled, or transferred according to its task contract.

**Spawning** creates a task from an asynchronous computation and places that task under the nearest enclosing async block.

**Detached spawning** creates a task whose lifetime is represented by the returned task handle. Detached tasks can capture only
state whose contract is valid for detached execution.

A **task handle** is an ownership-extending value for a task. The handle carries responsibility for joining, cancelling, or
otherwise completing the task according to its contract.

Applying `catch` to a task handle join observes the task boundary and produces `RunResult<T>`, where `T` is the spawned
computation's declared result type.

**Awaiting** an asynchronous computation drives it to completion and produces its declared result.

**Cancelling** a task or incomplete asynchronous computation destroys its owned captured state and releases the capabilities it
holds according to Bray's destruction and finalization rules.

Destroying an incomplete asynchronous computation cancels it.

When cancellation is observed through `catch` applied to a task or thread boundary, the catch expression produces
`RunResult.Cancelled`.

Detached asynchronous work exists only through explicit task handles. A detached task owns or otherwise validly extends the
lifetime of all state it uses.

Low-level async runtime machinery is part of the trusted substrate. Executors, reactors, wakers, completion queues, foreign async
callbacks, device async integration, and custom scheduling primitives are implemented through trusted capabilities and exposed
through safe async contracts.

---

## Run boundaries

A **run boundary** is the boundary of a task or thread.

A task is an owned run boundary for asynchronous execution scheduled under an async block or task handle.

A **thread** is an owned run boundary for synchronous execution scheduled outside the current execution flow.

A **thread handle** is an ownership-extending value for a thread. The handle carries responsibility for joining, cancelling when
the thread contract permits cancellation, or otherwise completing the thread according to its contract.

Applying `catch` to a thread handle join observes the thread boundary and produces `RunResult<T>`, where `T` is the thread entry
computation's declared result type.

A run boundary records panic from the run it owns and reports it as `RunResult.Panicked` when observed through `catch`.

Run boundaries do not resume panicked computations.

`RunResult<T>` does not replace recoverable domain failure. A fallible task or thread whose ordinary result is `Result<T, E>` is
observed through `catch` as `RunResult<Result<T, E>>`.

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

## Explicit exits

An **explicit exit** leaves a control-flow region and supplies the value or outcome required by that region.

A `return` exits the current callable execution scope and supplies the callable's result.

A `yield` targeting a single-yield region exits that region and supplies its value.

A `yield` targeting a multi-yield region contributes an element according to that region's contract.

A `break` exits the current loop or iteration region and supplies the target region's result.

`break;` is shorthand for `break unit;`.

A `continue` exits the current loop or iteration step and begins the next step according to the target region's contract.

Every explicit exit satisfies the target region's type, ownership state, initialization state, destruction state, finalization
obligations, capability contract, effect contract, and execution mode.

Nested regions have distinct exit targets. An explicit exit targets the nearest region of the kind it exits.

---

## Panic

A **panic** is an exceptional control-flow outcome outside ordinary result contracts.

Panic is used for programmer errors, violated invariants, failed assertions, failed runtime contract checks, failed asserted
index access, and states not modeled as ordinary failure.

Recoverable domain failure is represented with `Result<T, E>` values, not panic.

A panic propagates outward until it reaches a panic-catching boundary.

The `catch` expression creates a panic-catching boundary.

For ordinary synchronous code, `catch` reports a caught panic as `Result.Error(error = report)`.

For task or thread observation, `catch` reports a caught panic as `RunResult.Panicked(report = report)`.

If a panic reaches the program root without being caught, the program terminates.

---

## Result and run result values

`Result<T, E>` is the compiler-known union type for recoverable domain failure.

`Result.Ok` carries a successful value of type `T`.

`Result.Error` carries a recoverable error value of type `E`.

`Result<T, PanicReport>` is the caught representation of synchronous panic.

Built-in checked conversions produce `Result<T, ConversionError>`.

`ConversionError` reports the built-in checked conversion failure category.

`RunResult<T>` is the compiler-known union type for observing a task or thread run boundary through `catch`.

`RunResult.Completed` carries the computation's declared result of type `T`.

`RunResult.Panicked` carries the panic report caught at the task or thread boundary.

`RunResult.Cancelled` records task or thread cancellation before normal completion.

`RunResult<T>` does not replace `Result<T, E>`. A fallible task or thread whose ordinary result is `Result<T, E>` is observed
through `catch` as `RunResult<Result<T, E>>`.

The `try` expression unwraps one `Result` or `RunResult` layer and propagates non-success outcomes to the nearest compatible
boundary.

---

## Callable execution scopes

A **callable execution scope** is the body of a callable program element.

Functions, local functions, lambdas, and asynchronous functions introduce callable execution scopes.

A callable execution scope has a declared result type. A `return` exits the current callable execution scope and supplies a value
compatible with that result type.

Nested callable execution scopes are independent. A `return` inside a nested callable exits the nested callable.

---

## Value-producing regions

A **value-producing region** is a program region whose purpose is to produce a value for an enclosing expression.

Block expressions, conditional expressions, match expressions, and other expression-oriented control-flow forms are value-producing
regions when used in value-producing position.

A value-producing region completes through an explicit region-result operation. The result operation produces the region's value
without exiting the enclosing callable execution scope.

---

## Yield-capable regions

A **yield-capable region** is a region that accepts `yield` expressions.

Bray has two kinds of yield-capable regions:

- **single-yield regions**, which produce exactly one value.
- **multi-yield regions**, which produce a sequence of values.

Value-producing block expressions, value-producing conditional expression arms, and match expression arms are single-yield regions.

General generator expressions and array generator expressions are multi-yield regions.

A `yield` expression targets the nearest enclosing yield-capable region.

Nested yield-capable regions capture their own yields. A `yield` inside an inner yield-capable region does not yield to an
outer region.

A single-yield region with result type other than `unit` must yield exactly one value on every normal completion path or have no
normal continuation.

A single-yield region with result type `unit` can complete naturally without `yield`.

`yield;` is shorthand for `yield unit;`.

A multi-yield region may yield zero or more values, unless the consuming context imposes a stricter cardinality contract.

A fixed-size array generator is a multi-yield region with a statically required yield count. The number of yielded values must
equal the array length.

A generator used to construct a fixed-size array can contain nested control flow, but every possible execution must produce the
required number of elements. If the compiler cannot prove the yield count, the program is rejected.

General generator expressions are multi-yield regions with variable cardinality. They may yield zero or more values.

Every yielded value must satisfy the target region's element/result type, ownership state, initialization state, destruction state,
finalization obligations, capability contract, effect contract, and execution mode.

---

## Break-capable regions

A **break-capable region** is a loop or iteration region that accepts `break` expressions.

Loop expressions, while expressions, for expressions, and generator iteration expressions are break-capable regions.

A `break` expression targets the nearest enclosing break-capable region.

`break value` exits the target region and supplies the target region's result.

`break;` is shorthand for `break unit;`.

The break value must satisfy the target region's result type, ownership state, initialization state, destruction state,
finalization obligations, capability contract, effect contract, and execution mode.

A loop expression receives its value through `break`.

The syntactic body block of a loop belongs to the loop expression's break-capable region.

The loop body does not capture `yield`.

An ordinary loop expression repeats until control leaves the loop.

Reaching the end of the loop body starts the next iteration.

`break value` exits the current loop or iteration region with that value.

`break;` exits the current loop or iteration region with `unit`.

`continue` skips the rest of the current loop body and starts the next iteration.

If a loop has reachable `break` expressions that target the loop, every such break value must be compatible with the loop's result
type.

Loop paths that keep iterating do not supply a loop result.

A while expression repeats only while its condition is true.

The false condition path selects the while else body when one is present, or exits the while expression with `unit` when no else
body is present.

A for expression repeats until its source is exhausted or control leaves the expression.

The natural-exhaustion path selects the for else body when one is present, or exits the for expression with `unit` when no else
body is present.

A loop with no reachable `break` to itself has no normal completion and has type `never`.

Loop expressions do not have an else body because they have no natural exhaustion path.

---

## Generator iteration expression

A general generator expression has the form:

```bray
{
    each <pattern> in <source>
    {
        ...
    }
}
```

A brace-enclosed expression whose only top-level child is a general generator iteration expression is a general generator
expression rather than an ordinary block expression.

General generator expressions contain exactly one top-level generator iteration expression.

A bare top-level `each` is not valid in an ordinary block expression.

A generator iteration expression has the form:

`each <pattern> in <source> { ... }`

A generator iteration expression appears as the top-level child of a general generator expression, inside an array generator
expression, or nested inside an enclosing generator region that accepts its yielded values.

The `<source>` expression must provide an iteration contract. The iteration contract defines the yielded element type, iteration
order, ownership behavior, borrowing behavior, cardinality information, and whether iteration is finite.

The `<pattern>` is checked against the yielded element type and must be irrefutable.

The iteration expression applies the pattern to each yielded element in the mode defined by the iteration contract. Bindings
introduced by the pattern receive their type, capability, lifetime, and ownership behavior from that pattern operation.

Per-element pattern bindings are scoped to the generator body. A new pattern application occurs for each iteration step.

The source expression is evaluated once before iteration begins.

The body of a generator iteration expression runs inside the enclosing generator region. Each `yield` inside the body contributes
a value to that generator region unless it is captured by a nested yield-capable region.

`break` exits the nearest generator iteration expression and supplies that iteration expression's result.

Because generator iteration expressions complete as `unit`, `break;` is the ordinary generator-iteration break form.

`continue` skips the rest of the current generator iteration step and starts the next iteration step.

A fixed-size array generator for [T; N] must yield exactly N values of type T.

If T is itself [U; M], then each yielded value must itself satisfy the fixed-size array construction rules for [U; M].

---

## Match expressions

A **match expression** evaluates a subject expression, compares it against a sequence of pattern arms, and produces the result
of the selected arm.

A match expression is an expression. It has a type, participates in ownership and capability checking, and is terminated with a
semicolon when used as a sequenced expression.

```bray
let area: r64 = match shape
{
    case Circle(radius)
    {
        yield math.pi * radius * radius;
    }

    case Rectangle(min, max)
    {
        yield (max.x - min.x) * (max.y - min.y);
    }

    case Empty
    {
        yield 0.0;
    }
};
```

A match expression evaluates its subject once.

A match body contains `case` arms.

Each arm has a pattern and a block expression body.

An arm can also have a `when` guard. A guard is a boolean expression checked in guard context after the arm pattern structurally
matches and before the arm body is selected.

Bindings introduced by an arm pattern are available in the guard and in the arm body. In the guard, those bindings are available
for observation. In the arm body, those bindings are available according to the match operation mode.

The first arm whose pattern matches and whose guard holds is selected.

A catch-all arm is written with the discard pattern.

```bray
case _
{
    yield fallback;
}
```

Match arm bodies are block expressions. The result of the selected arm body becomes the result of the match expression.

If the match expression has result type `unit`, an arm body can complete normally.

If the match expression has a result type other than `unit`, every reachable normal completion path in every selected arm body
supplies a value with `yield` or ends in a `never` expression.

All match arms must merge to a coherent type, ownership state, initialization state, destruction state, finalization state,
capability state, and fact context.

A match expression can use refutable patterns.

A match expression over a closed union performs coverage checking against the union’s closed variant set.

A match expression over a nullable value performs coverage checking over absent state and present contained values.

Guarded arms provide conditional coverage through the shared fact and predicate system.

A guarded arm contributes only the coverage subregion where the guard is statically proven true.

A guard that is statically proven false makes that arm unreachable for the proven-false subregion.

A statically unknown guard can still select the arm at runtime, but does not contribute the unknown subregion to required
exhaustiveness.

Alternative patterns contribute coverage for each alternative.

Arm order is semantically meaningful. The selected arm is the first arm that matches structurally and passes its guard.

Later arms are checked against the subject space not already definitely covered by earlier arms.

A later arm whose pattern can never be selected is unreachable.

A successful arm pattern refines the fact context for the guard and the arm body. For union variants, this includes the active
variant and the initialized payload fields.

The match operation mode determines how pattern bindings are produced. The core modes are observe, shared borrow, mutable borrow,
consume, and copy.

The default match operation mode is observe.

A consuming match uses consume mode.

In consume mode, selected payloads and fields can be moved out according to ownership rules.

A consuming match with guards evaluates structural matching and guards through observation first. Consuming bindings are produced
for the selected arm body after the guard holds.

A match expression can match through borrowed or type-form subjects when the subject type and pattern form support it.

A match expression can use `box` patterns for owned indirection.

Match expressions are structural, deterministic, and coverage-checked according to the subject type and pattern set.

Guards supply additional boolean conditions after structural matching.

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

A named callable contract can give a reusable name to a callable type form. This is not a general type alias and cannot name an
arbitrary type expression.

A **generic parameter** is part of a declaration's contract. Its constraints are static predicate expressions that define which
operations, ownership behavior, capabilities, effects, and behavioral contracts the generic declaration relies on.

Generic code is checked against its declared constraints. A generic body uses only the behavior guaranteed by those constraints.

Types participate in ownership, borrowing, aliasing, movement, destruction, effects, behavioral contracts, polymorphism, and code
generation.

A type can expose ordinary operations, behavioral contract implementations, type-valued trait member bindings, constants,
constructors, destructors, and trusted contracts according to its declaration.

Type checking determines whether expressions, calls, patterns, bindings, control-flow exits, generic instantiations, and
declarations satisfy the type contracts they use.

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
- **unit type:** the type with exactly one value. The value is spelled `unit`. It represents completion without meaningful
  returned data.
- **never type:** the type with no values. It represents computation that does not produce a value because control flow does not
  continue normally from that point.
- **nullable values:** represent either a present value of a contained type or absence of a value. Absence is a valid
  initialized state of the nullable type form.
- **result values:** represent either successful completion with a value, recoverable domain failure with an error value, or
  caught synchronous panic when the error value is `PanicReport`.
- **run result values:** represent completion, panic, or cancellation observed from a task or thread run boundary.
- **task handles:** represent owned responsibility for a spawned asynchronous task.
- **panic reports:** preserve the panic message and diagnostic context carried by a panic.
- **tuples:** a fixed-size ordered product type. A tuple's element types are part of its type. Tuple ownership, borrowing,
  movement, copying, initialization, and destruction are derived from its elements.
- **arrays:** a fixed-size ordered sequence type. An array's element type and length are part of its type. Array ownership,
  borrowing, movement, copying, initialization, and destruction are derived from its elements.
- **slices:** an unsized contiguous sequence type. A slice's element type is part of its type. Slice length is runtime state
  carried by an indirection boundary such as `&[T]`, `&mut [T]`, or `box[S] [T]`.
- **trait views:** unsized types that expose a value through a specific trait application while hiding the concrete implementing
  type.

Conversions are explicit operations, except for literals whose value is valid for the target type. Non-literal values do not
implicitly cast, widen, narrow, reinterpret, allocate, borrow, clone, move, or dispatch through conversion-like behavior.

Numeric operations are defined by the contracts of the participating types. Overflow, narrowing, rounding, division by zero,
NaN behavior, and platform-dependent behavior are part of those contracts.

Built-in types can have compiler-known layout, code generation, and intrinsic behavior. That knowledge does not exempt them from
the language's ordinary semantic rules.

---

## Product types

A **product type** is a named type whose value is composed from a fixed set of named fields.

A product type is declared with `struct`.

A `struct` has one primary representation declaration. The primary representation declaration defines the type's identity, fields,
field order, representation-level ownership contract, initialization contract, destruction contract, default field values, and
layout contract when one is declared.

Fields belong to the primary representation declaration. Methods, constructors, behavioral contract implementations, and other
implementation blocks can be declared separately, but they do not add fields or change the representation.

A field is an owned subvalue unless its type defines a different ownership relationship.

A `struct` value is fully initialized when all required fields are initialized. A field with a declared default value can be omitted
during construction; the default expression initializes that field. A field without a default value must be supplied by the
construction expression or by an explicit constructor.

Field defaults are part of the `struct` declaration. Defaults are explicit representation-level behavior, not inference.

Field access creates an access path into the struct's storage. Observing a field requires read capability over the reached field.
Mutating a field requires mutation authority over the reached field.

Borrowing can target individual fields when the compiler can prove the field access paths are disjoint. Disjoint field borrows are
independent according to the ordinary aliasing and capability rules.

Moving a field out of an owned `struct` access path is a partial move when the product rules permit it. Moving a `struct` as a
complete value moves the whole value.

A consuming product pattern can also move selected fields out of a `struct` according to the pattern operation rules. After a
partial move, the `struct` is partially initialized and destruction affects only the fields that remain initialized.

Copy behavior is explicit. A `struct` is copyable only when its declaration or derived contract makes it copyable, and every field
satisfies the required copy contract.

Destruction is deterministic. Destroying a fully initialized `struct` destroys its initialized fields in reverse declaration order.

The default layout of a `struct` is compiler-defined. A stable layout, ABI layout, packed layout, or foreign-compatible layout exists
only through an explicit layout contract.

A `struct` participates in ownership, borrowing, mutation authority, initialization, destruction, conversion, behavioral contracts,
and visibility according to its declared fields and contracts.

---

## Constraints

A **constraint** is a compile-time requirement attached to a generic parameter, declaration, expression, or contract.

Constraints describe what the compiler knows about an otherwise generic entity. They define the operations, behavioral contracts,
ownership behavior, copy behavior, destruction behavior, capability requirements, effects, execution mode, and finalization
obligations that generic code can rely on.

Constraints are written as static predicate expressions.

Static predicate expressions use the predicate expression model in static constraint context.

A generic body is checked against its declared constraints. The body can use only behavior guaranteed by those constraints.

A generic instantiation satisfies a constraint when the supplied type, value, capability, effect, lifetime, or other generic
argument provides the required contract.

Constraints are part of the public semantic contract of a declaration. Changing constraints changes what callers may supply and
what the generic body may assume.

---

## Behavioral contracts

A **behavioral contract** is a named compile-time contract that describes behavior a type provides.

A behavioral contract defines required operations, type-valued trait members, constants, capability requirements, effect requirements,
execution-mode requirements, and semantic obligations.

A type satisfies a behavioral contract through an explicit implementation.

Generic code uses behavioral contracts through constraints. When a generic parameter is constrained by a behavioral contract, the
generic body can use the behavior declared by that contract.

A behavioral contract can describe synchronous behavior, asynchronous behavior, consuming behavior, mutating behavior, observing
behavior, internal-effect behavior, and trusted behavior when those are part of the contract.

A behavioral contract is part of the public semantic surface of a program. Changing a contract changes what implementers must
provide and what callers can rely on.

---

## Type forms

A **type form** is a syntactic and semantic form that produces a type.

Type forms are compiler-recognized type-level constructs. They define how a type is built from one or more subject types,
compile-time arguments, or structural components.

A type form can affect ownership, storage, borrowing, layout, lifetime behavior, callable behavior, initialization, destruction,
finalization, or value representation.

Type forms are part of the core type grammar.

Examples:

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
T?
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

### Type expressions

A **type expression** is syntax that denotes a type in a type context.

Type expressions are composed from type names, qualified type paths, generic type applications, type-valued member references,
type forms, type-form arguments, structural type forms, and parenthesized type expressions.

Type expressions do not perform runtime evaluation.

Parentheses around a single type expression group that type expression and make type-form composition order explicit.

```bray
(box[Heap] Point)?
box[Heap] (Point?)
&mut (box[Heap] Node)
```

Grouping parentheses do not produce a new type.

Tuple type forms use comma-separated element types.

A parenthesized single type expression without a comma is grouping.

```bray
(Point)
```

A one-element tuple type uses a trailing comma.

```bray
(Point,)
```

The trailing comma distinguishes a one-element tuple type from grouping parentheses.

### Prefix type forms

A **prefix type form** appears before its subject type or subject entity.

```bray
&T
&mut T
view TraitApplication
box T
box[Heap] T
```

`&T` is the shared-borrow type form.

`&mut T` is the mutable-borrow type form.

`view TraitApplication` is the trait-view type form.

The `view` type form uses a trait application as its subject entity.

`box T` is the default owned-indirection type form.

`box[S] T` is the owned-indirection type form using storage policy type `S`.

The square-bracket part of a prefix type form contains compile-time arguments for that type form.

```bray
box[Heap] List<i32>
box[AllocatorStorage<MyAllocator>] Node
```

### Postfix type forms

A **postfix type form** appears after its subject type.

```bray
T?
```

`T?` is the nullable type form.

It produces a type whose values and access paths have a nullable storage state.

The absence expression is `none`.

### Structural type forms

A **structural type form** uses a larger syntactic structure to produce a type.

```bray
[T]
[T; N]
(T1, T2)
func(left: T1, right: T2) -> R
```

`[T]` is the unsized slice type form.

`[T; N]` is the fixed-size array type form.

`N` must be greater than zero.

`(T1, T2)` is the tuple type form.

`func(left: T1, right: T2) -> R` is the callable type form.

Structural type forms can contain one or more subject types and compile-time values.

### Type-form arguments

A type form can accept compile-time arguments.

```bray
box[Heap] T
box[AllocatorStorage<MyAllocator>] T
[T; N]
```

Type-form arguments are part of the produced type.

For example:

```bray
box[Heap] Point
box[ArenaStorage] Point
```

are distinct types because the storage policy argument differs.

### Subject type

A **subject type** is the type that a type form is applied to.

In:

```bray
box[Heap] Point
```

`Point` is the subject type.

In:

```bray
Point?
```

`Point` is the subject type.

In:

```bray
&mut Buffer
```

`Buffer` is the subject type.

The `view` type form has a trait application subject rather than a subject type.

```bray
view Sink
```

`Sink` is the trait application subject.

### Composition

Type forms compose recursively.

```bray
box[Heap] List<i32>
box[Heap] Point?
&mut box[Heap] Node
box[Heap] [u8]
[box[Heap] Node; 4]
func(buffer: &Buffer, index: usize) -> u8
```

The meaning of a composed type is determined by applying each type form according to the type grammar, explicit grouping
parentheses, and the semantic contract of that form.

### Core rule

A type form is introduced by the language when the form has core semantic meaning.

A type form earns core status when ordinary named types and behavioral contracts cannot express the construct without losing
required compiler knowledge about ownership, storage, borrowing, layout, lifetime, callable behavior, initialization, destruction,
or finalization.

`box` is a type form because owned indirection affects recursive type sizing, ownership transfer, destruction, borrow projection,
and storage identity.

`?` is a type form because nullability is a core value-state shape used throughout the language.

`func(...) -> ...` is a type form because callable values carry parameter, result, ownership, effect, execution, and contract
semantics.

`[T; N]` and `[T]` are type forms because contiguous sequence storage, bounds, element access, slice projection, and element
lifecycle behavior require compiler-visible structure.

---

## Implementations

An **implementation** is an explicit declaration that makes a type satisfy a behavioral contract.

An implementation defines how the type provides the operations, type-valued trait member bindings, constants, capability
requirements, effect requirements, execution modes, and semantic obligations required by the contract.

Implementations are nominal relationships between a type and a behavioral contract. A type satisfies a contract only through a
participating implementation available in the relevant checking context.

An implementation that participates in a checking context is part of the program's semantic surface. It participates in type
checking, generic constraint satisfaction, dispatch, documentation, and public API compatibility.

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

An import does not silently extend overload sets, implementation overload families, operators, conversions, behavioral contracts, or other polymorphic
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

## Lifecycle declarations

A **lifecycle declaration** defines special behavior attached to a type's construction, finalization, destruction, or scoped use.

Lifecycle declarations are not behavioral contracts. They are part of the type's lifecycle contract and are interpreted directly
by the compiler.

A **constructor** creates a fully initialized value of its declaring type. Constructors are declared with `construct`.

The primary constructor has no name after `construct`.

A constructor with a name after `construct` becomes a named constructor under the type.

```bray
construct(pos path: Path, mode: FileMode = FileMode.read) -> Self
{
    ...
}

construct temp(pos directory: Path, prefix: String = "tmp") -> Self
{
    ...
}
```

A **finalizer** completes a required lifecycle obligation before ownership ends. Finalizers are declared with `finalize`.

A finalizer can be synchronous or asynchronous according to its result contract. A value whose type declares a required finalizer
carries a finalization obligation tracked by the compiler.

```bray
async finalize() -> Result<unit, FileError>
{
    ...
}
```

A **destructor** performs synchronous cleanup when ownership ends. Destructors are declared with `destruct`.

A destructor returns `unit`. The result type can be omitted, and if present must be `unit`.

```bray
destruct()
{
    ...
}
```

A destructor cannot be asynchronous and cannot produce a recoverable result. Fallible or asynchronous cleanup belongs to
finalization.

A **scope enter declaration** defines how a value creates a scoped capability.

A **scope exit declaration** defines how that scoped capability is released when the scope exits.

Scope enter and exit declarations are declared with `enter` and `exit`. `with` expressions use them to support resource-scope
idioms such as lock guards, temporary permissions, transactions, and scoped runtime registrations.

Lifecycle declarations participate in ownership, borrowing, mutation authority, finalization obligations, effects, and trusted
capability checking.

---

## Lifecycle declaration signatures and selection

Lifecycle declaration signatures use the same parameter grammar, default-argument rules, contract clauses, effect clauses,
capability clauses, generic constraints, and trusted declaration rules as callable declarations unless a lifecycle kind defines a
narrower rule.

Lifecycle declarations are written inside a type body or implementation body. The declaring type is implicit.

The lifecycle keyword identifies the lifecycle slot. Only named constructors write a user-chosen name after the lifecycle keyword.

Lifecycle declaration bodies can use declaration parameters, visible declarations, `Self`, and any compiler-introduced lifecycle
receiver binding made available by the lifecycle kind.

The binding name `self` is reserved for compiler-introduced lifecycle receivers and cannot be declared as an ordinary lifecycle
parameter.

Constructor declarations use either the primary constructor form or a named constructor form:

```bray
construct(pos path: Path) -> Self

construct temp(pos directory: Path) -> Result<Self, FileError>
```

The first example declares the primary constructor for the declaring type.

The second example declares a named constructor under the declaring type.

Constructor parameters are supplied by construction-call syntax and follow ordinary argument-binding rules.

The successful constructor result is the declaring type.

A constructor can declare `Self` directly or `Result<Self, E>`.

A constructor cannot be asynchronous.

Finalizer declarations return `unit` directly or through `Result`:

```bray
finalize() -> unit

async finalize() -> Result<unit, TransactionError>
```

The finalizer has no caller-supplied parameters.

The successful finalizer result is `unit`.

A finalizer can declare `unit` directly or `Result<unit, E>`.

If the result type is omitted, `unit` is inferred as the result type.

A finalizer can be asynchronous.

Destructor declarations have this form:

```bray
destruct()
```

The destructor has no caller-supplied parameters.

The destructor result is `unit`. The result type can be omitted, and if present must be `unit`.

A destructor cannot be asynchronous and cannot declare `Result<unit, E>`.

Scope enter declarations return the scoped capability directly or through `Result`:

```bray
enter() -> FileLease
```

When enter can fail, it returns `Result`:

```bray
enter() -> Result<LockGuard, LockError>
```

The enter declaration has no caller-supplied parameters.

The successful enter result is the scoped capability value used by the `with` body.

An enter declaration can declare the scoped-capability type directly or `Result<T, E>` where `T` is the scoped-capability type.

An enter declaration can be asynchronous.

Scope exit declarations receive the scoped capability and return `unit` directly or through `Result`:

```bray
exit(scoped: FileLease) -> unit
```

When exit can fail, it returns `Result`. It can also use `async`:

```bray
async exit(scoped: TransactionScope) -> Result<unit, TransactionError>
```

The exit declaration has exactly one scoped-capability parameter.

The scoped-capability parameter type must match the successful scoped-capability type of the enter declaration it exits.

The successful exit result is `unit`.

An exit declaration can declare `unit` directly or `Result<unit, E>`.

If the result type is omitted, `unit` is inferred as the result type.

An exit declaration can be asynchronous.

Lifecycle selection starts from the lifecycle kind and the exact type whose lifecycle is being used.

Constructor lookup uses the constructor path.

The primary constructor path is the type path.

Named constructor paths are members of the type path.

```bray
File(path)
File.temp(directory)
```

Finalizer and destructor selection uses the type whose ownership is being finalized or destroyed.

With-expression enter selection uses the initializer type and the selected `with` receiver access path.

With-expression exit selection uses the already-selected enter declaration and the exact scoped-capability type produced by that
enter declaration.

A `with` binding type annotation checks the scoped-capability value after enter selection. It does not select the enter
declaration.

Lifecycle declarations do not form implicit overload sets.

Two visible lifecycle declarations for the same exact lifecycle kind, type, and lifecycle path cannot coexist in the same
coherence domain.

Lifecycle selection does not use expected result type.

Lifecycle selection does not rank candidates.

A lifecycle use must resolve to exactly one applicable declaration.

If no declaration is applicable, the lifecycle use is rejected.

If more than one declaration is applicable, the lifecycle use is rejected as ambiguous.

For constructors, explicit construction arguments are checked after constructor lookup selects a single declaration.

Default arguments are applied only after a single lifecycle declaration has been selected.

Default arguments do not participate in lifecycle selection.

Lifecycle declarations provided by trait implementations participate only after the relevant exact trait implementation has been
selected by the ordinary trait and implementation selection rules.

---

## Lifecycle ordering

For a value that uses every lifecycle phase, the lifecycle order is:

```text
construct -> ordinary use -> enter -> with body -> exit -> finalize -> destruct -> field/payload destruction
```

The ordinary use and with-body phases include observation, borrowing, mutation, movement, copying, calls, and access through the
value's type contract.

Construction initializes storage.

Storage becomes fully initialized only after construction completes.

If construction exits before the whole value is fully initialized, already-initialized subparts are destroyed in reverse
initialization order.

A completed construction can establish finalization obligations.

Scoped use happens through `with` expressions.

A `with` expression applies `enter`, evaluates its body while the scoped capability is live, then applies the matching `exit`.

The matching `exit` runs before the with body's control-flow outcome continues.

The matching `exit` also runs before ordinary local destruction caused by leaving the with body.

Scope exit resolves owned values whose ownership remains in the scope.

For each such value, any required finalization obligation must be resolved before destruction begins.

Finalization is obligation-driven. A value with an outstanding finalization obligation cannot be destroyed.

The obligation must be completed, transferred to another owner that assumes it, or converted into an explicit fallback ownership
form before ownership ends.

Finalization obligations represent fallible or asynchronous cleanup before destruction.

Destruction is synchronous and infallible.

When a fully initialized value has a custom destructor, the whole-value destructor runs before the value's fields or active payload
parts are destroyed.

After the whole-value destructor returns, initialized fields or active payload parts are destroyed in the type's destruction order.

If no custom destructor exists, initialized fields or active payload parts are destroyed directly in the type's destruction order.

Partial values do not run whole-value finalizers or whole-value destructors.

Partial values resolve lifecycle only for initialized parts.

A type with whole-value lifecycle behavior can restrict partial moves when the lifecycle behavior depends on whole-value
invariants.

Panic propagation and cancellation use the same lifecycle ordering as ordinary scope exit.

Active `with` exits run first, then remaining owned locals are resolved in reverse ownership-scope order.

If lifecycle cleanup produces a failure while panic propagation or cancellation is already in progress, the surrounding panic,
run-result, or cancellation model must represent that combined outcome. If it cannot, the surrounding program is rejected.

---

## Destruction and finalization

**Destruction** ends ownership of a value and releases the resources governed by its destruction contract.

Destruction is deterministic. A fully initialized owned value is destroyed exactly once unless ownership moves elsewhere or the
value enters an explicit ownership construct with a different lifetime contract.

Destruction is synchronous. Leaving a scope destroys local owned values whose ownership remains in that scope, in the order defined
by Bray.

A moved-from value is not destroyed by the old owner. Partially initialized storage destroys only the parts that were initialized.

A type can define a destructor with a `destruct` lifecycle declaration. The destructor is synchronous, returns `unit`, and runs
as part of destruction.

**Finalization** is a required lifecycle obligation that must be completed before ownership ends.

A type can define a finalizer with a `finalize` lifecycle declaration. A finalizer can be synchronous or asynchronous according to
its result contract.

A value can carry a finalization obligation as part of its type contract. The compiler tracks finalization obligations across
movement, scope exit, cancellation, and destruction.

A value with a finalization obligation must be finalized, transferred to another owner that assumes the obligation, or converted
into an explicit fallback ownership form before the owning scope exits.

Asynchronous finalization is completed through asynchronous execution. Ordinary destruction remains synchronous.

---

## Effects and capability contracts

An **effect** is an observable or declared consequence of evaluating a program element beyond producing a value.

A **capability contract** declares which capabilities an operation requires, holds, creates, transfers, or releases.

Effects and capabilities are part of function signatures, behavioral contracts, generic constraints, callable values, lifecycle
declarations, scope enter/exit declarations, and trusted declarations.

The core capability categories are:

- **Observe:** reads or inspects without visible mutation or internal mutation.
- **Observe with internal effects:** preserves the abstract value while performing declared internal effects such as caching,
  metrics, lazy initialization, locking, or reference-count updates.
- **Mutate:** changes the abstract value reached through an access path.
- **Consume:** takes ownership of a value.
- **Finalize:** completes a required lifecycle obligation before ownership ends.
- **Enter scope:** creates a scoped capability through a lifecycle declaration.
- **Exit scope:** releases a scoped capability through a lifecycle declaration.
- **Trusted:** uses declared trusted memory capabilities.

A program element can use only the effects and capabilities available through its bindings, parameters, constraints, execution
mode, lifecycle state, and surrounding context.

Generic code is checked against declared effects and capability contracts. A generic body uses only the effects and capabilities
guaranteed by its constraints.

Effects and capability contracts are part of overload resolution, behavioral contract satisfaction, dynamic dispatch, lifecycle
checking, and public API compatibility.

---

## Trusted declarations

A **trusted declaration** is a declaration that uses bounded unchecked memory power through Bray's trusted capability model.

A module must opt in to trusted declarations before it can contain trusted functions. The module directive permits trusted
declarations. It does not make the module's ordinary declarations trusted.

A **trusted function** declares the exact trusted capabilities it uses. The set of trusted capabilities is closed:

- `raw_memory`
- `unchecked_alias`
- `unchecked_init`
- `foreign_call`
- `layout_reinterpret`
- `manual_alloc`
- `device_memory`
- `intrinsic`

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
