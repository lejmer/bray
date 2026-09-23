# Array expressions

An **array expression** constructs a fixed-size array value.

An array expression with individually supplied elements uses comma-separated element expressions inside square brackets.

```bray
let values: [i32; 4] = [1, 2, 3, 4];
```

The array length is the number of element expressions.

An array expression with no supplied elements is rejected.
An empty byte string `b""` instead has the fixed array type `bytes<0>`.

A byte string literal has the same value and ordinary array behavior as its encoded fixed `u8` array.
For example, `b"lib\0"` is `[108, 105, 98, 0]`, while `b"lib"` has no terminator.
Indexing returns `u8`; slicing and borrowing use the existing array and byte slice rules.

The array element type is determined by the expected array type when one is available, or inferred from the element
expressions when no expected array type is available.

When an expected array type is available, the expected array length must equal the number of supplied element
expressions.

```bray
let values: [i64; 3] = [1, 2, 3];
```

Each element expression is checked against the expected array element type.

Expected element type can guide literal typing, variant shorthand, struct construction shorthand, box construction
shorthand, conversion checking, and nested expression checking.

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

Array element expressions are evaluated left to right.

Each array element has its own initialization state while the array is being constructed.

If evaluation of an element exits through `return`, `yield`, `break`, `continue`, `never`, cancellation, panic, or
another non-local exit before the array is fully initialized, already-initialized element temporaries are handled by the
corresponding control-flow, ownership, destruction, and finalization rules.

An array expression owns its elements when the element expressions produce owned values moved into the array.

An array expression copies an element when the element expression is copied according to the element type’s copy
contract.

An array expression can contain borrowed values when element expressions produce borrow values.

Moving a complete array moves every initialized element as part of the array move.

Copying an array requires the element type to satisfy the required copy contract.

Borrowing an array borrows the array storage.

Indexing into an array access path creates an access path to an element.

Shared borrowing an array can provide shared access to elements according to Bray's borrowing rules.

Mutable borrowing an array can provide mutable access to elements when the array access path, element access path, and
element type permit mutation.

Moving an element out of an array is a partial move of the array when the array rules permit element moves.

A partial move from an array requires ownership of the array and no conflicting active borrows.

After an array element has been moved out, the array is partially initialized.

A partially moved array can be reinitialized or consumed by a rule that accounts for its state.

Destruction of a partially moved array destroys only still-initialized elements.

Destruction of a fully initialized array destroys its initialized elements according to Bray array destruction order.

Array expressions participate in effect checking and capability checking through their element expressions.

The array expression’s effects are the combined effects of evaluating its element expressions.

The array expression’s finalization obligations are the combined finalization obligations of values produced by its
element expressions and retained by the resulting array.

An array expression can establish conditions at that program point.

Conditions can include array length, element initialization, element type, and conditions established by element
expressions.

Mutation, movement, consumption, destruction, reinitialization, finalization, or capability loss can invalidate
conditions about array elements or the array as a whole.

An array expression can be converted with `as` when the recursive explicit convertibility rules permit array conversion.

```bray
let a: [i32; 4] = [1, 2, 3, 4];
let b: [i64; 4] = a as [i64; 4];
```

Array-to-array conversion requires the same length and an explicitly valid conversion from the source element type to
the target element type.

Array conversion preserves length and shape.

## Repeated-element array expressions

A repeated-element array expression uses `[element; count]`.

```bray
let zeros: [i32; 4] = [0; 4];
```

The count determines the array length.

The count participates in the array type.

The count must be greater than zero.

The count must be known where the array length is required as a compile-time value.

The element expression is checked against the expected array element type when one is available.

The repeated element expression must be copyable or define a valid repeat contract.

A repeated-element array expression evaluates the element expression according to the repeat contract and initializes
each array element according to that contract.

For copy-based repetition, the repeated value is copied into each element according to the element type’s copy contract.

A repeated-element array expression is fully initialized when every array element has been initialized.

Effects and finalization obligations of the repeated element expression and repeat operation become part of the
repeated-element array expression.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Tuple expressions](tuple-expressions.md)
- Next: [Range expressions](range-expressions.md)
