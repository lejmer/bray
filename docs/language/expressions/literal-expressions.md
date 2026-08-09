# Literal expressions

A **literal expression** directly denotes a literal value written in source text.

Literal expressions are expression grammar forms. They are checked in expression context and receive a type during binding and type checking.

The literal expression grammar consists of integer literals, real literals, imaginary literals, boolean literals, character
literals, and string literals.

```bray
1
1.0
2.0i
true
false
'a'
"text"
""
```

A literal expression can produce a value directly, or it can participate in a larger expression such as an arithmetic expression, tuple expression, array expression, struct construction expression, union variant construction expression, predicate expression, or contract clause.

Literal expressions are pure expression forms. Evaluating a literal expression creates no user-visible side effect.

---

## Integer literals

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

## Real literals

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
let y: r32 = std.round_to<r32>(x, rule = NearestEven);
```

The literal `1.0` can adapt to `r64`. The already-typed value `x` uses an ordinary standard-library rounding operation to become `r32` when narrowing or rounding behavior is required.

---

## Imaginary literals

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

## Complex literal expressions

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

## Boolean literals

A **boolean literal** directly denotes a value of type `bool`.

```bray
true
false
```

`true` and `false` have type `bool`.

Boolean literal expressions can be used in condition expressions, predicate expressions, contract expressions, match guards, local binding initializers, and other expression contexts expecting `bool`.

Boolean literals are already typed. They do not use numeric literal adaptation.

---

## Character literals

A **character literal** directly denotes a value of type `char`.

```bray
'a'
```

A `char` value is a Unicode scalar value.

A character literal is enclosed in single quotes.

A character literal has type `char`.

A character literal is not a byte literal.

Single quotes do not delimit strings.

`''` is invalid.

A character literal must contain exactly one Unicode scalar value after escape processing.

`'ab'` is invalid.

Character literals are already typed as `char`.

---

## String literals

A **string literal** directly denotes a value of type `string`.

```bray
"hello"
""
"line\nbreak"
```

A string literal is enclosed in double quotes.

Single quotes never delimit strings.

`""` is the empty string.

A string literal has type `string`.

A string literal contains zero or more Unicode scalar values after escape processing.

No interpolation is performed by string literals.

Escape and representation rules for string literals are defined by the scalar and literal rules.

They are already typed as `string`.

---

## Unit and never in expression context

`unit` is a type with exactly one value.

In type position, `unit` names the unit type.

In expression position, `unit` is the unit value.

A block expression or callable body can produce `unit` by completing normally in a `unit` context.

```bray
func log(pos message: string)
{
    print(message);
}
```

A unit value expression has type `unit`.

```bray
let done: unit = unit;
```

A callable returning `unit` can complete normally or return `unit` explicitly.

```bray
func log(pos message: string)
{
    print(message);
    return unit;
}
```

`never` is a type with no values.

A `never` expression is produced by expressions that have no normal continuation.

The canonical never-producing expression forms are:

- `return value` and `return;`,
- `yield value` and `yield;` when they target a single-yield region,
- `break value` and `break;`,
- `continue`,
- panic expressions,
- nullable propagation on the absent path,
- result and run-result propagation on non-success paths,
- calls whose declared result type is `never`,
- expression forms whose every reachable path has type `never`.

```bray
return value;
```

The operand of `return value` is checked against the nearest callable execution scope's declared result type.

The `return value` expression itself has type `never`.

A `never` expression can satisfy any expected type at a control-flow merge because it has no normal continuation.

At a control-flow merge, `never` contributes no value and does not determine the merged result type.

This does not make `never` a value of the expected type.

Panic-producing expressions have type `never` because their normal continuation does not run.

---

## Literal typing by context

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

let shape: Shape = Circle(radius = 1.0, center = origin);
```

In each example, the expected type controls literal typing.

When no expected type controls a numeric literal, the default literal type applies.

---

## Literal defaults

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

## Literal adaptation

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

## Numeric literal suffixes

Bray uses type context rather than numeric type suffixes.

```bray
let x: i32 = 1;
let y: r64 = 1.0;
```

Numeric literal syntax has no type suffixes. The suffix `i` is reserved for forming imaginary literals.

```bray
let z: c128 = 1.0 + 2.0i;
```

Type spelling belongs to type annotations, parameter types, field types, result types, array element types, tuple element types, and conversion targets.

---

## Literal expressions in predicate context

Literal expressions can appear in predicate expressions and contract clauses.

```bray
predicate non_empty(length: usize) =
    length > 0;
```

Numeric literals in predicate expressions are checked under predicate-expression rules.

Integer-valued predicate arithmetic uses contract arithmetic semantics.

Contract arithmetic does not silently wrap.

A runtime assertion generated from a predicate expression must preserve the predicate-expression meaning of the literal and arithmetic operation.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Local binding declarations inside block expressions](local-binding-declarations-inside-block-expressions.md)
- Next: [Name expressions](name-expressions.md)
