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

| Token  | Precedence | Associativity | Primary function                  | Overloadable                                       |
|--------|------------|---------------|-----------------------------------|----------------------------------------------------|
| `**`   | 12         | right         | exponentiation                    | yes, through `Exponentiate<Rhs>.exponentiate`      |
| `*`    | 10         | left          | multiplication                    | yes, through `Multiply<Rhs>.multiply`              |
| `/`    | 10         | left          | division                          | yes, through `Divide<Rhs>.divide`                  |
| `%`    | 10         | left          | remainder                         | yes, through `Remainder<Rhs>.remainder`            |
| `@`    | 10         | left          | linear-algebra multiplication     | yes, through `MatrixMultiply<Rhs>.matrix_multiply` |
| `+`    | 9          | left          | addition                          | yes, through `Add<Rhs>.add`                        |
| `-`    | 9          | left          | subtraction                       | yes, through `Subtract<Rhs>.subtract`              |
| `<<`   | 8          | left          | shift left                        | yes, through `ShiftLeft<Rhs>.shift_left`           |
| `>>`   | 8          | left          | shift right                       | yes, through `ShiftRight<Rhs>.shift_right`         |
| `&`    | 7          | left          | bitwise and                       | yes, through `BitAnd<Rhs>.bit_and`                 |
| `^`    | 6          | left          | bitwise xor                       | yes, through `BitXor<Rhs>.bit_xor`                 |
| `\|`   | 5          | left          | bitwise or                        | yes, through `BitOr<Rhs>.bit_or`                   |
| `==`   | 4          | none          | equality comparison               | yes, through `Equatable<Rhs>.equals`               |
| `!=`   | 4          | none          | inequality comparison             | yes, derived from `Equatable<Rhs>.equals`          |
| `<`    | 4          | none          | less-than comparison              | yes, derived from `Comparable<Rhs>.compare`        |
| `<=`   | 4          | none          | less-than-or-equal comparison     | yes, derived from `Comparable<Rhs>.compare`        |
| `>`    | 4          | none          | greater-than comparison           | yes, derived from `Comparable<Rhs>.compare`        |
| `>=`   | 4          | none          | greater-than-or-equal comparison  | yes, derived from `Comparable<Rhs>.compare`        |
| `&&`   | 2          | left          | short-circuit boolean conjunction | no                                                 |
| `\|\|` | 1          | left          | short-circuit boolean disjunction | no                                                 |

Exponentiation binds tighter than prefix unary negation, bitwise complement, and boolean negation.

Therefore `-x ** y` is parsed as `-(x ** y)`.

Comparisons are non-associative.

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

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Method call expressions](method-call-expressions.md)
- Next: [Static function call expressions](static-function-call-expressions.md)
