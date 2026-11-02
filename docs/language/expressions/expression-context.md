# Expression context

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
- range bound typing,
- tuple element typing,
- array element typing,
- struct construction shorthand,
- union variant shorthand,
- box construction shorthand,
- conversion checking.

Expected type and expected result type do not select callable overload arms. Overload selection follows the explicitly
supplied arguments, receiver rules, generic constraints, and target-availability rules defined by the callable and
implementation overload specifications.

Example:

```bray
let shape: Shape = Circle(center = origin, radius = 10.0);
```

The expected type `Shape` lets `Circle(...)` resolve as a variant construction expression for `Shape` after ordinary
lexical lookup finds no declaration named `Circle`.

Example:

```bray
let node: box List<i32> = box(Empty);
```

The expected type `box List<i32>` lets `box(...)` expect an inner `List<i32>`, which lets `Empty` resolve as a variant
of `List<i32>`.

## Value type acceptance

Value type acceptance checks an expression against a required type after contextual typing. It applies to declared
initializers, assignments, returned values, and values supplied by `yield` or `break`.

An expression satisfies a required type `T` when its checked type is `T`, its checked type is `never`, or its checked
type is `U` and `T` is `U?`. The last case constructs a present nullable value. `never` supplies no value because it has
no normal continuation. Type aliases and equivalent type spellings denote their resolved type.

Contextual typing can first determine the type of a literal or construction expression. Numeric literals follow
[literal adaptation](literal-expressions.md#literal-adaptation). Tuple and array expressions pass their expected element
types to their elements. Struct, union, box, callable, and trait-view expressions follow their own contextual formation
rules. These rules do not change an already-typed numeric value to another numeric type.

Value type acceptance does not search for `ConvertTo` implementations or apply the recursive conversion rules of `as`.
An explicit [conversion expression](conversion-expressions.md) can produce the required type before this check.

Type acceptance alone does not authorize an operation. The expression must also meet the destination's ownership,
borrowing, initialization, dependency, lifecycle, capability, effect, and contract requirements. Returns and other
ownership transfers preserve dependencies under
[scope exits and ownership boundaries](../ownership-and-borrowing/scope-exits-and-ownership-boundaries.md).
Assignments follow [assignment expressions](assignment-expressions.md), including replacement of an existing value.

Call arguments use [argument type matching](arguments.md#argument-type-matching). Receivers use
[receiver matching](method-call-expressions.md#receiver-matching). Explicit conversions and error propagation have their
own relations. None of these relations supplies a general implicit conversion between distinct types.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Expression results](expression-results.md)
- Next: [Sequenced expressions](sequenced-expressions.md)
