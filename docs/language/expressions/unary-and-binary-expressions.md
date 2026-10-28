# Unary and binary expressions

A **unary expression** applies a prefix unary token to one operand.

A **binary expression** applies an infix binary token between two operands.

```bray
let inverse = -value;
let flipped = ~bits;
let total = left + right;
```

Unary and binary tokens, precedence, and associativity are fixed by the language.

Overloadable unary and binary token behavior is provided by compiler-known operator traits.

Source code does not bind arbitrary functions, methods, or traits to unary or binary tokens.

The following tables define the complete unary and binary expression token set.

Precedence is relative. Larger precedence numbers bind tighter.

Unary expression tokens:

| Token | Precedence | Primary function    | Overloadable                  |
|-------|------------|---------------------|-------------------------------|
| `-`   | 11         | arithmetic negation | yes, through `Negate.negate`  |
| `~`   | 11         | bitwise complement  | yes, through `BitNot.bit_not` |
| `!`   | 11         | boolean negation    | no                            |

Binary expression tokens:

| Token     | Precedence | Associativity | Primary function                  | Overloadable                                       |
|-----------|------------|---------------|-----------------------------------|----------------------------------------------------|
| `**`      | 12         | right         | exponentiation                    | yes, through `Exponentiate<Rhs>.exponentiate`      |
| `*`       | 10         | left          | multiplication                    | yes, through `Multiply<Rhs>.multiply`              |
| `/`       | 10         | left          | division                          | yes, through `Divide<Rhs>.divide`                  |
| `%`       | 10         | left          | remainder                         | yes, through `Remainder<Rhs>.remainder`            |
| `@`       | 10         | left          | linear-algebra multiplication     | yes, through `MatrixMultiply<Rhs>.matrix_multiply` |
| `+`       | 9          | left          | addition                          | yes, through `Add<Rhs>.add`                        |
| `-`       | 9          | left          | subtraction                       | yes, through `Subtract<Rhs>.subtract`              |
| `<<`      | 8          | left          | shift left                        | yes, through `ShiftLeft<Rhs>.shift_left`           |
| `>>`      | 8          | left          | shift right                       | yes, through `ShiftRight<Rhs>.shift_right`         |
| `&`       | 7          | left          | bitwise and                       | yes, through `BitAnd<Rhs>.bit_and`                 |
| `^`       | 6          | left          | bitwise xor                       | yes, through `BitXor<Rhs>.bit_xor`                 |
| `\|`      | 5          | left          | bitwise or                        | yes, through `BitOr<Rhs>.bit_or`                   |
| `==`      | 4          | none          | equality comparison               | yes, through `Equatable<Rhs>.equals`               |
| `!=`      | 4          | none          | inequality comparison             | yes, derived from `Equatable<Rhs>.equals`          |
| `<`       | 4          | none          | less-than comparison              | yes, derived from `Comparable<Rhs>.compare`        |
| `<=`      | 4          | none          | less-than-or-equal comparison     | yes, derived from `Comparable<Rhs>.compare`        |
| `>`       | 4          | none          | greater-than comparison           | yes, derived from `Comparable<Rhs>.compare`        |
| `>=`      | 4          | none          | greater-than-or-equal comparison  | yes, derived from `Comparable<Rhs>.compare`        |
| `matches` | 4          | none          | structural pattern test           | no                                                 |
| `&&`      | 2          | left          | short-circuit boolean conjunction | no                                                 |
| `\|\|`    | 1          | left          | short-circuit boolean disjunction | no                                                 |

Exponentiation binds tighter than prefix unary negation, bitwise complement, and boolean negation.

Therefore `-x ** y` is parsed as `-(x ** y)`.

Comparisons and [structural pattern tests](pattern-test-expressions.md) are non-associative. The operand after
`matches` is a pattern, including any `|` alternatives, rather than a runtime value expression.

To combine comparisons, use boolean operators explicitly.

`!`, `&&`, and `||` require `bool` operands and produce `bool`.

`&&` and `||` are short-circuiting.

Borrow expressions, conversion expressions, field access, calls, indexing, slicing, assignment, construction,
pattern-bearing forms, `try`, `catch`, `await`, and lifecycle forms are separate expression forms.

They are not unary or binary expression tokens.

The overloadable token set is defined by the compiler-known [operator traits](../types/traits.md#operator-traits).

The expression:

```bray
left + right
```

is a binary expression that resolves as a call to the public operator trait member selected for the left operand type
and the right operand type.

For binary `+`, the relevant trait application is:

```bray
LeftType(Add<RightType>)
```

The result type of a unary or binary expression using an overloadable token is the selected operator trait member's
declared result after applying the selected implementation's type-valued member bindings.

Overloadable token resolution follows the implementation selection rules defined by
[Operator traits](../types/traits.md#operator-traits).

Unary and binary expressions using overloadable tokens follow the public participation, internal-access, and operand
authority rules defined by [Operator traits](../types/traits.md#operator-traits).

Unary and binary expressions using overloadable tokens do not consume operands.

Unary and binary operand evaluation order is defined by the general expression evaluation order rules.

## Real exponentiation

Real operands of `**` have the same real type, and the result has that type. `**=` and built-in generic exponentiation
use the same rules. These operations do not implicitly change precision or convert a real-domain result to complex.

A positive or negative zero exponent returns positive one for every base, including zero, infinity, and NaN. A base of
positive one returns positive one for every exponent, including NaN. Negative one raised to either infinite exponent
also returns positive one. Other NaN inputs produce the selected format's positive quiet NaN without preserving payloads.

A finite negative nonzero base with a finite nonintegral exponent produces NaN. An integral exponent produces a negative
result exactly when the base is negative and the exponent is odd. Integrality and parity use all bits of the selected
exponent representation, including exponents larger than a machine integer.

For zero bases and finite nonzero exponents, a positive exponent produces zero and a negative exponent produces infinity.
The result is negative only for a negative zero base with an odd integral exponent. For infinite bases and finite
nonzero exponents, a positive exponent produces infinity and a negative exponent produces zero. The same odd-integral
rule determines the sign, including for negative infinity with a fractional exponent.

For infinite exponents, only the base's magnitude matters. A magnitude greater than one produces positive infinity for a
positive infinite exponent and positive zero for a negative infinite exponent. A magnitude less than one produces
positive zero and positive infinity respectively. A magnitude of one produces positive one.

Constant evaluation and native execution share software arithmetic in the selected IEEE format, including binary16 and
binary128. Finite powers use bounded integer exponentiation or logarithm/exponential composition with at least 128 guard
bits beyond the selected precision. Working precision also grows with the exponent's magnitude. Final encoding rounds
to nearest with ties to even, preserving the sign of underflowed zeros and overflowed infinities. General power results
are not guaranteed to be correctly rounded. Zero raised to a negative exponent follows these power rules in constants
as well as at runtime, independently of real division's constant-expression zero-divisor rule.

## Complex arithmetic

Complex operands of `+`, `-`, `*`, `/`, and `**` have the same complex type. Unary `-` negates both components.
The result has that complex type. These operators do not implicitly convert real values to complex values or change
component precision. Equality compares both real components using IEEE equality. Inequality is true if either component
is unequal, including an unordered comparison with NaN.

Addition and subtraction use the selected IEEE component arithmetic. Negation flips each component's sign, including
signed zeros. For finite multiplication and division, intermediate products and sums have sufficient range and precision
to avoid intermediate overflow, underflow, and cancellation loss. Each final component rounds to its selected IEEE format
using round to nearest with ties to even. Constant evaluation and native execution use the same deterministic numerical
implementation for multiplication, division, and exponentiation. Native addition, subtraction, and negation use component
instructions directly.

For multiplication and division, a complex value with an infinite component represents complex infinity, even if its other
component is NaN. Multiplication normalizes its infinite components to signed ones and its other components to signed zeros, computes the component
products, and scales the result by positive infinity. A NaN component in the other operand becomes signed zero for this
recovery. This preserves the direction of determinate infinite results. An indeterminate component such as infinity times
zero is NaN.

Division by a complex zero scales each numerator component by an infinity with the sign of the divisor's real zero.
Consequently zero divided by zero has two NaN components. A finite numerator divided by complex infinity yields signed
zeros determined by the normalized divisor. Complex infinity divided by a finite nonzero divisor yields the corresponding
infinite or indeterminate components. Infinity divided by infinity has two NaN components. These rules also apply to closed
constant expressions. Real division's existing constant-expression zero-divisor rule is separate.

Complex exponentiation computes the principal value of `exp(exponent * log(base))`. The argument lies between negative pi
and positive pi. On the negative real axis, a positive imaginary zero selects positive pi and a negative imaginary zero
selects negative pi. Square roots therefore preserve the two sides of that branch cut. Powers on exact real, imaginary,
and diagonal axes preserve exact zero components when the resulting phase is a multiple of half pi. Infinite magnitudes
retain these zero components.

A zero exponent returns `1 + 0i`, including for a NaN or infinite base. A base of `1 + 0i` or `1 - 0i` returns `1 + 0i`,
including for a nonfinite exponent. An exponent of `1 + 0i` or `1 - 0i` returns the base. Other NaN inputs produce two NaN
components. A zero base with a positive real exponent has zero magnitude. A negative real exponent gives it infinite
magnitude. A zero or infinite base with a nonreal exponent produces two NaN components. Infinite exponents are supported
on the positive real axis by the real magnitude limit and otherwise produce two NaN components.

General powers use software transcendental arithmetic with at least 128 guard bits beyond component precision. Working
precision also grows with the exponent's magnitude to preserve phase accuracy. Composite power results are not guaranteed
to be correctly rounded. Multiplication, division, and power results use positive quiet NaNs for indeterminate components
and do not preserve NaN payloads. The exponent-one case preserves base bits except for NaN normalization.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Method call expressions](method-call-expressions.md)
- Next: [Static function call expressions](static-function-call-expressions.md)
