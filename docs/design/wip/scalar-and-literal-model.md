# Scalar and literal model

## Overview

Bray has a small fixed set of built-in scalar types and a compiler-known text type.

The scalar and literal model covers:

1. integer types,
2. real floating-point types,
3. complex floating-point types,
4. machine-sized integer types,
5. `bool`,
6. `char`,
7. `string`,
8. `unit`,
9. `never`,
10. numeric literals,
11. imaginary literals,
12. character literals,
13. string literals,
14. explicit scalar and composite conversion.

Scalar types are ordinary value types. They participate in ownership, copying, borrowing, mutation authority, initialization,
destruction, generic constraints, contract expressions, and overload resolution according to their type contracts.

The compiler-known `string` type is also an ordinary value type, but it is not a scalar type.

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

Overflow in ordinary runtime signed integer arithmetic panics.

Wrapping, saturating, widening, checked, or otherwise policy-specific arithmetic requires an explicit operation with that contract.

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

Overflow in ordinary runtime unsigned integer arithmetic panics.

Wrapping, saturating, widening, checked, or otherwise policy-specific arithmetic requires an explicit operation with that contract.

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

Character literals are delimited by single quotes.

```bray
'a'
'\n'
'\u{03BB}'
```

A character literal has type `char`.

After escape processing, a character literal must contain exactly one Unicode scalar value.

`''` is invalid.

`'ab'` is invalid.

Single quotes do not delimit strings.

---

## `string` type

Bray defines the compiler-known text type:

```bray
string
```

`string` represents a finite sequence of Unicode scalar values.

`string` has protected representation.

The language-defined contents of a string literal use UTF-8 as their canonical encoding.

A `string` is not an array of `char` or `u8`.

Indexing, slicing, and encoding views for `string` are string operations, not scalar-literal semantics.

---

## string literals

A string literal is delimited by double quotes.

```bray
"hello"
""
"line\nbreak"
```

A string literal has type `string`.

`""` is the empty string.

Single quotes never delimit strings.

`''` is invalid because single quotes delimit character literals.

A string literal contains zero or more Unicode scalar values after escape processing.

A line break cannot appear unescaped inside a string literal.

No interpolation is performed by string literals.

The valid string-literal escape sequences are:

- `\"` for a double quote,
- `\\` for a backslash,
- `\n` for newline,
- `\r` for carriage return,
- `\t` for tab,
- `\0` for the Unicode scalar value U+0000,
- `\u{H...}` for a Unicode scalar value written with hexadecimal digits.

The `\u{H...}` escape is valid only when the hexadecimal value is a Unicode scalar value.

Unknown escape sequences are invalid.

Character literals use the same escape sequences as string literals, except that `\'` is also valid in a character literal for a single quote.

---

## String operations

String operations observe a `string` as a finite sequence of Unicode scalar values.

The observable canonical encoding of a `string` is UTF-8.

The representation remains protected. User code cannot assume that the protected representation is an array, a slice, a pointer,
or an exposed UTF-8 buffer.

String equality is defined over Unicode scalar value sequences.

Two strings are equal when they contain the same number of Unicode scalar values and each scalar value at each position is equal.

String equality does not perform Unicode normalization, case folding, locale collation, width folding, grapheme-cluster comparison,
or encoding-form comparison.

Canonically equivalent Unicode text can be unequal as `string` values when the scalar value sequences differ.

String inequality is the negation of string equality.

`==` and `!=` are valid for `string` operands and produce `bool`.

`<`, `<=`, `>`, and `>=` are not built-in string operations.

Lexicographic ordering, locale collation, case-insensitive comparison, normalization-aware comparison, and natural sorting are
explicit standard-library operations with their own contracts.

The language-defined length of a `string` is its Unicode scalar value count.

The empty string has length `0`.

String length is not byte length and not grapheme-cluster count.

Core `[]` indexing and slicing syntax is not defined for `string`.

String indexing and slicing are explicit standard-library operations because their units, failure behavior, and allocation behavior
are part of the operation contract.

The compiler-recognized standard-library string operation identities are contained in the `std.string` module and are as follows:

| Declaration identity       | Declaration contract                                                           |
|----------------------------|--------------------------------------------------------------------------------|
| `std.string.count`         | `const func count(pos text: string) -> usize`                                  |
| `std.string.is_empty`      | `const func is_empty(pos text: string) -> bool`                                |
| `std.string.equal`         | `const func equal(pos left: string, pos right: string) -> bool`                |
| `std.string.at`            | `func at(pos text: string, pos index: usize) -> char?`                         |
| `std.string.slice`         | `func slice(pos text: string, start: usize, end: usize) -> string?`            |
| `std.string.utf8`          | `func utf8(pos text: &string) -> &[u8]`                                        |
| `std.string.from_utf8`     | `func from_utf8(pos bytes: &[u8]) -> Result<string, std.string.EncodingError>` |
| `std.string.EncodingError` | standard-library union type with variant `InvalidUtf8`                         |

`std.string.count(text)` returns the Unicode scalar value count.

`std.string.is_empty(text)` returns `true` exactly when `std.string.count(text) == 0`.

`std.string.equal(left, right)` has the same equality semantics as `left == right`.

`std.string.at(text, index)` uses Unicode scalar value indexing and returns `none` when `index >= std.string.count(text)`.

`std.string.slice(text, start = start, end = end)` uses Unicode scalar value boundaries and returns `none` unless
`0 <= start <= end <= std.string.count(text)`.

When valid, `std.string.slice(text, start = start, end = end)` returns a `string` containing the selected scalar value subsequence.

`std.string.utf8(&text)` returns a shared byte slice view of the string's canonical UTF-8 bytes.

The UTF-8 view is read-only and does not expose mutable representation.

`std.string.from_utf8(bytes)` returns `Result.Ok(text)` when `bytes` is well-formed UTF-8.

It returns `Result.Error(std.string.EncodingError.InvalidUtf8)` when the byte sequence is not well-formed UTF-8.

String literals are not normalized.

String construction from UTF-8 bytes preserves the scalar value sequence represented by those bytes.

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

## Built-in scalar operations

Built-in scalar operations are language-defined operations for compiler-known scalar operands.

They are part of the compiler-known operator surface for those scalar types.

In this section, integer types include fixed-width integer types and machine-sized integer types.

Unless a rule below says otherwise, a built-in binary scalar operation requires both operands to have the same scalar type and
produces that same type.

Numeric literal adaptation can use the selected built-in operation as type context.

Already-typed non-literal values do not implicitly convert to satisfy a built-in scalar operation.

For example, `i32 + i64` is rejected unless one operand is explicitly converted.

Runtime scalar operation failure raises an ordinary panic.

A scalar operation that would panic at runtime is rejected in constant-evaluation context.

In predicate expressions and contract expressions, integer-valued arithmetic uses contract arithmetic semantics.

Because predicate expressions must be total, integer `/` and `%` in predicate-expression context require the current fact context to
prove that the divisor is nonzero and that signed division overflow cannot occur on any reachable path.

### Unary scalar operations

| Token | Operand types                        | Result type | Failure behavior                           |
|-------|--------------------------------------|-------------|--------------------------------------------|
| `!`   | `bool`                               | `bool`      | none                                       |
| `-`   | signed integers                      | operand     | panics for the minimum representable value |
| `-`   | real floating-point types            | operand     | IEEE sign negation                         |
| `-`   | complex floating-point types         | operand     | component-wise IEEE sign negation          |
| `~`   | unsigned integers, including `usize` | operand     | none                                       |

Unary `-` is not defined for unsigned integers.

Unary `~` is not defined for signed integers, real types, complex types, `bool`, `char`, `unit`, `never`, or `string`.

### Integer arithmetic

The integer arithmetic operators are:

| Token | Operand types                         | Result type | Runtime failure behavior                      |
|-------|---------------------------------------|-------------|-----------------------------------------------|
| `+`   | signed or unsigned integer, same type | operand     | panics on overflow                            |
| `-`   | signed or unsigned integer, same type | operand     | panics on overflow                            |
| `*`   | signed or unsigned integer, same type | operand     | panics on overflow                            |
| `/`   | signed or unsigned integer, same type | operand     | panics on division by zero or signed overflow |
| `%`   | signed or unsigned integer, same type | operand     | panics on division by zero or signed overflow |
| `**`  | integer base and `usize` exponent     | base type   | panics on overflow                            |

Integer division truncates toward zero.

Integer remainder is defined by:

```text
left == (left / right) * right + (left % right)
```

when `right != 0` and the division does not overflow.

For signed integers, the remainder has the sign of the left operand or is zero.

For signed integers, dividing the minimum representable value by `-1` panics because the mathematical quotient is not
representable in the operand type.

The same signed-overflow edge is rejected for `%`.

Integer exponentiation requires a `usize` exponent.

Negative integer exponents are not represented by the built-in `**` operation.

### Real arithmetic

The real arithmetic operators are:

| Token | Operand types             | Result type | Behavior                    |
|-------|---------------------------|-------------|-----------------------------|
| `+`   | same real type            | operand     | IEEE addition               |
| `-`   | same real type            | operand     | IEEE subtraction            |
| `*`   | same real type            | operand     | IEEE multiplication         |
| `/`   | same real type            | operand     | IEEE division               |
| `**`  | same real type            | operand     | IEEE power operation        |

Real arithmetic follows the IEEE 754 semantics of the selected real type.

Real arithmetic can produce signed zero, infinities, and NaN values according to IEEE rules.

Real division by zero follows IEEE division semantics and does not panic solely because the divisor is zero.

The `%` token is not a built-in real operation.

Real remainder policies are explicit standard-library operations.

### Complex arithmetic

The complex arithmetic operators are:

| Token | Operand types             | Result type | Behavior                    |
|-------|---------------------------|-------------|-----------------------------|
| `+`   | same complex type         | operand     | complex addition            |
| `-`   | same complex type         | operand     | complex subtraction         |
| `*`   | same complex type         | operand     | complex multiplication      |
| `/`   | same complex type         | operand     | complex division            |

Complex arithmetic operates over the component real type of the selected complex type.

Component arithmetic follows IEEE semantics.

Complex arithmetic can produce signed zero, infinities, and NaN component values according to IEEE rules.

The `%` and `**` tokens are not built-in complex operations.

Complex exponentiation, polar operations, magnitude, phase, conjugation, and component extraction are explicit standard-library
operations.

### Bitwise and shift operations

Built-in bitwise and shift operations are defined only for unsigned integer types, including `usize`.

Signed integer bit manipulation requires an explicit operation or conversion whose contract states how signed values are interpreted.

The bitwise operators are:

| Token | Operand types                     | Result type | Failure behavior |
|-------|-----------------------------------|-------------|------------------|
| `~`   | unsigned integer                  | operand     | none             |
| `&`   | same unsigned integer type        | operand     | none             |
| `^`   | same unsigned integer type        | operand     | none             |
| `\|`  | same unsigned integer type        | operand     | none             |

The shift operators are:

| Token | Left operand types | Right operand type | Result type  | Runtime failure behavior                                                                                                     |
|-------|--------------------|--------------------|--------------|------------------------------------------------------------------------------------------------------------------------------|
| `<<`  | unsigned integer   | `usize`            | left operand | panics when the shift count is greater than or equal to the left operand bit width, or when any one bit would be shifted out |
| `>>`  | unsigned integer   | `usize`            | left operand | panics when the shift count is greater than or equal to the left operand bit width                                           |

Left shift is checked.

It does not silently discard shifted-out one bits.

Right shift fills with zero bits.

Wrapping, rotating, saturating, unchecked, or bit-discarding shift behavior requires an explicit standard-library operation with that
contract.

### Boolean operations

The boolean operators are:

| Token  | Operand types | Result type | Behavior                    |
|--------|---------------|-------------|-----------------------------|
| `!`    | `bool`        | `bool`      | logical negation            |
| `&&`   | `bool`        | `bool`      | short-circuit conjunction   |
| `\|\|` | `bool`        | `bool`      | short-circuit disjunction   |
| `==`   | `bool`        | `bool`      | equality                    |
| `!=`   | `bool`        | `bool`      | inequality                  |

`&&` evaluates its right operand only when the left operand is `true`.

`||` evaluates its right operand only when the left operand is `false`.

Relational ordering operators are not defined for `bool`.

### Equality and ordering

This summary includes `unit` and `string` for operator completeness even though `string` is not a scalar type.

`==` and `!=` are defined for:

- `bool`,
- `char`,
- signed integer types,
- unsigned integer types,
- machine-sized integer types,
- real floating-point types,
- complex floating-point types,
- `unit`,
- `string`.

For `unit`, `unit == unit` is `true` and `unit != unit` is `false`.

For real floating-point values, equality follows IEEE semantics.

NaN is not equal to any value, including itself.

Positive zero and negative zero compare equal.

For complex floating-point values, equality is component-wise real equality.

A complex value with a NaN component is not equal to any value, including itself.

`!=` is the logical negation of `==`.

`<`, `<=`, `>`, and `>=` are defined for:

- `char`,
- signed integer types,
- unsigned integer types,
- machine-sized integer types,
- real floating-point types.

Character ordering compares Unicode scalar value numbers.

Integer ordering compares represented numeric values.

Real ordering follows IEEE ordered comparison.

If either real operand is NaN, `<`, `<=`, `>`, and `>=` produce `false`.

Relational ordering operators are not defined for complex values, `bool`, `unit`, `never`, or `string`.

### Matrix multiplication token

The `@` token has no built-in scalar operation.

It is reserved for linear-algebra multiplication through compiler-known operator traits and participating implementations.

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

Those require ordinary operations with explicit callable contracts.

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

Real-to-integer conversion requires an ordinary fallible or lossy numeric operation.

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

## Fallible and lossy numeric operations

Lossy, fallible, saturating, wrapping, truncating, rounding, reshaping, flattening, allocation-changing, or layout-reinterpreting
conversions are ordinary operations with explicit callable contracts.

The standard library fallible conversion operation is `std.convert<Target>(source)`.

`std.convert<Target>(source)` returns `Result<Target, E>`.

Built-in fallible scalar conversions use `ConversionError` as `E`.

User-defined fallible conversions use the selected `CheckedConvertTo<Target>.Error` type as `E`.

Examples:

```bray
let x: Result<i32, ConversionError> = std.convert<i32>(value);
let y: i32 = try std.convert<i32>(value);

let rounded: r32 = std.round_to<r32>(value, rule = NearestEven);
let truncated: i32 = std.truncate_to<i32>(value);
let saturated: u8 = std.saturate_to<u8>(value);
let wrapped: u8 = std.wrap_to<u8>(value);
```

Rounding policy is an ordinary function argument, not conversion-expression syntax.

The Expression Model defines `as` conversion expression syntax.

The Type Model defines the `ConvertTo<Target>` and `CheckedConvertTo<Target>` trait contracts.

The Compiler-Known and Standard Library Model defines availability and compiler recognition for standard-library numeric
operations.

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
7. `S` has a participating `ConvertTo<T>` implementation.

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

A tuple cannot be converted to a non-tuple user type unless the tuple type has a participating `ConvertTo<T>` implementation for the target type.

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

Lossy or fallible conversions require ordinary operations with explicit callable contracts.

Composite conversion is recursive but structure-preserving.

No suffix-based numeric typing exists.

No conversion silently changes shape, layout, allocation, or ownership semantics.
