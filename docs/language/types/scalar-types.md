# Scalar Types

A **scalar type** is a compiler-known named value type whose values are treated as atomic by the type system.

Scalar types are:

- fixed-width signed integer types,
- fixed-width unsigned integer types,
- machine-sized integer types,
- real floating-point types,
- complex floating-point types,
- `bool`,
- `char`,
- `unit`,
- `never`.

Scalar type names are
[compiler-known declarations](../compiler-known-and-standard-library/compiler-known-declarations.md).

User code cannot redeclare a scalar type name, add representation members to a scalar type, add variants to a scalar
type, or attach lifecycle declarations to a scalar type.

Scalar types have no user-visible fields, payload fields, tuple elements, array elements, or structural representation
members.

Scalar values have no partial-move state.

A scalar access path is initialized exactly when it contains a scalar value.

The `never` type has no values and no initialized storage state.

Every scalar type with values satisfies the copy contract.

Copying a scalar value copies the scalar value and creates no alias to mutable storage.

Moving a scalar value transfers the scalar value and creates no finalization or destruction obligation beyond ending the
old value's storage state according to ordinary ownership rules.

Destroying a scalar value ends the scalar value's storage state and runs no user code.

Scalar values can be borrowed according to ordinary borrow rules.

A mutable borrow of scalar storage grants mutation authority over the scalar value stored at that access path.

Scalar operations use the exact operand types after literal adaptation and explicit conversions have been applied.

Non-literal scalar values do not implicitly convert, promote, widen, narrow, or change numeric domain for assignment,
calls, operators, overload selection, construction, or pattern matching.

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
- [Types index](../types.md)
- Previous: [Type categories](type-categories.md)
- Next: [Type declarations](type-declarations.md)
