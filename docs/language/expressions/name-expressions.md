# Name expressions

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

In pattern context, a bare identifier first resolves against pattern-capable declarations available from the subject type and lexical scope.

If the identifier does not resolve to a pattern-capable declaration, it introduces a binding.

In expression context, a bare identifier resolves to an existing visible binding or declaration.

---

## Binding name expressions

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

## Immutable binding access

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

## Mutable binding access

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

## Parameter name expressions

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
func normalize(pos mut buffer: Buffer) -> Buffer
{
    return buffer;
}
```

Borrow parameters have borrow types.

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

A parameter name expression is checked according to the parameter’s type, ownership mode, borrow mode, mutation authority, lifetime, and capability contract.

---

## Function name expressions

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

The callable contract includes parameter names, parameter types, result type, execution mode, ownership behavior, borrowing behavior, mutation requirements, lifetime requirements, capability requirements, caller-visible effects, trusted caller obligations, and finalization behavior.

A function with caller obligations can be used as a value only where the expected callable type preserves those obligations.

---

## Type name expressions

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

A type name used in a path expression can expose variants, constants, static functions, named constructors, or other type-associated declarations.

```bray
Shape.Circle(center = origin, radius = 1.0)
Point.origin()
```

Type-level name expressions are compile-time entities.

They do not produce runtime values unless used by a type-associated expression form that produces a value.

---

## Module and package name expressions

A name expression that resolves to a module or package produces a compile-time path entity.

```bray
math
pkg
```

Module and package name expressions are used as the left side of path expressions.

```bray
math.sin(x)
pkg.module.Type
```

A module or package name expression participates in path resolution.

It does not produce a runtime value.

---

## Constant name expressions

A name expression can resolve to a visible constant declaration.

The ordinary constant declaration syntax is:

```bray
const default_capacity: usize = 64;
const default_prefix: string = "tmp";
```

In grammar terms:

```text
const identifier ':' type-expression '=' constant-expression ';'
```

A visibility modifier can appear before `const` only in declaration contexts that support visibility.

```bray
internal const page_size: usize = 4096;
```

The type annotation is required.

The initializer is checked in constant-initializer context.

The initializer must be a compile-time constant expression compatible with the declared type.

Literal adaptation uses the declared constant type as its expected type.

A constant declaration introduces a named compile-time value in its declaration scope.

The constant name must be unique in the value namespace of that scope.

A constant declaration introduces an unqualified lookup name into its declaration scope.

That unqualified lookup name must not already resolve in the same lookup namespace from that scope.

Constants can be declared in modules, type bodies, implementation bodies, trait bodies, and block scopes that allow declarations.

A constant declared in a module is reached through ordinary module path resolution.

A constant declared in a type body or inherent implementation is associated with that type and can be reached through a type path according to path-expression rules.

A constant declared in a trait body or trait implementation is a constant-valued member governed by [Traits](../types/traits.md#constant-valued-members-in-traits).

A constant declaration in a generic declaration can reference that declaration's type parameters and const parameters when the initializer is valid in static generic context. The constant's value is fixed for each concrete generic instantiation.

A constant has no runtime storage identity.

Using a constant in runtime expression context materializes the constant value for that use.

The initializer is not evaluated at runtime.

A constant cannot be assigned, mutably borrowed, moved from as storage, consumed as a unique storage identity, or destroyed as a declaration.

`mut const` is not a declaration form.

The declared constant type must support constant materialization.

A value with unique runtime identity, runtime-owned resource state, finalization obligations, destructor side effects, or mutable storage identity cannot be a constant value.

A compile-time constant expression can use:

- literals,
- constants already visible in the current scope,
- const parameters visible in the current generic context,
- compiler-known target facts visible for the selected target profile,
- tuple, array, nullable, product, and union variant construction whose components are constant expressions and whose type has no runtime construction, finalization, or destructor obligation,
- unary and binary expressions whose operands are constant expressions and whose selected operation is compiler-known and valid in constant-initializer context,
- calls to const callables whose arguments are constant expressions and whose callable contract is valid in constant-initializer
  context,
- field access, tuple projection, and array element access over constant expressions when the selected sub-value is itself valid as a constant.

A compile-time constant expression is evaluated by the compiler using ordinary Bray expression semantics in a restricted
constant-evaluation context.

Constant evaluation is not a macro system, source rewriting system, or separate compile-time language.

Normal expression typing, overload selection, result propagation, panic rules, ownership rules, borrowing rules, and evaluation order
apply unless a constant-evaluation rule explicitly rejects the expression form.

A compile-time constant expression cannot read runtime storage, borrow runtime storage, assign, mutate, move from a runtime access path, allocate storage, perform I/O, spawn work, await, suspend, catch or raise panics as runtime behavior, use runtime dynamic dispatch, depend on address identity, or call a non-const callable.

Control-flow expressions are valid in constant-evaluation context only when their selected path can be evaluated without runtime
storage, runtime effects, or runtime dispatch.

Loop expressions are not valid in constant-evaluation context.

This includes `loop`, `while`, `for`, `each`, array generator expressions, general generator iteration expressions, and
boolean fold expressions.

Finite aggregate construction is still valid when every element or field initializer is itself a valid compile-time constant
expression.

Only compiler-known operations and const callables explicitly defined as valid in constant-initializer context can be evaluated by a
constant initializer.

For unary and binary expressions in constant-initializer context, the selected operation must be a built-in operation over built-in scalar types, `string`, `unit`, or nullable constants whose contained value is valid in constant-initializer context.

User-defined operator implementations are valid in constant-initializer context only when the selected implementation member is a
const callable and all operands are valid constant expressions.

Integer-valued constant arithmetic uses contract arithmetic semantics and is exact while the constant expression is checked.

The final constant value must be representable in the declared constant type.

Floating-point constant arithmetic uses the same semantics as the selected runtime floating-point type.

Floating-point constants do not use unbounded precision.

A constant initializer that evaluates to `never`, panics, fails a contract, fails a conversion, divides by zero, overflows after
conversion into the declared type, cannot prove termination, exceeds implementation resource limits, or depends on a target fact
unavailable for the selected target profile is rejected.

Implementation resource limits for constant evaluation must be deterministic for a compiler invocation and must be reported as
diagnostics, not as runtime behavior.

Target facts can participate in constant evaluation.

A constant whose initializer reads target facts is target-dependent.

A target-dependent constant is evaluated separately for each selected target profile.

Compiled interface metadata for a target-dependent constant records its dependency on the target profile facts that affect its value.

A target-dependent constant is not evaluated once globally and reused across targets.

Constant declarations cannot be cyclic.

A constant initializer cannot reference the constant being declared, directly or through another constant initializer cycle.

A constant name expression produces the constant’s value or a compile-time constant entity according to context.

In expression context, a bare identifier can resolve to a constant.

In pattern context, a bare identifier can resolve to a constant when that constant is pattern-capable in the current subject context.

Constant matching can also use a qualified path pattern.

```bray
Color.Red
```

If pattern name resolution is ambiguous, the pattern is rejected.

---

## Name resolution and shadowing

Name resolution is deterministic.

A name expression resolves according to the current lexical scope, declaration scope, module context, package context, and using declarations.

A local binding is introduced once.

A binding cannot be rebound.

A local binding declaration introduces one or more unqualified lookup names into the current scope.

Each introduced unqualified lookup name must not already resolve in the same lookup namespace from that scope.

Name shadowing is checked by unqualified lookup name and lookup namespace.

Qualified paths distinguish declarations through their resolved left-hand entity.

The first component of a qualified path participates in ordinary unqualified name resolution.

Each component after `.` is resolved inside the namespace, type, value, access path, trait application, or other path-capable entity
selected by the preceding component.

Therefore, a declaration reachable as `some.thing` and a declaration reachable as `thing` can both be visible in the same scope when
`some` and `thing` are distinct unqualified lookup names.

When a declaration is intentionally exposed through an unqualified name, that exposed name participates in the same shadowing rule
as any other unqualified lookup name in that lookup namespace.

This rule keeps name expressions stable and prevents later local declarations from changing the meaning of earlier names in the same scope.

When a name expression is ambiguous after applying the language’s resolution rules, the program is rejected.

---

## Name expressions and initialization state

A name expression that reaches a local binding is checked against the binding’s initialization state.

A fully initialized binding can be observed, borrowed, moved, copied, consumed, or destroyed as a complete value according to its type and capabilities.

An uninitialized binding cannot be used as a complete value.

A partially initialized binding can be used only through access paths to initialized parts when the operation permits partial-state access.

A moved-from binding cannot be used as a complete value until reinitialized.

A destroyed binding is outside the set of usable value states.

---

## Name expressions and fact context

A name expression can read facts from the fact context.

Facts can describe initialization state, active union variant, field availability, borrow state, mutation authority, predicate facts, trusted facts, and other flow-sensitive information.

Using a name expression can also invalidate facts when the use moves, consumes, mutably borrows, assigns through, finalizes, or destroys the reached storage.

A name expression that only observes a stable value preserves facts that remain true under observation.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Literal expressions](literal-expressions.md)
- Next: [Path expressions](path-expressions.md)
