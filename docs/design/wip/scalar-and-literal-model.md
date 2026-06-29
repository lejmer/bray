# Scalar and literal model

## Overview

Bray has a small fixed set of built-in scalar types.

The scalar model covers:

1. integer types,
2. real floating-point types,
3. complex floating-point types,
4. machine-sized integer types,
5. `bool`,
6. `char`,
7. `unit`,
8. `never`,
9. numeric literals,
10. imaginary literals,
11. explicit scalar and composite conversion.

Scalar types are ordinary value types. They participate in ownership, copying, borrowing, mutation authority, initialization,
destruction, generic constraints, contract expressions, and overload resolution according to their type contracts.

---

## Signed integer types

Bray defines the following signed integer types:

```bray
i8
i16
i32
i64
i128
```

Signed integer types have fixed bit width.

Signed integer operations are typed operations over their declared type.

Integer overflow behavior for ordinary runtime arithmetic is a separate language rule and must be defined explicitly.
Contract arithmetic does not use machine overflow semantics.

---

## Unsigned integer types

Bray defines the following unsigned integer types:

```bray
u8
u16
u32
u64
u128
```

Unsigned integer types have fixed bit width.

Unsigned integer operations are typed operations over their declared type.

Unsigned integer overflow behavior for ordinary runtime arithmetic is a separate language rule and must be defined explicitly.
Contract arithmetic does not silently wrap.

---

## Machine-sized integer types

Bray defines two machine-sized integer types:

```bray
usize
isize
```

`usize` is the unsigned integer type used for sizes, lengths, indexes, and address-sized quantities.

`isize` is the signed integer type corresponding to the target machine word size.

The exact width of `usize` and `isize` is target-dependent.

Code that requires a fixed-width integer must use a fixed-width integer type such as `u64` or `i64`.

---

## Real floating-point types

Bray defines the following real floating-point types:

```bray
r16
r32
r64
r128
```

Their meanings are:

```text
r16    IEEE 754 binary16, if supported on the target
r32    IEEE 754 binary32
r64    IEEE 754 binary64
r128   IEEE 754 binary128, if supported on the target
```

`r32` and `r64` are core real floating-point types.

`r16` and `r128` are available only when the target supports the required representation and operations.

A program that requires `r16` or `r128` must either target platforms that support those types or declare an appropriate target
constraint.

Bray uses `r*` names because these are real floating-point scalar types. The name avoids overloading `f` for both “float”
and “function”.

---

## Complex floating-point types

Bray defines the following complex floating-point types:

```bray
c32
c64
c128
c256
```

Complex type names use total width, not component width.

Their meanings are:

```text
c32     two r16 components, if r16 is supported on the target
c64     two r32 components
c128    two r64 components
c256    two r128 components, if r128 is supported on the target
```

So:

```text
c64  = complex number with two r32 components
c128 = complex number with two r64 components
```

A complex value has a real component and an imaginary component.

Complex values are scalar values, not tuples. They have their own type identity, operations, conversion rules, and literal
behavior.

Complex types are native built-in scalar types. They are not library structs.

---

## Boolean type

Bray defines the boolean type:

```bray
bool
```

`bool` represents truth values.

Boolean values are used by conditionals, loops, predicate expressions, contract expressions, and boolean operators.

The exact literal spellings for boolean values are:

```bray
true
false
```

---

## Character type

Bray defines the character type:

```bray
char
```

A `char` is a Unicode scalar value.

A `char` is not a byte.

A string is not an array of `char` by default. String representation, encoding, slicing, and indexing are part of the string
model, not the scalar model.

---

## Unit type

Bray defines the unit type:

```bray
unit
```

`unit` has exactly one value.

The unit value is spelled `unit`.

In type position, `unit` names the unit type.

In expression position, `unit` is the unit value.

`unit` represents successful completion without meaningful data.

A callable with an omitted result type returns `unit`.

```bray
func log(pos message: String)
{
    print(message);
}
```

This means:

```bray
func log(pos message: String) -> unit
{
    print(message);
}
```

The omitted result type does not mean the result type is inferred.

---

## Never type

Bray defines the never type:

```bray
never
```

`never` has no values.

A computation of type `never` does not complete normally.

A `never` expression can satisfy any expected value type because it produces no value on the current control-flow path.

At a control-flow merge, `never` contributes no value and does not determine the merged result type.

This does not make `never` a value of the expected type and does not define an implicit conversion from `never`.

The canonical never-producing expression forms are `return`, `yield` targeting a single-yield region, `break value`, `break;`,
`continue`, panic expressions, propagation paths that exit the current continuation, calls whose declared result type is `never`,
and expression forms whose every reachable path has type `never`.

---

## Type annotations

A type annotation uses `:`.

```bray
let x: i32 = 1;
let y: r64 = 1.0;
let z: c128 = 1.0 + 2.0i;
```

The `:` annotates the declared thing before it.

Callable result types use `->`, not `:`.

```bray
func add(left: i32, right: i32) -> i32
{
    return left + right;
}
```

So:

```text
:    type annotation for the declared thing before it
->   result type of a callable
```

---

## Numeric literals

Integer, real, and imaginary literals are initially untyped.

A numeric literal receives its type from context.

```bray
let x: i32 = 1;
let y: r64 = 1.0;
let z: c128 = 1.0 + 2.0i;
```

The literal `1` can be interpreted as an integer type when the expected type requires an integer.

The literal `1.0` can be interpreted as a real floating-point type when the expected type requires a real.

The literal `2.0i` can be interpreted as an imaginary component when the expected type requires a complex value.

Numeric literal adaptation is not the same as implicit conversion of already-typed values.

---

## Literal defaults

When no expected type determines a numeric literal type, Bray uses default literal types.

The default integer literal type is:

```bray
i32
```

The default real literal type is:

```bray
r64
```

The default complex literal type is:

```bray
c128
```

These defaults apply only when the literal is otherwise unconstrained.

A literal default does not create implicit conversion between non-literal values.

---

## No numeric suffixes

Bray does not use numeric literal suffixes.

Invalid style:

```bray
let x = 1i32;
let y = 1.0r64;
```

Valid style:

```bray
let x: i32 = 1;
let y: r64 = 1.0;
```

The expected type should come from annotations, parameter types, return types, field types, array element types, tuple element
types, pattern subject types, or other type context.

---

## Imaginary literals

An imaginary literal uses an `i` suffix on a numeric literal.

```bray
2i
2.0i
```

An imaginary literal represents an imaginary component, not a variable named `i`.

The identifier `i` remains an ordinary identifier and can be used as a loop index, binding name, pattern binding name, or
parameter name.

```bray
let z: c128 = 1.0 + 2.0i;
```

Here `2.0i` is an imaginary literal.

This does not make bare `i` magic.

```bray
let i: i32 = 2;
```

This is an ordinary binding.

---

## Complex literal expressions

A complex literal expression can be formed from real and imaginary literals when the expected type is complex.

```bray
let z: c128 = 1.0 + 2.0i;
```

The real and imaginary parts are adapted to the component type of the expected complex type.

For `c128`, both components are `r64`.

```bray
let z: c128 = 1.0 + 2.0i;
```

means a complex value with:

```text
real component      1.0 as r64
imaginary component 2.0 as r64
```

For `c64`, both components are `r32`.

```bray
let z: c64 = 1.0 + 2.0i;
```

means a complex value with:

```text
real component      1.0 as r32
imaginary component 2.0 as r32
```

---

## Literal adaptation vs implicit conversion

Literal adaptation applies only to literals.

Non-literal numeric values do not implicitly convert between integer, real, and complex domains.

Valid:

```bray
let z: c128 = 1.0 + 2.0i;
```

Invalid:

```bray
let x: r64 = 1.0;
let z: c128 = x + 2.0i;
```

The second example is invalid because `x` is already a typed `r64` value. Bray does not implicitly lift a real value into a
complex value.

Complex construction from non-literal real and imaginary parts must be explicit.

---

## Complex construction from non-literal parts

A complex value can be explicitly constructed from a two-element tuple using `as`.

```bray
let real: r64 = 1.0;
let imag: r64 = 2.0;

let z: c128 = (real, imag) as c128;
```

A two-element tuple can be converted to a built-in complex type when both tuple elements are explicitly convertible to the
complex type's component real type.

```bray
let z: c128 = (real, imag) as c128;
```

This is explicit construction, not implicit numeric lifting.

---

## Plain `as`

Plain `as` is explicit, total, and value-preserving.

A value of source type `S` can be converted to target type `T` with plain `as` only when the conversion is guaranteed to succeed
and preserve the represented value.

Examples:

```bray
let a: i32 = 127;
let b: i64 = a as i64;

let x: r32 = 1.5;
let y: r64 = x as r64;

let w: c64 = 1.0 + 2.0i;
let z: c128 = w as c128;
```

Plain `as` does not perform lossy conversion.

Plain `as` does not perform fallible conversion.

Plain `as` does not perform wrapping, saturating, truncating, rounding, reshaping, flattening, allocation-changing, or
layout-reinterpreting conversion.

Those require named conversion modes or separately declared operations.

---

## Explicit scalar convertibility

A built-in scalar value can be converted with plain `as` when Bray defines the conversion as explicit, total, and
value-preserving.

Examples of valid plain `as` conversions:

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

A smaller signed integer can be converted to a larger signed integer if all values of the source type are representable in the
target type.

A smaller unsigned integer can be converted to a larger unsigned integer if all values of the source type are representable in
the target type.

A narrower real type can be converted to a wider real type if the conversion preserves every source value according to Bray's
real conversion rules.

A narrower complex type can be converted to a wider complex type if each component conversion is valid.

Examples of invalid plain `as` conversions:

```text
i64  -> i32
u64  -> u32
r64  -> r32
r64  -> i32
i32  -> r32
c128 -> c64
c128 -> r64
```

These are invalid for plain `as` because they can lose information, fail, or change the numeric domain.

---

## Signed and unsigned conversion

Plain `as` between signed and unsigned integer types is valid only when every source value is representable in the target type.

For example:

```text
u8 -> i16    valid
i8 -> i16    valid
i8 -> u8     invalid
u16 -> i16   invalid
```

The exact conversion table is derived from representable value ranges, not from type names alone.

---

## Integer and real conversion

Plain `as` from integer to real is valid only if every value of the source integer type is exactly representable in the target
real type.

For example:

```text
i8  -> r32    valid
u8  -> r32    valid
i32 -> r32    invalid
i32 -> r64    valid if every i32 value is exactly representable in r64
i64 -> r64    invalid
```

Plain `as` from real to integer is invalid because it can fail, truncate, round, or lose fractional information.

```bray
let x: r64 = 1.5;
let y: i32 = x as i32; // invalid
```

Real-to-integer conversion requires a named conversion mode.

---

## Real and complex conversion

Plain `as` from complex to complex is valid when both component conversions are valid.

```bray
let z: c64 = 1.0 + 2.0i;
let w: c128 = z as c128;
```

Plain `as` from real to complex is not valid for non-literal values.

```bray
let x: r64 = 1.0;
let z: c128 = x as c128; // invalid
```

A non-literal complex value must be constructed explicitly from real and imaginary components:

```bray
let z: c128 = (x, 0.0) as c128;
```

Plain `as` from complex to real is invalid because it discards the imaginary component.

```bray
let z: c128 = 1.0 + 2.0i;
let x: r64 = z as r64; // invalid
```

Complex component extraction must use an explicit operation.

---

## Named conversion modes

Lossy, fallible, saturating, wrapping, truncating, rounding, reshaping, flattening, allocation-changing, or layout-reinterpreting
conversions require named conversion modes or explicit declared operations.

Named conversion modes include:

```text
checked
saturating
wrapping
truncating
rounding
```

Their meanings are:

```text
checked     succeeds only if representable; otherwise returns a nullable value or result
saturating  clamps to the target range
wrapping    uses modular integer conversion
truncating  discards fractional part
rounding    rounds according to a declared rounding rule
```

Example syntax:

```bray
let x = value as checked i32;
let y = value as rounding r32;
```

A rounding conversion must declare or imply a rounding rule.

The Expression Model defines conversion expression syntax, checked-conversion result shape, and rounding-rule syntax.

---

## Recursive explicit convertibility

Plain `as` uses a recursive convertibility rule over type structure.

A value of source type `S` can be converted to target type `T` with plain `as` when one of these holds:

1. `S` and `T` are the same type.
2. `S` and `T` are built-in scalar numeric types and Bray defines a total value-preserving explicit scalar conversion from
   `S` to `T`.
3. `S` is a two-element tuple and `T` is a built-in complex type, and both tuple elements can be explicitly converted to `T`'s
   component real type.
4. `S` and `T` are tuple types with the same arity, and each source element type is explicitly convertible to the corresponding
   target element type.
5. `S` and `T` are array types with the same length, and the source element type is explicitly convertible to the target element
   type.
6. `S` and `T` are nullable types, and the source contained type is explicitly convertible to the target contained type.
7. `T` declares an explicit conversion from `S`.

Composite conversion preserves structure.

It may convert elements recursively, but it does not reshape, flatten, transpose, reinterpret layout, allocate a different container
shape, or infer user-defined construction.

---

## Tuple conversion

Tuple-to-tuple conversion is allowed only when the tuples have the same arity and each source element can be explicitly converted to
the corresponding target element.

```bray
let a: (i32, r32) = (1, 2.0);
let b: (i64, r64) = a as (i64, r64);
```

A tuple cannot be converted to a non-tuple user type unless the target type declares an explicit conversion from the source
tuple type.

Two-element tuple to complex conversion is allowed when the target is a built-in complex type and both elements can be explicitly
converted to the complex component real type.

```bray
let real: r64 = 1.0;
let imag: r64 = 2.0;

let z: c128 = (real, imag) as c128;
```

---

## Array conversion

Array-to-array conversion is allowed only when both arrays have the same length and the source element type can be explicitly
converted to the target element type.

```bray
let a: [i32; 4] = [1, 2, 3, 4];
let b: [i64; 4] = a as [i64; 4];
```

Nested arrays convert recursively.

```bray
let a: [[i32; 4]; 3] = ...;
let b: [[i64; 4]; 3] = a as [[i64; 4]; 3];
```

Array conversion does not change length, shape, layout, or storage representation.

---

## Nullable conversion

Nullable-to-nullable conversion is allowed when the contained source type can be explicitly converted to the contained
target type.

```bray
let a: i32? = ...;
let b: i64? = a as i64?;
```

Nested conversion is recursive:

```bray
let a: [(i32, r32?); 4] = ...;
let b: [(i64, r64?); 4] = a as [(i64, r64?); 4];
```

The absent state remains absent. The present value, if present, is converted recursively.

The absence expression is `none`.

---

## Ownership of conversion

An `as` conversion consumes the source value unless the source is copyable or borrowed explicitly.

Element-wise conversion of owned composites moves through the structure and produces a new value with the target type.

If the source value is copyable, conversion may copy according to the source type's copy contract.

If the source is borrowed, conversion operates through the borrow according to the borrow and conversion contracts.

---

## Literals and arrays

Array expressions can use scalar literals, and those literals receive type context from the array element type.

```bray
let values: [i32; 4] = [1, 2, 3, 4];
let floats: [r64; 3] = [1.0, 2.0, 3.0];
let complex: [c128; 2] = [1.0 + 2.0i, 3.0 + 4.0i];
```

Every element must be compatible with the array element type.

Literal adaptation can occur for each literal element.

Non-literal elements must already have compatible types or use explicit conversion.

---

## Literals and tuples

Tuple expressions can use scalar literals, and those literals receive type context from the tuple element types.

```bray
let pair: (i32, r64) = (1, 2.0);
let zparts: (r64, r64) = (1.0, 2.0);
let z: c128 = zparts as c128;
```

A one-element tuple requires a trailing comma:

```bray
let single: (i32,) = (1,);
```

Without the comma, parentheses are expression grouping:

```bray
let grouped: i32 = (1 + 2);
```

---

## Scalar literals in contract expressions

Numeric literals can appear in contract expressions.

```bray
predicate can_index(index: usize, length: usize) =
    index < length;

predicate non_empty(length: usize) =
    length > 0;
```

Contract expressions use contract arithmetic semantics.

Integer-valued contract arithmetic is exact and does not silently wrap.

A runtime assertion generated from a contract expression must preserve the meaning of the contract expression.

---

## Design principles

Scalar types are explicit.

Complex numbers are native scalar values.

Complex type names use total width.

Numeric literals are context-typed.

Literal patterns use the same context-typed literal rules.

Literal adaptation is not implicit conversion.

Non-literal numeric values do not implicitly convert between integer, real, and complex domains.

Bare `i` is not magic.

Imaginary literals use an `i` suffix on numeric literals.

Plain `as` is total and value-preserving.

Lossy or fallible conversions require named conversion modes.

Composite conversion is recursive but structure-preserving.

No suffix-based numeric typing exists.

No conversion silently changes shape, layout, allocation, or ownership semantics.
