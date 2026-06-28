# Expression Model

## Overview

An **expression** is a program element that produces a value, an access path, a control-flow outcome, or a compile-time entity.

Bray is expression-oriented. Block expressions, construction expressions, call expressions, match expressions, generator expressions, conversion expressions, borrow expressions, assignment expressions, and control-flow expressions all have types and participate in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

Expressions are distinct from declarations and directives.

A **declaration** introduces a named program entity or semantic relationship.

A **directive** is a compile-time instruction affecting interpretation, checking, diagnostics, target behavior, or build behavior.

---

## Expression results

Every expression has a checked result.

An expression result can be:

- a value,
- an access path,
- a control-flow outcome,
- a compile-time entity,
- `unit`,
- `never`.

An expression of type `unit` represents completion without meaningful data.

An expression of type `never` has no normal continuation.

An expression that produces an access path can be observed, borrowed, mutably borrowed, assigned through, moved from, copied from, consumed, or destroyed according to its type, ownership story, initialization state, and capability state.

An expression that produces a compile-time entity can participate in type checking, path resolution, contract checking, or other compile-time semantics according to the entity kind.

---

## Expression context

Expressions are checked in context.

Expression context can provide:

- expected type,
- expected ownership mode,
- expected borrow mode,
- expected capability mode,
- expected result type,
- expected storage policy type,
- expected union type,
- expected callable type,
- expected effect contract,
- expected pattern operation mode.

Expected type can guide:

- numeric literal typing,
- imaginary literal typing,
- tuple element typing,
- array element typing,
- struct construction shorthand,
- union variant shorthand,
- box construction shorthand,
- conversion checking,
- callable overload resolution.

Example:

```bray
let shape: Shape = .Circle(center = origin, radius = 10.0);
```

The expected type `Shape` lets `.Circle(...)` resolve as a variant construction expression for `Shape`.

Example:

```bray
let node: box List<i32> = box(.Empty);
```

The expected type `box List<i32>` lets `box(...)` expect an inner `List<i32>`, which lets `.Empty` resolve as a variant of `List<i32>`.

---

## Sequenced expressions

A **sequenced expression** is an expression evaluated as part of an ordered sequence inside a block expression.

Sequenced expressions are terminated with semicolons.

```bray
{
    log(message = "hello");
    log(message = "world");
}
```

Semicolons are mandatory for sequenced expressions.

A sequenced expression can be used for its value, effects, lifecycle behavior, control-flow outcome, fact-context changes, or `unit` completion according to the surrounding context.

A `return` expression used in a sequence is terminated with a semicolon.

```bray
func f() -> i32
{
    return 1;
}
```

A `yield` expression used in a sequence is terminated with a semicolon.

```bray
let x: i32 =
{
    yield 1;
};
```

---

## Block expressions

A **block expression** is a braced expression region.

```bray
{
    ...
}
```

Every block expression has a type.

A block expression introduces a scope for:

- local bindings,
- local declarations where permitted,
- ownership tracking,
- borrow tracking,
- destruction,
- finalization tracking,
- capability checking,
- effect checking,
- fact-context refinement.

A block expression can produce `unit`, `never`, or another value type.

A block expression in value-producing context receives its value through `yield`.

```bray
let x: i32 =
{
    yield 1;
};
```

A block expression in `unit` context can complete normally.

```bray
{
    log(message = "done");
}
```

A block expression whose control flow has no normal continuation has type `never`.

A block expression’s exits must merge to coherent type, ownership, initialization, destruction, finalization, capability, effect, and fact-context state.

---

## Callable-body block expressions

A **callable-body block expression** is the block expression used as the body of a callable program element.

Functions have callable-body block expressions.

Other callable forms such as local functions, anonymous functions, closures, and async functions also have callable-body block expressions.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

A callable-body block expression creates a **callable execution scope**.

`return` exits the nearest callable execution scope.

A callable body supplies callable result values through `return`.

```bray
func f() -> i32
{
    return 1;
}
```

A callable body with declared result type `unit` can complete normally.

```bray
func log(message: String)
{
    print(message = message);
}
```

A callable body with declared result type other than `unit` must ensure every reachable normal completion path either supplies a callable result through `return` or reaches a `never` expression.

A callable with omitted result type has result type `unit`.

```bray
func log(message: String)
{
    print(message = message);
}
```

This has the same callable result contract as:

```bray
func log(message: String) -> unit
{
    print(message = message);
}
```

A `return` expression must supply a value compatible with the callable’s declared result type.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

TODO: Define explicit unit-return syntax for callables returning `unit`.

A `return` expression has type `never` in the current control-flow path because control exits the callable execution scope.

A `never` expression can satisfy any callable result requirement because it has no normal continuation.

```bray
func fail(message: String) -> never
{
    abort(message = message);
}
```

A callable declared to return `never` has no normal completion path.

A callable-body block expression can contain nested callable declarations. Each nested callable creates its own callable execution scope.

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

A callable-body block expression can contain nested yield-capable regions.

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

`yield` targets the nearest enclosing yield-capable region.

`return` targets the nearest enclosing callable execution scope.

A `yield` inside a nested value-producing block expression supplies that nested block expression, not the callable result.

Callable result values are supplied through `return`.

A callable-body block expression introduces a scope for local bindings, local declarations where permitted, ownership tracking, borrow tracking, destruction, finalization tracking, capability checking, effect checking, and fact-context refinement.

Local bindings introduced inside the callable-body block expression are visible according to ordinary block-expression scope rules.

Local owned values whose ownership remains in the callable-body block expression are destroyed when the callable-body block expression exits.

Callable-body exits include normal completion, `return`, `never`, and cancellation of an async computation.

TODO: Define any additional control-flow forms that leave a callable-body block expression.

A `return` expression first evaluates its returned expression.

The returned value is transferred to the callable result according to the result type, ownership rules, copy rules, borrow rules, and lifetime rules.

A local value moved into the callable result is no longer destroyed as a local owned value.

Local owned values that remain in the callable-body block expression after the returned value is formed are destroyed according to Bray destruction order.

A returned borrow must be valid for the callable result contract.

A callable body cannot return a borrow that outlives the storage it reaches.

A callable body cannot return a value with unresolved finalization obligations unless the callable result type or surrounding contract transfers those obligations.

A callable-body block expression must leave every reachable exit with coherent type, ownership state, initialization state, destruction state, finalization state, capability state, effect state, and fact-context state.

For async callables, the callable-body block expression is checked in async execution context.

Calling an async callable creates an owned async computation.

Suspension points in an async callable capture live values, borrows, capabilities, effects, and finalization obligations into the async computation’s contract.

Destroying an incomplete async computation cancels it, destroys owned state, and releases capabilities according to the async computation’s contract.

Ordinary destruction remains synchronous.

Asynchronous finalization obligations must be completed through asynchronous execution, transferred, or converted into an explicit fallback ownership form before the owning scope exits.

---

## Local binding declarations inside block expressions

A **local binding declaration** introduces one or more local bindings inside a block expression.

A local binding declaration uses `let`.

```bray
let x: i32 = 1;
let mut y: i32 = 2;
```

A local binding declaration contains a pattern.

```bray
let pattern = initializer;
let pattern: Type = initializer;
```

The pattern determines which bindings are introduced.

A simple binding is a binding pattern.

```bray
let x: i32 = 1;
```

A mutable local binding is a mutable binding pattern.

```bray
let mut x: i32 = 1;
```

`mut` before a binding name applies to the local owned binding introduced by the pattern.

A local binding declaration can destructure with any irrefutable pattern accepted for the initializer type.

```bray
let (x, y): (i32, i32) = pair;

let { x, y }: Point = point;

let [first, second, third]: [i32; 3] = values;
```

A local binding declaration requires an irrefutable pattern.

A refutable pattern belongs to match expressions, union handling, or another construct that defines behavior for failed matching.

A union variant pattern is valid in a local binding declaration only when it is irrefutable for the subject type.

A product pattern is irrefutable when all selected subpatterns are irrefutable and the product shape is guaranteed by the subject type.

A tuple pattern is irrefutable when all element subpatterns are irrefutable and the tuple arity matches the subject type.

A fixed-size array pattern is irrefutable when it accounts for the fixed array shape and every listed element pattern is irrefutable.

The initializer expression is evaluated once.

The initializer expression is checked against the annotated type when a type annotation is present.

The type annotation on a local binding declaration applies to the initializer subject matched by the pattern.

```bray
let { x, y }: Point = point;
```

Here `Point` is the expected type of `point` and the subject type for the pattern `{ x, y }`.

When a type annotation is absent, the initializer expression determines the subject type, and the pattern is checked against that subject type.

```bray
let { x, y } = point;
```

The pattern can provide expected type context to subexpressions only through the subject type established by annotation, initializer type, or surrounding context.

A local binding declaration completes when the initializer has been evaluated, the pattern has matched, and every binding introduced by the pattern has been initialized.

Bindings introduced by the pattern become visible after the local binding declaration completes.

Bindings introduced by the pattern are not visible inside the initializer expression.

Every binding name introduced by a pattern must be unique within that pattern.

A binding introduced by a local binding declaration must not shadow an existing visible binding.

A local binding declaration introduces each pattern binding exactly once.

A binding cannot be rebound.

A binding is a name for an access path, value, or other declaration result. The binding is not itself storage.

A local value binding introduces an access path to storage with the declared or inferred capability.

A binding pattern without `mut` introduces an immutable binding.

```bray
let x = value;
```

A mutable binding pattern introduces a mutable owned local binding.

```bray
let mut x = value;
```

A mutable owned local binding grants mutation authority over the local access path, subject to the type’s field, payload, and representation contracts.

Field mutability still applies.

```bray
struct Point
{
    x: r64;
    y: r64;
}

let mut p: Point = { x = 1.0, y = 2.0, };
```

The binding `p` is mutable, but fields declared without `mut` remain immutable after initialization.

A borrow type controls access to reached storage through the borrow layer.

```bray
let r: &mut Buffer = &mut buffer;
```

The `&mut` type form and borrow expression grant mutation authority over the reached storage according to Bray's borrowing rules.

`mut` before a binding name controls local owned binding authority.

The initializer expression can produce an owned value, copied value, borrowed value, access path, or other expression result accepted by the pattern context.

For local binding declarations, the pattern operation mode initializes local bindings from the initializer result.

If the initializer result is an owned value, the pattern can move parts of that value into the introduced bindings.

If the initializer result is copyable and the context selects copy behavior, the pattern can copy parts into introduced bindings.

If the initializer result is a borrow value, the pattern can introduce bindings to borrowed access paths according to the borrow type and pattern operation mode.

If the initializer result is an access path to an existing owned value, using it as an owned initializer moves from that access path unless the type is copyable or the expression explicitly borrows.

```bray
let b = a;
```

This moves `a` into `b` when `a` is non-copyable.

The old access path `a` becomes moved-from until reinitialized.

A destructuring local binding can partially move from an existing access path.

```bray
let { x, y } = point;
```

If `point` is an owned non-copyable value and the pattern moves fields out, `point` becomes partially initialized after the declaration unless the entire value is consumed by the destructuring rule.

A partially moved value cannot be used as a complete value until reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved value destroys only the still-initialized parts.

When the initializer is a temporary value produced solely for the local binding declaration, destructuring consumes that temporary into the introduced bindings, and no remaining named subject exists after the declaration.

Product patterns in local binding declarations match fields by name.

```bray
let { x, y }: Point = point;
```

Field order does not matter.

Duplicate fields are errors.

Unknown fields are errors.

Missing fields are errors unless `..` is present.

Field shorthand introduces same-name bindings.

```bray
let { x, y }: Point = point;
```

This means the fields `x` and `y` are bound as local bindings named `x` and `y`.

`..` explicitly accounts for remaining fields and introduces no bindings.

```bray
let { x, .. }: Point = point;
```

Tuple patterns in local binding declarations match tuple elements by position.

```bray
let (left, right): (i32, i32) = pair;
```

Tuple arity must match the subject tuple type.

Each element pattern is checked against the corresponding tuple element type.

A one-element tuple pattern uses a trailing comma.

```bray
let (single,): (i32,) = tuple;
```

Fixed-size array patterns in local binding declarations match array elements by position.

```bray
let [first, second, third]: [i32; 3] = values;
```

The number of listed element patterns must match the array length unless `..` is present.

```bray
let [first, .., last]: [i32; 4] = values;
```

Each listed element pattern is checked against the array element type.

`..` explicitly accounts for remaining elements and introduces no bindings.

Box patterns in local binding declarations match through owned indirection when the subject type is `box[S] T`.

```bray
let box(inner): box[Heap] Node = node;
```

The inner pattern is checked against `T`.

The local binding declaration’s operation mode determines whether the contained value is observed, borrowed, copied, or consumed.

A consuming `box(inner)` pattern consumes the box and moves through the owned indirection according to `box` ownership rules.

A borrowing `box(inner)` pattern projects a borrow of the contained value according to the box storage policy and the required storage behavior.

A local binding declaration can establish facts in the fact context.

Facts established by local binding patterns can include product field availability, tuple shape, fixed array shape, active union variant when the pattern is irrefutable for the subject, payload initialization, literal equality when an irrefutable literal context exists, and initialized local bindings.

Facts established by the initializer expression also flow into the fact context when valid after the declaration.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts established by a local binding declaration.

A local binding declaration participates in finalization tracking.

If an introduced binding owns a value with a finalization obligation, the obligation is tracked from the point the binding is initialized.

The binding must be finalized, transferred to another owner that assumes the obligation, or converted into an explicit fallback ownership form before the owning scope exits.

A local binding declaration participates in destruction.

Each owned binding introduced by the pattern is destroyed when its owning scope exits, unless ownership has moved elsewhere or the value has entered another ownership construct.

A binding moved out before scope exit is not destroyed by the old binding.

A partially initialized binding destroys only initialized parts.

If initializer evaluation exits through `return`, `yield`, `break`, `continue`, `never`, or panic/abort-like control flow before the declaration completes, the pattern bindings are not introduced.

TODO: Define how additional control-flow forms interact with initializer evaluation before local binding declarations complete.

Values and temporaries already initialized during initializer evaluation are handled by the corresponding control-flow and destruction rules.

A local binding declaration is a declaration inside a block expression, but its initializer and pattern matching are expression-checked operations.

Local binding declarations participate in block-expression sequencing and must be terminated according to the block expression’s sequencing rules.

---

## Literal expressions

A **literal expression** directly denotes a literal value written in source text.

Literal expressions are expression grammar forms. They are checked in expression context and receive a type during binding and type checking.

Literal expressions currently include integer literals, real literals, imaginary literals, boolean literals, and character literals.

```bray
1
1.0
2.0i
true
false
'a'
```

A literal expression can produce a value directly, or it can participate in a larger expression such as an arithmetic expression, tuple expression, array expression, struct construction expression, union variant construction expression, predicate expression, or contract clause.

Literal expressions are pure expression forms. Evaluating a literal expression creates no user-visible side effect.

---

### Integer literals

An **integer literal** is a numeric literal without a fractional part, exponent marker requiring real interpretation, or imaginary suffix.

```bray
0
1
127
```

Integer literals are initially untyped.

An integer literal receives its type from expression context when an expected integer type exists.

```bray
let x: i32 = 1;
let y: u64 = 1;
```

When no context fixes the type of an integer literal, the default integer literal type is `i32`.

The literal value must be representable in the selected integer type.

The selected integer type determines the runtime value type of the literal expression.

Integer literal typing is literal adaptation. It does not create implicit conversion rules for already-typed non-literal values.

```bray
let x: i32 = 1;
let y: i64 = x as i64;
```

The literal `1` can adapt to `i32` in the first binding. The already-typed value `x` uses explicit conversion to become `i64`.

---

### Real literals

A **real literal** is a numeric literal with real-literal syntax.

```bray
1.0
0.5
```

Real literals are initially untyped.

A real literal receives its type from expression context when an expected real type exists.

```bray
let x: r32 = 1.0;
let y: r64 = 1.0;
```

When no context fixes the type of a real literal, the default real literal type is `r64`.

The literal value must be representable according to the selected real type’s literal conversion rules.

The selected real type determines the runtime value type of the literal expression.

Real literal typing is literal adaptation. It does not create implicit conversion rules for already-typed non-literal values.

```bray
let x: r64 = 1.0;
let y: r32 = x as rounding r32;
```

The literal `1.0` can adapt to `r64`. The already-typed value `x` uses an explicit named conversion mode to become `r32` when narrowing or rounding behavior is required.

---

### Imaginary literals

An **imaginary literal** is a numeric literal followed by the imaginary suffix `i`.

```bray
2i
2.0i
```

An imaginary literal represents an imaginary component.

An imaginary literal is initially untyped.

An imaginary literal receives its component type from expression context when an expected complex type exists.

```bray
let z: c128 = 1.0 + 2.0i;
```

For `c128`, the real and imaginary components are `r64`.

For `c64`, the real and imaginary components are `r32`.

```bray
let z: c64 = 1.0 + 2.0i;
```

An imaginary literal does not make the identifier `i` special.

```bray
let i: i32 = 2;
```

Here `i` is an ordinary binding name.

An imaginary literal can participate in a complex literal expression when the surrounding expression context expects a complex type.

```bray
let z: c128 = 1.0 + 2.0i;
```

A real literal plus an imaginary literal can form a complex value by literal adaptation when the expected type is a built-in complex type.

Already-typed non-literal real values require explicit complex construction.

```bray
let real: r64 = 1.0;
let imag: r64 = 2.0;

let z: c128 = (real, imag) as c128;
```

---

### Complex literal expressions

A **complex literal expression** is an expression built from literal real and imaginary components in a context expecting a built-in complex type.

```bray
let z: c128 = 1.0 + 2.0i;
```

The expected complex type determines the component real type.

The real component literal adapts to the complex component type.

The imaginary component literal adapts to the complex component type.

The expression produces a value of the expected complex type.

For `c128`:

```bray
let z: c128 = 1.0 + 2.0i;
```

the real component is interpreted as `r64`, and the imaginary component is interpreted as `r64`.

For `c64`:

```bray
let z: c64 = 1.0 + 2.0i;
```

the real component is interpreted as `r32`, and the imaginary component is interpreted as `r32`.

Complex literal formation is based on literal adaptation.

A non-literal real value is already typed and uses explicit construction to become part of a complex value.

```bray
let x: r64 = 1.0;
let z: c128 = (x, 2.0) as c128;
```

---

### Boolean literals

A **boolean literal** directly denotes a value of type `bool`.

```bray
true
false
```

`true` and `false` have type `bool`.

Boolean literal expressions can be used in condition expressions, predicate expressions, contract expressions, match guards, local binding initializers, and other expression contexts expecting `bool`.

Boolean literals are already typed. They do not use numeric literal adaptation.

---

### Character literals

A **character literal** directly denotes a value of type `char`.

```bray
'a'
```

A `char` value is a Unicode scalar value.

A character literal has type `char`.

A character literal is not a byte literal.

TODO: Define string literal syntax and string representation.

Character literals are already typed as `char`.

---

### Unit and never in expression context

`unit` is a type with exactly one value.

A block expression or callable body can produce `unit` by completing normally in a `unit` context.

```bray
func log(message: String)
{
    print(message = message);
}
```

TODO: Define the explicit source spelling for the unit value.

`never` is a type with no values.

A `never` expression is produced by control-flow constructs that have no normal continuation, such as `return` and abort-like expressions.

TODO: Define the complete set of never-producing constructs.

```bray
return value;
```

A `never` expression can satisfy any expected type at a control-flow merge because it has no normal continuation.

---

### Literal typing by context

Literal expressions are checked using expected type context when context is available.

Expected type context can come from:

- local binding type annotations,
- function return types,
- function parameter types,
- named argument parameter types,
- struct field types,
- union variant payload field types,
- tuple element types,
- array element types,
- conversion target types,
- predicate expression context,
- match arm result context,
- block expression result context,
- box inner expected type propagation,
- other type-directed expression contexts.

Examples:

```bray
let x: i32 = 1;

let pair: (i32, r64) = (1, 2.0);

let values: [i64; 3] = [1, 2, 3];

let shape: Shape = .Circle(radius = 1.0, center = origin);
```

In each example, the expected type controls literal typing.

When no expected type controls a numeric literal, the default literal type applies.

---

### Literal defaults

When no expected type determines a numeric literal type, Bray uses default literal types.

The default integer literal type is `i32`.

The default real literal type is `r64`.

The default complex literal type is `c128`.

Defaults apply only to otherwise unconstrained literals.

Defaults do not create implicit conversion between non-literal values.

```bray
let x = 1;
let y = 1.0;
let z = 1.0 + 2.0i;
```

These infer `i32`, `r64`, and `c128` respectively when no other context applies.

---

### Literal adaptation

Literal adaptation is the process that assigns a compatible type to an initially untyped literal expression.

Literal adaptation applies to literal expressions.

Literal adaptation can adapt integer literals to integer types.

Literal adaptation can adapt real literals to real types.

Literal adaptation can adapt real and imaginary literals to complex component types when forming a complex literal expression.

Literal adaptation can happen recursively inside tuple expressions, array expressions, struct construction expressions, union variant construction expressions, and named argument expressions.

Literal adaptation does not apply to already-typed non-literal values.

```bray
let x: i32 = 1;
let y: i64 = x as i64;
```

`1` adapts to `i32`.

`x` is already an `i32` value and uses explicit conversion to become `i64`.

---

### Numeric literal suffixes

Bray uses type context rather than numeric type suffixes.

```bray
let x: i32 = 1;
let y: r64 = 1.0;
```

The only suffix currently used in numeric literal syntax is `i` for imaginary literals.

```bray
let z: c128 = 1.0 + 2.0i;
```

Type spelling belongs to type annotations, parameter types, field types, result types, array element types, tuple element types, and conversion targets.

---

### Literal expressions in predicate context

Literal expressions can appear in predicate expressions and contract clauses.

```bray
predicate non_empty(length: usize) =
    length > 0;
```

Numeric literals in predicate expressions are checked under predicate-expression rules.

Integer-valued predicate arithmetic uses contract arithmetic semantics.

Contract arithmetic does not silently wrap.

A runtime assertion generated from a predicate expression must preserve the predicate-expression meaning of the literal and arithmetic operation.

---

## Name expressions

A **name expression** is an expression consisting of a single identifier.

```bray
value
buffer
count
```

A name expression resolves to a visible binding or declaration in expression context.

A name expression can produce a value, an access path, a callable declaration, a callable value, a type-level entity, a module entity, a package entity, or another compile-time entity depending on the resolved declaration.

Name expressions are expression grammar forms.

Pattern identifiers are checked by pattern grammar rules instead.

In pattern context, a bare identifier introduces a binding.

In expression context, a bare identifier resolves to an existing visible binding or declaration.

---

### Binding name expressions

A name expression that resolves to a local value binding produces an access path to that binding’s storage.

```bray
let x: i32 = 1;
let y: i32 = x;
```

The expression `x` reaches the local binding `x`.

Using the access path can observe, borrow, mutably borrow, move, copy, consume, or participate in assignment according to the binding’s type, ownership state, initialization state, and capability state.

If the binding holds a copyable value and the expression context requires an owned value, the value can be copied according to the type’s copy contract.

If the binding holds a non-copyable owned value and the expression context requires an owned value, the value is moved.

After a move, the old binding’s access path is moved-from until reinitialized.

A moved-from binding can be reinitialized when the storage and type contract permit it.

A moved-from binding cannot be observed, borrowed, moved, copied, consumed, or destroyed as a complete value.

---

### Immutable binding access

A binding introduced without `mut` has immutable local access authority.

```bray
let x: i32 = 1;
```

The binding can be observed.

The binding can be moved or copied according to ownership and copy rules.

The binding can be borrowed immutably when borrowing rules permit it.

The binding grants no local mutation authority over the bound access path.

Owned immutability and field mutability are distinct from ownership.

---

### Mutable binding access

A binding introduced with `mut` has mutable local access authority over the bound access path.

```bray
let mut x: i32 = 1;
```

A mutable binding can be assigned through when the type and storage contract permit assignment.

A mutable binding can be mutably borrowed when borrowing rules permit it.

A mutable binding can be moved, copied, observed, borrowed, consumed, or reinitialized according to ownership and capability rules.

Field mutability still applies to reached fields.

```bray
struct Point
{
    x: r64;
    y: r64;
}

let mut p: Point = { x = 1.0, y = 2.0, };
```

The binding `p` has mutable local access authority over the `Point` value. Fields declared without `mut` remain immutable after initialization.

---

### Parameter name expressions

A name expression that resolves to a parameter reaches the parameter binding inside the callable body.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

`left` and `right` are parameter name expressions inside the function body.

Owned parameters are immutable by default.

A mutable owned parameter uses `mut` before the parameter name in the function signature.

```bray
func normalize(mut buffer: Buffer) -> Buffer
{
    return buffer;
}
```

Borrow parameters have borrow types.

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

A parameter name expression is checked according to the parameter’s type, ownership mode, borrow mode, mutation authority, lifetime, and capability contract.

---

### Function name expressions

A name expression that resolves to a function declaration produces a callable declaration or callable value according to context.

```bray
let op: func(left: i32, right: i32) -> i32 = add;
```

A function name used as the callee of a call expression participates in callable resolution.

```bray
add(left = 1, right = 2)
```

A function name used in a value context produces a function value when the function’s callable contract matches the expected callable type.

The function value carries its full callable contract.

The callable contract includes parameter names, parameter types, result type, execution mode, ownership behavior, borrowing behavior, mutation requirements, lifetime requirements, capability requirements, effects, trusted caller obligations, and finalization behavior.

A function with caller obligations can be used as a value only where the expected callable type preserves those obligations.

---

### Type name expressions

A name expression that resolves to a type produces a type-level entity.

Type-level entities appear in type positions, construction expressions, static function paths, trait or implementation paths, contract positions, and other compile-time contexts.

```bray
Point
Buffer
Shape
```

A type name used as the head of a construction expression can participate in struct construction or another type-defined construction form.

```bray
Point
{
    x = 1.0,
    y = 2.0,
}
```

A type name used in a path expression can expose variants, static functions, named constructors, or other type-associated declarations.

```bray
Shape.Circle(center = origin, radius = 1.0)
Point.origin()
```

Type-level name expressions are compile-time entities.

They do not produce runtime values unless used by a type-associated expression form that produces a value.

---

### Module and package name expressions

A name expression that resolves to a module or package produces a compile-time path entity.

```bray
math
pkg
```

Module and package name expressions are used as the left side of path expressions.

```bray
math.sin(angle = x)
pkg.module.Type
```

A module or package name expression participates in path resolution.

It does not produce a runtime value.

---

### Constant name expressions

A name expression can resolve to a visible constant declaration.

TODO: Define constant declaration syntax and binding rules.

A constant name expression produces the constant’s value or a compile-time constant entity according to context.

In expression context, a bare identifier can resolve to a constant.

In pattern context, bare identifiers bind. Constant matching in patterns uses a path pattern.

```bray
Color.Red
```

This distinction keeps pattern binding predictable while allowing constants to be used normally in expressions.

---

### Name resolution and shadowing

Name resolution is deterministic.

A name expression resolves according to the current lexical scope, declaration scope, module context, package context, and using declarations.

A local binding is introduced once.

A binding cannot be rebound.

A local binding declaration must not shadow an existing visible binding.

This rule keeps name expressions stable and prevents later local declarations from changing the meaning of earlier names in the same scope.

When a name expression is ambiguous after applying the language’s resolution rules, the program is rejected.

---

### Name expressions and initialization state

A name expression that reaches a local binding is checked against the binding’s initialization state.

A fully initialized binding can be observed, borrowed, moved, copied, consumed, or destroyed as a complete value according to its type and capabilities.

An uninitialized binding cannot be used as a complete value.

A partially initialized binding can be used only through access paths to initialized parts when the operation permits partial-state access.

A moved-from binding cannot be used as a complete value until reinitialized.

A destroyed binding is outside the set of usable value states.

---

### Name expressions and fact context

A name expression can read facts from the fact context.

Facts can describe initialization state, active union variant, field availability, borrow state, mutation authority, predicate facts, trusted facts, and other flow-sensitive information.

Using a name expression can also invalidate facts when the use moves, consumes, mutably borrows, assigns through, finalizes, or destroys the reached storage.

A name expression that only observes a stable value preserves facts that remain true under observation.

---

## Path expressions

A **path expression** is an expression formed by connecting path components with `.`.

```bray
math.sin
point.x
pkg.module.Type
Shape.Circle
```

Bray uses `.` for package paths, module paths, type paths, associated declarations, variant access, field access, method access, static function access, and nested access.

The binder determines the meaning of each path expression from the resolved left-hand entity and the selected right-hand component.

The left-hand side of `.` can resolve to:

- a package,
- a module,
- a type,
- a trait or trait application,
- a union type,
- a value,
- an access path,
- a box or other type-form value,
- another path-capable compile-time entity.

The result of a path expression can be:

- another path-capable entity,
- a field access path,
- a callable declaration,
- a callable value,
- a union variant constructor,
- a no-payload union variant value,
- a static function,
- a method candidate,
- a type-level entity,
- a module entity,
- a package entity,
- another compile-time entity.

---

### Package and module paths

A package or module path selects a declaration within a package or module namespace.

```bray
pkg.module.Type
pkg.module.function
math.sin
```

Package and module paths are compile-time paths.

They do not execute code.

They do not initialize modules.

They do not import unqualified names.

A referenced external path must be reachable through the current package or module context, or it must be declared by a `using` declaration.

```bray
using geometry.shapes;

func area(circle: geometry.shapes.Circle) -> r64
{
    ...
}
```

`using` declares that a module, package path, or declaration path is intentionally used by the current module.

`using` participates in dependency checking, visibility checking, internal-use acknowledgement, diagnostics, and tooling.

`using` does not import unqualified names.

`using` does not execute code.

`using` does not extend overload sets, conversions, operators, or behavioral contracts except through explicitly referenced paths.

---

### Internal path access

A path that reaches an internal declaration outside its intended scope requires acknowledgement.

Call-site acknowledgement uses `internal`.

```bray
let x = internal some.module.helper();
```

Module-level acknowledgement uses `using internal`.

```bray
using internal some.module.helper;
```

`using internal` acknowledges use of specific internal declarations or declaration paths.

`using internal` applies to declarations or declaration paths rather than entire modules or packages.

Acknowledgement is lexical.

Acknowledgement does not propagate through re-exports.

Re-exporting an internal declaration requires its own explicit acknowledgement and produces an internal export unless exposed through a public wrapper.

TODO: Define the full re-export rules for internal declarations.

A public API exposes internal declarations only through an explicit public wrapper that removes the internal declaration from the public signature.

---

### Type paths

A path whose left-hand side is a type selects a type-associated declaration.

```bray
Point.origin
Buffer.from_bytes
Shape.Circle
ParseResult<i32>.EndOfInput
```

Type-associated declarations include static functions, named constructors, union variants, and other declarations associated with the type by type declarations, implementations, or behavioral contracts.

A static function path can be called.

```bray
Point.origin()
```

A union payload variant path can be used as a variant construction expression.

```bray
Shape.Circle(center = origin, radius = 1.0)
```

A no-payload union variant path produces the corresponding union value.

```bray
ParseResult<i32>.EndOfInput
```

No-payload union variant construction uses no parentheses.

A type path can be used in compile-time contexts without producing a runtime value.

---

### Expected-type variant paths

A leading-dot variant path refers to a variant of the expected union type.

```bray
.Circle(center = origin, radius = 1.0)
.Empty
```

A leading-dot variant path is valid when the expression context provides a known expected union type and that union contains the named variant.

A leading-dot payload variant path participates in union variant construction.

A leading-dot no-payload variant path produces the no-payload variant value.

If the expected union type is absent, the full union path is required.

```bray
Shape.Circle(center = origin, radius = 1.0)
Shape.Empty
```

Expected-type propagation can make leading-dot variant paths available inside type-form construction expressions such as `box(...)`.

```bray
let node: box List<i32> = box(.Empty);
```

Here the expected type `box List<i32>` gives `box(...)` an inner expected type `List<i32>`, and `.Empty` resolves as a variant of `List<i32>`.

---

### Field paths

A path whose left-hand side is a value or access path can select a field.

```bray
point.x
point.y
```

A field path produces an access path to the selected field when the subject expression provides a compatible access path.

Field access observes, borrows, mutably borrows, moves, copies, consumes, or assigns through the selected field according to the operation context.

Observation of a field requires observe capability.

Mutable access to a field requires mutation authority over the reached storage and a field contract that permits mutation.

Fields are immutable by default.

A field declared `mut` can be mutated through a compatible mutable access path.

A mutable binding grants mutation authority over the binding’s access path. Field mutability still controls mutation of the reached field.

Field paths can be chained.

```bray
rectangle.min.x
```

Each component in the chain is checked in order.

The result of each component becomes the left-hand side for the next component.

Field access through a union payload requires active-variant refinement proving that the selected payload exists.

Field access through `box` or another type form follows the access and projection rules of that type form.

---

### Method paths and method calls

A path whose left-hand side is a value or access path can select a method candidate.

```bray
buffer.length
buffer.clear
point.distance_to
```

A method call expression calls the selected method candidate.

```bray
buffer.length()
buffer.clear()
point.distance_to(other = p)
```

Method resolution uses the receiver type, receiver capability, inherent implementations, trait implementations, visible declarations, constraints, and overload rules.

The method receiver is supplied by the left-hand expression.

The method receiver can be qualified by an exact trait application to select a trait implementation before method lookup.

```bray
buffer(Iterator<Bytes>).next()
```

This is a trait-qualified receiver expression.

It is not a runtime call, cast, conversion, or wrapper construction.

The method receiver mode is declared by the method kind.

```bray
func length() -> usize;
mut func clear();
consume func into_bytes() -> Bytes;
```

A method path used without a call can produce a callable value when the expected callable type captures the receiver and preserves the callable contract.

TODO: Define bound-method value rules.

---

### Static paths and static calls

A path whose left-hand side is a type, trait application, module, or package can select a static function.

```bray
Point.origin
Buffer.from_bytes
math.sin
```

A static function path can be called.

```bray
Point.origin()
Buffer.from_bytes(bytes = bytes)
math.sin(x = angle)
```

A static function call has no `self` receiver.

Static functions declared in trait or implementation contexts use `static func`.

```bray
static func origin() -> Point
{
    return Point { x = 0.0, y = 0.0, };
}
```

---

### Path expressions and call syntax

A path expression can be the callee of a call expression.

```bray
math.sin(x = angle)
Point.origin()
Shape.Circle(center = origin, radius = 1.0)
```

Function calls use named arguments for their parameters.

```bray
add(left = 1, right = 2)
```

Callable-like construction forms use named arguments where field or parameter identity matters.

Struct construction fields use names.

Union variant payload fields use names.

Named constructors and static functions use named arguments according to their parameter declarations.

Tuple expressions and array expressions are structural expressions and use positional element syntax because their element positions are their structure.

---

### Path expressions and assignment

A path expression that produces an assignable access path can be the destination of assignment.

```bray
point.x = 1.0;
counter.value = counter.value + 1;
```

Assignment through a path requires mutation authority over the destination access path.

Assignment through a field path requires the reached field to permit mutation.

Assignment through an active union payload field requires active-variant refinement.

Assignment through a type-form projection requires the type form to permit assignment to the reached storage.

Assignment returns `unit`.

Assignment invalidates facts that depend on the previous value or mutated storage.

---

### Path expressions and ownership

A path expression that produces an access path participates in ownership checking.

Observing a path expression preserves ownership.

Borrowing a path expression creates a borrow of the reached storage.

Mutably borrowing a path expression requires mutation authority and compatible exclusivity.

Moving from a path expression moves the reached value when the reached value is owned and the type is not copied instead.

Copying from a path expression requires the reached type to satisfy the copy contract.

Consuming through a path expression requires ownership of the reached value.

Moving a field or payload through a path expression can partially move the containing value.

A partially moved containing value cannot be used as a complete value until reinitialized or consumed by a rule that accounts for its state.

---

### Path expressions and fact context

A path expression can use and refine facts in the fact context.

Examples:

- active union variant facts enable payload field access,
- borrow facts determine whether a path can be borrowed or mutably borrowed,
- initialization facts determine whether a path can be used,
- trusted facts can permit trusted operations reached through paths,
- visibility and internal-use facts determine access to internal declarations.

A path expression can invalidate facts when it is used to move, consume, assign, mutably borrow, destroy, finalize, or otherwise change reached storage.

A path expression that only observes stable storage preserves facts that remain true under observation.

---

### Path resolution failures

A path expression is rejected when resolution fails.

Resolution fails when:

- a path component does not exist on the resolved left-hand entity,
- the component exists but is not visible,
- the component is internal and lacks required acknowledgement,
- the component is ambiguous,
- the selected operation requires capabilities that are unavailable,
- the selected operation requires facts that are absent,
- the selected operation is not valid for the left-hand entity kind,
- the path would expose an internal declaration through a public API without an explicit public wrapper.

Path resolution errors are reported during binding and checking.

---

## Index access expressions

An **index access expression** reaches an indexed element or component through an indexed access contract.

```bray
items[index]
matrix[row][column]
```

The expression before `[` is the indexed subject.

The expression inside `[` is the index expression.

The indexed subject is evaluated as an expression and must have a type that supports indexed access.

The index expression is evaluated as an expression and must have a type accepted by the indexed subject’s indexing contract.

For fixed-size arrays, index access reaches an array element.

```bray
values[index]
```

For nested arrays, repeated index access composes.

```bray
matrix[row][column]
```

The first index access reaches an element of the outer array. The second index access reaches an element of the inner array.

Index access can produce an access path when the subject expression produces a compatible access path.

An index access path can be observed, borrowed, mutably borrowed, moved from, copied from, consumed, assigned through, or destroyed according to the subject access path, element type, index contract, ownership state, initialization state, and capability state.

Observation through index access requires observe capability for the subject and the reached element.

Mutable access through index access requires mutation authority over the subject access path and mutation authority over the reached element.

Assignment through index access requires the index expression to identify an assignable element access path.

```bray
items[index] = value;
```

Moving from an indexed element is an ownership operation.

Moving from an indexed element requires ownership of the reached element and no conflicting active borrows.

Moving an element out of an aggregate can leave the aggregate partially initialized when the aggregate model permits partial moves.

A partially moved aggregate can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved aggregate destroys only still-initialized parts.

Copying from an indexed element requires the element type to satisfy the copy contract.

Borrowing an indexed element creates a borrow of the reached element.

Mutable borrowing an indexed element requires compatible exclusivity for the reached element.

Index access can refine or use facts in the fact context.

Facts can establish that an index is valid, that an element is initialized, or that an indexed access is within the subject’s bounds when the indexing contract exposes such facts.

Mutation, movement, consumption, destruction, reinitialization, or finalization of the subject or reached element can invalidate facts about indexed access.

TODO: Define bounds-checking behavior, index type requirements, slice behavior, and custom indexing contracts.

---

## Assignment expressions

An **assignment expression** writes a new value into an assignable access path.

```bray
x = value;
point.x = 2.0;
items[index] = value;
```

The left side of an assignment expression must produce an assignable access path.

The right side of an assignment expression must produce a value compatible with the destination type.

Assignment requires mutation authority over the destination access path.

Assignment returns `unit`.

```bray
counter.value = counter.value + 1;
```

The destination access path can be a local binding access path, field access path, indexed access path, active union payload field access path, or another expression form that produces an assignable access path.

A local binding can be assigned through when it has mutable local access authority and the storage and type contract permit reinitialization.

```bray
let mut x: i32 = 1;
x = 2;
```

A field can be assigned through when the reached field permits mutation and the access path has mutation authority.

```bray
struct Counter
{
    mut value: i64;
}

let mut counter: Counter = { value = 0, };
counter.value = 1;
```

A mutable binding alone grants mutation authority over the binding’s access path. Field mutability still controls mutation of reached fields.

```bray
struct Point
{
    x: r64;
    y: r64;
}

let mut p: Point = { x = 1.0, y = 2.0, };
```

The binding `p` has mutable local access authority. The fields `x` and `y` remain immutable after initialization because their field declarations do not declare mutation.

Assignment to a union value replaces the active variant when the union access path has mutation authority.

```bray
shape = .Rectangle(min = a, max = b);
```

Whole-union replacement ends the old active variant payload and initializes the new active variant payload.

Assignment to a union payload field requires active-variant refinement, mutation authority over the union value, and a mutable payload field.

Assignment ends the previous value in the destination according to destruction and lifecycle rules.

Assignment initializes or re-initializes the destination with the new value.

Reinitialization must be permitted by the destination storage and type contract.

The assigned value is moved into the destination unless the type is copyable or another explicit rule applies.

If the right side moves from an access path, that source access path becomes moved-from until reinitialized.

If assignment overwrites a value with a finalization obligation, the obligation must be completed, transferred, or converted into an explicit fallback ownership form before the old value’s ownership ends.

Assignment invalidates facts that depend on the previous value stored in the destination.

Assignment invalidates facts that depend on storage changed by the assignment.

Assignment preserves facts that remain true after the destination is reinitialized.

Assignment participates in effect and capability checking.

TODO: Define the general evaluation order for assignment subexpressions.

The assignment operation itself requires the destination access path and assigned value to be established before the destination is reinitialized.

---

## Borrow expressions

A **borrow expression** creates a non-owning access path to reached storage.

A shared borrow expression uses `&`.

```bray
&value
```

A mutable borrow expression uses `&mut`.

```bray
&mut value
```

A shared borrow expression produces a value whose type uses the shared-borrow type form.

```bray
&T
```

A mutable borrow expression produces a value whose type uses the mutable-borrow type form.

```bray
&mut T
```

A shared borrow requires an access path that can be observed.

A mutable borrow requires mutation authority over the reached storage and compatible exclusivity for the duration of the borrow.

Borrowing does not transfer ownership of the reached storage.

The owner remains responsible for destruction and finalization unless another explicit ownership construct changes that responsibility.

A shared borrow allows observation through the borrow.

Multiple compatible shared borrows can exist at the same time.

A mutable borrow grants temporary exclusive mutation authority over the storage reached by that borrow layer.

While a mutable borrow is active, incompatible operations on the borrowed storage are suspended.

Borrowing suspends movement, destruction, mutation, or other incompatible operations on the original access path for the duration of the borrow.

A borrow has a lifetime.

The borrow is valid only while the reached storage remains valid and the borrow’s capability contract remains satisfied.

A borrow cannot outlive the storage it reaches.

A borrow expression can borrow a local binding, field access path, indexed access path, active union payload access path, dereferenced type-form projection, or another expression that produces a compatible access path.

Nested borrow types are allowed.

```bray
&T
&mut T
&&T
&&mut T
&mut &T
&mut &mut T
```

Each borrow layer has its own capability.

An outer shared borrow provides shared access to the next layer.

An outer mutable borrow provides mutation authority over the next layer.

The reachable operation depends on the whole access path, including every borrow layer.

A reborrow can be created from an existing borrow.

A reborrow has authority no greater than the borrow it comes from.

A reborrow suspends incompatible use of the original borrow for the reached storage while the reborrow is active.

Borrow expressions participate in fact-context checking.

Facts about borrowed storage can remain available through a borrow when observation preserves those facts.

Mutation through a mutable borrow invalidates facts that depend on the changed storage.

Movement, destruction, reinitialization, finalization, or capability loss invalidates facts that depend on the borrowed storage.

---

## Function call expressions

A **function call expression** calls a callable declaration or callable value.

```bray
add(left = 1, right = 2)
```

Function call arguments are always named.

Argument names are part of the call syntax.

Argument names cannot be omitted.

```bray
add(left = 1, right = 2)
```

A function call with unnamed positional arguments is rejected.

```bray
add(1, 2)
```

A function call expression has a callee and an argument list.

The callee must resolve to a callable declaration or callable value.

The argument list supplies argument expressions to callable parameters by name.

Each argument name must correspond to exactly one parameter in the callable contract.

Duplicate argument names are errors.

Unknown argument names are errors.

Missing parameters are errors unless the missing parameters have defaults.

Every supplied argument expression is checked against the corresponding parameter’s type, ownership mode, borrow mode, mutation requirements, lifetime requirements, capability requirements, effect requirements, and contract requirements.

A callable parameter can receive an owned value, copied value, shared borrow, mutable borrow, consumed value, or other allowed argument form according to its parameter contract.

If a parameter takes an owned value, the corresponding argument is moved into the call unless the argument type is copyable or another explicit rule applies.

If a parameter takes a shared borrow, the corresponding argument must provide a compatible observable access path or borrow value.

If a parameter takes a mutable borrow, the corresponding argument must provide mutation authority and compatible exclusivity for the reached storage.

If a parameter consumes a value, the argument’s old access path becomes unavailable after the call unless reinitialized.

A call expression produces the callable’s declared result.

A call to a callable returning `unit` produces `unit`.

A call to a callable returning `never` has no normal continuation.

A call to an async callable produces an owned async computation.

A call expression can use ordinary contract facts from the fact context to satisfy `requires(...)`.

A call expression can use trusted facts from the fact context to satisfy trusted requirements.

A call expression that needs a trusted caller obligation must have the obligation established in the fact context, explicitly acknowledged at a trust boundary, or exposed through the surrounding declaration’s contract.

A call expression can establish facts from the callable’s `ensures(...)` clause after successful completion.

Facts established by a call are tied to the values, storage identities, lifetimes, capabilities, and versions referenced by the ensures clause.

A call expression participates in overload resolution when the callee resolves to an overload declaration.

A call resolves to exactly one callable after name resolution, argument-name matching, type checking, ownership checking, capability checking, effect checking, contract checking, and overload resolution.

Overload resolution uses only arguments explicitly supplied by the caller.

Default arguments do not make an overload arm selectable.

Result type and expected type do not participate in overload resolution.

Ambiguous calls are rejected.

TODO: Define the general evaluation order for callee and argument expressions.

---

## Arguments

An **argument** is a named expression supplied to a callable parameter, constructor parameter, method parameter, static function parameter, named constructor parameter, or another callable-like parameter.

```bray
left = 1
right = 2
mode = FileMode.write
```

Arguments are always named in callable calls.

The argument name identifies the parameter being supplied.

The argument expression supplies the value or access path checked against that parameter.

```bray
add(left = 1, right = 2)
```

Argument names cannot be omitted.

Argument order does not determine parameter binding.

Argument order is source order for evaluation only if the evaluation-order model chooses source-order argument evaluation.

Argument order does not change which parameter receives which argument.

Duplicate arguments for the same parameter are errors.

Arguments for parameters that do not exist are errors.

A required parameter without a supplied argument and without a default is an error.

An argument expression is checked in the expected context of the corresponding parameter.

Expected parameter context can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, tuple element typing, array element typing, and conversion checking.

```bray
draw(shape = .Circle(center = origin, radius = 1.0))
```

Here the parameter type of `shape` can provide the expected union type for `.Circle(...)`.

Arguments can supply owned values.

```bray
consume(buffer = buffer)
```

Arguments can supply shared borrows.

```bray
read(buffer = &buffer)
```

Arguments can supply mutable borrows.

```bray
fill(buffer = &mut buffer)
```

Arguments can supply values constructed inline.

```bray
draw(shape = Shape.Circle(center = origin, radius = 1.0))
```

Argument expressions participate in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

A function call, method call, or static function call cannot use tuple-style positional syntax to satisfy parameters.

Tuple expressions and array expressions remain positional structural expressions because their positions are the structure being constructed, not callable parameter binding.

---

## Defaulted arguments

A **defaulted argument** is an omitted parameter value supplied by the parameter’s declared default expression.

A callable parameter can declare a default value.

```bray
func retry(count: i32 = 3, delay: Duration = Duration.seconds(value = 1))
{
    ...
}
```

A call can omit a parameter that has a default.

```bray
retry();
retry(count = 5);
retry(delay = Duration.seconds(value = 2));
```

The omitted parameter receives its declared default expression.

A parameter without a default must be supplied by an argument.

Default expressions are checked in the declaration context where they are written.

A default expression cannot depend on call-site local bindings unless those bindings are supplied through explicit arguments or otherwise available through the callable’s declared context.

A default expression is evaluated when the corresponding argument is omitted.

A defaulted argument participates in the call expression as if the omitted argument expression had been supplied by the declaration.

Defaulted argument evaluation participates in type checking, ownership checking, effects, capability checking, finalization obligations, trusted capability checking, and fact-context behavior.

Effects of a default expression become effects of the call expression when the default is used.

Finalization obligations created by a default expression become obligations of the call expression result or local temporaries according to ownership rules.

Trusted capabilities used by a default expression must be permitted by the declaration that owns the default expression.

A default expression must satisfy the parameter type and contract.

A default expression for a parameter is evaluated only when the parameter is omitted.

Supplying an explicit argument suppresses evaluation of that parameter’s default expression.

Default arguments are applied to direct calls after the callable has been selected.

Default arguments do not participate in overload selection.

Duplicate supplied arguments remain errors even when a parameter has a default.

Unknown supplied arguments remain errors even when other parameters have defaults.

TODO: Define the ordering of explicit argument evaluation and default argument evaluation.

Struct field defaults and union variant payload defaults follow their own construction-field default rules. They use the same principle that omitted fields evaluate their defaults as part of construction.

---

## Method call expressions

A **method call expression** calls an instance-level function through a receiver expression.

```bray
buffer.length()
buffer.clear()
point.distance_to(other = other)
```

The expression before the method name is the receiver expression.

The receiver is supplied implicitly by the method call syntax.

Method parameters other than the receiver are supplied by named arguments.

Argument names cannot be omitted.

```bray
point.distance_to(other = other)
```

A method call with unnamed non-receiver arguments is rejected.

```bray
point.distance_to(other)
```

Inside a method body, `self` is the receiver keyword.

`Self` refers to the implementing type inside traits and implementation blocks.

The receiver is not written as an ordinary parameter in method declarations.

In trait and implementation contexts, `func` declares an instance method by default.

```bray
func length() -> usize;
```

`func` declares a shared receiver method.

`mut func` declares a mutable receiver method.

```bray
mut func clear();
```

`consume func` declares a consuming receiver method.

```bray
consume func into_bytes() -> Bytes;
```

`consume mut func` declares a consuming receiver method whose method body has mutable local authority over `self`.

A method call checks the receiver expression against the method’s receiver mode.

A shared receiver method requires a compatible observable receiver access path.

A mutable receiver method requires mutation authority and compatible exclusivity for the receiver storage.

A consuming receiver method requires ownership of the receiver value.

A consuming receiver method makes the receiver’s old access path unavailable after the call unless reinitialized.

A method call resolves through the receiver type, receiver capability, inherent implementations, participating trait implementations, visible declarations, constraints, and overload rules.

A method call resolves to exactly one method after receiver checking, argument-name matching, type checking, ownership checking, capability checking, effect checking, contract checking, and overload resolution.

For overloaded methods, the receiver mode and explicitly supplied method arguments select the overload arm.

For method calls through a trait implementation overload family, receiver mode, receiver compatibility, member name, and explicitly supplied method arguments select the implementation arm.

Result type, expected type, and type-valued member outputs do not select an implementation arm.

If more than one implementation arm remains possible, the method call is rejected as ambiguous.

If the receiver is trait-qualified, the exact trait application is selected before member lookup.

Method call arguments follow the same named-argument rules as function calls.

A method call produces the method’s declared result.

A method call to an async method produces an owned async computation.

A method call can establish facts from the method’s `ensures(...)` clause after successful completion.

A method call can require ordinary or trusted preconditions through `requires(...)`.

Trusted caller obligations must be present in the fact context, acknowledged at a trust boundary, or exposed through the surrounding declaration’s contract.

A method path used without a call can produce a callable value when the expected callable type captures or represents the receiver and preserves the callable contract.

TODO: Define bound-method value rules.

TODO: Define the general evaluation order for receiver and argument expressions.

---

## Static function call expressions

A **static function call expression** calls a type-level function associated with a type, trait application, implementation, module, package, or other path-capable entity.

```bray
Point.origin()
Buffer.from_bytes(bytes = bytes)
math.sin(x = angle)
```

A static function call has no `self` receiver.

Static functions are declared with `static func` inside trait and implementation blocks.

```bray
static func origin() -> Point
{
    return Point { x = 0.0, y = 0.0, };
}
```

A static function call expression has a callee path and an argument list.

The callee path must resolve to a static callable declaration or callable value.

Arguments are always named.

Argument names cannot be omitted.

```bray
Buffer.from_bytes(bytes = bytes)
math.sin(x = angle)
```

A static function call with unnamed arguments is rejected.

```bray
math.sin(angle)
```

Every supplied argument name must correspond to exactly one parameter in the static callable contract.

Duplicate argument names are errors.

Unknown argument names are errors.

Missing parameters are errors unless those parameters have defaults.

Each argument expression is checked against the corresponding parameter’s type, ownership mode, borrow mode, mutation requirements, lifetime requirements, capability requirements, effect requirements, and contract requirements.

A static function call produces the static function’s declared result.

A static function call to an async static function produces an owned async computation.

A static function call can use ordinary and trusted facts from the fact context to satisfy preconditions.

A static function call can establish facts from the static function’s `ensures(...)` clause after successful completion.

A static function call participates in overload resolution when the path resolves to an overload declaration.

A static function call resolves to exactly one callable after path resolution, argument-name matching, type checking, ownership checking, capability checking, effect checking, contract checking, and overload resolution.

TODO: Define the general evaluation order for callee path resolution and argument expressions.

---

## Tuple expressions

A **tuple expression** constructs a tuple value from a fixed sequence of element expressions.

```bray
(1, 2)
```

A tuple expression has fixed arity.

The tuple arity is the number of element expressions in the tuple expression.

The tuple type contains the type of each element in order.

```bray
let pair: (i32, r64) = (1, 2.0);
```

A tuple expression with two or more elements uses comma-separated element expressions.

```bray
(1, 2)
(1, 2, 3)
```

A one-element tuple expression uses a trailing comma.

```bray
let single: (i32,) = (1,);
```

Parentheses around one expression without a tuple comma form an expression grouping.

```bray
let grouped: i32 = (1 + 2);
```

TODO: Define unit value spelling.

A tuple expression is checked against an expected tuple type when one is available.

```bray
let pair: (i32, r64) = (1, 2.0);
```

When an expected tuple type is available, the expected tuple arity must equal the tuple expression arity.

Each element expression is checked against the corresponding expected element type.

Expected element types can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, conversion checking, and nested expression checking.

```bray
let nested: ((i32, r64), bool) = ((1, 2.0), true);
```

The outer expected tuple type gives the first element the expected type `(i32, r64)`. The nested tuple expression is then checked by the same tuple-expression rules.

When no expected tuple type is available, each element expression is checked from its own context, and the resulting element types form the tuple type.

```bray
let pair = (1, 2.0);
```

When no other context applies, default literal types apply to unconstrained numeric literal elements.

Nested tuple expressions compose recursively.

```bray
let nested: ((i32, i32), bool) = ((1, 2), true);
```

A tuple element can itself be any expression that produces a value compatible with that element position.

A tuple expression is fully initialized when every element is fully initialized.

Each tuple element has its own initialization state while the tuple is being constructed.

If evaluation of an element exits through `return`, `yield`, `break`, `continue`, `never`, cancellation, abort-like control flow, or another non-local exit before the tuple is fully initialized, already-initialized element temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A tuple expression owns its elements when the element expressions produce owned values that are moved into the tuple.

A tuple expression copies an element when the element expression is copied according to that element type’s copy contract.

A tuple expression can contain borrowed values when an element expression produces a borrow value.

Moving a complete tuple moves every initialized element as part of the tuple move.

Copying a tuple requires every element type to satisfy the required copy contract.

Borrowing a tuple borrows the tuple storage.

Projecting a tuple element from a tuple access path creates an access path to that element.

Shared borrowing a tuple can provide shared access to tuple elements according to Bray's borrowing rules.

Mutable borrowing a tuple can provide mutable access to tuple elements when the tuple access path, element access path, and element type permit mutation.

Moving an element out of a tuple is a partial move of the tuple.

A partial move from a tuple requires ownership of the tuple and no conflicting active borrows.

After a tuple element has been moved out, the tuple is partially initialized.

A partially moved tuple can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved tuple destroys only still-initialized elements.

Destruction of a fully initialized tuple destroys its initialized elements according to Bray tuple destruction order.

Tuple expressions participate in effect checking and capability checking through their element expressions.

The tuple expression’s effects are the combined effects of evaluating its element expressions.

The tuple expression’s finalization obligations are the combined finalization obligations of values produced by its element expressions and retained by the resulting tuple.

A tuple expression can establish facts in the fact context.

Facts can include tuple arity, element initialization, element types, and facts established by element expressions.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about tuple elements or the tuple as a whole.

A tuple expression can be converted with `as` when the recursive explicit convertibility rules permit tuple conversion.

```bray
let a: (i32, r32) = (1, 2.0);
let b: (i64, r64) = a as (i64, r64);
```

Tuple-to-tuple conversion requires the same arity and an explicitly valid conversion for each corresponding element.

A two-element tuple can be explicitly converted to a built-in complex type when both elements can be explicitly converted to the complex type’s component real type.

```bray
let z: c128 = (real, imag) as c128;
```

---

## Array expressions

An **array expression** constructs a fixed-size array value.

An array expression with individually supplied elements uses comma-separated element expressions inside square brackets.

```bray
let values: [i32; 4] = [1, 2, 3, 4];
```

The array length is the number of element expressions.

The array element type is determined by the expected array type when one is available, or inferred from the element expressions when no expected array type is available.

When an expected array type is available, the expected array length must equal the number of supplied element expressions.

```bray
let values: [i64; 3] = [1, 2, 3];
```

Each element expression is checked against the expected array element type.

Expected element type can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, conversion checking, and nested expression checking.

When no expected array type is available, the element expressions must determine a coherent array element type.

Unconstrained numeric literals use default literal types when no expected element type fixes them.

Nested arrays are constructed compositionally.

```bray
let matrix: [[i32; 2]; 2] =
[
    [1, 2],
    [3, 4],
];
```

The outer array has element type `[i32; 2]`.

Each inner array expression is checked against `[i32; 2]`.

Each inner array expression follows the same array-expression rules recursively.

Every supplied element expression must produce a value compatible with the array element type.

An array expression is fully initialized when every element is fully initialized.

Each array element has its own initialization state while the array is being constructed.

If evaluation of an element exits through `return`, `yield`, `break`, `continue`, `never`, cancellation, abort-like control flow, or another non-local exit before the array is fully initialized, already-initialized element temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

An array expression owns its elements when the element expressions produce owned values moved into the array.

An array expression copies an element when the element expression is copied according to the element type’s copy contract.

An array expression can contain borrowed values when element expressions produce borrow values.

Moving a complete array moves every initialized element as part of the array move.

Copying an array requires the element type to satisfy the required copy contract.

Borrowing an array borrows the array storage.

Indexing into an array access path creates an access path to an element.

Shared borrowing an array can provide shared access to elements according to Bray's borrowing rules.

Mutable borrowing an array can provide mutable access to elements when the array access path, element access path, and element type permit mutation.

Moving an element out of an array is a partial move of the array when the array model permits element moves.

A partial move from an array requires ownership of the array and no conflicting active borrows.

After an array element has been moved out, the array is partially initialized.

A partially moved array can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved array destroys only still-initialized elements.

Destruction of a fully initialized array destroys its initialized elements according to Bray array destruction order.

Array expressions participate in effect checking and capability checking through their element expressions.

The array expression’s effects are the combined effects of evaluating its element expressions.

The array expression’s finalization obligations are the combined finalization obligations of values produced by its element expressions and retained by the resulting array.

An array expression can establish facts in the fact context.

Facts can include array length, element initialization, element type, and facts established by element expressions.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about array elements or the array as a whole.

An array expression can be converted with `as` when the recursive explicit convertibility rules permit array conversion.

```bray
let a: [i32; 4] = [1, 2, 3, 4];
let b: [i64; 4] = a as [i64; 4];
```

Array-to-array conversion requires the same length and an explicitly valid conversion from the source element type to the target element type.

Array conversion preserves length and shape.

### Repeated-element array expressions

A repeated-element array expression uses `[element; count]`.

```bray
let zeros: [i32; 4] = [0; 4];
```

The count determines the array length.

The count participates in the array type.

The count must be known where the array length is required as a compile-time value.

The element expression is checked against the expected array element type when one is available.

The repeated element expression must be copyable or define a valid repeat contract.

A repeated-element array expression evaluates the element expression according to the repeat contract and initializes each array element according to that contract.

For copy-based repetition, the repeated value is copied into each element according to the element type’s copy contract.

A repeated-element array expression is fully initialized when every array element has been initialized.

Effects and finalization obligations of the repeated element expression and repeat operation become part of the repeated-element array expression.

---

## Array generator iteration expressions

An **array generator iteration expression** constructs a fixed-size array by iterating over a source and yielding array elements.

```bray
let xs: [i32; 4] =
[
    each i in 0..4
    {
        yield i * 2;
    }
];
```

An array generator iteration expression appears inside an array expression.

The expression has the form:

```bray
[
    each pattern in source
    {
        ...
    }
]
```

The `source` expression is evaluated once before iteration begins.

The source expression must provide an iteration contract.

The iteration contract defines:

- the element type,
- the element access mode,
- the iteration order,
- the cardinality,
- whether the cardinality is statically known,
- whether the iteration is finite,
- ownership and borrowing behavior for each produced element.

The `pattern` is checked against the source element type.

The pattern must be irrefutable for the source element type.

The pattern operation mode is determined by the source iteration contract.

The pattern can introduce one or more iteration bindings.

Each iteration creates fresh bindings from the pattern.

Iteration bindings are scoped to the iteration body.

Iteration bindings are not visible in the source expression.

Iteration bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and finalization rules.

The iteration body is a block expression in array-generator context.

Array-generator context is yield-capable.

`yield` inside the iteration body supplies an element to the nearest enclosing array generator region.

A fixed-size array generator must yield exactly one array element per iteration.

A fixed-size array generator must yield exactly `N` total elements, where `N` is the length of the resulting array type.

The compiler must be able to prove the required cardinality.

When the compiler cannot prove that the array generator yields exactly the required number of elements, the array generator expression is rejected.

The yielded value is checked against the array element type.

Expected array element type can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, conversion checking, and nested expression checking inside yielded expressions.

A yielded value is moved into the array unless it is copied according to the element type’s copy contract or another explicit rule applies.

The array is fully initialized when every required element has been yielded and initialized.

If iteration exits before the array is fully initialized through `return`, `break`, `continue`, `never`, cancellation, abort-like control flow, or another control-flow exit, initialized elements and live temporaries are handled by the corresponding ownership, destruction, and finalization rules.

`continue` targets the nearest iteration region.

In a fixed-size array generator, any control-flow path that continues an iteration before yielding that iteration’s required element is rejected unless the compiler can prove the required yield still occurs.

`break` targets the nearest iteration region that accepts `break`.

In a fixed-size array generator, breaking out before producing the required number of elements is rejected unless the surrounding construct defines a valid array result for that exit.

Nested yield-capable regions capture their own yields.

```bray
let matrix: [[i32; 2]; 2] =
[
    each row in 0..2
    {
        yield [
            each col in 0..2
            {
                yield row + col;
            }
        ];
    }
];
```

In the nested example, the inner `yield row + col;` supplies the inner array generator.

The outer `yield [...]` supplies the outer array generator.

The source expression of each generator is evaluated once for that generator.

Nested array generator expressions are checked recursively.

Array generator iteration expressions participate in effect checking and capability checking through the source expression, pattern operation, iteration body, yielded expressions, and iteration contract.

Effects of the source expression occur once before iteration.

Effects of the iteration body occur once per executed iteration.

Finalization obligations created in an iteration body must be completed, transferred, or moved into yielded values before the iteration body exits.

Finalization obligations of yielded values become part of the resulting array.

Facts established by the source expression, pattern, and iteration body are scoped according to the iteration region.

Facts tied to an iteration binding expire at the end of that iteration unless they are transferred into the yielded value or another surviving storage location.

---

## General generator iteration expressions

A **general generator iteration expression** iterates over a source inside a multi-yield generator region.

TODO: Define general generator enclosing syntax.

The iteration form is:

```bray
each pattern in source
{
    ...
}
```

A general generator iteration expression is valid inside a generator region that accepts zero or more yielded values.

The `source` expression is evaluated once before iteration begins.

The source expression must provide an iteration contract.

The iteration contract defines:

- the element type,
- the element access mode,
- the iteration order,
- the cardinality when known,
- whether the iteration is finite,
- ownership and borrowing behavior for each produced element.

The `pattern` is checked against the source element type.

The pattern must be irrefutable for the source element type.

The pattern operation mode is determined by the source iteration contract.

Each iteration creates fresh bindings from the pattern.

Iteration bindings are scoped to the iteration body.

Iteration bindings are not visible in the source expression.

Iteration bindings are destroyed or ended at the end of each iteration according to ownership, borrowing, destruction, and finalization rules.

The iteration body is a block expression in generator-iteration context.

Generator-iteration context is yield-capable when an enclosing generator region accepts yielded values.

A general generator iteration body can yield zero or more values unless the enclosing generator region imposes a stricter cardinality rule.

Each yielded value is supplied to the nearest enclosing generator region.

A yielded value must be compatible with the enclosing generator’s expected element type when one is known.

Expected generator element type can guide literal typing, variant shorthand, struct construction shorthand, box construction shorthand, conversion checking, and nested expression checking inside yielded expressions.

Nested yield-capable regions capture their own yields.

An inner `yield` supplies the inner yield-capable region.

A `yield` in the generator iteration body supplies the nearest enclosing generator region that the `yield` targets.

`continue` targets the nearest iteration region.

`break` targets the nearest iteration region that accepts `break`.

A general generator iteration expression can have unknown or runtime cardinality when the enclosing generator region accepts variable cardinality.

A general generator iteration expression must have statically provable cardinality when the enclosing generator region requires statically known cardinality.

Array generator regions require statically provable cardinality matching the array length.

Effects of the source expression occur once before iteration.

Effects of the iteration body occur once per executed iteration.

Finalization obligations created inside an iteration body must be completed, transferred, converted into an explicit fallback ownership form, or moved into yielded values before the iteration body exits.

Facts established by the source expression, pattern, and iteration body are scoped according to the iteration region.

Facts tied to an iteration binding expire at the end of that iteration unless they are transferred into a yielded value or another surviving storage location.

A general generator iteration expression participates in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

The completion result of a generator iteration expression is `unit`.

The values produced by `yield` are delivered to the enclosing generator region rather than becoming the direct completion result of the iteration expression.

---

## Struct construction expressions

A **struct construction expression** creates a fully initialized value of a struct type.

A full struct construction expression names the struct type before the construction body.

```bray
let p = Point
{
    x = 1.0,
    y = 2.0,
};
```

An expected-type struct construction expression omits the struct type when the expected type is known.

```bray
let p: Point =
{
    x = 1.0,
    y = 2.0,
};
```

The expected-type form is valid when expression context provides a known struct type.

The full type form is required when the expected type is absent or insufficient for resolution.

The construction body contains field initializers.

Field initializers are comma-separated.

Trailing commas are allowed.

Field initializers use `=`.

```bray
Point
{
    x = 1.0,
    y = 2.0,
}
```

Each field initializer names a field of the struct.

Field names cannot be omitted.

Field order does not matter.

A construction expression that initializes the same field more than once is rejected.

A construction expression that names a field not declared by the struct is rejected.

Every field without a default must be initialized by the construction expression.

A field with a default can be omitted.

An omitted defaulted field is initialized from its declared default expression.

A field default is checked in the struct declaration context.

A field default cannot reference sibling fields.

A field default cannot reference `self`.

A field default is evaluated when the field is omitted during construction.

A supplied field initializer suppresses evaluation of that field’s default expression.

A field initializer expression is checked against the declared field type.

The declared field type can provide expected type context to the initializer expression.

Expected field type can guide literal typing, union variant shorthand, nested struct construction shorthand, box construction shorthand, tuple element typing, array element typing, and conversion checking.

```bray
let shape: Shape =
{
    center = { x = 0.0, y = 0.0, },
    kind = .Circle(radius = 1.0),
};
```

A struct construction expression is fully initialized when every required field has been initialized and every omitted defaulted field has been initialized from its default.

Each field has its own initialization state during construction.

If evaluation exits before construction completes, already-initialized field values and temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A struct construction expression produces an owned value of the constructed struct type.

Each supplied initializer value is moved into its field unless the value is copied according to its type’s copy contract or another explicit rule applies.

Each defaulted field value is moved into its field unless the default expression produces a copied value or another explicit rule applies.

Effects of supplied field initializer expressions are effects of the struct construction expression.

Effects of evaluated default expressions are effects of the struct construction expression.

Finalization obligations created by supplied field initializer expressions or evaluated default expressions become obligations of the constructed value, local temporaries, or surrounding context according to ownership and lifecycle rules.

A struct construction expression participates in capability checking.

A field initializer can use only the capabilities available in the construction expression’s surrounding context.

A field default can use only the capabilities available to the declaration that defines the default and to the construction context according to the default-expression rules.

Trusted capabilities used by defaults or field initializers must be permitted by the surrounding trusted declaration or rejected according to the Contract and Trust Model.

A struct construction expression can establish facts in the fact context.

Facts can include the constructed type, full initialization of the constructed value, initialized fields, and facts established by field initializer expressions.

Facts about omitted defaulted fields can be established when the default expression establishes those facts and the facts remain valid after construction.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about the constructed value or its fields.

A struct construction expression creates a new value. Initialization performed by the construction expression is initialization, not ordinary mutation.

A value being constructed has no stable observable identity until construction is complete.

Field mutability controls post-initialization mutation of fields. It does not restrict initialization of fields during construction.

```bray
struct Counter
{
    mut value: i64;
}

let c: Counter =
{
    value = 0,
};
```

The field `value` is initialized during construction. Its `mut` field declaration controls later mutation through compatible mutable access paths.

TODO: Define the general evaluation order of field initializer expressions and default expressions.

---

## Union variant construction expressions

A **union variant construction expression** creates a fully initialized value of a union type with one active variant.

A full payload variant construction expression names the union type and variant.

```bray
let shape = Shape.Circle(center = origin, radius = 10.0);
```

An expected-type payload variant construction expression uses leading-dot shorthand when the expected union type is known.

```bray
let shape: Shape = .Circle(center = origin, radius = 10.0);
```

A full no-payload variant construction expression names the union type and variant.

```bray
let result = ParseResult<i32>.EndOfInput;
```

An expected-type no-payload variant construction expression uses leading-dot shorthand when the expected union type is known.

```bray
let result: ParseResult<i32> = .EndOfInput;
```

A leading-dot variant construction expression is valid when expression context provides a known union type and that union contains the named variant.

The full union path is required when the expected union type is absent or insufficient for resolution.

A no-payload variant construction expression uses no parentheses.

A payload variant construction expression uses parentheses containing payload field initializers.

Payload field initializers are comma-separated.

Trailing commas are allowed.

Payload field initializers use `=`.

```bray
Shape.Circle(
    center = origin,
    radius = 10.0,
)
```

Payload fields are always named.

Payload field names cannot be omitted.

Payload field order does not matter.

A variant construction expression that initializes the same payload field more than once is rejected.

A variant construction expression that names a payload field not declared by the selected variant is rejected.

Every payload field without a default must be initialized by the construction expression.

A payload field with a default can be omitted.

An omitted defaulted payload field is initialized from its declared default expression.

A variant payload default is checked in the union declaration context.

A variant payload default cannot reference sibling payload fields.

A variant payload default cannot reference `self`.

A variant payload default is evaluated when the payload field is omitted during construction.

A supplied payload field initializer suppresses evaluation of that payload field’s default expression.

A payload field initializer expression is checked against the declared payload field type.

The declared payload field type can provide expected type context to the initializer expression.

Expected payload field type can guide literal typing, union variant shorthand, nested struct construction shorthand, box construction shorthand, tuple element typing, array element typing, and conversion checking.

```bray
let event: Event = .Nested(
    inner = .Started(time = now),
);
```

Variant contracts are checked during construction.

A `requires(...)` clause on a variant must be satisfied by the construction expression.

A successful variant construction can establish facts declared by the variant’s `ensures(...)` clause.

A successful variant construction establishes that the produced union value has the selected active variant.

For a payload variant, successful construction establishes that the selected payload exists and that its initialized payload fields are initialized.

For a no-payload variant, successful construction establishes the active variant and introduces no payload fields.

A union variant construction expression is fully initialized when the active tag has been initialized and the selected variant payload, if any, has been fully initialized.

Inactive variant payloads have no initialized values.

If evaluation exits before construction completes, already-initialized payload values and temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A union variant construction expression produces an owned value of the union type.

Each supplied payload initializer value is moved into its payload field unless the value is copied according to its type’s copy contract or another explicit rule applies.

Each defaulted payload value is moved into its payload field unless the default expression produces a copied value or another explicit rule applies.

Effects of supplied payload initializer expressions are effects of the union variant construction expression.

Effects of evaluated payload defaults are effects of the union variant construction expression.

Finalization obligations created by supplied payload initializer expressions or evaluated payload defaults become obligations of the constructed union value, local temporaries, or surrounding context according to ownership and lifecycle rules.

A union variant construction expression participates in capability checking.

A payload initializer can use only the capabilities available in the construction expression’s surrounding context.

A payload default can use only the capabilities available to the declaration that defines the default and to the construction context according to the default-expression rules.

Trusted capabilities used by defaults, payload initializers, or variant contracts must be permitted by the surrounding trusted declaration or rejected according to the Contract and Trust Model.

A union variant construction expression can establish facts in the fact context.

Facts can include the union type, selected active variant, initialized active payload, initialized payload fields, and facts established by payload initializer expressions, defaults, or variant contracts.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about the constructed union value or active payload.

A union variant construction expression creates a new value. Initialization performed by the construction expression is initialization, not ordinary mutation.

A value being constructed has no stable observable identity until construction is complete.

Payload field mutability controls post-initialization mutation of payload fields. It does not restrict initialization of payload fields during construction.

---

## Box construction expressions

A **box construction expression** creates an owned indirection value.

A default-storage box construction expression uses `box(...)`.

```bray
let node: box List<i32> = box(List<i32>.Empty);
```

An explicit-storage box construction expression uses `box[S](...)`, where `S` is the storage policy type.

```bray
let node: box[Heap] List<i32> = box[Heap](List<i32>.Empty);
```

A box construction expression produces a value whose type uses the `box` type form.

```bray
box[S] T
```

`S` is the storage policy type.

`T` is the contained subject type.

`box T` uses the default storage policy.

`box[S] T` uses storage policy type `S`.

The storage policy type must satisfy the storage behavior required by `box[S] T`.

A `box[S](value, ...)` expression constructs a `box[S] T` from a contained value of type `T`.

The contained value expression is the first runtime argument to the box construction expression.

Additional runtime arguments are named.

Argument names cannot be omitted.

```bray
let point: box[AllocatorStorage<MyAllocator>] Point =
    box[AllocatorStorage<MyAllocator>](
        { x = 1.0, y = 2.0, },
        storage = storage,
    );
```

A box construction expression without explicit `[S]` uses the expected box type when one is available.

```bray
let node: box[Heap] List<i32> = box(List<i32>.Empty);
```

Here the expected type provides storage policy `Heap` and contained type `List<i32>`.

A box construction expression can propagate expected contained type into the contained value expression.

```bray
let node: box List<i32> = box(.Empty);
```

Here the expected type `box List<i32>` gives `box(...)` the contained expected type `List<i32>`, which lets `.Empty` resolve as a variant of `List<i32>`.

When no expected box type is available and no explicit storage policy is supplied, the box construction expression uses the default storage policy and infers the contained type from the contained value expression.

When no expected box type is available and an explicit storage policy is supplied, the contained type is inferred from the contained value expression.

The contained value expression is checked against the expected contained type when one is available.

The expected contained type can guide literal typing, union variant shorthand, struct construction shorthand, tuple element typing, array element typing, conversion checking, and nested expression checking.

The storage policy runtime arguments are checked against the storage construction behavior required by the selected storage policy type.

A storage policy argument must correspond to a declared parameter required by the storage construction behavior.

Duplicate storage policy arguments are errors.

Unknown storage policy arguments are errors.

Missing required storage policy arguments are errors unless the corresponding parameter has a default.

Box construction evaluates the contained value expression and initializes indirect storage with that value.

The contained value is moved into the box storage unless the value is copied according to its type’s copy contract or another explicit rule applies.

The resulting `box[S] T` owns the indirect storage and the contained `T`.

Moving a `box[S] T` moves ownership of the indirection value.

Destroying a `box[S] T` destroys the contained `T` and releases storage according to the storage policy.

Borrowing a `box[S] T` can project borrows of the contained `T` according to the box type form and storage behavior.

A mutable borrow of a `box[S] T` can project mutable access to the contained `T` when the box access path, storage policy, and contained type permit it.

A box construction expression is fully initialized when the storage state has been created, the contained value has been initialized in the storage, and the `box[S] T` value has been formed.

If evaluation exits before box construction completes, already-initialized values, storage state, and temporaries are handled by the corresponding control-flow, ownership, destruction, finalization, and storage-release rules.

A box construction expression participates in effect checking and capability checking.

Effects of the contained value expression are effects of the box construction expression.

Effects of storage construction behavior are effects of the box construction expression.

Finalization obligations created by the contained value expression, storage construction behavior, or contained value become obligations of the resulting box, local temporaries, or surrounding context according to ownership and lifecycle rules.

Trusted capabilities used by storage construction behavior must be permitted by the trusted declaration that implements that behavior or rejected according to the Contract and Trust Model.

A box construction expression can establish facts in the fact context.

Facts can include the produced box type, storage policy type, contained type, full initialization of the box value, initialization of the contained value, and facts established by the contained value expression or storage construction behavior.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts about the box value, storage state, or contained value.

TODO: Define the general evaluation order of the contained value expression and additional storage policy arguments.

The box construction operation itself initializes indirect storage with the contained value before the resulting box value becomes available.

---

## Type-form construction expressions

A **type-form construction expression** is a construction expression associated with a type form.

A type form is a compiler-recognized type-level construct.

A type form can define construction behavior when constructing values of that type form requires compiler-recognized semantics.

`box` is the currently defined type form with construction expression syntax.

```bray
box[S] T
box[S](value, ...)
```

The type form determines the produced type.

The type form determines the subject type or subject types.

The type form determines any compile-time arguments.

The type form determines the relationship between the constructed value and the subject value.

The type form can affect ownership, storage, borrowing, layout, lifetime behavior, initialization, destruction, finalization, access-path projection, effect checking, and capability checking.

The square-bracket part of a type-form construction expression contains compile-time arguments for the type form.

```bray
box[Heap](value)
```

The parentheses contain runtime construction arguments.

Runtime arguments are named when the type-form construction behavior defines parameters.

Argument names cannot be omitted for named runtime parameters.

A type-form construction expression can use expected type context.

Expected type context can supply the type form, compile-time arguments, subject type, storage policy type, or other type-form-specific expected information.

Expected subject type can propagate inward to the subject value expression when the type form defines a single clear subject type.

A type-form construction expression is not an ordinary function call.

A type-form construction expression is checked by the compiler according to the construction behavior defined for that type form.

The construction behavior can invoke ordinary declarations, trait behavior, lifecycle declarations, storage behavior, trusted declarations, and contract clauses, but the type-form construction expression itself remains a compiler-recognized expression form.

Only type forms with defined construction behavior have type-form construction expression syntax.

A type form with no construction behavior cannot be used as a construction expression merely because it has type syntax.

A type-form construction expression produces a fully initialized value when all construction steps required by the type form have completed.

If evaluation exits before construction completes, already-initialized values, partially initialized storage, temporaries, and acquired capabilities are handled by the corresponding control-flow, ownership, destruction, finalization, and capability rules.

A type-form construction expression participates in type checking, ownership checking, initialization checking, destruction checking, finalization tracking, effect checking, capability checking, trusted capability checking, and fact-context refinement.

A type-form construction expression can establish facts in the fact context according to the construction behavior of the type form.

Facts established by a type-form construction expression remain valid only while the values, storage identities, lifetimes, capabilities, and versions they depend on remain valid.

TODO: Define the general evaluation order of runtime construction arguments.

---

## Match expressions

A **match expression** evaluates a subject expression, compares it against a sequence of pattern arms, and produces the result of the selected arm.

```bray
let area: r64 = match shape
{
    case .Circle(radius)
    {
        yield math.pi * radius * radius;
    }

    case .Rectangle(min, max)
    {
        yield (max.x - min.x) * (max.y - min.y);
    }

    case .Empty
    {
        yield 0.0;
    }
};
```

A match expression evaluates its subject once.

A match body contains `case` arms.

Each arm has a pattern and a block expression body.

An arm can have a `when` guard.

```bray
match subject
{
    case pattern
    {
        ...
    }

    case pattern when guard
    {
        ...
    }
}
```

A guard is an observe-only boolean expression evaluated after the arm pattern structurally matches and before the arm body is selected.

Bindings introduced by an arm pattern are available in the guard and in the arm body.

In the guard, pattern bindings are available for observation.

In the arm body, pattern bindings are available according to the match operation mode.

The first arm whose pattern matches and whose guard holds is selected.

A catch-all arm uses the discard pattern.

```bray
case _
{
    yield fallback;
}
```

The selected arm body produces the match expression result.

If the match expression result type is `unit`, an arm body can complete normally.

If the match expression result type is a value type other than `unit`, every reachable normal completion path in every selected arm body supplies a value with `yield` or ends in a `never` expression.

All match arms must merge to a coherent type, ownership state, initialization state, destruction state, finalization state, capability state, and fact context.

A match expression can use refutable patterns.

A match expression over a closed union performs coverage checking against the union’s closed variant set.

Guarded arms contribute full coverage when the compiler can prove the guard always holds for the matched state.

Alternative patterns contribute coverage for each alternative.

Arm order is semantically meaningful.

A later arm whose pattern can never be selected is unreachable.

A successful arm pattern refines the fact context for the guard and the arm body.

For union variants, refinement includes the active variant and initialized payload fields.

The default match operation mode is observe.

A consuming match uses consume mode.

```bray
let bytes = match consume buffer
{
    case .Inline(data)
    {
        yield data;
    }

    case .Heap(data)
    {
        yield data;
    }
};
```

In consume mode, selected payloads and fields can be moved out according to ownership rules.

A consuming match with guards evaluates structural matching and guards through observation first.

Consuming bindings are produced for the selected arm body after the guard holds.

A match expression can match through borrowed or type-form subjects when the subject type and pattern form support it.

---

## Yield expressions

A **yield expression** supplies a value to the nearest enclosing yield-capable region.

```bray
yield value;
```

Yield-capable regions include:

- value-producing block expressions,
- match arm block expressions,
- generator expressions,
- array generator expressions.

A single-yield region must receive exactly one yielded value on every normal completion path.

A multi-yield region can receive zero or more yielded values according to the region’s contract.

A fixed-size array generator receives exactly the number of yielded values required by the array length.

Nested yield-capable regions capture their own yields.

An inner `yield` supplies the inner region.

The yielded value must be compatible with the target region’s expected result or element type.

A `yield` expression changes control flow within the target yield-capable region.

---

## Return expressions

A **return expression** exits the nearest callable execution scope.

```bray
return value;
```

The returned value must be compatible with the callable’s declared result type.

A callable with result type `unit` can complete normally.

TODO: Define explicit unit-return syntax.

A `return` expression has type `never` in the current control-flow path because control exits the callable execution scope.

`return` targets the nearest callable execution scope.

A nested callable creates a separate callable execution scope.

---

## Break expressions

A **break expression** exits the nearest loop or iteration region that accepts `break`.

```bray
break;
```

`break` targets the nearest compatible loop or iteration region.

TODO: Define loop-expression syntax for `break`.

---

## Continue expressions

A **continue expression** starts the next iteration of the nearest loop or iteration region that accepts `continue`.

```bray
continue;
```

`continue` targets the nearest compatible loop or iteration region.

TODO: Define loop-expression syntax for `continue`.

---

## Await expressions

TODO: Define await expression syntax and semantics.

---

## Conversion expressions

A **conversion expression** explicitly converts a source expression to a target type or through a named conversion mode.

Plain conversion uses `as`.

```bray
let b: i64 = a as i64;
let y: r64 = x as r64;
let z: c128 = w as c128;
```

The expression before `as` is the source expression.

The type after `as` is the target type.

The target type is syntactically present in a plain conversion expression.

A plain `as` conversion is explicit, total, and value-preserving.

A plain `as` conversion is valid only when every possible source value can be represented by the target type without failure, truncation, wrapping, saturation, rounding loss, domain loss, shape change, layout reinterpretation, allocation change, or hidden construction behavior.

A conversion expression evaluates the source expression and produces a value of the target type when the conversion is valid.

A conversion expression consumes the source value unless the source is copyable or borrowed explicitly.

If the source expression reaches a non-copyable owned value, the conversion moves from that source access path.

After a conversion moves from a source access path, the source access path is moved-from until reinitialized.

If the source expression reaches a copyable value and the conversion context uses copy behavior, the conversion copies according to the source type’s copy contract.

If the source expression is a borrow, the conversion operates through the borrow according to the borrow and conversion contracts.

A conversion expression cannot silently drop, complete, or erase a finalization obligation.

A conversion that changes finalization behavior must be an explicitly declared conversion operation with a contract defining that lifecycle behavior.

Plain `as` supports recursive explicit convertibility.

A value of source type `S` can be converted to target type `T` with plain `as` when one of these rules applies:

1. `S` and `T` are the same type.
2. `S` and `T` are built-in scalar numeric types and Bray defines a total value-preserving explicit scalar conversion from `S` to `T`.
3. `S` is a two-element tuple and `T` is a built-in complex type, and both tuple element types are explicitly convertible to `T`’s component real type.
4. `S` and `T` are tuple types with the same arity, and each source element type is explicitly convertible to the corresponding target element type.
5. `S` and `T` are array types with the same length, and the source element type is explicitly convertible to the target element type.
6. `S` and `T` are optional types, and the source contained type is explicitly convertible to the target contained type.
7. `T` declares an explicit conversion from `S`.

Composite conversion preserves structure.

Composite conversion can convert elements recursively.

Composite conversion does not reshape, flatten, transpose, reinterpret layout, allocate a different container shape, or infer user-defined construction.

Tuple-to-tuple conversion requires the same arity.

```bray
let a: (i32, r32) = (1, 2.0);
let b: (i64, r64) = a as (i64, r64);
```

Each source tuple element is converted to the corresponding target tuple element.

A two-element tuple can be converted to a built-in complex type when both tuple elements can be explicitly converted to the complex type’s component real type.

```bray
let z: c128 = (real, imag) as c128;
```

The first tuple element becomes the real component.

The second tuple element becomes the imaginary component.

Array-to-array conversion requires the same length.

```bray
let a: [i32; 4] = [1, 2, 3, 4];
let b: [i64; 4] = a as [i64; 4];
```

Each source array element is converted to the target array element type.

Array conversion preserves length and shape.

Optional-to-optional conversion converts the present value recursively and preserves the absence state.

```bray
let a: i32? = ...;
let b: i64? = a as i64?;
```

TODO: Define absence literal syntax and optional handling syntax.

A built-in scalar conversion is valid with plain `as` only when it is total and value-preserving.

Examples of valid plain scalar conversions include widening same-domain conversions such as:

```text
i8  -> i16
i16 -> i32
i32 -> i64
i64 -> i128

u8  -> u16
u16 -> u32
u32 -> u64
u64 -> u128

r32 -> r64

c64 -> c128
```

Signed-to-unsigned and unsigned-to-signed integer conversion is valid with plain `as` only when every source value is representable in the target type.

Integer-to-real conversion is valid with plain `as` only when every source value is exactly representable in the target real type.

Real-to-integer conversion is not a plain `as` conversion.

Narrowing real conversion is not a plain `as` conversion.

Complex-to-real conversion is not a plain `as` conversion.

Non-literal real-to-complex conversion is not a plain `as` conversion.

A complex value constructed from non-literal real and imaginary parts uses explicit two-element tuple conversion.

```bray
let real: r64 = 1.0;
let imag: r64 = 2.0;

let z: c128 = (real, imag) as c128;
```

Named conversion modes are conversion expressions for conversions that are not plain `as` conversions.

Named conversion modes include:

```text
checked
saturating
wrapping
truncating
rounding
```

Example syntax:

```bray
let x = value as checked i32;
let y = value as rounding r32;
```

A `checked` conversion succeeds only when the source value is representable in the target type.

A `saturating` conversion clamps to the target range.

A `wrapping` conversion uses modular integer conversion.

A `truncating` conversion discards information according to the conversion mode’s contract.

A `rounding` conversion rounds according to a declared rounding rule.

TODO: Define checked-conversion result type.

TODO: Define rounding-rule syntax.

TODO: Define user-defined conversion declaration syntax.

Conversion expressions participate in type checking, ownership checking, initialization checking, destruction checking, finalization tracking, effect checking, capability checking, trusted obligation checking, and fact-context refinement.

A conversion expression can require facts declared by the selected conversion contract.

A conversion expression can establish facts declared by the selected conversion contract.

Trusted caller obligations used by a conversion expression must be available in the fact context, explicitly acknowledged at a trust boundary, or exposed through the surrounding declaration’s contract.

---

## Pattern-bearing expressions

A **pattern-bearing expression** is an expression form that applies a pattern to a subject value, subject access path, or subject element.

Pattern-bearing expression forms include match expressions, array generator iteration expressions, general generator iteration expressions, and local destructuring constructs.

TODO: Define pattern-bearing behavior for loop and control-flow constructs with pattern positions.

Patterns are a dedicated grammar category.

A pattern-bearing expression supplies a subject type to the pattern.

A pattern-bearing expression supplies a pattern operation mode.

The core pattern operation modes are:

```text
observe
shared borrow
mutable borrow
consume
copy
```

The pattern is checked against the subject type.

The pattern operation mode determines how bindings introduced by the pattern are produced.

A pattern binding can receive an owned value, copied value, observed access path, shared borrowed access path, or mutable borrowed access path according to the operation mode.

A pattern-bearing expression defines whether the pattern must be irrefutable or can be refutable.

Local destructuring requires an irrefutable pattern.

Iteration binding requires an irrefutable pattern for the iteration element type.

Match expressions accept refutable patterns and perform coverage checking according to the subject type and arm set.

A context that requires irrefutable patterns rejects refutable patterns during checking.

A context that accepts refutable patterns defines what happens when the pattern does not match.

A successful pattern can introduce bindings.

A successful pattern can refine the subject.

A successful pattern can add facts to the fact context.

Facts established by a pattern can include active union variant, literal equality, field availability, initialized payload fields, tuple shape, fixed array shape, and narrowed control-flow state.

Pattern-introduced bindings are scoped to the region defined by the pattern-bearing expression.

Pattern-introduced bindings are initialized when the pattern has successfully matched and the operation mode has produced the corresponding bound values or access paths.

Pattern-introduced bindings participate in ownership, borrowing, mutation authority, initialization, destruction, finalization, capability checking, effect checking, and fact-context refinement.

A pattern-bearing expression can partially move from its subject in consume mode.

Partial movement through a pattern requires ownership of the subject and no conflicting active borrows.

A partially moved subject becomes partially initialized.

A partially moved subject can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved subject destroys only the still-initialized parts.

A pattern-bearing expression can borrow from its subject in shared borrow or mutable borrow mode.

Borrowing through a pattern must satisfy Bray's borrowing rules.

A mutable borrow pattern operation requires mutation authority and compatible exclusivity for the reached storage.

A pattern-bearing expression can copy from its subject in copy mode when the reached types satisfy the required copy contracts.

Pattern matching is structural, deterministic, and effect-free.

Pattern matching can inspect tags, fields, tuple elements, array elements, and type-form structure according to the subject type and operation mode.

Pattern matching can bind names, refine facts, and move, copy, borrow, or observe parts according to ownership rules.

Pattern matching does not execute ordinary user code.

Pattern matching does not call ordinary functions or methods.

Pattern matching does not allocate, perform I/O, run asynchronous work, finalize, destroy, or enter resource scopes.

Additional boolean filtering is handled by guards when the surrounding expression form defines guards.

TODO: Define the complete guard syntax and rules for match and control-flow forms.

---

## Predicate expressions

A **predicate expression** is a restricted contract-level expression checked in predicate-expression context.

Predicate expressions are used in predicate bodies, `requires(...)` clauses, `ensures(...)` clauses, `with(...)` clauses, and other contract-level positions.

```bray
predicate fits(length: usize, capacity: usize) =
    length <= capacity;
```

A predicate expression is pure, deterministic, total, terminating, and observational.

Predicate expressions form their own checking context.

The Contract and Trust Model defines value predicate context and static constraint context.

The parser can reuse ordinary expression grammar pieces, but binding and checking apply predicate-expression restrictions.

A predicate body is a single predicate expression.

```bray
predicate non_empty(length: usize) =
    length > 0;
```

Block expressions are excluded from predicate expressions.

```bray
predicate valid(length: usize, capacity: usize) =
{
    yield length <= capacity;
};
```

This is rejected.

Predicate expressions can reference predicate parameters.

Predicate expressions can reference `self` where applicable.

TODO: Define result-binding syntax before allowing predicate expressions to reference result bindings in postconditions.

Predicate expressions can reference constants and associated constants.

Predicate expressions can use field access through observable access paths.

Predicate expressions can inspect tuples, arrays, optional values, and union values by observation.

Predicate expressions can use arithmetic under contract arithmetic semantics.

Integer-valued contract arithmetic is exact and does not silently wrap.

Predicate expressions can use boolean operators.

Predicate expressions can use comparisons.

Predicate expressions can call predicates.

Predicate expressions can call contract functions.

TODO: Define conditional expression syntax before allowing predicate expressions to use expression-only conditional forms.

TODO: Define whether predicate expressions support finite bounded quantifier expressions.

Predicate expressions exclude local binding declarations.

Predicate expressions exclude assignment.

Predicate expressions exclude mutation.

Predicate expressions exclude movement and consumption.

Predicate expressions exclude destruction.

Predicate expressions exclude finalization.

Predicate expressions exclude allocation.

Predicate expressions exclude I/O.

Predicate expressions exclude ordinary function calls.

Predicate expressions exclude method calls unless they resolve to contract functions.

Predicate expressions exclude asynchronous execution forms.

Predicate expressions exclude resource-scope behavior.

Predicate expressions exclude runtime loops.

Predicate expressions exclude dynamic dispatch with effects.

Predicate expressions exclude trusted capability use.

Predicate expressions exclude reading mutable global state.

Predicate expressions exclude dependence on time, randomness, scheduler state, address layout, or unspecified evaluation order.

A predicate expression with type `bool` can serve as an ordinary contract requirement.

A trusted predicate call can appear in a contract clause when prefixed with `trusted`.

```bray
requires(
    length <= capacity,
    trusted core.memory.owned_allocation(pointer = pointer, capacity = capacity),
)
```

Ordinary predicate expressions and trusted predicate calls are distinct contract requirements.

Predicate expressions can establish value facts when used in `ensures(...)`.

Predicate expressions can require value facts when used in `requires(...)`.

Predicate expressions can require static constraint facts when used in `with(...)`.

Value facts introduced by predicate expressions are tied to the values, storage identities, lifetimes, capabilities, and versions mentioned by the expression.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate facts that depend on the affected state.

Static constraint facts are compile-time facts scoped to the constrained declaration and its generic checking context.

Runtime assertions generated from predicate expressions must preserve predicate-expression semantics.

TODO: Define result-binding syntax for postconditions.

TODO: Define contract function declaration syntax.

TODO: Define quantifier syntax and rules.

TODO: Define asynchronous expression forms.

TODO: Define resource-scope expression syntax.

---

## Guard expressions

A **guard expression** is an additional boolean check attached to a pattern arm by the expression form that defines the guard.

Match expressions use `when` guards on `case` arms.

```bray
case pattern when guard
{
    ...
}
```

A guard is evaluated after the arm pattern structurally matches.

A guard is evaluated before the arm body is selected.

A guard must produce `bool`.

Bindings introduced by the pattern are available in the guard.

Facts established by successful structural pattern matching are available in the guard.

Surrounding facts from the fact context are available in the guard when they remain valid at the guard evaluation point.

The arm body is selected only when the pattern matches and the guard evaluates to `true`.

A pattern arm whose pattern matches but whose guard evaluates to `false` does not select that arm.

Arm selection then continues according to the surrounding expression form’s arm-order rules.

For match expressions, the first arm whose pattern matches and whose guard evaluates to `true` is selected.

A guard can establish facts for the selected arm body when the guard condition is known to hold after evaluation.

Facts established by a guard remain valid only while the values, storage identities, lifetimes, capabilities, and versions they depend on remain valid.

TODO: Define guarded coverage rules for exhaustiveness checking.

TODO: Define the allowed expression subset for guards.

TODO: Define guard effect and capability rules.

TODO: Define guard interaction with consuming match operation mode.

---

## Trusted acknowledgement expressions

TODO: Define trusted acknowledgement expressions.

---

## Assertion expressions

TODO: Define assertion expressions.

---

## Optional type expressions

The optional type form is `T?`.

TODO: Define optional type expressions.

---

## Resource-scope expressions

TODO: Define resource-scope expressions.

---

## Closure and anonymous function expressions

TODO: Define closure and anonymous function expressions.

---

## Conditional expressions

TODO: Define conditional expressions.

---

## Loop expressions

TODO: Define loop expressions.

---

## Expression effects and capabilities

Expression checking includes effects and capabilities.

An expression can use only the effects and capabilities available from:

- bindings,
- parameters,
- constraints,
- execution mode,
- lifecycle state,
- trusted declarations,
- fact context,
- surrounding context.

Expressions participate in:

- observe capability,
- observe-with-internal-effects capability,
- mutation authority,
- consume capability,
- finalization capability,
- trusted capability checking,
- scope enter capability,
- scope exit capability,
- async execution capability.

An expression that uses a trusted implementation capability must appear inside a trusted declaration with the matching `uses(...)` clause.

An expression that depends on a trusted caller obligation must have that obligation in the fact context, acknowledge it at a trust boundary, or expose it through the surrounding declaration’s contract.

---

## Expression ownership

Expressions interact with ownership.

An expression can:

- create a value,
- move a value,
- copy a value,
- borrow a value,
- mutably borrow a value,
- consume a value,
- partially move a value,
- initialize storage,
- reinitialize storage,
- destroy storage.

Construction expressions create fully initialized values when all required parts are initialized.

Assignment expressions reinitialize storage when the destination and type contract permit it.

Consuming expressions make the consumed value unavailable through its old access path unless it is reinitialized.

Partial moves leave the subject partially initialized.

Destruction of partially initialized values destroys only initialized parts.

---

## Expression initialization behavior

Expressions that create values must establish valid initialization state.

Struct construction initializes required fields and omitted defaulted fields.

Union variant construction initializes the active tag and active payload.

Tuple construction initializes every tuple element.

Array construction initializes every array element.

Box construction initializes indirect storage with the contained value.

Assignment initializes or re-initializes the destination.

Partial moves change the source value’s initialization state.

Reinitialization is allowed when the storage and type contract permit it.

---

## Expression destruction and finalization behavior

Expressions can trigger destruction and finalization obligations through scope exit, assignment, movement, consumption, cancellation, and resource-scope behavior.

Assignment ends the previous destination value according to destruction and lifecycle rules.

Leaving a block expression destroys local owned values whose ownership remains in the block expression scope.

A value with a finalization obligation must be finalized, transferred to another owner that assumes the obligation, or converted into an explicit fallback ownership form before the owning scope exits.

Async finalization is completed through asynchronous execution.

Ordinary destruction remains synchronous.

---

## Expression fact-context behavior

Expressions can add, remove, or refine facts in the fact context.

Examples:

- successful pattern matching refines an active union variant,
- match arm selection refines the subject state,
- branch conditions can establish ordinary boolean facts,
- assertions can establish ordinary contract facts,
- construction can establish type and variant facts,
- assignment can invalidate facts about the destination,
- mutation can invalidate facts about affected storage,
- movement can invalidate facts about moved values,
- destruction can invalidate facts about destroyed storage,
- trusted declarations can establish trusted facts through `ensures(...)`.

Fact-context behavior is flow-sensitive.

Facts are tied to values, storage identities, lifetimes, capabilities, and versions.

Facts expire when the values or storage they depend on change in a way that can affect truth.

---

## Expression evaluation order

TODO: Define the general evaluation order for operands, callees, arguments, field initializers, defaults, array elements, tuple elements, and construction forms.

The expression model already fixes these specific evaluation facts:

- a match expression evaluates its subject once,
- an `each` source expression is evaluated once before iteration,
- a struct or variant field default is evaluated when that field is omitted,
- a box construction expression evaluates the contained value before initializing indirect storage,
- a guard is evaluated after structural pattern matching and before selecting the arm body.

TODO: Ensure the evaluation-order rule preserves ownership, borrowing, destruction, finalization, capability, effect, and fact-context correctness.

---

## Expression TODOs

- TODO: Define ordinary conditional expression syntax.
- TODO: Define ordinary loop expression syntax.
- TODO: Define resource-scope expression syntax.
- TODO: Define await surface syntax.
- TODO: Define unit-return syntax.
- TODO: Define exact evaluation order.
- TODO: Define assertion syntax.
- TODO: Define checked-conversion result shape.
- TODO: Define optional absence literal.
- TODO: Define optional handling syntax.
- TODO: Define dynamic dispatch expression syntax.
- TODO: Define closure and anonymous function syntax.
- TODO: Define general generator expression syntax.
- TODO: Define operator precedence and operator overloading syntax.
- TODO: Define indexing contract details.
- TODO: Define whether loops can produce result values.

---

## Design principles

Expressions are typed.

Expressions can produce values, access paths, control-flow outcomes, or compile-time entities.

Block expressions always have a type.

Sequenced expressions use semicolons.

Callable results are supplied with `return`.

Yield-capable regions receive values through `yield`.

Construction expressions use named fields where field identity matters.

Struct construction can omit the type when the expected type is known.

Union variant construction can use leading-dot shorthand when the expected union type is known.

No-payload union variants construct without parentheses.

`box(...)` is the owned-indirection construction expression.

Function calls and method calls are governed by callable contracts.

Assignment returns `unit`.

Patterns belong to pattern-bearing expressions and remain their own grammar category.

Match expressions are expressions and produce the selected arm result.

Predicate expressions use a restricted contract-expression context.

Expressions participate in ownership, borrowing, initialization, destruction, finalization, effects, capabilities, and fact-context refinement.
