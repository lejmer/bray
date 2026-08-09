# Assignment expressions

An **assignment expression** writes a new value into an assignable access path.

```bray
x = value;
point.x = 2.0;
items[index] = value;
```

The left side of an assignment expression must produce an assignable access path.

The right side of an assignment expression must produce a value compatible with the destination type on every normal completion path.

A right side of type `never` satisfies the destination type because no value reaches the assignment write.

If the right side has no normal continuation, the destination is not reinitialized and the assignment expression has type `never` on that path.

Assignment requires mutation authority over the destination access path.

On normal completion, assignment returns `unit`.

```bray
counter.value += 1;
```

Compound assignment applies a binary operation to the current destination value and the right-side value, then assigns the operation
result back to the destination. Bray provides `+=`, `-=`, `*=`, `/=`, `%=`, `@=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, and `**=`.

Each compound assignment uses the same built-in or trait-backed operation as its binary operator. The operation result must be
compatible with the destination type.

The destination access path is evaluated exactly once. Compound assignment reads the established destination before evaluating the
right side and writes the operation result after the right side completes normally.

The destination access path can be a local binding access path, field access path, indexed access path, active union payload field access path, or another expression form that produces an assignable access path.

A local binding can be assigned through when it has mutable local access authority and the storage and type contract permit reinitialization.

```bray
let mut x: i32 = 1;
x = 2;
```

A field can be assigned through when the reached field permits mutation and the access path has mutation authority.

```bray
struct Counter
{
    mut value: i64;
}

let mut counter: Counter = { value = 0, };
counter.value = 1;
```

A mutable binding alone grants mutation authority over the binding’s access path. Field mutability still controls mutation of reached fields.

```bray
struct Point
{
    x: r64;
    y: r64;
}

let mut p: Point = { x = 1.0, y = 2.0, };
```

The binding `p` has mutable local access authority. The fields `x` and `y` remain immutable after initialization because their field declarations do not declare mutation.

Assignment to a union value replaces the active variant when the union access path has mutation authority.

```bray
shape = Rectangle(min = a, max = b);
```

Whole-union replacement ends the old active variant payload and initializes the new active variant payload.

Assignment to a union payload field requires active-variant refinement, mutation authority over the union value, and a mutable payload field.

Assignment ends the previous value in the destination according to destruction and lifecycle rules.

Assignment initializes or re-initializes the destination with the new value.

Reinitialization must be permitted by the destination storage and type contract.

The assigned value is moved into the destination unless the type is copyable or another explicit rule applies.

If the right side moves from an access path, that source access path becomes moved-from until reinitialized.

If assignment overwrites a value with a finalization obligation, the obligation must be completed, transferred, or converted into an explicit fallback ownership form before the old value’s ownership ends.

Assignment invalidates conditions that depend on the previous value stored in the destination.

Assignment invalidates conditions that depend on storage changed by the assignment.

Assignment preserves conditions that remain true after the destination is reinitialized.

Assignment participates in effect and capability checking.

The destination expression is evaluated first and establishes the destination access path.

The assigned value expression is evaluated after the destination access path is established.

The assignment operation itself requires the destination access path and assigned value to be established before the destination is reinitialized.

## Navigation

- [Language index](../index.md)
- [Expressions index](../expressions.md)
- Previous: [Index access expressions](index-access-expressions.md)
- Next: [Borrow expressions](borrow-expressions.md)
