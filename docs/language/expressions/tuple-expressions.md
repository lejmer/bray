# Tuple expressions

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

The unit value is spelled `unit`.

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

Tuple element expressions are evaluated left to right.

Each tuple element has its own initialization state while the tuple is being constructed.

If evaluation of an element exits through `return`, `yield`, `break`, `continue`, `never`, cancellation, panic, or another non-local exit before the tuple is fully initialized, already-initialized element temporaries are handled by the corresponding control-flow, ownership, destruction, and finalization rules.

A tuple expression owns its elements when the element expressions produce owned values that are moved into the tuple.

A tuple expression copies an element when the element expression is copied according to that element type’s copy contract.

A tuple expression can contain borrowed values when an element expression produces a borrow value.

Moving a complete tuple moves every initialized element as part of the tuple move.

Copying a tuple requires every element type to satisfy the required copy contract.

Borrowing a tuple borrows the tuple storage.

Projecting a tuple element from a tuple access path creates an access path to that element.

Tuple element projection uses dot-number syntax.

```bray
let x = pair.0;
let y = pair.1;
```

Tuples do not support bracket indexing or slicing.

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

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Static function call expressions](static-function-call-expressions.md)
- Next: [Array expressions](array-expressions.md)
